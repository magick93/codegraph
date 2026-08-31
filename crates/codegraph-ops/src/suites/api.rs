//! API integration suite — mirrors the bash `cmd_api` (11 stages) generically.
//!
//! Entity-specific checks are driven by `manifest.smoke`; hurl contract tests
//! by `manifest.hurl`; DB access via `config.api_db`. Everything else is
//! derived from the generated app (binary name, migrations dir, ports).

use std::path::Path;
use std::process::Command;

use crate::config::OpsConfig;
use crate::db::{psql_exec_file_ok, psql_query};
use crate::error::{OpsError, OpsResult};
use crate::ext::run_hooks;
use crate::migrate::run_api_migrations_with_options;
use crate::output;
use crate::proc::{ManagedProcess, Supervisor};
use crate::wait::wait_for_url;

#[derive(Debug, Clone)]
pub struct ApiArgs {
    pub keep: bool,
    pub skip_build: bool,
    pub skip_generate: bool,
    pub migrate: bool,
    pub rebuild: bool,
    pub regen: bool,
    pub release: bool,
    pub metrics_file: Option<String>,
    /// Retry failed hurl files up to this many times (0 = no retries).
    pub retry: u32,
}

/// True when a failed hurl file may be retried: the number of attempts used
/// so far (`attempts_used`, 1 = first attempt) is still below the allowed
/// total (`max_retries + 1`).
pub fn should_retry(attempts_used: u32, max_retries: u32) -> bool {
    attempts_used < max_retries.saturating_add(1)
}

/// Pass/fail counters with verbose log-tail support.
#[derive(Debug, Default)]
pub struct TestCounters {
    pub passes: usize,
    pub failures: usize,
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
        output::fail(format!("  FAIL {}", msg.as_ref()));
        self.failures += 1;
        if output::is_verbose() {
            let log = "/tmp/codegraph-ops-app.log";
            if let Ok(content) = std::fs::read_to_string(log) {
                let tail: Vec<&str> = content.lines().rev().take(5).collect();
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
pub async fn run_api(config: &OpsConfig, args: &ApiArgs) -> OpsResult<()> {
    run_hooks(config, "pre_api").await?;
    let result = run_api_inner(config, args).await;
    let _ = run_hooks(config, "post_api").await;
    result
}

async fn run_api_inner(config: &OpsConfig, args: &ApiArgs) -> OpsResult<()> {
    let mut counters = TestCounters::new();

    // ---- 1. Preflight ----
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

    // Binary smoke tests (only when the admin CLI exists in the scaffold).
    let bin_dir = if args.release || args.skip_build && is_release_binary(config) {
        config.app_dir.join("target/release")
    } else {
        config.app_dir.join("target/debug")
    };
    let binary = bin_dir.join(config.app_binary_name());
    if !binary.is_file() {
        counters.fail_test(format!(
            "no binary at {} — run with build or --rebuild",
            binary.display()
        ));
        return Err(OpsError::TestFailure("binary missing".into()));
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
        // Hook failures are warnings — hooks are consumer-owned.
        if let Err(e) = crate::ext::run_hooks(config, "post_migrate").await {
            output::warn(format!("post_migrate hook failed: {e}"));
        }
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

    // API keys (only if public.create_api_key exists in the scaffold).
    let has_create_key = psql_query(
        &config.api_db,
        "SELECT count(*) FROM pg_proc WHERE proname = 'create_api_key';",
    )
    .await
    .unwrap_or_default();
    let mut auth_header: Option<String> = None;
    let mut api_key_b: Option<String> = None;
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
        {
            if let Ok(key) = provision_api_key(config, &org_b, "ops-test-key-b").await {
                counters.pass("Org B API key provisioned");
                api_key_b = Some(key);
            }
        }
    }
    config.metrics.end();

    // ---- 3. Server ----
    output::section("3. Server");
    config.metrics.begin("Start Axum");

    let mut supervisor = Supervisor::new(args.keep);
    let bind = format!(
        "{}:{}",
        config.manifest.servers.bind_addr, config.manifest.servers.api_port
    );
    let mut server_cmd = Command::new(&binary);
    server_cmd
        .arg("start")
        .arg("--bind-addr")
        .arg(&bind)
        .arg("--database-url")
        .arg(config.api_db.url())
        .env("DATABASE_URL", config.api_db.url())
        .env("SUPABASE_JWT_SECRET", config.jwt_secret.clone());
    let api_proc = ManagedProcess::spawn(server_cmd, "Axum (API)", &config.log_file)?;
    supervisor.add(api_proc);

    if let Err(e) = wait_for_url(&format!("{}/swagger-ui/", config.api_url()), 30, "Axum").await {
        print_log_tail(&config.log_file, 20);
        return Err(e);
    }
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
                    }
                    cmd.arg(&f);
                    let (passed, reqs, error_lines) = match cmd.output() {
                        Ok(out) => {
                            let stdout = format!(
                                "{}{}",
                                String::from_utf8_lossy(&out.stdout),
                                String::from_utf8_lossy(&out.stderr)
                            );
                            if hurl_suite_passed(&stdout) {
                                (true, parse_requests(&stdout), Vec::new())
                            } else {
                                (
                                    false,
                                    0,
                                    stdout
                                        .lines()
                                        .filter(|l| l.contains("error:") || l.contains("Error"))
                                        .map(|l| l.to_string())
                                        .take(8)
                                        .collect(),
                                )
                            }
                        }
                        Err(e) => (false, 0, vec![e.to_string()]),
                    };
                    if passed {
                        total_requests += reqs;
                        counters.pass(format!("{name} ({reqs} request(s))"));
                        break;
                    }
                    if !should_retry(attempts_used, args.retry) {
                        counters.fail_test(name);
                        for line in &error_lines {
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

    let api_key_mig = find_file(&migration_dir, "api_key");
    if api_key_mig {
        counters.pass("API key migration generated");
    } else {
        counters.fail_test("no API key migration found");
    }
    let rls_files = count_files_with_suffix(&migration_dir, "_rls.sql");
    if rls_files > 0 {
        counters.pass(format!("RLS migration files: {rls_files}"));
    } else {
        counters.fail_test("no RLS migration files");
    }
    config.metrics.end();

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
    if let (Some(path), Some(key_a), Some(key_b)) =
        (isolation_path, auth_header.as_ref(), api_key_b.as_ref())
    {
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
                    let stdout = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
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
    if shutdown_ok {
        if let Ok(log) = std::fs::read_to_string(&config.log_file) {
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
    }
    config.metrics.end();

    // ---- 11. Regeneration (optional) ----
    if args.regen {
        output::section("11. Regeneration validation");
        config.metrics.begin("Regenerate + cargo check");
        if let Some(graph) = &config.manifest.graph_binary {
            let run_out = regenerate(config, graph);
            if run_out.contains("error") {
                counters.fail_test("Regeneration failed");
                for line in run_out.lines().filter(|l| l.contains("error")).take(5) {
                    println!("    {line}");
                }
            } else {
                counters.pass("Templates regenerated");
                let check_out = cargo_check_in(&config.app_dir);
                if check_out.contains("^error") {
                    counters.fail_test("Regenerated code does not compile");
                } else {
                    counters.pass("Regenerated code compiles");
                }
            }
        } else {
            output::warn("no graph_binary configured — skipping regen");
        }
        config.metrics.end();
    }

    // ---- Summary ----
    let ok = counters.summary();
    if let Some(metrics_file) = &args.metrics_file {
        let _ = config.metrics.append_tsv(Path::new(metrics_file), "api");
    }
    supervisor.shutdown_all().await;

    if ok {
        Ok(())
    } else {
        Err(OpsError::TestFailure(format!(
            "{} of {} API tests failed",
            counters.failures,
            counters.passes + counters.failures
        )))
    }
}

/// Whether the release binary is the one to use.
fn is_release_binary(config: &OpsConfig) -> bool {
    let release = config
        .app_dir
        .join("target/release")
        .join(config.app_binary_name());
    release.is_file()
}

/// Provision an API key via public.create_api_key(org, name, permissions).
async fn provision_api_key(config: &OpsConfig, org_id: &str, name: &str) -> OpsResult<String> {
    let sql = format!(
        "SELECT public.create_api_key('{org_id}'::uuid, '{name}', \
         '[{{\"entity_type\":\"*\",\"entity_id\":\"*\",\"action\":\"*\"}}]'::jsonb);"
    );
    let out = psql_query(&config.api_db, &sql).await?;
    parse_api_key_json(&out).ok_or_else(|| OpsError::TestFailure("could not parse API key".into()))
}

fn parse_api_key_json(out: &str) -> Option<String> {
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return None;
    }
    let v = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    v.get("key").and_then(|k| k.as_str()).map(|s| s.to_string())
}

fn extract_json_field(body: &str, path: &str) -> String {
    let Some(value) = parse_json(body) else {
        return String::new();
    };
    let mut cur = value;
    for key in path.split('.') {
        if let Some(v) = cur.get(key) {
            cur = v.clone();
        } else {
            return String::new();
        }
    }
    cur.as_str().map(|s| s.to_string()).unwrap_or_default()
}

fn parse_json(body: &str) -> Option<serde_json::Value> {
    serde_json::from_str(body).ok().or_else(|| {
        // Fallback: extract the first {...} block from the body (curl tail).
        let start = body.find('{')?;
        let end = body.rfind('}')?;
        serde_json::from_str(&body[start..=end]).ok()
    })
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for esc in chars.by_ref() {
                if esc.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn run_capture(binary: &Path, args: &[&str], cwd: &Path) -> CapturedOutput {
    run_capture_env(binary, args, cwd, &[])
}

struct CapturedOutput {
    stdout: String,
}

impl CapturedOutput {
    fn output_contains(&self, needle: &str) -> bool {
        self.stdout.contains(needle)
    }
}

fn run_capture_env(
    binary: &Path,
    args: &[&str],
    cwd: &Path,
    envs: &[(&str, &str)],
) -> CapturedOutput {
    let mut cmd = Command::new(binary);
    cmd.args(args).current_dir(cwd);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let stdout = cmd
        .output()
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .unwrap_or_default();
    CapturedOutput { stdout }
}

/// Whether a `hurl --test` run passed, based on its summary block.
///
/// Hurl prints a summary like:
///
/// ```text
/// Executed files:    2
/// Executed requests: 10 (333.3/s)
/// Succeeded files:   2 (100.0%)
/// Failed files:      0 (0.0%)
/// ```
///
/// A naive `contains("100.0%")` check is a false positive: a fully-failed run
/// prints `Failed files: 2 (100.0%)`. The authoritative signal is the
/// `Failed files:` count — the suite passed iff it is 0.
fn hurl_suite_passed(stdout: &str) -> bool {
    for line in stdout.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("Failed files:") else {
            continue;
        };
        return rest
            .split_whitespace()
            .next()
            .and_then(|n| n.trim().parse::<u32>().ok())
            == Some(0);
    }
    // No `Failed files:` summary at all (unexpected output) — never claim pass.
    false
}

/// Pluralize the last path segment of a smoke entity route using the same
/// simple rules as the codegen `pluralize` Tera filter (append `s`; `s`-final
/// → `es`; `y`-final not preceded by ey/ay/oy → `ies`). Used when the
/// manifest doesn't carry the resolved plural route.
fn pluralize_entity_route(entity: &str) -> String {
    let (prefix, seg) = match entity.rsplit_once('/') {
        Some((p, s)) => (Some(p), s),
        None => (None, entity),
    };
    let plural = if seg.ends_with('s') {
        format!("{seg}es")
    } else if seg.ends_with('y')
        && !seg.ends_with("ey")
        && !seg.ends_with("ay")
        && !seg.ends_with("oy")
    {
        format!("{}ies", &seg[..seg.len() - 1])
    } else {
        format!("{seg}s")
    };
    match prefix {
        Some(p) => format!("{p}/{plural}"),
        None => plural,
    }
}

/// Split a `curl -w "\n%{http_code}"` response into (body, status).
fn split_status_body(text: &str) -> (String, String) {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let status = lines
        .pop()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    (lines.join("\n"), status)
}

/// GET an HTTP URL, returning (status, body). Body excludes the status tail.
async fn http_get_body(url: &str, headers: &[(&str, &str)]) -> OpsResult<(String, String)> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s").arg("-w").arg("\n%{http_code}");
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg(url);
    let out = cmd.output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, status) = split_status_body(&text);
    Ok((status, body))
}

/// POST an HTTP URL with a JSON body, returning (status, body).
async fn http_post_body(
    url: &str,
    data: &str,
    headers: &[(&str, &str)],
) -> OpsResult<(String, String)> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-X")
        .arg("POST")
        .arg("-w")
        .arg("\n%{http_code}");
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg("-d").arg(data).arg(url);
    let out = cmd.output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, status) = split_status_body(&text);
    Ok((status, body))
}

/// GET an HTTP URL, returning just the status code.
async fn http_status(url: &str, headers: &[(&str, &str)]) -> OpsResult<u16> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}");
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg(url);
    let out = cmd.output()?;
    let code = String::from_utf8_lossy(&out.stdout).trim().to_string();
    code.parse::<u16>()
        .map_err(|e| OpsError::Http(format!("bad status {code:?}: {e}")))
}

fn print_log_tail(log_path: &Path, n: usize) {
    if let Ok(content) = std::fs::read_to_string(log_path) {
        let lines: Vec<&str> = content.lines().rev().take(n).collect();
        output::warn(format!("--- {} (last {n} lines) ---", log_path.display()));
        for line in lines.iter().rev() {
            println!("    {line}");
        }
        output::warn("--- end ---");
    }
}

fn find_file(dir: &Path, needle: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .to_lowercase()
            .contains(needle)
    })
}

fn count_files_with_suffix(dir: &Path, suffix: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(suffix))
        .count()
}

fn parse_requests(hurl_output: &str) -> usize {
    hurl_output
        .lines()
        .find_map(|l| {
            let t = l.trim();
            if t.starts_with("Executed requests:") {
                return t
                    .strip_prefix("Executed requests:")
                    .and_then(|rest| rest.split_whitespace().next())
                    .and_then(|n| n.trim().parse::<usize>().ok());
            }
            t.strip_suffix(" request")
                .and_then(|n| n.rsplit(' ').next())
                .and_then(|n| n.trim().parse::<usize>().ok())
        })
        .unwrap_or(0)
}

/// Regenerate the app via the graph binary; returns captured combined output.
fn regenerate(config: &OpsConfig, graph_binary: &str) -> String {
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg(graph_binary)
        .arg("--")
        .arg("run")
        .current_dir(&config.root_dir);
    if let Some(schemas) = &config.manifest.schemas_dir {
        cmd.arg("--schemas").arg(schemas);
    }
    if let Some(classifier) = &config.manifest.classifier {
        cmd.arg("--classifier").arg(classifier);
    }
    if let Some(cfg) = &config.manifest.domain_config {
        cmd.arg("--config").arg(cfg);
    }
    cmd.arg("--output").arg(&config.app_dir);
    cmd.output()
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .unwrap_or_default()
}

fn cargo_check_in(dir: &Path) -> String {
    Command::new("cargo")
        .arg("check")
        .current_dir(dir)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_codes() {
        assert_eq!(strip_ansi("\u{1b}[0;31mred\u{1b}[0m plain"), "red plain");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
    }

    #[test]
    fn parses_hurl_success_and_requests() {
        assert!(
            parse_requests("Succeeded files: 1\nExecuted files: 1\nRequests: 12 request\n") > 0
        );
        assert_eq!(parse_requests("no data"), 0);
    }

    #[test]
    fn hurl_pass_check_is_not_fooled_by_failed_files_percentage() {
        // Real hurl 7 summary lines (aligned columns preserved as-is).
        let success = "Executed files:    1\nExecuted requests: 1 (333.3/s)\n\
                       Succeeded files:   1 (100.0%)\nFailed files:      0 (0.0%)\nDuration: 3 ms";
        assert!(
            hurl_suite_passed(success),
            "a run with Failed files: 0 must pass"
        );

        // Regression: the old `contains("100.0%")` check also matched this.
        let failure = "Executed files:    1\nExecuted requests: 0 (0.0/s)\n\
                       Succeeded files:   0 (0.0%)\nFailed files:      1 (100.0%)\nDuration: 1 ms";
        assert!(
            !hurl_suite_passed(failure),
            "Failed files: 1 (100.0%) must NOT count as a pass"
        );

        let mixed =
            "Executed files:    3\nSucceeded files:   2 (66.7%)\nFailed files:      1 (33.3%)";
        assert!(!hurl_suite_passed(mixed));

        // Unexpected output (no summary) never claims a pass.
        assert!(!hurl_suite_passed("hurl: command not found"));
        assert!(!hurl_suite_passed(""));
    }

    #[test]
    fn pluralizes_smoke_entity_route() {
        assert_eq!(
            pluralize_entity_route("recruiting/candidate"),
            "recruiting/candidates"
        );
        assert_eq!(
            pluralize_entity_route("compensation/pay-run"),
            "compensation/pay-runs"
        );
        assert_eq!(pluralize_entity_route("common/address"), "common/addresses");
        assert_eq!(pluralize_entity_route("common/status"), "common/statuses");
        assert_eq!(
            pluralize_entity_route("recruiting/category"),
            "recruiting/categories"
        );
        assert_eq!(pluralize_entity_route("candidate"), "candidates");
        assert_eq!(
            pluralize_entity_route("common/employment-permit"),
            "common/employment-permits"
        );
    }

    #[test]
    fn extracts_json_field_dotted() {
        let body = r#"{"data": {"id": "abc-123"}, "meta": {}}"#;
        assert_eq!(extract_json_field(body, "data.id"), "abc-123");
        // Non-string fields return empty.
        assert_eq!(extract_json_field(body, "meta"), "");
        assert_eq!(extract_json_field(body, "nope"), "");
    }

    #[test]
    fn parses_api_key_from_json() {
        assert_eq!(
            parse_api_key_json(r#"{"key":"k-123","org_id":"o"}"#).as_deref(),
            Some("k-123")
        );
        assert_eq!(parse_api_key_json("(0 rows)"), None);
        assert_eq!(parse_api_key_json(""), None);
    }

    #[test]
    fn detects_files_by_name_and_suffix() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("0002_api_key_management.sql"), "x").unwrap();
        std::fs::write(dir.path().join("0100_candidate_rls.sql"), "x").unwrap();
        std::fs::write(dir.path().join("0200_table.sql"), "x").unwrap();
        assert!(find_file(dir.path(), "api_key"));
        assert_eq!(count_files_with_suffix(dir.path(), "_rls.sql"), 1);
    }

    #[test]
    fn parses_status_and_body_split() {
        let text = "body line 1\nbody line 2\n200".to_string();
        let (body, status) = split_status_body(&text);
        assert_eq!(status, "200");
        assert_eq!(body, "body line 1\nbody line 2");
    }

    #[test]
    fn retry_decision_respects_budget() {
        // No retries: even the first failure is final.
        assert!(!should_retry(1, 0));
        // max 3: attempts 1..3 may retry, attempt 4 is final.
        assert!(should_retry(1, 3));
        assert!(should_retry(2, 3));
        assert!(should_retry(3, 3));
        assert!(!should_retry(4, 3));
        // Overflow-safe: max value still allows the documented total.
        assert!(should_retry(1, u32::MAX));
    }
}
