//! Full E2E suite (port of hr-platform/test.sh `cmd_e2e`, genericised):
//! supabase → generate → migrate → build → services → playwright.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::OpsConfig;
use crate::error::{OpsError, OpsResult};
use crate::output;
use crate::proc::{ManagedProcess, Supervisor};
use crate::wait::wait_for_url;

/// Logs for the e2e services.
const APP_LOG: &str = "/tmp/codegraph-ops-e2e-app.log";
const SVELTEKIT_LOG: &str = "/tmp/codegraph-ops-sveltekit-e2e.log";

/// Arguments for the full E2E suite.
pub struct E2eArgs {
    pub keep: bool,
    pub skip_build: bool,
    pub skip_generate: bool,
    pub release: bool,
    pub headed: bool,
    pub playwright_args: Vec<String>,
}

/// Full E2E: supabase → generate → migrate → build → services → playwright.
/// Requires manifest.supabase and manifest.database.e2e to be set (else
/// Err(Config) explaining what's missing).
///
/// `post_e2e` hooks fire on EVERY path (success and failure) — best-effort,
/// with failures warned, so consumer cleanup steps always run.
pub async fn run_e2e(config: &OpsConfig, args: &E2eArgs) -> OpsResult<()> {
    let result = run_e2e_inner(config, args).await;
    if let Err(e) = crate::ext::run_hooks(config, "post_e2e").await {
        output::warn(format!("post_e2e hook failed: {e}"));
    }
    result
}

async fn run_e2e_inner(config: &OpsConfig, args: &E2eArgs) -> OpsResult<()> {
    output::section("=== E2E Suite ===");

    if config.manifest.supabase.is_none() {
        return Err(OpsError::Config(
            "e2e requires a manifest [[supabase]] section (dir + standard local keys)".to_string(),
        ));
    }
    if config.manifest.database.e2e.is_none() {
        return Err(OpsError::Config(
            "e2e requires manifest database.e2e (the supabase postgres target, usually :54322)"
                .to_string(),
        ));
    }
    let Some(supabase_dir) = config.supabase_dir.as_ref() else {
        return Err(OpsError::Config(
            "supabase.dir did not resolve to a path".to_string(),
        ));
    };

    let mut supervisor = Supervisor::new(args.keep);

    // 1. Supabase.
    output::section("E2E 1. Supabase");
    let health_url = supabase_health_url(config);
    if http_ok(&health_url).await {
        output::ok(format!("Supabase already running ({health_url})"));
    } else {
        output::info("Starting Supabase (npx supabase start)...");
        run_blocking("npx", &["supabase", "start"], supabase_dir)?;
        output::ok("Supabase started");
    }
    // pre_e2e hooks (e.g. the pgmq patch) need the supabase container running
    // and must complete BEFORE the migration symlink + `supabase db reset`.
    crate::ext::run_hooks(config, "pre_e2e").await?;

    // 2. Generate.
    output::section("E2E 2. Generate");
    if !args.skip_generate {
        let binary = match &config.manifest.graph_binary {
            Some(b) if config.manifest.schemas_dir.is_some() => Some(b),
            Some(_) => {
                output::warn("schemas_dir not configured — skipping generation");
                None
            }
            None => {
                output::warn("no graph_binary configured — skipping generation");
                None
            }
        };
        if let Some(binary) = binary {
            crate::ext::run_hooks(config, "pre_generate").await?;
            output::info(format!("Building {binary} (release)..."));
            run_blocking(
                "cargo",
                &["build", "-p", binary, "--release"],
                &config.workspace_root,
            )
            .map_err(|e| OpsError::TestFailure(format!("graph binary build failed: {e}")))?;
            let gen_bin = config
                .workspace_root
                .join("target")
                .join("release")
                .join(binary);
            if !gen_bin.is_file() {
                return Err(OpsError::TestFailure(format!(
                    "{binary} build produced no binary at {}",
                    gen_bin.display()
                )));
            }
            let gen_args = generate_args(config);
            output::info("Generating app...");
            let gen_bin_str = gen_bin.to_string_lossy().into_owned();
            let gen_arg_refs: Vec<&str> = gen_args.iter().map(String::as_str).collect();
            run_blocking(&gen_bin_str, &gen_arg_refs, &config.root_dir)?;
            if !generation_outputs(config) {
                return Err(OpsError::TestFailure(
                    "code generation produced no output (src/ui/migrations empty)".to_string(),
                ));
            }
            output::ok("App generated");
            crate::ext::run_hooks(config, "post_generate").await?;
        }
    } else {
        output::info("Generation skipped (--skip-generate)");
    }

    // 3. Migrate.
    output::section("E2E 3. Database");
    let app_migrations = config.app_dir.join("migrations");
    let supabase_migrations = supabase_dir.join("supabase").join("migrations");
    if app_migrations.is_dir() {
        crate::migrate::link_migrations_to_supabase(&app_migrations, &supabase_migrations)?;
    } else {
        output::warn(format!(
            "no migrations dir at {} — skipping symlink",
            app_migrations.display()
        ));
    }
    output::info("Resetting database (npx supabase db reset)...");
    run_blocking("npx", &["supabase", "db", "reset"], supabase_dir)
        .map_err(|e| OpsError::TestFailure(format!("supabase db reset failed: {e}")))?;
    output::ok("Database reset with migrations");

    let seed = supabase_dir.join("supabase").join("seed.sql");
    if seed.is_file() {
        if let Some(e2e) = &config.e2e_db {
            crate::db::psql_exec_file_ok(e2e, &seed).await?;
            output::ok("seed.sql applied");
        }
    }

    crate::ext::run_hooks(config, "post_migrate").await?;

    // 4. Provision the API key (shared file, also used by the ui/cli suites).
    let api_key = super::ui::read_or_provision_api_key(config).await?;
    match &api_key {
        Some(_) => output::ok("API key provisioned"),
        None => output::warn("API key not provisioned — auth-dependent tests will fail"),
    }

    // 5. Build.
    output::section("E2E 4. Build");
    if !args.skip_build {
        let mut build_args = vec!["build"];
        if args.release {
            build_args.push("--release");
        }
        run_blocking("cargo", &build_args, &config.app_dir)
            .map_err(|e| OpsError::TestFailure(format!("app build failed: {e}")))?;
    }
    let binary =
        pick_binary(&config.app_dir, &config.app_binary_name(), args.release).ok_or_else(|| {
            OpsError::TestFailure(format!(
                "no app binary under {} — run without --skip-build",
                config.app_dir.join("target").display()
            ))
        })?;
    output::ok(format!("Using binary {}", binary.display()));

    // 6. Services.
    output::section("E2E 5. Start Services");
    let api_url = config.api_url();
    let db_url = config
        .e2e_app_db
        .as_ref()
        .map(|t| t.url())
        .unwrap_or_else(|| config.api_db.url());
    {
        let mut cmd = Command::new(&binary);
        cmd.arg("start")
            .arg("--bind-addr")
            .arg(bind_addr_with_port(config))
            .arg("--database-url")
            .arg(&db_url);
        cmd.env("CORS_ALLOWED_ORIGINS", config.ui_url());
        cmd.env("SUPABASE_JWT_SECRET", &config.jwt_secret);
        cmd.env("SUPABASE_URL", super::ui::supabase_base_url(config));
        match ManagedProcess::spawn(cmd, "Axum app", Path::new(APP_LOG)) {
            Ok(proc) => supervisor.add(proc),
            Err(e) => {
                return Err(OpsError::Command(format!(
                    "failed to spawn app server: {e}"
                )));
            }
        }
    }
    if let Err(e) = wait_for_url(&format!("{api_url}/health"), 30, "Axum").await {
        print_log_tail(APP_LOG);
        return Err(e);
    }
    output::ok("Axum API running");

    // pre_playwright hooks (e.g. UI-sync rsync steps) must land BEFORE the
    // SvelteKit production build so synced sources get compiled.
    crate::ext::run_hooks(config, "pre_playwright").await?;

    if !config.ui_dir.join("node_modules").is_dir() {
        if let Err(e) = run_blocking("pnpm", &["install"], &config.ui_dir) {
            output::warn(format!("pnpm install failed (continuing): {e}"));
        }
    }
    output::info("Building SvelteKit production bundle (best-effort)...");
    if let Err(e) = run_blocking("pnpm", &["run", "build"], &config.ui_dir) {
        output::warn(format!(
            "pnpm run build failed (preview may serve a stale bundle): {e}"
        ));
    }
    let ui_url = config.ui_url();
    {
        let mut cmd = Command::new("pnpm");
        cmd.arg("exec")
            .arg("vite")
            .arg("preview")
            .arg("--port")
            .arg(config.manifest.servers.ui_port.to_string());
        cmd.current_dir(&config.ui_dir);
        cmd.env("PUBLIC_API_URL", &api_url);
        if let Some(key) = &api_key {
            cmd.env("PUBLIC_API_KEY", key);
        }
        match ManagedProcess::spawn(cmd, "SvelteKit preview", Path::new(SVELTEKIT_LOG)) {
            Ok(proc) => supervisor.add(proc),
            Err(e) => {
                return Err(OpsError::Command(format!(
                    "failed to spawn vite preview: {e}"
                )));
            }
        }
    }
    if let Err(e) = wait_for_url(&ui_url, 45, "SvelteKit").await {
        print_log_tail(SVELTEKIT_LOG);
        return Err(e);
    }
    output::ok(format!("SvelteKit preview running at {ui_url}"));

    // 7. Playwright.
    output::section("E2E 6. Playwright Tests");
    if let Err(e) = run_blocking(
        "npx",
        &["playwright", "install", "chromium"],
        &config.ui_dir,
    ) {
        output::warn(format!(
            "playwright install chromium failed (continuing): {e}"
        ));
    }
    let mut cmd = Command::new("npx");
    cmd.arg("playwright").arg("test");
    if args.headed {
        cmd.arg("--headed");
    }
    cmd.args(&args.playwright_args);
    for (key, value) in super::ui::playwright_env(config, api_key.as_deref()) {
        cmd.env(key, value);
    }
    if let Some(chromium) = find_chromium(&[
        Path::new("/snap/bin/chromium"),
        Path::new("/usr/bin/chromium-browser"),
    ]) {
        cmd.env("PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH", chromium);
    }
    cmd.current_dir(&config.ui_dir);
    let status = cmd
        .status()
        .map_err(|e| OpsError::Command(format!("failed to spawn playwright: {e}")))?;
    let passed = status.success();

    // 8. Summary.
    output::section("=== E2E Summary ===");
    if passed {
        output::ok("ALL E2E TESTS PASSED");
    } else {
        output::fail("SOME E2E TESTS FAILED");
    }
    supervisor.shutdown_all().await;
    if passed {
        Ok(())
    } else {
        Err(OpsError::TestFailure(
            "Playwright E2E suite failed".to_string(),
        ))
    }
}

/// Build the graph-binary `run ...` argument vector. Only manifest flags
/// whose values are `Some` are passed.
fn generate_args(config: &OpsConfig) -> Vec<String> {
    let schemas = config
        .manifest
        .schemas_dir
        .as_ref()
        .expect("caller checks schemas_dir")
        .to_string_lossy()
        .into_owned();
    let mut args = vec!["run".to_string(), "--schemas".to_string(), schemas];
    if let Some(classifier) = &config.manifest.classifier {
        args.push("--classifier".to_string());
        args.push(classifier.to_string_lossy().into_owned());
    }
    if let Some(domain_config) = &config.manifest.domain_config {
        args.push("--config".to_string());
        args.push(domain_config.to_string_lossy().into_owned());
    }
    if let Some(profile) = &config.manifest.profile {
        args.push("--profile".to_string());
        args.push(profile.clone());
    }
    args.push("--output".to_string());
    args.push(config.app_dir.to_string_lossy().into_owned());
    args
}

/// True when generation produced anything (src/ui/migrations with content).
fn generation_outputs(config: &OpsConfig) -> bool {
    dir_has_content(&config.app_dir.join("src"))
        || dir_has_content(&config.app_dir.join("migrations"))
        || dir_has_content(&config.app_dir.join("ui"))
}

fn dir_has_content(dir: &Path) -> bool {
    dir.is_dir()
        && std::fs::read_dir(dir)
            .map(|mut it| it.next().is_some())
            .unwrap_or(false)
}

/// Locate the app binary: preferred profile first, falling back to the other.
pub fn pick_binary(app_dir: &Path, name: &str, release: bool) -> Option<PathBuf> {
    let (preferred, other) = if release {
        ("release", "debug")
    } else {
        ("debug", "release")
    };
    for profile in [preferred, other] {
        let candidate = app_dir.join("target").join(profile).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// `{bind_addr}:{api_port}` — unless bind_addr already carries a port.
fn bind_addr_with_port(config: &OpsConfig) -> String {
    let bind = &config.manifest.servers.bind_addr;
    if bind.contains(':') {
        bind.clone()
    } else {
        format!("{bind}:{}", config.manifest.servers.api_port)
    }
}

/// Health URL to probe for a running supabase stack.
fn supabase_health_url(config: &OpsConfig) -> String {
    config
        .manifest
        .supabase
        .as_ref()
        .and_then(|s| s.health_url.clone())
        .unwrap_or_else(|| "http://localhost:54321/auth/v1/health".to_string())
}

/// First existing candidate path (system chromium fallback for Playwright).
fn find_chromium(candidates: &[&Path]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|p| p.is_file())
        .map(|p| p.to_path_buf())
}

/// True when `curl -sf` succeeds against `url`.
async fn http_ok(url: &str) -> bool {
    Command::new("curl")
        .args(["-sf", "--max-time", "5"])
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a blocking command to completion, returning Err(Command) with a
/// stdout/stderr tail on non-zero exit.
fn run_blocking(bin: &str, args: &[&str], cwd: &Path) -> OpsResult<()> {
    let out = Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| {
            OpsError::Command(format!("failed to spawn {bin} in {}: {e}", cwd.display()))
        })?;
    if out.status.success() {
        return Ok(());
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Err(OpsError::Command(format!(
        "{bin} {args:?} failed in {} (exit {:?}):\n{}",
        cwd.display(),
        out.status.code(),
        tail(&text, 800)
    )))
}

fn print_log_tail(path: &str) {
    if let Ok(contents) = std::fs::read_to_string(path) {
        let t = tail(&contents, 800);
        if !t.is_empty() {
            output::fail(format!("--- {path} (tail) ---\n{t}"));
        }
    }
}

/// Last `max` chars of `s`, prefixed with a truncation marker (UTF-8 safe).
fn tail(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let skipped = chars.len() - max;
    let rest: String = chars[skipped..].iter().collect();
    format!("…[truncated {skipped} chars]\n{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::{OpsDatabase, OpsDbTarget, OpsManifest};

    fn manifest_with(graph_binary: Option<&str>) -> OpsManifest {
        OpsManifest {
            app_name: "demo-app".into(),
            graph_binary: graph_binary.map(String::from),
            schemas_dir: Some("schemas".into()),
            classifier: Some("classifier.toml".into()),
            domain_config: None,
            profile: Some("default".into()),
            output_dir: "generated-app".into(),
            ui_dir: None,
            smoke: None,
            api_version: "v1".to_string(),
            servers: codegraph_config::OpsServers {
                api_port: 3000,
                ui_port: 5173,
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
                },
                e2e: None,
                e2e_app: None,
            },
            supabase: None,
            capabilities: Default::default(),
            hurl: None,
            hooks: vec![],
            extensions: vec![],
        }
    }

    fn config_for(manifest: OpsManifest) -> OpsConfig {
        OpsConfig::from_manifest(manifest, std::path::PathBuf::from("/tmp/repo")).unwrap()
    }

    #[test]
    fn pick_binary_prefers_requested_profile() {
        let dir = tempfile::tempdir().unwrap();
        let rel = dir.path().join("target/release");
        let dbg = dir.path().join("target/debug");
        std::fs::create_dir_all(&rel).unwrap();
        std::fs::create_dir_all(&dbg).unwrap();
        std::fs::write(rel.join("demo-app"), "x").unwrap();
        std::fs::write(dbg.join("demo-app"), "x").unwrap();
        assert_eq!(
            pick_binary(dir.path(), "demo-app", true),
            Some(rel.join("demo-app"))
        );
        assert_eq!(
            pick_binary(dir.path(), "demo-app", false),
            Some(dbg.join("demo-app"))
        );
    }

    #[test]
    fn pick_binary_falls_back_to_other_profile() {
        let dir = tempfile::tempdir().unwrap();
        let rel = dir.path().join("target/release");
        std::fs::create_dir_all(&rel).unwrap();
        std::fs::write(rel.join("demo-app"), "x").unwrap();
        assert_eq!(
            pick_binary(dir.path(), "demo-app", false),
            Some(rel.join("demo-app"))
        );
    }

    #[test]
    fn pick_binary_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(pick_binary(dir.path(), "demo-app", true), None);
    }

    #[test]
    fn bind_addr_with_port_appends_or_keeps() {
        let mut manifest = manifest_with(None);
        manifest.servers.bind_addr = "0.0.0.0".into();
        let cfg = config_for(manifest.clone());
        assert_eq!(bind_addr_with_port(&cfg), "0.0.0.0:3000");
        manifest.servers.bind_addr = "127.0.0.1:4444".into();
        let cfg = config_for(manifest);
        assert_eq!(bind_addr_with_port(&cfg), "127.0.0.1:4444");
    }

    #[test]
    fn supabase_health_url_uses_manifest_or_default() {
        let cfg = config_for(manifest_with(None));
        assert_eq!(
            supabase_health_url(&cfg),
            "http://localhost:54321/auth/v1/health"
        );
        let mut manifest = manifest_with(None);
        manifest.supabase = Some(codegraph_config::OpsSupabase {
            dir: "supabase".into(),
            health_url: Some("http://localhost:54321/auth/v1/health".into()),
            anon_key: None,
            service_key: None,
            jwt_secret: None,
        });
        let cfg = config_for(manifest);
        assert_eq!(
            supabase_health_url(&cfg),
            "http://localhost:54321/auth/v1/health"
        );
    }

    #[test]
    fn find_chromium_picks_first_existing() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("chromium");
        std::fs::write(&fake, "x").unwrap();
        assert_eq!(
            find_chromium(&[Path::new("/nonexistent/chromium"), &fake]),
            Some(fake)
        );
        assert_eq!(find_chromium(&[Path::new("/nonexistent/chromium")]), None);
    }

    #[test]
    fn generate_args_include_only_some_flags() {
        let cfg = config_for(manifest_with(Some("hr-graph")));
        let args = generate_args(&cfg);
        assert_eq!(args[0], "run");
        assert!(args.contains(&"--schemas".to_string()));
        assert!(args.contains(&"--classifier".to_string()));
        assert!(args.contains(&"--profile".to_string()));
        assert!(!args.contains(&"--config".to_string()));
        assert!(args.contains(&"--output".to_string()));
        assert!(args.contains(&"/tmp/repo/generated-app".to_string()));
    }

    #[test]
    fn generation_outputs_detects_non_empty_dirs() {
        let cfg = config_for(manifest_with(None));
        assert!(!generation_outputs(&cfg));
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("generated-app/src");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("main.rs"), "fn main() {}").unwrap();
        let cfg = OpsConfig::from_manifest(manifest_with(None), dir.path().to_path_buf()).unwrap();
        assert!(generation_outputs(&cfg));
    }
}
