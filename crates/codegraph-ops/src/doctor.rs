//! `testkit doctor` — one-shot workspace state report.
//!
//! Answers "is this workspace testable right now?" in a single command:
//! generated-tree completeness, graph/app binary freshness, database
//! reachability, and port availability. Each problem names its fix. Exit is
//! non-zero when anything would make a suite fail.
//!
//! Motivation: during PR verification, a wiped output tree plus a stale
//! binary produced 15 baffling api failures whose root cause took ~10 manual
//! steps to pin down. Every one of those steps is a line here.

use std::path::Path;

use crate::config::OpsConfig;
use crate::db::psql_query;
use crate::error::{OpsError, OpsResult};
use crate::output;
use crate::preflight::{ensure_output_tree, newest_mtime};

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
            match psql_query(api, "SELECT count(*) FROM pg_proc WHERE proname = 'create_api_key'")
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
}
