//! API integration suite — mirrors the bash `cmd_api` (11 stages) generically.
//!
//! Entity-specific checks are driven by `manifest.smoke`; hurl contract tests
//! by `manifest.hurl`; DB access via `config.api_db`. Everything else is
//! derived from the generated app (binary name, migrations dir, ports).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::OpsConfig;
use crate::db::{psql_exec, psql_exec_file_ok, psql_query};
use crate::error::{OpsError, OpsResult};
use crate::ext::{HookPolicy, run_hooks};
use crate::migrate::run_api_migrations_with_options;
use crate::output;
use crate::proc::{ManagedProcess, Supervisor};
use crate::results::SuiteFailure;
use crate::wait::wait_for_url;

mod capture;
mod regenerate;
mod report;

use capture::{extract_json_field, parse_json, run_capture, run_capture_env, strip_ansi};
use regenerate::{
    assert_generation_clean, cargo_check_in, cornucopia_db_env, is_release_binary, regenerate,
};
use report::{count_files_with_suffix, find_file, print_log_tail, write_api_report};

#[cfg(test)]
use capture::split_status_body;
pub(crate) use capture::{
    http_get_body, http_post_body, http_status, hurl_error_excerpt, hurl_log_path,
    hurl_suite_passed, parse_requests, pluralize_entity_route, write_hurl_log,
};
pub(super) use regenerate::cargo_build_in;
#[cfg(test)]
use regenerate::regenerate_args;

#[derive(Debug, Clone)]
pub struct ApiArgs {
    pub keep: bool,
    pub skip_build: bool,
    pub skip_generate: bool,
    pub migrate: bool,
    pub rebuild: bool,
    pub regen: bool,
    pub release: bool,
    /// Retry failed hurl files up to this many times (0 = no retries).
    pub retry: u32,
    /// Write a machine-readable `--results` JSON report to this path.
    pub results_file: Option<String>,
    /// Tolerate generation errors (skipped entities) instead of failing.
    pub allow_gen_errors: bool,
    /// Warn instead of failing when the generator rev in the app's
    /// `.codegraph-manifest.json` differs from this testkit's pinned rev.
    pub allow_gen_rev_mismatch: bool,
    /// Reuse a registry-known server already running on the api port instead
    /// of taking it over (the old server keeps serving — possibly stale).
    pub reuse: bool,
}

/// True when a failed hurl file may be retried: the number of attempts used
/// so far (`attempts_used`, 1 = first attempt) is still below the allowed
/// total (`max_retries + 1`).
pub fn should_retry(attempts_used: u32, max_retries: u32) -> bool {
    attempts_used < max_retries.saturating_add(1)
}

/// Pass/fail counters with failure diagnostics.
#[derive(Debug, Default)]
pub struct TestCounters {
    pub passes: usize,
    pub failures: usize,
    /// Every failed check, in order — surfaced in the summary and the
    /// `--results` JSON so consumers see *what* failed without re-parsing
    /// styled output.
    pub failure_log: Vec<SuiteFailure>,
}

impl TestCounters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pass(&mut self, msg: impl AsRef<str>) {
        output::ok(format!("  PASS {}", msg.as_ref()));
        self.passes += 1;
    }

    pub fn fail_test(&mut self, msg: impl AsRef<str>) {
        let msg = msg.as_ref();
        output::fail(format!("  FAIL {msg}"));
        self.failures += 1;
        self.failure_log.push(SuiteFailure {
            context: String::new(),
            title: msg.to_string(),
        });
        // Failure context is attached ALWAYS (not only under --verbose): a
        // failed check explains itself, removing a --verbose rerun.
        let log = "/tmp/codegraph-ops-app.log";
        if let Ok(content) = std::fs::read_to_string(log) {
            let tail: Vec<&str> = content.lines().rev().take(5).collect();
            if tail.iter().any(|l| !l.trim().is_empty()) {
                output::warn("--- server log tail ---");
                for line in tail.iter().rev() {
                    println!("    {line}");
                }
                output::warn("--- end ---");
            }
        }
    }

    /// Print summary line; returns true if no failures.
    pub fn summary(&self) -> bool {
        let total = self.passes + self.failures;
        if self.failures == 0 {
            println!(
                "\n{}{}ALL {} API TESTS PASSED{}",
                output::bold(""),
                output::GREEN_DEF,
                total,
                output::NC_DEF
            );
            true
        } else {
            println!(
                "\n{}{}{} of {} API TESTS FAILED{}",
                output::bold(""),
                output::RED_DEF,
                self.failures,
                total,
                output::NC_DEF
            );
            false
        }
    }
}

/// Run the API integration suite. Returns Err(TestFailure) if any check failed.
///
/// Hook fatality (#357): every hook point routes through the hook's manifest
/// `fatal` flag (missing = fatal). `post_api` is now REPORTED — previously
/// its result was silently dropped (`let _ =`); a non-fatal failure warns
/// and is recorded in the results JSON, a fatal failure aborts the suite.
pub async fn run_api(config: &OpsConfig, args: &ApiArgs) -> OpsResult<()> {
    let mut hook_failures: Vec<String> = Vec::new();
    hook_failures.extend(run_hooks(config, "pre_api", HookPolicy::PerHook).await?);
    // The api suite targets the same Supabase-provisioned database as e2e
    // ([database.api] == [database.e2e]): the stack must be up before any
    // DB-touching stage. pre_db hooks (e.g. the pgmq after-create patch)
    // need the container running, so they fire right after it is up.
    output::section("Supabase stack");
    config.metrics.begin("Supabase");
    let ensured = match config.supabase_dir.as_ref() {
        Some(dir) => crate::suites::e2e::supabase_ensure_up(config, dir).await,
        None => Err(OpsError::TestFailure(
            "[supabase] dir missing in manifest — the api suite needs the stack".into(),
        )),
    };
    config.metrics.end();
    ensured?;
    hook_failures.extend(run_hooks(config, "pre_db", HookPolicy::PerHook).await?);
    let (counters, ok) = match run_api_inner(config, args, &mut hook_failures).await {
        Ok(pair) => pair,
        Err(e) => {
            // Early suite failure: post_api still fires (as before), but its
            // failures never mask the inner error. No summary report was
            // written — the CLI-level early-failure writer covers this path.
            let _ = run_hooks(config, "post_api", HookPolicy::PerHook).await;
            return Err(e);
        }
    };
    let post_api_fatal: Option<OpsError> =
        match run_hooks(config, "post_api", HookPolicy::PerHook).await {
            Ok(failed) => {
                hook_failures.extend(failed);
                None
            }
            Err(e) => Some(e),
        };
    write_api_report(config, args, &counters, ok, &hook_failures);
    if !ok {
        // The inner failure wins over a fatal post_api failure.
        return Err(OpsError::TestFailure(format!(
            "{} of {} API tests failed",
            counters.failures,
            counters.passes + counters.failures
        )));
    }
    if let Some(e) = post_api_fatal {
        return Err(e);
    }
    Ok(())
}

async fn run_api_inner(
    config: &OpsConfig,
    args: &ApiArgs,
    hook_failures: &mut Vec<String>,
) -> OpsResult<(TestCounters, bool)> {
    let mut counters = TestCounters::new();

    // ---- 0. Fast doctor (#358) ----
    // Cheap tool/filesystem/port checks fail in seconds with hints instead
    // of after the generate+build stage below (a missing hurl used to
    // surface only here, minutes into a full rebuild).
    crate::doctor::run_fast_doctor(config, crate::doctor::FastDoctorSuite::Api).await?;

    // ---- 0. Generate + build ----
    // By default the suite regenerates from the manifest's profile and
    // rebuilds the app, so `testkit api` alone is generate → build → test
    // (provider parity: the cornucopia manifest gets its profile passed to
    // the graph binary and CORNUCOPIA_DATABASE_URL exported for the build).
    // --skip-build skips both; --skip-generate skips only generation.
    stage_generate_build(config, args, hook_failures).await?;

    // ---- 1. Preflight ----
    // Returns whether the api port is served by a reused registry server
    // (`--reuse`) — stage_server then skips booting its own instance.
    let server_reused = stage_preflight(config, args, &mut counters).await?;

    // ---- 2. Database ----
    let (migration_dir, auth_header, api_key_b, api_key_limited) =
        stage_database(config, args, &mut counters, hook_failures).await?;

    // ---- 3. Server ----
    let binary = app_binary_path(config, args);
    let mut supervisor = stage_server(config, args, &binary, server_reused, &mut counters).await?;

    // ---- 4. Hurl API tests ----
    stage_hurl(
        config,
        args,
        auth_header.as_ref(),
        api_key_b.as_ref(),
        api_key_limited.as_ref(),
        &mut counters,
    )
    .await?;

    // ---- 5. Curl smoke tests ----
    stage_curl_smoke(config, auth_header.as_ref(), &mut counters).await;

    // ---- 6. DB inspection ----
    stage_db_inspection(config, &migration_dir, &mut counters).await;

    // ---- 7. Health endpoint ----
    stage_health(config, &mut counters).await;

    // ---- 8. Cross-tenant RLS isolation ----
    stage_rls_isolation(
        config,
        auth_header.as_ref(),
        api_key_b.as_ref(),
        &mut counters,
    )
    .await;

    // ---- 9. Server log check ----
    stage_server_log(config, &mut counters).await;

    // ---- 10. Graceful shutdown verification ----
    stage_graceful_shutdown(config, &mut supervisor, &mut counters).await;

    // ---- 11. Regeneration (optional) ----
    stage_regeneration(config, args, &mut counters, hook_failures).await?;

    // ---- Summary ----
    let ok = counters.summary();
    supervisor.shutdown_all().await;
    Ok((counters, ok))
}

async fn stage_generate_build(
    config: &OpsConfig,
    args: &ApiArgs,
    hook_failures: &mut Vec<String>,
) -> OpsResult<()> {
    if !args.skip_build {
        output::section("0. Generate + build");
        config.metrics.begin("Generate + build");
        if !args.skip_generate {
            match (&config.manifest.graph_binary, &config.manifest.schemas_dir) {
                (Some(graph), Some(_)) => {
                    hook_failures
                        .extend(run_hooks(config, "pre_generate", HookPolicy::PerHook).await?);
                    let gen_output = regenerate(config, graph, args.release)
                        .inspect_err(|e| output::fail(e.to_string()))?;
                    if !args.allow_gen_errors {
                        assert_generation_clean(&gen_output)?;
                    }
                    hook_failures
                        .extend(run_hooks(config, "post_generate", HookPolicy::PerHook).await?);
                    // #357: the freshly written manifest names the generator
                    // rev — a stale graph binary is a hard error here, not a
                    // drift discovered after ~10 minutes of suite work.
                    crate::freshness::check_generator_rev(
                        &config.app_dir,
                        args.allow_gen_rev_mismatch,
                    )?;
                    output::ok("Templates regenerated");
                }
                (Some(_), None) => output::warn("schemas_dir not configured — skipping generation"),
                (None, _) => output::warn("no graph_binary configured — skipping generation"),
            }
        } else {
            output::warn("generation skipped (--skip-generate)");
        }
        cargo_build_in(config, args.release).map_err(|e| {
            output::fail(&e);
            OpsError::TestFailure(format!("app build failed: {e}"))
        })?;
        output::ok("App built");
        config.metrics.end();
    }
    Ok(())
}

async fn stage_preflight(
    config: &OpsConfig,
    args: &ApiArgs,
    counters: &mut TestCounters,
) -> OpsResult<bool> {
    output::section("1. Preflight");
    config.metrics.begin("Preflight");

    if psql_query(&config.api_db, "SELECT 1").await.is_ok() {
        counters.pass("Postgres running");
    } else {
        counters.fail_test("Postgres not reachable");
        output::fail(format!(
            "API tests need Postgres at {}:{}. Start it first (docker compose / supabase).",
            config.api_db.host, config.api_db.port
        ));
        return Err(OpsError::TestFailure(
            "preflight failed: Postgres not reachable".into(),
        ));
    }

    let has_hurl = Command::new("hurl")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if has_hurl {
        counters.pass("hurl installed");
    } else {
        counters.fail_test("hurl not found");
        return Err(OpsError::MissingTool(
            "hurl",
            "install hurl or set manifest.hurl = none",
        ));
    }

    if config.manifest.smoke.is_some() {
        let has_python = Command::new("python3")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if has_python {
            counters.pass("python3 installed");
        } else {
            counters.fail_test("python3 not found");
            return Err(OpsError::MissingTool(
                "python3",
                "install python3 for smoke JSON checks",
            ));
        }
    } else {
        output::warn("no smoke entity configured — skipping python checks");
    }

    // Port preflight: the api server binds `{api_port}` later in the suite;
    // an unrelated process holding it used to fail the run ten minutes in
    // with the real error buried in the app log. Fail in seconds instead.
    // Registry-known prior servers (`--keep` leaks) are taken over by
    // default, or reused with `--reuse`.
    let server_reused = match crate::preflight::ensure_port_available(
        &config.root_dir,
        config.manifest.servers.api_port,
        args.reuse,
    ) {
        Ok(outcome) => {
            let reused = outcome.reused();
            match &outcome {
                crate::preflight::PortOutcome::Free => {
                    counters.pass(format!("Port {} free", config.manifest.servers.api_port));
                }
                crate::preflight::PortOutcome::Reused { name } => {
                    counters.pass(format!(
                        "Port {} served by reused registry service {name} (--reuse)",
                        config.manifest.servers.api_port
                    ));
                }
                crate::preflight::PortOutcome::TookOver { name, pid } => {
                    counters.pass(format!(
                        "Port {} freed (took over {name}, pid {pid})",
                        config.manifest.servers.api_port
                    ));
                }
            }
            reused
        }
        Err(e) => {
            counters.fail_test(e.to_string());
            return Err(e);
        }
    };

    // Output-tree completeness: a wiped/partial generated dir (interrupted
    // regen) used to surface only as baffling auth/migration failures deep
    // in the suite while a stale binary booted happily. Fail in seconds.
    if let Err(e) = crate::preflight::ensure_output_tree(&config.app_dir) {
        counters.fail_test(e.to_string());
        return Err(e);
    }
    counters.pass("Generated output tree complete (src/, migrations/)");

    // Binary smoke tests (only when the admin CLI exists in the scaffold).
    let binary = app_binary_path(config, args);
    // Stale-binary guard: a binary older than the newest source file means
    // the suite would silently test an app that doesn't match the current
    // generator output. Fail fast with an actionable hint.
    if let Err(e) = crate::preflight::ensure_binary_fresh(&config.app_dir, &binary) {
        counters.fail_test(e.to_string());
        return Err(e);
    }
    counters.pass(format!("Binary built ({})", binary.display()));

    if config.manifest.capabilities.has_admin_cli {
        // version
        let version_out = run_capture(&binary, &["version"], config.root_dir.as_path());
        if version_out.output_contains("Git commit") {
            counters.pass("app version");
        } else {
            counters.fail_test("app version failed");
        }
        // bare invocation
        let help_out = run_capture(&binary, &[], config.root_dir.as_path());
        if ["start", "migrate", "doctor", "init", "version"]
            .iter()
            .any(|w| help_out.output_contains(w))
        {
            counters.pass("app help lists subcommands");
        } else {
            counters.fail_test("app help missing subcommands");
        }
        // start --help
        if run_capture(&binary, &["start", "--help"], config.root_dir.as_path())
            .output_contains("bind-addr")
        {
            counters.pass("start --help");
        } else {
            counters.fail_test("start --help failed");
        }
        // migrate --help
        if run_capture(&binary, &["migrate", "--help"], config.root_dir.as_path())
            .output_contains("database-url")
        {
            counters.pass("migrate --help");
        } else {
            counters.fail_test("migrate --help failed");
        }
        // init
        let init_out = config.root_dir.join("ops-init-test.toml");
        let _ = std::fs::remove_file(&init_out);
        let _ = run_capture(
            &binary,
            &["init", "--output", init_out.to_str().unwrap_or("")],
            config.root_dir.as_path(),
        );
        if init_out.is_file()
            && std::fs::read_to_string(&init_out)
                .map(|c| c.contains("bind_addr"))
                .unwrap_or(false)
        {
            counters.pass("init creates config");
            let _ = std::fs::remove_file(&init_out);
        } else {
            counters.fail_test("init: no config or missing bind_addr");
            let _ = std::fs::remove_file(&init_out);
        }
        // doctor
        let doctor = run_capture_env(
            &binary,
            &["doctor"],
            config.root_dir.as_path(),
            &[("DATABASE_URL", config.api_db.url().as_str())],
        );
        if ["PASS", "FAIL", "WARN"]
            .iter()
            .any(|w| doctor.output_contains(w))
        {
            counters.pass("doctor runs checks");
        } else {
            output::warn(format!(
                "doctor output unexpected: {}",
                doctor.stdout.trim()
            ));
        }
        // stop/status --help
        if run_capture(&binary, &["stop", "--help"], config.root_dir.as_path())
            .output_contains("pid-file")
            || run_capture(&binary, &["stop", "--help"], config.root_dir.as_path())
                .output_contains("bind-addr")
        {
            counters.pass("stop --help");
        } else {
            counters.fail_test("stop --help failed");
        }
        if run_capture(&binary, &["status", "--help"], config.root_dir.as_path())
            .output_contains("pid-file")
            || run_capture(&binary, &["status", "--help"], config.root_dir.as_path())
                .output_contains("bind-addr")
        {
            counters.pass("status --help");
        } else {
            counters.fail_test("status --help failed");
        }
        // start + status + stop integration
        let probe_port = 30099;
        let pid_file = config
            .root_dir
            .join(format!("{}-{probe_port}.pid", config.app_binary_name()));
        let _ = std::fs::remove_file(&pid_file);
        let mut start_cmd = Command::new(&binary);
        start_cmd
            .arg("start")
            .arg("--bind-addr")
            .arg(format!("127.0.0.1:{probe_port}"))
            // The app writes its pid file into its cwd; without this the pid
            // file lands in the harness's cwd while the probe polls root_dir.
            .current_dir(&config.root_dir)
            .env("DATABASE_URL", config.api_db.url())
            .env("SUPABASE_JWT_SECRET", config.jwt_secret.clone());
        if std::env::var_os("APP_DATABASE_URL").is_none() {
            start_cmd.env("APP_DATABASE_URL", app_pool_url(&config.api_db));
        }
        if let Some((key, value)) = cornucopia_db_env(config) {
            start_cmd.env(key, value);
        }
        let mut probe = ManagedProcess::spawn(start_cmd, "app-probe", &config.log_file)?;
        let mut waited = 0;
        while waited < 10 && !pid_file.is_file() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            waited += 1;
        }
        if pid_file.is_file() {
            counters.pass("start creates PID file");
        } else {
            counters.fail_test("start: no PID file created");
        }
        let status_out = run_capture(
            &binary,
            &["status", "--bind-addr", &format!("127.0.0.1:{probe_port}")],
            config.root_dir.as_path(),
        );
        if status_out.output_contains("running") {
            counters.pass("status detects running server");
        } else {
            counters.fail_test("status output unexpected");
        }
        let _ = run_capture(
            &binary,
            &[
                "stop",
                "--bind-addr",
                &format!("127.0.0.1:{probe_port}"),
                "--timeout",
                "10",
            ],
            config.root_dir.as_path(),
        );
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if !probe.alive() {
            counters.pass("stop kills server");
        } else {
            let _ = probe.graceful_shutdown(3).await;
            counters.fail_test("stop did not kill server");
        }
        let _ = std::fs::remove_file(&pid_file);
    } else {
        output::warn("no admin CLI capability — skipping binary smoke tests");
    }
    config.metrics.end();

    Ok(server_reused)
}

async fn stage_database(
    config: &OpsConfig,
    args: &ApiArgs,
    counters: &mut TestCounters,
    hook_failures: &mut Vec<String>,
) -> OpsResult<(PathBuf, Option<String>, Option<String>, Option<String>)> {
    // ---- 2. Database ----
    output::section("2. Database");
    config.metrics.begin("DB migrate");

    let migration_dir = config.app_dir.join("migrations");
    if args.migrate {
        if let Some(reset) = &config.manifest.database.api.reset_sql {
            let reset_path = config.root_dir.join(reset);
            let _ = psql_exec_file_ok(&config.api_db, &reset_path).await;
        }
        if migration_dir.is_dir() {
            run_api_migrations_with_options(&migration_dir, &config.api_db, &config.grant_options)
                .await?;
        } else {
            output::warn(format!(
                "no migrations dir at {} — skipping migration",
                migration_dir.display()
            ));
        }
        if let Some(seed) = &config.manifest.database.api.seed_sql {
            let seed_path = config.root_dir.join(seed);
            let _ = psql_exec_file_ok(&config.api_db, &seed_path).await;
        }
        // Consumer-provided post-migration steps (e.g. hr-reports views).
        // Fatality follows the hook's manifest `fatal` flag (missing =
        // fatal) — #357 replaced the blanket warn-only behavior; consumers
        // with warn-only expectations (e.g. hr-specs' api post_migrate) set
        // `fatal = false` explicitly.
        hook_failures
            .extend(crate::ext::run_hooks(config, "post_migrate", HookPolicy::PerHook).await?);
    } else {
        output::info("--no-migrate: skipping reset + migration");
    }

    let table_count = psql_query(
        &config.api_db,
        "SELECT count(*) FROM information_schema.tables \
         WHERE table_schema NOT IN ('pg_catalog','information_schema','public','auth','storage','graphql','extensions');",
    )
    .await
    .unwrap_or_default();
    let count: i64 = table_count.trim().parse().unwrap_or(0);
    if count > 0 {
        counters.pass(format!("{count} domain tables exist"));
    } else {
        counters.fail_test("no domain tables found after migration");
    }

    // App pool (#169): pin the app_user password so the server can connect as
    // the NOBYPASSRLS app role (APP_DATABASE_URL).
    provision_app_pool(config, counters).await;

    // API keys (only if public.create_api_key exists in the scaffold).
    let has_create_key = psql_query(
        &config.api_db,
        "SELECT count(*) FROM pg_proc WHERE proname = 'create_api_key';",
    )
    .await
    .unwrap_or_default();
    let mut auth_header: Option<String> = None;
    let mut api_key_b: Option<String> = None;
    let mut api_key_limited: Option<String> = None;
    if has_create_key.trim() == "0" || has_create_key.is_empty() {
        output::warn("create_api_key() not found — API-key auth checks skipped");
    } else {
        let org_a = config
            .manifest
            .hurl
            .as_ref()
            .and_then(|h| h.org_id_a.clone())
            .unwrap_or_else(|| "00000000-0000-0000-0000-000000000001".to_string());
        if let Ok(key) = provision_api_key(config, &org_a, "ops-test-key").await {
            counters.pass(format!(
                "API key provisioned (prefix: {})",
                &key[..key.len().min(7)]
            ));
            auth_header = Some(format!("Authorization: Bearer {key}"));
        } else {
            counters.fail_test("could not extract API key");
        }
        if let Some(org_b) = config
            .manifest
            .hurl
            .as_ref()
            .and_then(|h| h.org_id_b.clone())
            && let Ok(key) = provision_api_key(config, &org_b, "ops-test-key-b").await
        {
            counters.pass("Org B API key provisioned");
            api_key_b = Some(key);
        }
        // Limited (read-only) key for scope-denial contract files (#169):
        // opted in via `hurl.limited_key = true`, exposed to hurl as
        // `api_key_limited`.
        if config
            .manifest
            .hurl
            .as_ref()
            .map(|h| h.limited_key)
            .unwrap_or(false)
        {
            if let Ok(key) =
                provision_read_only_api_key(config, &org_a, "ops-test-key-limited").await
            {
                counters.pass("Read-only (limited) API key provisioned");
                api_key_limited = Some(key);
            } else {
                counters.fail_test("could not provision read-only API key");
            }
        }
    }
    config.metrics.end();

    Ok((migration_dir, auth_header, api_key_b, api_key_limited))
}

async fn stage_server(
    config: &OpsConfig,
    args: &ApiArgs,
    binary: &Path,
    server_reused: bool,
    counters: &mut TestCounters,
) -> OpsResult<Supervisor> {
    // ---- 3. Server ----
    output::section("3. Server");
    config.metrics.begin("Start Axum");

    let mut supervisor = Supervisor::new(args.keep);
    if server_reused {
        // --reuse: the registry-known instance keeps serving (documented
        // caveat: it may be a stale build). All HTTP checks below run
        // against it as usual; the graceful-shutdown stage finds no managed
        // process and degrades to a warning.
        output::warn(
            "--reuse: not booting the app server — the running registry service keeps serving",
        );
        counters.pass("Server reused (--reuse)");
        config.metrics.end();
        return Ok(supervisor);
    }
    let bind = format!(
        "{}:{}",
        config.manifest.servers.bind_addr, config.manifest.servers.api_port
    );
    let mut server_cmd = Command::new(binary);
    server_cmd
        .arg("start")
        .arg("--bind-addr")
        .arg(&bind)
        .arg("--database-url")
        .arg(config.api_db.url())
        .env("DATABASE_URL", config.api_db.url())
        .env("SUPABASE_JWT_SECRET", config.jwt_secret.clone());
    // App pool (#169): export the app_user URL unless the caller manages it.
    if std::env::var_os("APP_DATABASE_URL").is_none() {
        server_cmd.env("APP_DATABASE_URL", app_pool_url(&config.api_db));
    }
    if let Some((key, value)) = cornucopia_db_env(config) {
        server_cmd.env(key, value);
    }
    // Pin the app's cwd to the manifest root: generated apps resolve their
    // integration config relative to the cwd (`config/default`), and the old
    // bash suite always ran from the repo root. Inheriting the caller's cwd
    // made the suite only work when invoked from the right directory.
    server_cmd.current_dir(&config.root_dir);
    let mut api_proc = ManagedProcess::spawn(server_cmd, "Axum (API)", &config.log_file)?;
    api_proc.set_registration(crate::proc::ServiceRegistration::new(
        config.root_dir.clone(),
        "api",
        config.manifest.servers.api_port,
        Some("/health"),
        "api",
        Some(if args.release {
            "release".to_string()
        } else {
            "debug".to_string()
        }),
    ));

    if let Err(e) = wait_for_url(&format!("{}/swagger-ui/", config.api_url()), 30, "Axum").await {
        print_log_tail(&config.log_file, 20);
        return Err(e);
    }
    // Health OK → record the service so `clean`/the next preflight can find
    // it even after a `--keep` leak. The entry is removed again on graceful
    // shutdown; a `--keep` leak deliberately leaves it behind.
    api_proc.record_service();
    supervisor.add(api_proc);
    counters.pass("Server started");
    if let Ok(200) = http_status(&format!("{}/swagger-ui/", config.api_url()), &[]).await {
        counters.pass("Swagger UI reachable");
    } else {
        counters.fail_test("Swagger UI: not 200");
    }
    if let Ok(200) = http_status(&format!("{}/api-docs/openapi.json", config.api_url()), &[]).await
    {
        counters.pass("OpenAPI JSON reachable");
    } else {
        counters.fail_test("OpenAPI JSON: not 200");
    }
    config.metrics.end();

    Ok(supervisor)
}

async fn stage_hurl(
    config: &OpsConfig,
    args: &ApiArgs,
    auth_header: Option<&String>,
    api_key_b: Option<&String>,
    api_key_limited: Option<&String>,
    counters: &mut TestCounters,
) -> OpsResult<()> {
    // ---- 4. Hurl API tests ----
    output::section("4. Hurl API tests");
    config.metrics.begin("Hurl API tests");

    let mut total_requests = 0usize;
    if let Some(hurl) = &config.manifest.hurl {
        let hurl_dir = config.root_dir.join(&hurl.dir);
        if hurl_dir.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(&hurl_dir)
                .map_err(OpsError::Io)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "hurl").unwrap_or(false))
                .map(|e| e.path())
                .collect();
            files.sort();
            // A configured hurl dir with nothing runnable is almost always a
            // mistake (the generator's contracts were swept, or skip swallows
            // everything) — never silently run zero API contracts.
            let runnable: Vec<_> = files
                .iter()
                .filter(|f| {
                    f.file_name()
                        .map(|n| !hurl.skip.contains(&n.to_string_lossy().into_owned()))
                        .unwrap_or(false)
                })
                .collect();
            if runnable.is_empty() {
                output::warn(format!(
                    "hurl dir {} yields zero runnable .hurl files — no API contracts ran",
                    hurl_dir.display()
                ));
            }
            for f in files {
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if hurl.skip.contains(&name) {
                    continue;
                }
                let mut attempts_used = 0u32;
                loop {
                    attempts_used += 1;
                    if attempts_used > 1 {
                        output::info(format!(
                            "retry {}/{} for {name}",
                            attempts_used - 1,
                            args.retry
                        ));
                    }
                    let mut cmd = Command::new("hurl");
                    cmd.arg("--test")
                        .arg("--variable")
                        .arg(format!("base_url={}", config.api_url()));
                    if let Some(h) = &auth_header {
                        let key = h.trim_start_matches("Authorization: Bearer ").to_string();
                        cmd.arg("--variable").arg(format!("api_key={key}"));
                        // Alias matching the cross-tenant file vocabulary
                        // (`api_key_a`), so the same hurl file works in the
                        // main loop and in the RLS-isolation stage.
                        cmd.arg("--variable").arg(format!("api_key_a={key}"));
                    }
                    if let Some(key) = api_key_b {
                        cmd.arg("--variable").arg(format!("api_key_b={key}"));
                    }
                    if let Some(key) = api_key_limited {
                        cmd.arg("--variable").arg(format!("api_key_limited={key}"));
                    }
                    cmd.arg(&f);
                    let (passed, reqs, output_text) = match cmd.output() {
                        Ok(out) => {
                            let text = format!(
                                "{}{}",
                                String::from_utf8_lossy(&out.stdout),
                                String::from_utf8_lossy(&out.stderr)
                            );
                            // Save the full combined output for EVERY hurl
                            // file (pass or fail): debugging failures needs
                            // the assert context, not just the error lines.
                            let log_path = hurl_log_path(config, &name);
                            if let Err(e) = write_hurl_log(&log_path, &text) {
                                output::warn(format!(
                                    "could not write hurl log {}: {e}",
                                    log_path.display()
                                ));
                            }
                            if hurl_suite_passed(&text) {
                                (true, parse_requests(&text), String::new())
                            } else {
                                (false, 0, text)
                            }
                        }
                        Err(e) => (false, 0, e.to_string()),
                    };
                    if passed {
                        total_requests += reqs;
                        counters.pass(format!("{name} ({reqs} request(s))"));
                        break;
                    }
                    if !should_retry(attempts_used, args.retry) {
                        counters.fail_test(format!(
                            "{name} — full output: {}",
                            hurl_log_path(config, &name).display()
                        ));
                        for line in hurl_error_excerpt(&output_text) {
                            println!("    {line}");
                        }
                        break;
                    }
                }
            }
        } else {
            output::warn(format!(
                "hurl dir {} missing — skipping",
                hurl_dir.display()
            ));
        }
    } else {
        output::info("no hurl config — skipping hurl tests");
    }
    output::info(format!("Total API requests: {total_requests}"));
    config.metrics.end();

    Ok(())
}

async fn stage_curl_smoke(
    config: &OpsConfig,
    auth_header: Option<&String>,
    counters: &mut TestCounters,
) {
    // ---- 5. Curl smoke tests ----
    output::section("5. Curl smoke tests");
    config.metrics.begin("Curl smoke tests");

    if let Some(smoke) = &config.manifest.smoke {
        let entity = &smoke.entity;
        // Generated routers nest under the plural path segment resolved from
        // the domain config / graph (e.g. `/api/v1/recruiting/candidates`),
        // not the singular entity slug. The manifest carries the resolved
        // route when known; otherwise pluralize the entity segment with the
        // same simple rules the codegen templates use.
        let route = smoke
            .route
            .clone()
            .unwrap_or_else(|| pluralize_entity_route(entity));
        let api_base = format!(
            "{}/api/{}/{}",
            config.api_url(),
            config.manifest.api_version,
            route
        );
        let headers: Vec<(&str, &str)> = vec![("Content-Type", "application/json")];
        let mut headers_all: Vec<(&str, &str)> = headers.clone();
        let mut auth_headers: Vec<(&str, &str)> = Vec::new();
        if let Some(h) = &auth_header {
            let (k, v) = h.split_once(':').unwrap_or(("Authorization", ""));
            headers_all.push((k, v.trim_start()));
            auth_headers.push((k, v.trim_start()));
        }
        // POST create
        let resp = http_post_body(&api_base, &smoke.create_body, &headers_all).await;
        let (status, body) = match resp {
            Ok((s, b)) => (s, b),
            Err(e) => {
                counters.fail_test(format!("POST /{entity}: {e}"));
                ("000".to_string(), String::new())
            }
        };
        let mut smoke_id = String::new();
        if status == "201" {
            counters.pass(format!("POST /{entity} (minimal) -> 201"));
            if body.contains("\"data\"") {
                counters.pass("  response has 'data' envelope");
            } else {
                counters.fail_test("  response missing 'data'");
            }
            if body.contains("\"meta\"") {
                counters.pass("  response has 'meta' envelope");
            } else {
                counters.fail_test("  response missing 'meta'");
            }
            smoke_id = extract_json_field(&body, "data.id");
            if smoke_id.is_empty() {
                counters.fail_test("  could not extract data.id");
            }
        } else {
            counters.fail_test(format!("POST /{entity} (minimal) -> {status}"));
        }
        // GET by id
        if !smoke_id.is_empty() {
            match http_status(&format!("{api_base}/{smoke_id}"), &auth_headers).await {
                Ok(200) => counters.pass("GET /{entity}/{{id}} -> 200"),
                Ok(s) => counters.fail_test(format!("GET /{entity}/{{id}} -> {s}")),
                Err(e) => counters.fail_test(format!("GET /{entity}/{{id}}: {e}")),
            }
        }
        // GET zero-uuid -> 404
        match http_status(
            &format!("{api_base}/00000000-0000-0000-0000-000000000000"),
            &auth_headers,
        )
        .await
        {
            Ok(404) => counters.pass("GET /{entity}/zero-uuid -> 404"),
            Ok(s) => counters.fail_test(format!("GET /{entity}/zero-uuid -> {s}")),
            Err(e) => counters.fail_test(format!("GET /{entity}/zero-uuid: {e}")),
        }
        // GET list
        match http_get_body(&format!("{api_base}?page=0&page_size=10"), &auth_headers).await {
            Ok((status, body)) if status == "200" => {
                counters.pass("GET /{entity} (list) -> 200");
                let is_array = parse_json(&body)
                    .and_then(|j| j.get("data").cloned())
                    .map(|v| v.is_array())
                    .unwrap_or(false);
                if is_array {
                    counters.pass("  list 'data' is an array");
                } else {
                    counters.fail_test("  list 'data' is not an array");
                }
            }
            Ok((s, _)) => counters.fail_test(format!("GET /{entity} (list) -> {s}")),
            Err(e) => counters.fail_test(format!("GET /{entity} (list): {e}")),
        }
    } else {
        output::info("no smoke entity configured — skipping curl smoke");
    }
    config.metrics.end();
}

async fn stage_db_inspection(
    config: &OpsConfig,
    migration_dir: &Path,
    counters: &mut TestCounters,
) {
    // ---- 6. DB inspection ----
    output::section("6. DB inspection");
    config.metrics.begin("DB inspection");

    let poi = psql_query(
        &config.api_db,
        "SELECT count(*) FROM information_schema.columns WHERE column_name = 'platform_organization_id';",
    )
    .await
    .unwrap_or_default();
    let poi_count: i64 = poi.trim().parse().unwrap_or(0);
    if poi_count > 0 {
        counters.pass(format!(
            "{poi_count} columns named platform_organization_id"
        ));
    } else {
        output::info("no platform_organization_id columns (tenant isolation may be disabled)");
    }

    let rls = psql_query(&config.api_db, "SELECT count(*) FROM pg_policies;")
        .await
        .unwrap_or_default();
    let rls_count: i64 = rls.trim().parse().unwrap_or(0);
    if rls_count > 0 {
        counters.pass(format!("RLS policies: {rls_count}"));
    } else {
        output::info("no RLS policies found");
    }

    let api_key_mig = find_file(migration_dir, "api_key");
    if api_key_mig {
        counters.pass("API key migration generated");
    } else {
        counters.fail_test("no API key migration found");
    }
    let rls_files = count_files_with_suffix(migration_dir, "_rls.sql");
    if rls_files > 0 {
        counters.pass(format!("RLS migration files: {rls_files}"));
    } else {
        counters.fail_test("no RLS migration files");
    }
    config.metrics.end();
}

async fn stage_health(config: &OpsConfig, counters: &mut TestCounters) {
    // ---- 7. Health endpoint ----
    output::section("7. Health endpoint");
    config.metrics.begin("GET /health");

    match http_get_body(&format!("{}/health", config.api_url()), &[]).await {
        Ok((status, body)) if status == "200" => {
            counters.pass("GET /health -> 200");
            let status_val = extract_json_field(&body, "status");
            if status_val == "ok" {
                counters.pass("  status = 'ok'");
            } else {
                counters.fail_test(format!("  status = '{status_val}'"));
            }
        }
        Ok((s, _)) => counters.fail_test(format!("GET /health -> {s}")),
        Err(e) => counters.fail_test(format!("GET /health: {e}")),
    }
    config.metrics.end();
}

async fn stage_rls_isolation(
    config: &OpsConfig,
    auth_header: Option<&String>,
    api_key_b: Option<&String>,
    counters: &mut TestCounters,
) {
    // ---- 8. Cross-tenant RLS isolation ----
    output::section("8. Cross-tenant RLS isolation");
    config.metrics.begin("RLS cross-tenant isolation");

    let isolation_file = config
        .manifest
        .hurl
        .as_ref()
        .and_then(|h| h.skip.first().cloned())
        .unwrap_or_else(|| "08_rls_isolation.hurl".to_string());
    let isolation_path = config
        .manifest
        .hurl
        .as_ref()
        .map(|h| config.root_dir.join(&h.dir).join(&isolation_file));
    if let (Some(path), Some(key_a), Some(key_b)) = (isolation_path, auth_header, api_key_b) {
        if path.is_file() {
            let key_a = key_a.trim_start_matches("Authorization: Bearer ");
            let out = Command::new("hurl")
                .arg("--test")
                .arg("--variable")
                .arg(format!("base_url={}", config.api_url()))
                .arg("--variable")
                .arg(format!("api_key_a={key_a}"))
                .arg("--variable")
                .arg(format!("api_key_b={key_b}"))
                .arg(&path)
                .output();
            match out {
                Ok(o) => {
                    let stdout = format!(
                        "{}{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    );
                    if hurl_suite_passed(&stdout) {
                        counters.pass("RLS isolation");
                    } else {
                        counters.fail_test("RLS isolation");
                        for line in stdout.lines().filter(|l| l.contains("error:")) {
                            println!("    {line}");
                        }
                    }
                }
                Err(e) => counters.fail_test(format!("RLS isolation: {e}")),
            }
        } else {
            output::warn(format!("RLS isolation file {} missing", path.display()));
        }
    } else {
        output::warn("skipped RLS isolation (missing hurl config or API keys)");
    }
    config.metrics.end();
}

async fn stage_server_log(config: &OpsConfig, counters: &mut TestCounters) {
    // ---- 9. Server log check ----
    output::section("9. Server log check");
    config.metrics.begin("Axum server log check");

    let error_count = std::fs::read_to_string(&config.log_file)
        .map(|c| {
            strip_ansi(&c)
                .lines()
                .filter(|l| l.contains(" ERROR "))
                .count()
        })
        .unwrap_or(0);
    if error_count == 0 {
        counters.pass("No errors in server log");
    } else {
        counters.fail_test(format!("Server log contains {error_count} error(s)"));
        let content = std::fs::read_to_string(&config.log_file).unwrap_or_default();
        for line in strip_ansi(&content)
            .lines()
            .filter(|l| l.contains(" ERROR "))
            .rev()
            .take(5)
        {
            println!("    {line}");
        }
    }
    config.metrics.end();
}

async fn stage_graceful_shutdown(
    config: &OpsConfig,
    supervisor: &mut Supervisor,
    counters: &mut TestCounters,
) {
    // ---- 10. Graceful shutdown verification ----
    output::section("10. Graceful shutdown");
    config.metrics.begin("SIGTERM graceful shutdown");

    let shutdown_ok = {
        let procs = supervisor.take_all();
        if let Some(mut proc) = procs.into_iter().find(|p| p.label == "Axum (API)") {
            let outcome = proc.graceful_shutdown(15).await;
            match outcome {
                crate::proc::ShutdownOutcome::Graceful { seconds } => {
                    counters.pass(format!("Server exited within {seconds}s after SIGTERM"));
                    true
                }
                crate::proc::ShutdownOutcome::ForceKilled { seconds } => {
                    counters.pass(format!("Server force-killed after {seconds}s"));
                    true
                }
                crate::proc::ShutdownOutcome::AlreadyExited => {
                    counters.pass("Server already exited");
                    true
                }
            }
        } else {
            output::warn("no server PID available for shutdown test");
            true
        }
    };
    if shutdown_ok && let Ok(log) = std::fs::read_to_string(&config.log_file) {
        let log = strip_ansi(&log);
        if log.contains("received SIGTERM") {
            counters.pass("Log: received SIGTERM");
        } else {
            output::warn("Log: no SIGTERM receipt message (app-specific)");
        }
        if log.contains("timer service shutting down") || log.contains("shutting down") {
            counters.pass("Log: service shutdown messages present");
        } else {
            output::warn("Log: no explicit shutdown messages (app-specific)");
        }
    }
    config.metrics.end();
}

async fn stage_regeneration(
    config: &OpsConfig,
    args: &ApiArgs,
    counters: &mut TestCounters,
    hook_failures: &mut Vec<String>,
) -> OpsResult<()> {
    // ---- 11. Regeneration (optional) ----
    if args.regen {
        output::section("11. Regeneration validation");
        config.metrics.begin("Regenerate + cargo check");
        if let Some(graph) = &config.manifest.graph_binary {
            // e2e parity: pre_generate hooks (e.g. clean-generated) must run
            // before regeneration — switching persistence providers with a
            // dirty tree left stale files behind and broke the compile check
            // with 290 errors in a real incident.
            hook_failures
                .extend(crate::ext::run_hooks(config, "pre_generate", HookPolicy::PerHook).await?);
            match regenerate(config, graph, args.release) {
                Ok(_) => {
                    counters.pass("Templates regenerated");
                    // Same generator-rev gate as stage 0 — the regen
                    // validation must not silently pass on stale output.
                    if let Err(e) = crate::freshness::check_generator_rev(
                        &config.app_dir,
                        args.allow_gen_rev_mismatch,
                    ) {
                        counters.fail_test(e.to_string());
                    }
                    match cargo_check_in(config) {
                        Ok(()) => counters.pass("Regenerated code compiles"),
                        Err(tail) => {
                            counters.fail_test("Regenerated code does not compile");
                            output::print_tail(&tail, 20);
                        }
                    }
                }
                Err(e) => {
                    counters.fail_test("Regeneration failed");
                    output::fail(e.to_string());
                }
            }
        } else {
            output::warn("no graph_binary configured — skipping regen");
        }
        config.metrics.end();
    }

    Ok(())
}

/// The app binary to boot: release when `--release` (or `--skip-build` with
/// only a release binary present), debug otherwise.
fn app_binary_path(config: &OpsConfig, args: &ApiArgs) -> PathBuf {
    let bin_dir = if args.release || args.skip_build && is_release_binary(config) {
        config.app_dir.join("target/release")
    } else {
        config.app_dir.join("target/debug")
    };
    bin_dir.join(config.app_binary_name())
}

/// Full-wildcard scope set: every entity, every action.
pub(crate) const FULL_WILDCARD_SCOPES: &str =
    r#"[{"entity_type":"*","entity_id":"*","action":"*"}]"#;

/// Read-only scope set: every entity, `read` action only. Out-of-scope
/// writes raise the `scope_enforced_*` RLS policies' P0403 (HTTP 403).
pub(crate) const READ_ONLY_SCOPES: &str =
    r#"[{"entity_type":"*","entity_id":"*","action":"read"}]"#;

/// Provision an API key via public.create_api_key(org, name, permissions).
pub(crate) async fn provision_api_key(
    config: &OpsConfig,
    org_id: &str,
    name: &str,
) -> OpsResult<String> {
    provision_api_key_with_scopes(config, org_id, name, FULL_WILDCARD_SCOPES).await
}

/// Provision a read-only API key: wildcard entity/id scope limited to the
/// `read` action. Out-of-scope writes against it must raise the
/// `scope_enforced_*` policies' P0403 INSUFFICIENT_SCOPE (HTTP 403).
pub(crate) async fn provision_read_only_api_key(
    config: &OpsConfig,
    org_id: &str,
    name: &str,
) -> OpsResult<String> {
    provision_api_key_with_scopes(config, org_id, name, READ_ONLY_SCOPES).await
}

/// Provision an API key with explicit scope JSON (legacy object vocabulary —
/// the same vocabulary the RLS scope policies enforce).
pub(crate) async fn provision_api_key_with_scopes(
    config: &OpsConfig,
    org_id: &str,
    name: &str,
    scopes_json: &str,
) -> OpsResult<String> {
    let sql = format!(
        "SELECT public.create_api_key('{org_id}'::uuid, '{name}', '{scopes_json}'::jsonb);"
    );
    let out = psql_query(&config.api_db, &sql).await?;
    parse_api_key_json(&out).ok_or_else(|| OpsError::TestFailure("could not parse API key".into()))
}

pub(crate) fn parse_api_key_json(out: &str) -> Option<String> {
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return None;
    }
    let v = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    v.get("key").and_then(|k| k.as_str()).map(|s| s.to_string())
}

/// Default password for the generated `app_user` role — matches the value
/// seeded by migration `0002_api_key_management.sql`.
pub(crate) const APP_USER_PASSWORD: &str = "app_user_pass";

/// The `app_user`-pool URL for a manifest DB target (#169): same host, port
/// and database, connecting as the NOBYPASSRLS `app_user` role.
pub(crate) fn app_pool_url(db: &crate::pg::PgTarget) -> String {
    crate::pg::PgTarget {
        user: "app_user".into(),
        password: APP_USER_PASSWORD.into(),
        role: "app_user".into(),
        ..db.clone()
    }
    .url()
}

/// Provision the `app_user` login password so the server can connect via
/// [`app_pool_url`]. No-op when the role does not exist (pre-0002 databases)
/// or when the caller already manages `APP_DATABASE_URL`.
async fn provision_app_pool(config: &OpsConfig, counters: &mut TestCounters) {
    if std::env::var_os("APP_DATABASE_URL").is_some() {
        output::info("APP_DATABASE_URL already set — skipping app_user provisioning");
        return;
    }
    let has_role = psql_query(
        &config.api_db,
        "SELECT count(*) FROM pg_roles WHERE rolname = 'app_user';",
    )
    .await
    .unwrap_or_default();
    if has_role.trim() != "1" {
        output::warn("app_user role missing — app pool not provisioned");
        return;
    }
    let sql = format!("ALTER ROLE app_user WITH PASSWORD '{APP_USER_PASSWORD}';");
    match psql_exec(&config.api_db, &sql).await {
        Ok(()) => counters.pass("app_user pool password provisioned"),
        Err(e) => counters.fail_test(format!("app_user password provisioning failed: {e}")),
    }
}

#[cfg(test)]
mod tests;
