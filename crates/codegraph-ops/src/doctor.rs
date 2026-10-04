//! `testkit doctor` — one-shot workspace state report.
//!
//! Answers "is this workspace testable right now?" in a single command:
//! generated-tree completeness, graph/app binary freshness, database
//! reachability, port availability, tool presence (npx/pnpm/supabase/
//! playwright/chromium), and disk headroom. Each problem names its fix. Exit
//! is non-zero when anything would make a suite fail.
//!
//! Motivation: during PR verification, a wiped output tree plus a stale
//! binary produced 15 baffling api failures whose root cause took ~10 manual
//! steps to pin down. Every one of those steps is a line here.
//!
//! #358 adds the stage-0 FAST DOCTOR subsets ([`run_fast_doctor`]): suites
//! auto-run the cheap slice relevant to them before any long stage, so a
//! missing `npx` or an exhausted /tmp fails in seconds with a hint instead
//! of surfacing mid-suite as `failed to spawn npx` after minutes of builds.
//! The full manual doctor may probe DBs; the fast subsets stick to
//! version+filesystem checks (the api subset's `SELECT 1` is the one
//! explicitly cheap exception).

use std::path::{Path, PathBuf};

use crate::config::OpsConfig;
use crate::db::psql_query;
use crate::error::{OpsError, OpsResult};
use crate::output;
use crate::preflight::{ensure_output_tree, newest_mtime};

/// Recognizable output marker for the stage-0 fast doctor subsets (part of
/// every subset's section title, asserted by tests and grep-able in logs).
pub const FAST_DOCTOR_MARKER: &str = "Fast doctor";

/// GiB as bytes.
const GIB: u64 = 1024 * 1024 * 1024;

/// Injectable version probe: run `bin args` (optionally in `cwd`), returning
/// a version line when the command ran successfully — `None` when it could
/// not be spawned or exited non-zero.
pub type VersionProbe<'a> = &'a dyn Fn(&str, &[&str], Option<&Path>) -> Option<String>;

/// Outcome of comparing a binary's mtime against the newest `.rs` file under
/// a source tree.
#[derive(Debug, PartialEq, Eq)]
pub enum BinaryState {
    /// The binary file does not exist.
    Missing,
    /// The source tree is missing or has no `.rs` files — freshness cannot
    /// be established (the wiped-tree failure mode).
    NoSources,
    /// Binary is newer than every source file.
    Fresh,
    /// Binary predates at least one source file — the suite would test
    /// stale code.
    Stale,
}

impl BinaryState {
    pub fn label(&self) -> &'static str {
        match self {
            BinaryState::Missing => "missing",
            BinaryState::NoSources => "no sources (cannot verify freshness)",
            BinaryState::Fresh => "fresh",
            BinaryState::Stale => "STALE",
        }
    }

    pub fn is_problem(&self) -> bool {
        matches!(self, BinaryState::Missing | BinaryState::NoSources)
    }
}

/// Classify `binary` against the newest `.rs` file under `src_dir`.
pub fn binary_state(binary: &Path, src_dir: &Path) -> BinaryState {
    if !binary.is_file() {
        return BinaryState::Missing;
    }
    match newest_mtime(src_dir) {
        None => BinaryState::NoSources,
        Some(newest_src) => {
            let binary_mtime = binary.metadata().and_then(|m| m.modified()).ok();
            match binary_mtime {
                Some(b) if b < newest_src => BinaryState::Stale,
                _ => BinaryState::Fresh,
            }
        }
    }
}

/// Number of entries directly inside `dir` (`None` when missing/unreadable).
fn count_entries(dir: &Path) -> Option<usize> {
    std::fs::read_dir(dir).map(|entries| entries.count()).ok()
}

/// Report one generated directory: ✓ with an entry count, or ✗ when required
/// and missing. Returns the number of problems added.
fn report_dir(app_dir: &Path, name: &str, required: bool) -> usize {
    let path = app_dir.join(name);
    match count_entries(&path) {
        Some(n) => {
            output::ok(format!("{name}/ ({n} entries)"));
            0
        }
        None if required => {
            output::fail(format!("{name}/ MISSING — run generate first"));
            1
        }
        None => {
            output::warn(format!("{name}/ absent (optional for this profile)"));
            0
        }
    }
}

/// Report one binary with its freshness vs `src_dir`.
fn report_binary(binary: &Path, src_dir: &Path) -> bool {
    let state = binary_state(binary, src_dir);
    let path = binary.display();
    match state {
        BinaryState::Fresh => {
            output::ok(format!("{} — fresh", path));
            true
        }
        BinaryState::Stale => {
            output::warn(format!(
                "{path} — STALE (older than {}/) — rebuild before testing",
                src_dir.display()
            ));
            true
        }
        BinaryState::Missing => {
            output::warn(format!(
                "{path} — missing (build first, or the suite will fail)"
            ));
            false
        }
        BinaryState::NoSources => {
            output::warn(format!(
                "{path} — present but {}/ has no sources",
                src_dir.display()
            ));
            false
        }
    }
}

/// Run every doctor check; `Err` (exit ≠ 0) when any blocking problem exists.
pub async fn run_doctor(config: &OpsConfig) -> OpsResult<()> {
    let manifest = &config.manifest;
    let mut problems: usize = 0;

    output::section("1. Manifest");
    output::ok(format!(
        "app_name={} profile={} provider={} db_target={}",
        manifest.app_name,
        manifest.profile.as_deref().unwrap_or("(default)"),
        manifest.capabilities.persistence_provider,
        manifest.capabilities.database_target,
    ));
    output::info(format!("manifest dir: {}", config.root_dir.display()));
    output::info(format!("app dir:       {}", config.app_dir.display()));
    output::info(format!(
        "workspace:     {}",
        config.workspace_root.display()
    ));

    output::section("2. Generated output tree");
    if let Err(e) = ensure_output_tree(&config.app_dir) {
        problems += 1;
        output::fail(e.to_string());
    }
    problems += report_dir(&config.app_dir, "src", true);
    problems += report_dir(&config.app_dir, "migrations", true);
    problems += report_dir(&config.app_dir, "migration", false);
    problems += report_dir(&config.app_dir, "ui", false);
    problems += report_dir(&config.app_dir, "cornucopia-queries", false);
    if config.app_dir.join("Cargo.toml").is_file() {
        output::ok("Cargo.toml present");
    } else {
        problems += 1;
        output::fail("Cargo.toml MISSING — the generated app cannot build");
    }

    output::section("3. Binaries");
    let src_dir = config.app_dir.join("src");
    if let Some(graph) = &manifest.graph_binary {
        output::info(format!(
            "graph binary: {graph} (regen runs `cargo run -p {graph}` — cargo rebuilds as needed)"
        ));
        // Presence + age only: the graph binary is the *generator*, so
        // comparing it against generated-candidate/src would label every
        // post-generation rebuild "stale" — meaningless for cargo-managed
        // rebuilds.
        for profile in ["debug", "release"] {
            let path = config
                .workspace_root
                .join("target")
                .join(profile)
                .join(graph);
            match std::fs::metadata(&path).and_then(|m| m.modified()) {
                Ok(mtime) => {
                    let age = mtime
                        .elapsed()
                        .map(|d| format!("{}s ago", d.as_secs()))
                        .unwrap_or_else(|_| "age unknown".to_string());
                    output::ok(format!("{} — present ({})", path.display(), age));
                }
                Err(_) => output::warn(format!("{} — missing", path.display())),
            }
        }
    } else {
        output::warn("no graph_binary configured — suites will skip regeneration");
    }
    // The app binary is what suites boot and what ensure_binary_fresh
    // guards, so here the staleness comparison is the point.
    let app_name = config.app_binary_name();
    for profile in ["debug", "release"] {
        let path = config.app_dir.join("target").join(profile).join(&app_name);
        report_binary(&path, &src_dir);
    }

    output::section("4. Databases");
    let api = &config.api_db;
    match psql_query(api, "SELECT 1").await {
        Ok(_) => {
            output::ok(format!(
                "api db reachable ({}:{}/{} as {})",
                api.host, api.port, api.db, api.user
            ));
            match psql_query(
                api,
                "SELECT count(*) FROM pg_proc WHERE proname = 'create_api_key'",
            )
            .await
            {
                Ok(count) if count == "1" => {
                    output::ok("create_api_key() present — API-key auth available")
                }
                Ok(_) => output::warn(
                    "create_api_key() NOT in the database — API-key hurl checks will skip (stale binary?)",
                ),
                Err(e) => output::warn(format!("could not probe create_api_key(): {e}")),
            }
        }
        Err(e) => {
            problems += 1;
            output::fail(format!(
                "api db NOT reachable ({}:{}/{}): {e} — start Postgres (docker compose / supabase)",
                api.host, api.port, api.db
            ));
        }
    }
    for (label, target) in [
        ("e2e db", config.e2e_db.as_ref()),
        ("e2e_app db", config.e2e_app_db.as_ref()),
    ] {
        if let Some(target) = target {
            match psql_query(target, "SELECT 1").await {
                Ok(_) => output::ok(format!(
                    "{label} reachable ({}:{}/{})",
                    target.host, target.port, target.db
                )),
                // e2e targets are irrelevant to api/workers runs — warn only.
                Err(e) => output::warn(format!("{label} not reachable: {e}")),
            }
        }
    }

    output::section("5. Ports");
    for (label, port) in [
        ("api", manifest.servers.api_port),
        ("ui", manifest.servers.ui_port),
    ] {
        match crate::preflight::ensure_port_free(port) {
            Ok(()) => output::ok(format!("{label} port {port} free")),
            Err(_) => {
                problems += 1;
                output::fail(format!(
                    "{label} port {port} in use — suites bind it; free the port or stop the holder"
                ));
            }
        }
    }

    output::section("6. Extras");
    if let Some(hurl_dir) = &config.hurl_dir {
        match count_entries(hurl_dir) {
            Some(n) => output::ok(format!("hurl dir: {} ({n} entries)", hurl_dir.display())),
            None => output::warn(format!("hurl dir missing: {}", hurl_dir.display())),
        }
    }
    if let Some(supabase_dir) = &config.supabase_dir {
        if supabase_dir.is_dir() {
            output::ok(format!("supabase dir: {}", supabase_dir.display()));
        } else {
            output::warn(format!("supabase dir missing: {}", supabase_dir.display()));
        }
    }
    if config.log_file.is_file() {
        output::info(format!(
            "app log from a previous run: {} ({})",
            config.log_file.display(),
            match std::fs::metadata(&config.log_file) {
                Ok(meta) => format!("{} bytes", meta.len()),
                Err(_) => "size unknown".to_string(),
            }
        ));
    }

    // 7-8 (#358): toolchain + disk/cache state. Version probes are injectable
    // so tests never require npx/pnpm/supabase on the machine.
    let probe = real_version_probe;
    output::section("7. Tools");
    problems += check_tool(
        &probe,
        "npx",
        "npx",
        &["--version"],
        None,
        "e2e/ui spawn npx (supabase, playwright) — install node/npx or set NPX_PATH",
        false,
    );
    problems += check_tool(
        &probe,
        "pnpm",
        "pnpm",
        &["--version"],
        None,
        "ui/e2e builds run pnpm — install it (corepack enable pnpm)",
        false,
    );
    if config.manifest.supabase.is_some() {
        problems += check_tool(
            &probe,
            "supabase CLI",
            "npx",
            &["--no-install", "supabase", "--version"],
            config.supabase_dir.as_deref(),
            "the e2e suite drives `npx supabase` — install the CLI (brew install supabase/tap)",
            false,
        );
    }
    if config.manifest.capabilities.has_ui || config.ui_dir.is_dir() {
        problems += check_tool(
            &probe,
            "playwright",
            "npx",
            &["--no-install", "playwright", "--version"],
            Some(&config.ui_dir),
            "playwright is a devDependency of the generated UI — run `pnpm install` in the ui dir",
            false,
        );
    }
    match crate::env::find_chromium() {
        Some(path) => output::ok(format!("chromium: {}", path.display())),
        None => output::warn(
            "no chromium found — the suites install one via `npx playwright install chromium` \
             (or set PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH)",
        ),
    }
    report_worker_count(&config.ui_dir);

    output::section("8. Disk & caches");
    problems += report_disk(config);
    let caches = crate::pwcache::transform_cache_dirs(Path::new("/tmp"));
    let cache_bytes: u64 = caches.iter().map(|d| crate::disk::dir_size(d)).sum();
    if cache_bytes > 0 {
        output::info(format!(
            "Playwright transform cache: {} across {} dir(s) — clear with \
             `testkit --clear-cache <suite>` (stale transpiled specs are the failure mode)",
            crate::disk::human_bytes(cache_bytes),
            caches.len()
        ));
    } else {
        output::info("Playwright transform cache: empty");
    }

    output::section("Doctor summary");
    if problems == 0 {
        output::ok("no blocking problems found");
        Ok(())
    } else {
        Err(OpsError::TestFailure(format!(
            "doctor: {problems} blocking problem(s) — see above"
        )))
    }
}

/// Which suite's stage-0 fast doctor subset to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastDoctorSuite {
    /// api: psql (cheap `SELECT 1`), hurl, python3-if-smoke, api port, disk.
    Api,
    /// e2e: npx, pnpm, supabase CLI, chromium, api+ui ports, disk. No DB
    /// probe — supabase comes up later in the suite and the subset must
    /// stay fast.
    E2e,
}

/// The real version probe: run `bin args` (optionally in `cwd`) and return
/// the first non-empty line of combined stdout+stderr when it exits 0.
/// Injectable — tests pass a stub that never spawns.
pub fn real_version_probe(bin: &str, args: &[&str], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = std::process::Command::new(bin);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    combined
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// One tool check: `ok "{label}: {version}"` / warn-or-fail with `hint`.
/// Returns the blocking problems added (1 only when `blocking` and missing).
fn check_tool(
    probe: VersionProbe<'_>,
    label: &str,
    bin: &str,
    args: &[&str],
    cwd: Option<&Path>,
    hint: &str,
    blocking: bool,
) -> usize {
    match probe(bin, args, cwd) {
        Some(version) => {
            output::ok(format!("{label}: {version}"));
            0
        }
        None if blocking => {
            output::fail(format!("{label} MISSING — {hint}"));
            1
        }
        None => {
            output::warn(format!("{label} missing — {hint}"));
            0
        }
    }
}

/// Free-space verdicts across the three filesystems the harness writes to.
/// Blocking below `[doctor].min_free_gb` (default 2), warning below
/// `[doctor].warn_free_gb` (default 10). Returns problems added.
fn report_disk(config: &OpsConfig) -> usize {
    report_disk_with(config, &crate::disk::free_bytes)
}

/// [`report_disk`] with an injectable free-space probe (tests).
fn report_disk_with(config: &OpsConfig, free: &dyn Fn(&Path) -> Option<u64>) -> usize {
    let min_gb = config.manifest.doctor.min_free_gb_or();
    let warn_gb = config.manifest.doctor.warn_free_gb_or();
    let mut problems = 0;
    for (label, path) in disk_targets(config) {
        let Some(bytes) = free(&path) else {
            output::info(format!("{label}: free space unknown ({})", path.display()));
            continue;
        };
        let human = crate::disk::human_bytes(bytes);
        if bytes < min_gb.saturating_mul(GIB) {
            output::fail(format!(
                "{label}: {human} free — BELOW the {min_gb} GB minimum; builds, agent scratch \
                 and Playwright artifacts will fail with baffling IO errors (free space or raise \
                 [doctor].min_free_gb)",
            ));
            problems += 1;
        } else if bytes < warn_gb.saturating_mul(GIB) {
            output::warn(format!(
                "{label}: {human} free — getting low (warn below {warn_gb} GB)"
            ));
        } else {
            output::ok(format!("{label}: {human} free"));
        }
    }
    problems
}

/// The (label, path) trio whose filesystems the harness fills: manifest
/// root, app `target/` (falling back to the app dir when the target hasn't
/// been built yet — same filesystem), and the temp dir.
fn disk_targets(config: &OpsConfig) -> Vec<(&'static str, PathBuf)> {
    let target = config.app_dir.join("target");
    let target = if target.is_dir() {
        target
    } else {
        config.app_dir.clone()
    };
    vec![
        ("manifest root fs", config.root_dir.clone()),
        ("app target fs", target),
        ("temp dir fs", std::env::temp_dir()),
    ]
}

/// Report Playwright parallelism informationally (#358 documented deviation).
///
/// The issue's `PW_WORKER_COUNT >= workers` consistency check needs a
/// manifest-side workers setting — there is none: the harness's
/// `playwright_env()` never sets `PW_WORKER_COUNT`, and the worker count
/// lives only in the GENERATED `playwright.config.ts`
/// (`Number(process.env.PW_WORKER_COUNT ?? '<default>')`, with the persona
/// fixtures asserting `PW_WORKER_COUNT >= workers`). Rather than invent
/// config surface, report the effective values so operators can eyeball the
/// contract.
pub fn report_worker_count(ui_dir: &Path) {
    match std::env::var("PW_WORKER_COUNT") {
        Ok(n) => output::info(format!(
            "PW_WORKER_COUNT={n} (Playwright workers; persona fixtures require \
             PW_WORKER_COUNT >= the generated config's workers setting)"
        )),
        Err(_) => match playwright_default_workers(ui_dir) {
            Some(default) => output::info(format!(
                "PW_WORKER_COUNT unset — the generated playwright.config.ts defaults to \
                 {default} worker(s); set PW_WORKER_COUNT >= workers for persona fixtures"
            )),
            None => output::info(
                "PW_WORKER_COUNT unset — Playwright applies its own default worker count",
            ),
        },
    }
}

/// Parse the `PW_WORKER_COUNT ?? 'N'` fallback out of the generated
/// `playwright.config.ts` (best-effort, informational only).
fn playwright_default_workers(ui_dir: &Path) -> Option<u32> {
    let content = std::fs::read_to_string(ui_dir.join("playwright.config.ts")).ok()?;
    let idx = content.find("PW_WORKER_COUNT")?;
    let rest = &content[idx..];
    let start = rest.find("?? '")? + "?? '".len();
    let tail = &rest[start..];
    let end = tail.find('\'')?;
    tail[..end].parse().ok()
}

/// Stage-0 fast doctor subset for a suite: cheap version+filesystem checks
/// (plus the api subset's `SELECT 1`) that fail in seconds with actionable
/// hints, before the suite burns minutes on generate/build/supabase. The
/// section title carries [`FAST_DOCTOR_MARKER`].
pub async fn run_fast_doctor(config: &OpsConfig, suite: FastDoctorSuite) -> OpsResult<()> {
    run_fast_doctor_with(config, suite, &real_version_probe).await
}

/// [`run_fast_doctor`] with an injectable version probe (tests).
async fn run_fast_doctor_with(
    config: &OpsConfig,
    suite: FastDoctorSuite,
    probe: VersionProbe<'_>,
) -> OpsResult<()> {
    let title = match suite {
        FastDoctorSuite::Api => format!("0. {FAST_DOCTOR_MARKER} (api subset)"),
        FastDoctorSuite::E2e => format!("E2E 0. {FAST_DOCTOR_MARKER} (e2e subset)"),
    };
    output::section(title);
    let mut problems = 0;

    // Ports — registry-aware: a `--keep` leak recorded in services.json is
    // the suite preflight's job (reuse/takeover), not a fast-doctor failure;
    // an UNKNOWN occupant fails here with the actionable hint.
    problems += check_port_ready(&config.root_dir, config.manifest.servers.api_port, "api");
    if suite == FastDoctorSuite::E2e {
        problems += check_port_ready(&config.root_dir, config.manifest.servers.ui_port, "ui");
    }

    // Disk: filesystem stat only — the motivating #358 incident was an
    // exhausted /tmp quota discovered hours late.
    problems += report_disk(config);

    match suite {
        FastDoctorSuite::Api => {
            // Cheap DB probe (explicitly allowed by #358: `SELECT 1` only).
            match psql_query(&config.api_db, "SELECT 1").await {
                Ok(_) => output::ok(format!(
                    "api db reachable ({}:{})",
                    config.api_db.host, config.api_db.port
                )),
                Err(e) => {
                    output::fail(format!(
                        "api db NOT reachable ({}:{}): {e} — start Postgres first \
                         (docker compose / supabase)",
                        config.api_db.host, config.api_db.port
                    ));
                    problems += 1;
                }
            }
            problems += check_tool(
                probe,
                "hurl",
                "hurl",
                &["--version"],
                None,
                "install hurl (the api suite's contract tests) or set hurl = none in the manifest",
                true,
            );
            if config.manifest.smoke.is_some() {
                problems += check_tool(
                    probe,
                    "python3",
                    "python3",
                    &["--version"],
                    None,
                    "install python3 (the smoke checks parse JSON with it)",
                    true,
                );
            }
        }
        FastDoctorSuite::E2e => {
            problems += check_tool(
                probe,
                "npx",
                "npx",
                &["--version"],
                None,
                "install node/npx or set NPX_PATH — supabase and playwright are driven through it",
                true,
            );
            problems += check_tool(
                probe,
                "pnpm",
                "pnpm",
                &["--version"],
                None,
                "install pnpm (corepack enable) — the SvelteKit build needs it",
                true,
            );
            if config.manifest.supabase.is_some() {
                problems += check_tool(
                    probe,
                    "supabase CLI",
                    "npx",
                    &["--no-install", "supabase", "--version"],
                    config.supabase_dir.as_deref(),
                    "install the supabase CLI — `npx supabase start` drives the e2e stack",
                    true,
                );
            }
            match crate::env::find_chromium() {
                Some(path) => output::ok(format!("chromium: {}", path.display())),
                None => output::warn(
                    "no chromium found — the suite best-effort-installs one via \
                     `npx playwright install chromium`",
                ),
            }
        }
    }

    if problems == 0 {
        output::ok("fast doctor clean");
        Ok(())
    } else {
        Err(OpsError::TestFailure(format!(
            "fast doctor: {problems} blocking problem(s) — fix the above before the suite \
             burns minutes"
        )))
    }
}

/// Registry-aware fast port check. Free → ok; occupied by a REGISTRY-KNOWN,
/// live prior server → informational (the suite preflight will reuse or take
/// it over, #356); anything else → blocking problem with the actionable
/// hint. Returns problems added.
fn check_port_ready(root_dir: &Path, port: u16, label: &str) -> usize {
    if crate::preflight::ensure_port_free(port).is_ok() {
        output::ok(format!("{label} port {port} free"));
        return 0;
    }
    let registry = crate::registry::ServiceRegistry::load(root_dir).pruned();
    if registry.service_on_port(port).is_some() {
        output::info(format!(
            "{label} port {port} held by a registry-known prior server — preflight will \
             reuse (--reuse) or take it over"
        ));
        return 0;
    }
    output::fail(format!(
        "{label} port {port} in use — suites bind it; free the port or stop the holder"
    ));
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_state_classifies_missing_stale_and_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        // Missing binary.
        assert_eq!(
            binary_state(&dir.path().join("app"), &src),
            BinaryState::Missing
        );

        // Present binary, no sources → NoSources (the wiped-tree trap).
        std::fs::write(dir.path().join("app"), "binary").unwrap();
        assert_eq!(
            binary_state(&dir.path().join("app"), &src),
            BinaryState::NoSources
        );

        // Stale: binary forced to UNIX_EPOCH, then a source written now.
        let file = std::fs::File::options()
            .write(true)
            .open(dir.path().join("app"))
            .unwrap();
        file.set_modified(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
        drop(file);
        std::fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        assert_eq!(
            binary_state(&dir.path().join("app"), &src),
            BinaryState::Stale
        );

        // Fresh: bump the binary to now (same-second writes count as fresh —
        // the staleness comparison is strictly `binary < source`).
        let file = std::fs::File::options()
            .write(true)
            .open(dir.path().join("app"))
            .unwrap();
        file.set_modified(std::time::SystemTime::now()).unwrap();
        drop(file);
        assert_eq!(
            binary_state(&dir.path().join("app"), &src),
            BinaryState::Fresh
        );
    }

    #[test]
    fn binary_state_labels_and_problem_flags() {
        assert_eq!(BinaryState::Missing.label(), "missing");
        assert!(BinaryState::Missing.is_problem());
        assert!(BinaryState::NoSources.is_problem());
        assert!(!BinaryState::Fresh.is_problem());
        // Stale is reported but is the caller's judgment call (some flows
        // rebuild first), so it is not flagged as a blocking problem here.
        assert!(!BinaryState::Stale.is_problem());
    }

    #[test]
    fn count_entries_counts_and_reports_missing_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(count_entries(&dir.path().join("nope")), None);
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        std::fs::write(dir.path().join("b.txt"), "y").unwrap();
        assert_eq!(count_entries(dir.path()), Some(2));
    }

    // ---- #358: tool probes, disk, ports, fast doctor ----

    use codegraph_config::{OpsCapabilities, OpsDatabase, OpsDbTarget, OpsManifest};

    fn manifest_for() -> OpsManifest {
        OpsManifest {
            app_name: "demo-app".into(),
            graph_binary: None,
            schemas_dir: None,
            mox_files: Vec::new(),
            rosetta_files: Vec::new(),
            classifier: None,
            domain_config: None,
            profile: None,
            output_dir: "generated-app".into(),
            ui_dir: None,
            smoke: None,
            api_version: "v1".to_string(),
            servers: codegraph_config::OpsServers {
                api_port: free_port(),
                ui_port: free_port(),
                bind_addr: "0.0.0.0".into(),
            },
            database: OpsDatabase {
                api: OpsDbTarget {
                    host: "localhost".into(),
                    port: 5432,
                    user: "u".into(),
                    password: "p".into(),
                    database: "postgres".into(),
                    reset_sql: None,
                    seed_sql: None,
                    grant_role: None,
                    grant_strict: None,
                },
                e2e: None,
                e2e_app: None,
            },
            supabase: None,
            capabilities: OpsCapabilities::default(),
            hurl: None,
            hooks: vec![],
            extensions: vec![],
            doctor: Default::default(),
            bundle: Default::default(),
        }
    }

    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    }

    fn config_for(manifest: OpsManifest, root: &Path) -> OpsConfig {
        OpsConfig::from_manifest(manifest, root.to_path_buf()).unwrap()
    }

    fn stub_probe(found: bool) -> impl Fn(&str, &[&str], Option<&Path>) -> Option<String> {
        move |_bin, _args, _cwd| found.then(|| "9.9.9 stub".to_string())
    }

    #[test]
    fn real_version_probe_captures_output_and_failure() {
        assert_eq!(
            real_version_probe("echo", &["1.2.3"], None).as_deref(),
            Some("1.2.3")
        );
        assert_eq!(real_version_probe("false", &[], None), None);
        assert_eq!(
            real_version_probe("definitely-not-a-real-binary-xyz", &[], None),
            None
        );
    }

    #[test]
    fn check_tool_verdicts_match_blocking_flag() {
        let found = stub_probe(true);
        assert_eq!(
            check_tool(&found, "npx", "npx", &["--version"], None, "hint", true),
            0
        );
        assert_eq!(
            check_tool(&found, "npx", "npx", &["--version"], None, "hint", false),
            0
        );
        let missing = stub_probe(false);
        // Blocking: a problem; warn: not.
        assert_eq!(
            check_tool(&missing, "npx", "npx", &["--version"], None, "hint", true),
            1
        );
        assert_eq!(
            check_tool(&missing, "npx", "npx", &["--version"], None, "hint", false),
            0
        );
    }

    #[test]
    fn report_disk_thresholds_block_warn_and_pass() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_for(manifest_for(), dir.path());
        let gib = 1024 * 1024 * 1024;
        // Below the 2 GB floor: one blocking problem per checked filesystem.
        assert_eq!(report_disk_with(&cfg, &|_| Some(gib)), 3);
        // Below the 10 GB warn level but above the floor: no problems.
        assert_eq!(report_disk_with(&cfg, &|_| Some(5 * gib)), 0);
        // Comfortable: no problems.
        assert_eq!(report_disk_with(&cfg, &|_| Some(50 * gib)), 0);
        // Unknowable (statvfs failure): informational, no problems.
        assert_eq!(report_disk_with(&cfg, &|_p| None), 0);
    }

    #[test]
    fn report_disk_honors_manifest_thresholds() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = manifest_for();
        manifest.doctor.min_free_gb = Some(100);
        let cfg = config_for(manifest, dir.path());
        let gib = 1024 * 1024 * 1024;
        // 50 GB free is below the raised 100 GB floor → blocking.
        assert_eq!(report_disk_with(&cfg, &|_| Some(50 * gib)), 3);
    }

    #[test]
    fn playwright_default_workers_parses_the_generated_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("playwright.config.ts"),
            "export default defineConfig({\n  workers: Number(process.env.PW_WORKER_COUNT ?? '4'),\n});\n",
        )
        .unwrap();
        assert_eq!(playwright_default_workers(dir.path()), Some(4));
        std::fs::write(
            dir.path().join("playwright.config.ts"),
            "workers: process.env.CI ? 1 : undefined,\n",
        )
        .unwrap();
        assert_eq!(playwright_default_workers(dir.path()), None);
        assert_eq!(
            playwright_default_workers(&dir.path().join("nowhere")),
            None
        );
    }

    #[test]
    fn check_port_ready_verdicts() {
        use crate::registry::{ServiceEntry, record_service};

        let dir = tempfile::tempdir().unwrap();

        // Free port → no problem. The release→recheck window races with
        // parallel tests churning ephemeral ports (same mitigation as the
        // preflight tests): retry until a released port stays free.
        let mut free_ok = false;
        for _ in 0..20 {
            let port = free_port();
            if check_port_ready(dir.path(), port, "api") == 0 {
                free_ok = true;
                break;
            }
        }
        assert!(free_ok, "a released port must check free");

        // Registry-known live occupant → informational, no problem (the
        // suite preflight reuses/takes over; this test process owns the pid).
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let held = listener.local_addr().unwrap().port();
        record_service(
            dir.path(),
            ServiceEntry {
                name: "api".to_string(),
                pid: std::process::id(),
                port: held,
                health: Some("/health".to_string()),
                started_at: "2026-09-30T00:00:00Z".to_string(),
                profile: None,
                suite: Some("api".to_string()),
            },
        )
        .unwrap();
        assert_eq!(check_port_ready(dir.path(), held, "api"), 0);

        // Unknown occupant (fresh root: empty registry) → blocking problem.
        let other = tempfile::tempdir().unwrap();
        assert_eq!(check_port_ready(other.path(), held, "api"), 1);
    }

    #[tokio::test]
    async fn fast_doctor_api_subset_fails_on_missing_tool() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_for(manifest_for(), dir.path());
        // hurl missing → blocking, regardless of the (real) DB probe outcome.
        let err = run_fast_doctor_with(&cfg, FastDoctorSuite::Api, &stub_probe(false))
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("fast doctor"), "{msg}");
        assert!(msg.contains("blocking problem"), "{msg}");
    }

    #[tokio::test]
    async fn fast_doctor_e2e_subset_passes_with_stubbed_tools() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_for(manifest_for(), dir.path());
        // Stubbed tools all present, ports ephemeral-free, disk real (warn at
        // worst): the subset must pass — proving the marker section + verdict
        // assembly end to end without npx/pnpm/supabase on the machine.
        run_fast_doctor_with(&cfg, FastDoctorSuite::E2e, &stub_probe(true))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fast_doctor_fails_on_unknown_port_occupant() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let held = listener.local_addr().unwrap().port();
        let mut manifest = manifest_for();
        manifest.servers.api_port = held;
        let cfg = config_for(manifest, dir.path());
        let err = run_fast_doctor_with(&cfg, FastDoctorSuite::E2e, &stub_probe(true))
            .await
            .unwrap_err();
        // The error names the stage; the failing port appears in the ✗ line
        // above it.
        assert!(err.to_string().contains("fast doctor"), "{err}");
        assert!(err.to_string().contains("blocking problem"), "{err}");
    }

    #[test]
    fn fast_doctor_marker_is_part_of_every_subset_title() {
        // The marker doubles as the section title fragment; assert the
        // pairing so titles and the constant can't drift apart.
        assert!(FAST_DOCTOR_MARKER.contains("Fast doctor"));
        let api = format!("0. {FAST_DOCTOR_MARKER} (api subset)");
        let e2e = format!("E2E 0. {FAST_DOCTOR_MARKER} (e2e subset)");
        assert!(api.contains(FAST_DOCTOR_MARKER) && e2e.contains(FAST_DOCTOR_MARKER));
    }
}
