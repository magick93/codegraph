//! CLI entry point for the ops harness (clap).
//!
//! Subcommands: `api`, `cli`, `e2e`, `ui`, `full`, `workers`, `clean`,
//! `smoke`, `quality`, `doctor`, `bundle`, `ext <name>`. Global flags:
//! `--config`, `--keep`, `--skip-build`, `--skip-generate`, `--release`,
//! `--verbose`, `--metrics`, `--metrics-format`, `--retry`, `--headed`,
//! `--grep`, `--results`, `--pw-retries`, `--allow-gen-errors`,
//! `--allow-gen-rev-mismatch`, `--reuse`, `--clear-cache`, `--no-bundle`,
//! `--codegraph-root`. The `e2e` subcommand additionally takes
//! `--skip-ui-build`/`--retry-failed`; `clean` takes `--deep`. Exit codes:
//! 0 success; 1 harness-reported failure; 2 timeout.
//!
//! The generated `testkit` binary wraps `codegraph_ops::cli::main()`.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::OpsConfig;
use crate::error::OpsError;
use crate::output;
use crate::suites::api::{run_api, ApiArgs};
use crate::suites::cli::{run_cli, CliArgs};
use crate::suites::e2e::{run_e2e, E2eArgs};
use crate::suites::quality::run_quality;
use crate::suites::smoke::{run_smoke, SmokeArgs};
use crate::suites::ui::{run_ui, UiArgs};
use crate::suites::workers::{run_workers, WorkersArgs};

const DEFAULT_MANIFEST: &str = "codegraph-ops.toml";

/// Output format for `--metrics`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum MetricsFormat {
    Tsv,
    Json,
}

#[derive(Parser)]
#[command(
    name = "testkit",
    about = "Test & deploy harness for codegraph-generated apps",
    after_help = "Exit codes: 0 success; 1 any harness-reported failure (config error, \
missing tool, failed checks or tests); 2 timeout. Usage errors (unknown \
flags) exit 2 via clap before the harness runs.\n\n\
Examples:\n  \
testkit api --metrics ci.tsv\n  \
testkit e2e --results e2e.json\n  \
testkit doctor --config codegraph-ops.toml",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    command: Cmd,

    /// Manifest file (default: codegraph-ops.toml found from the cwd or the
    /// testkit executable, walking parents).
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Leave services running after tests.
    #[arg(long, global = true)]
    keep: bool,

    /// Skip generate + build (reuse existing output).
    #[arg(long, global = true)]
    skip_build: bool,

    /// Tolerate generation errors (skipped entities) instead of failing the
    /// suite. Default: any generation error is fatal, so silent template
    /// breakage cannot shrink test coverage.
    #[arg(long, global = true)]
    allow_gen_errors: bool,

    /// Warn instead of failing when the generator rev recorded in the app's
    /// .codegraph-manifest.json (codegraphCommit) differs from the rev this
    /// testkit was built from — i.e. the tested output was produced by a
    /// stale graph binary. Default: a mismatch is a hard error right after
    /// generation.
    #[arg(long, global = true)]
    allow_gen_rev_mismatch: bool,

    /// Skip generation only.
    #[arg(long, global = true)]
    skip_generate: bool,

    /// Build in release mode.
    #[arg(long, global = true)]
    release: bool,

    /// Also stream quiet stages (dependency installs, browser downloads) and
    /// echo each stage's full captured output inline. Default: stage starts,
    /// labeled child-process output ([build] Compiling foo), and durations on
    /// stage end. Failure tails print regardless of this flag.
    #[arg(long, global = true)]
    verbose: bool,

    /// Append stage timings to FILE (TSV or JSON via --metrics-format).
    #[arg(long, global = true, value_name = "FILE")]
    metrics: Option<PathBuf>,

    /// Format for --metrics (default: tsv).
    #[arg(long, global = true, value_enum, default_value_t = MetricsFormat::Tsv)]
    metrics_format: MetricsFormat,

    /// Retry failed hurl files in the api suite up to N times (default: 0).
    #[arg(long, global = true, value_name = "N", default_value_t = 0)]
    retry: u32,

    /// Show the browser (Playwright).
    #[arg(long, global = true)]
    headed: bool,

    /// Filter Playwright tests by pattern (repeatable).
    #[arg(long, global = true, value_name = "PATTERN")]
    grep: Vec<String>,

    /// Write a machine-readable JSON results report to FILE (api, e2e,
    /// workers): pass/fail counts, failing titles, stage timings, exit code.
    #[arg(long, global = true, value_name = "FILE")]
    results: Option<PathBuf>,

    /// Playwright retry count for the e2e suite (passed through as
    /// `--retries=N`).
    #[arg(long, global = true, value_name = "N")]
    pw_retries: Option<u32>,

    /// Override the codegraph checkout root; exported as `CODEGRAPH_ROOT`
    /// for hooks and generated path-dep normalization.
    #[arg(long, global = true, value_name = "PATH")]
    codegraph_root: Option<PathBuf>,

    /// Reuse a registry-known service already running on a needed port
    /// (.testkit/services.json) instead of killing it and rebinding.
    /// CAVEAT: the harness assumes the running service serves the current
    /// build — generate/build still run, but the OLD server keeps serving.
    #[arg(long, global = true)]
    reuse: bool,

    /// Clear the Playwright transpile cache (/tmp/playwright-transform-cache-*)
    /// at run start. The e2e and ui suites clear it before Playwright anyway;
    /// this forces the clear up front for every other suite.
    #[arg(long, global = true)]
    clear_cache: bool,

    /// Skip the automatic failure artifact bundle ({root}/test-results/
    /// artifacts-*) that suites otherwise assemble on failure (#358).
    #[arg(long, global = true)]
    no_bundle: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// API integration tests (preflight, migrate, hurl, curl smoke, RLS...).
    #[command(after_help = "Examples:\n  \
testkit api --metrics ci.tsv\n  \
testkit api --skip-build --retry 2")]
    Api {
        /// Skip DB reset + migration (tables already exist).
        #[arg(long)]
        no_migrate: bool,
        /// Force rebuild of the binary.
        #[arg(long)]
        rebuild: bool,
        /// Regenerate from templates and verify compilation.
        #[arg(long)]
        regen: bool,
    },
    /// CLI e2e tests (starts the API first if not running).
    #[command(after_help = "Examples:\n  \
testkit cli")]
    Cli,
    /// Full E2E: Supabase -> generate -> build -> Playwright.
    #[command(after_help = "Examples:\n  \
testkit e2e --results e2e.json\n  \
testkit e2e --retry-failed --grep Owner\n  \
testkit e2e -- --last-failed")]
    E2e {
        /// Skip the SvelteKit production build (preview may serve a stale
        /// bundle — normally the build failure is fatal).
        #[arg(long)]
        skip_ui_build: bool,
        /// When the main Playwright run fails, rerun only the failed tests
        /// (`--last-failed`) in the same session; a green retry counts as
        /// transient and passes the suite.
        #[arg(long)]
        retry_failed: bool,
        /// Extra args passed through to Playwright.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// UI-only Playwright runner (requires the API running).
    #[command(after_help = "Examples:\n  \
testkit ui --headed --grep webhooks")]
    Ui {
        /// Extra args passed through to Playwright.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Run the API suite then the E2E suite.
    #[command(after_help = "Examples:\n  \
testkit full --metrics timings.tsv")]
    Full,
    /// Workers topology (per-domain workers + gateway, cornucopia):
    /// regenerate -> migrate plain Postgres -> build -> boot -> smoke + hurl.
    #[command(after_help = "Examples:\n  \
testkit workers --results workers.json")]
    Workers,
    /// Stop services and remove generated output.
    #[command(after_help = "Examples:\n  \
testkit clean\n  \
testkit clean --deep")]
    Clean {
        /// Also remove Playwright triage artifacts ({root}/test-results).
        /// Default clean KEEPS them — failed-run screenshots/traces stay
        /// available for debugging.
        #[arg(long)]
        deep: bool,
    },
    /// One-shot state report: generated tree, binaries, databases, ports,
    /// tools, disk.
    #[command(after_help = "Examples:\n  \
testkit doctor --config codegraph-ops.toml")]
    Doctor,
    /// Assemble a failure artifact bundle on demand (server logs, hurl +
    /// Playwright results, the last run's results/metrics) into
    /// {root}/test-results/artifacts-*. Suites do this automatically on
    /// failure unless --no-bundle is given.
    #[command(after_help = "Examples:\n  \
testkit bundle")]
    Bundle,
    /// Smoke-test a remote deployment.
    #[command(after_help = "Examples:\n  \
testkit smoke --api-url https://api.example.com --web-url https://app.example.com\n  \
testkit smoke --expected-commit 9d1e0f --worker https://billing.example.com")]
    Smoke {
        #[arg(long, default_value = "http://localhost:3000")]
        api_url: String,
        #[arg(long, default_value = "http://localhost:5173")]
        web_url: String,
        #[arg(long)]
        expected_commit: Option<String>,
        #[arg(long)]
        auth_health_url: Option<String>,
        /// Worker base URLs to ping (repeatable).
        #[arg(long = "worker", value_name = "URL")]
        workers: Vec<String>,
    },
    /// Run repo quality gates (test, clippy, fmt, generate, check).
    #[command(after_help = "Examples:\n  \
testkit quality doc")]
    Quality {
        /// Extra cargo gates to run (e.g. `doc`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Run a test extension (from the manifest or trait registry).
    #[command(after_help = "Examples:\n  \
testkit ext --list\n  \
testkit ext refresh-views")]
    Ext {
        /// List registered extensions.
        #[arg(long)]
        list: bool,
        name: Option<String>,
        /// Extra args passed to the extension.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Run the harness; returns the process exit code.
pub async fn main() -> i32 {
    let cli = Cli::parse();
    output::set_verbose(cli.verbose);
    if cli.clear_cache {
        crate::pwcache::clear_and_report();
    }
    if let Some(root) = &cli.codegraph_root {
        // Child processes (hooks, cargo, the generated app) inherit this, so
        // generated manifests and hooks can reference the checkout via
        // `{env:CODEGRAPH_ROOT}` instead of a hardcoded absolute path.
        std::env::set_var("CODEGRAPH_ROOT", root);
        output::info(format!("CODEGRAPH_ROOT={}", root.display()));
    }

    let manifest_path = match cli.config.clone() {
        Some(path) => path,
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| cwd.clone());
            find_manifest(&cwd, &exe_dir).unwrap_or_else(|| PathBuf::from(DEFAULT_MANIFEST))
        }
    };
    let config = match OpsConfig::load(&manifest_path) {
        Ok(c) => c,
        Err(e) => {
            output::fail(format!("{e}"));
            output::info(format!(
                "use --config to point at a codegraph-ops.toml (default: {DEFAULT_MANIFEST})"
            ));
            return e.exit_code();
        }
    };

    let result = match &cli.command {
        Cmd::Api {
            no_migrate,
            rebuild,
            regen,
        } => {
            let args = ApiArgs {
                keep: cli.keep,
                skip_build: cli.skip_build,
                skip_generate: cli.skip_generate,
                migrate: !no_migrate,
                rebuild: *rebuild,
                regen: *regen,
                release: cli.release,
                retry: cli.retry,
                results_file: cli.results.as_ref().map(|p| p.display().to_string()),
                allow_gen_errors: cli.allow_gen_errors,
                allow_gen_rev_mismatch: cli.allow_gen_rev_mismatch,
                reuse: cli.reuse,
            };
            output::bold("Running API integration tests");
            run_api(&config, &args).await
        }
        Cmd::Cli => {
            output::bold("Running CLI e2e tests");
            if !api_health_ok(&config) {
                output::info("API not running — starting it via `api --keep` first");
                let args = ApiArgs {
                    keep: true,
                    skip_build: cli.skip_build,
                    skip_generate: cli.skip_generate,
                    migrate: true,
                    rebuild: false,
                    regen: false,
                    release: cli.release,
                    retry: cli.retry,
                    results_file: cli.results.as_ref().map(|p| p.display().to_string()),
                    allow_gen_errors: cli.allow_gen_errors,
                    allow_gen_rev_mismatch: cli.allow_gen_rev_mismatch,
                    reuse: cli.reuse,
                };
                if let Err(e) = run_api(&config, &args).await {
                    return fail_suite(&cli, &config, "api", e);
                }
            }
            let args = CliArgs {
                skip_build: cli.skip_build,
                verbose: cli.verbose,
            };
            run_cli(&config, &args).await
        }
        Cmd::E2e {
            skip_ui_build,
            retry_failed,
            extra,
        } => {
            let args = E2eArgs {
                keep: cli.keep,
                skip_build: cli.skip_build,
                skip_generate: cli.skip_generate,
                release: cli.release,
                headed: cli.headed,
                skip_ui_build: *skip_ui_build,
                retry_failed: *retry_failed,
                results_file: cli.results.as_ref().map(|p| p.display().to_string()),
                allow_gen_rev_mismatch: cli.allow_gen_rev_mismatch,
                reuse: cli.reuse,
                playwright_args: build_playwright_args(&cli, extra),
            };
            output::bold("Running end-to-end tests");
            run_e2e(&config, &args).await
        }
        Cmd::Ui { extra } => {
            let args = UiArgs {
                keep: cli.keep,
                headed: cli.headed,
                reuse: cli.reuse,
                playwright_args: build_playwright_args(&cli, extra),
            };
            output::bold("Running UI Playwright tests");
            run_ui(&config, &args).await
        }
        Cmd::Full => {
            // `full` runs e2e even when api failed (old bash behavior) and
            // accumulates exit codes.
            return run_full(&cli, &config).await;
        }
        Cmd::Workers => {
            let args = WorkersArgs {
                keep: cli.keep,
                skip_generate: cli.skip_generate,
                release: cli.release,
                results_file: cli.results.as_ref().map(|p| p.display().to_string()),
                allow_gen_rev_mismatch: cli.allow_gen_rev_mismatch,
                reuse: cli.reuse,
            };
            output::bold("Running workers-topology tests");
            run_workers(&config, &args).await
        }
        Cmd::Clean { deep } => {
            cmd_clean(&config, *deep).await;
            Ok(())
        }
        Cmd::Doctor => {
            output::bold("Running doctor (workspace state report)");
            crate::doctor::run_doctor(&config).await
        }
        Cmd::Bundle => {
            output::bold("Assembling artifact bundle");
            let extras = bundle_extra_files(&cli);
            let refs: Vec<&Path> = extras.iter().map(|p| p.as_path()).collect();
            let report = crate::bundle::assemble_for_config(&config, "bundle", &refs);
            if report.copied == 0 && report.skipped == 0 {
                output::warn(
                    "nothing to bundle — no server logs, hurl or Playwright results found",
                );
            }
            Ok(())
        }
        Cmd::Smoke {
            api_url,
            web_url,
            expected_commit,
            auth_health_url,
            workers,
        } => {
            let args = SmokeArgs {
                api_url: api_url.clone(),
                web_url: web_url.clone(),
                expected_commit: expected_commit.clone(),
                auth_health_url: auth_health_url.clone(),
                workers: workers.clone(),
            };
            output::bold(format!("Smoke-testing {}", args.api_url));
            run_smoke(&args).await
        }
        Cmd::Quality { extra } => {
            output::bold("Running quality gates");
            run_quality(&config, extra).await
        }
        Cmd::Ext { list, name, args } => {
            if *list {
                output::info("Registered extensions:");
                for n in crate::ext::extension_names() {
                    println!("  - {n}");
                }
                return 0;
            }
            let Some(name) = name else {
                output::fail("ext requires a name (or --list)");
                return 1;
            };
            crate::ext::run_extension(name, &config, args).await
        }
    };

    let suite = subcommand_name(&cli.command);
    match result {
        Ok(()) => finish_ok(&cli, &config, suite),
        Err(e) => fail_suite(&cli, &config, suite, e),
    }
}

/// Run the API suite, then ALWAYS the E2E suite (matching the old bash:
/// failures accumulate; non-zero if either suite failed). Returns the
/// combined exit code.
async fn run_full(cli: &Cli, config: &OpsConfig) -> i32 {
    output::bold("Running full test suite (API + E2E)");
    let api_args = ApiArgs {
        keep: cli.keep,
        skip_build: cli.skip_build,
        skip_generate: cli.skip_generate,
        migrate: true,
        rebuild: false,
        regen: false,
        release: cli.release,
        retry: cli.retry,
        results_file: cli.results.as_ref().map(|p| p.display().to_string()),
        allow_gen_errors: cli.allow_gen_errors,
        allow_gen_rev_mismatch: cli.allow_gen_rev_mismatch,
        reuse: cli.reuse,
    };
    let api_code = match run_api(config, &api_args).await {
        Ok(()) => None,
        Err(e) => Some(fail_suite(cli, config, "api", e)),
    };
    println!();
    output::bold("════════════════════════════════════════════");
    println!();
    let e2e_args = E2eArgs {
        keep: cli.keep,
        skip_build: cli.skip_build,
        skip_generate: cli.skip_generate,
        release: cli.release,
        headed: cli.headed,
        skip_ui_build: false,
        retry_failed: false,
        results_file: cli.results.as_ref().map(|p| p.display().to_string()),
        allow_gen_rev_mismatch: cli.allow_gen_rev_mismatch,
        reuse: cli.reuse,
        playwright_args: build_playwright_args(cli, &[]),
    };
    let e2e_code = match run_e2e(config, &e2e_args).await {
        Ok(()) => None,
        Err(e) => Some(fail_suite(cli, config, "e2e", e)),
    };
    let code = combine_codes(api_code, e2e_code);
    if code == 0 {
        finish_ok(cli, config, "full");
    }
    code
}

/// Combine two suite exit codes for `full`: any failed suite wins; when both
/// failed, the larger code is returned; 0 only when both passed.
fn combine_codes(api_code: Option<i32>, e2e_code: Option<i32>) -> i32 {
    match (api_code, e2e_code) {
        (Some(a), Some(b)) => a.max(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => 0,
    }
}

/// Successful-run tail: append metrics (if requested) or print the summary.
fn finish_ok(cli: &Cli, config: &OpsConfig, subcommand: &str) -> i32 {
    if let Some(path) = &cli.metrics {
        let appended = match cli.metrics_format {
            MetricsFormat::Tsv => config.metrics.append_tsv(path, subcommand).is_ok(),
            MetricsFormat::Json => config.metrics.append_json(path, subcommand).is_ok(),
        };
        if !appended {
            output::warn(format!("could not append metrics to {}", path.display()));
        }
    } else {
        config.metrics.print_summary();
    }
    0
}

fn subcommand_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Api { .. } => "api",
        Cmd::Cli => "cli",
        Cmd::E2e { .. } => "e2e",
        Cmd::Ui { .. } => "ui",
        Cmd::Full => "full",
        Cmd::Workers => "workers",
        Cmd::Clean { .. } => "clean",
        Cmd::Doctor => "doctor",
        Cmd::Bundle => "bundle",
        Cmd::Smoke { .. } => "smoke",
        Cmd::Quality { .. } => "quality",
        Cmd::Ext { .. } => "ext",
    }
}

/// The run's own artifact files (`--results` JSON, `--metrics`) that exist on
/// disk — copied into a failure bundle.
fn bundle_extra_files(cli: &Cli) -> Vec<PathBuf> {
    [cli.results.as_ref(), cli.metrics.as_ref()]
        .into_iter()
        .flatten()
        .filter(|p| p.is_file())
        .cloned()
        .collect()
}

fn build_playwright_args(cli: &Cli, extra: &[String]) -> Vec<String> {
    let mut args = Vec::new();
    if cli.headed {
        args.push("--headed".to_string());
    }
    for pattern in &cli.grep {
        args.push("--grep".to_string());
        args.push(pattern.clone());
    }
    if let Some(retries) = cli.pw_retries {
        args.push(format!("--retries={retries}"));
    }
    args.extend(extra.iter().cloned());
    args
}

fn api_health_ok(config: &OpsConfig) -> bool {
    std::process::Command::new("curl")
        .arg("-sf")
        .arg("--max-time")
        .arg("5")
        .arg(format!("{}/health", config.api_url()))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn report_error(subcommand: &str, e: OpsError) -> i32 {
    let code = failure_exit_code(&e);
    output::fail(format!("{subcommand}: {e}"));
    if let Some(h) = crate::error::hint(&e) {
        println!("  {}", output::dim(format!("hint: {h}")));
    }
    code
}

/// Exit code for a suite failure: test failures are 1 like every other
/// fatal error except timeouts (2, matching bash `exit 2`).
fn failure_exit_code(e: &OpsError) -> i32 {
    match e {
        OpsError::TestFailure(_) => 1,
        _ => e.exit_code(),
    }
}

/// Fail a suite: write the early-failure results JSON when the suite returned
/// Err WITHOUT having written its completed-run report (#357 — port
/// conflicts, missing tools, stale binaries and supabase failures used to
/// skip `--results` entirely), then print the error. The stage recorded in
/// the early report is the most recent ▸-level section title
/// (`output::current_section`).
///
/// On failure the artifact bundler also runs (#358) unless `--no-bundle`:
/// server logs, hurl logs, the Playwright summary and this run's
/// results/metrics land in one triage directory, announced by the LAST
/// summary line (`▸ artifacts: …`). Best-effort — bundling never masks the
/// real failure.
fn fail_suite(cli: &Cli, config: &OpsConfig, suite: &str, e: OpsError) -> i32 {
    if let Some(path) = &cli.results {
        if !crate::results::report_written(suite) {
            let report = crate::results::EarlyFailureReport {
                suite: suite.to_string(),
                manifest: config.manifest_path.display().to_string(),
                stage: output::current_section(),
                error: e.to_string(),
                exit: failure_exit_code(&e),
            };
            if let Err(write_err) = report.write(Path::new(path)) {
                output::warn(format!("could not write early results: {write_err}"));
            }
        }
    }
    let code = report_error(suite, e);
    if !cli.no_bundle && auto_bundles(suite) {
        let extras = bundle_extra_files(cli);
        let refs: Vec<&Path> = extras.iter().map(|p| p.as_path()).collect();
        let _ = crate::bundle::assemble_for_config(config, suite, &refs);
    }
    code
}

/// Suites that assemble a failure artifact bundle automatically (#358).
/// `doctor`/`smoke`/`quality`/`ext`/`bundle` are reports or one-shots, not
/// test runs with server logs worth triaging.
fn auto_bundles(suite: &str) -> bool {
    matches!(suite, "api" | "cli" | "e2e" | "ui" | "full" | "workers")
}

/// Locate the manifest when `--config` is absent: walk UP from `cwd`
/// looking for `codegraph-ops.toml`; if not found, walk UP from `exe_dir`
/// (the testkit executable's directory). Returns the first match.
pub fn find_manifest(cwd: &Path, exe_dir: &Path) -> Option<PathBuf> {
    for dir in walk_up(cwd).chain(walk_up(exe_dir)) {
        let candidate = dir.join(DEFAULT_MANIFEST);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Iterator over `start` and its ancestors (inclusive).
fn walk_up(start: &Path) -> impl Iterator<Item = &Path> {
    std::iter::successors(Some(start), |dir| dir.parent())
}

/// Stop services and remove generated output (mirrors bash `cmd_clean`,
/// extended by #356: registry teardown, workers ports, sveltekit logs, and
/// the opt-in `--deep` removal of Playwright triage artifacts).
async fn cmd_clean(config: &OpsConfig, deep: bool) {
    use crate::migrate::remove_supabase_links;

    // 1. Registry teardown: kill every service a previous `--keep` run left
    // behind (by pid), then drop the registry file. Unknown orphans still
    // fall to the fuser port sweep below.
    output::info("Stopping registered services...");
    let killed = crate::registry::teardown(&config.root_dir);
    if killed > 0 {
        output::ok(format!("{killed} registered service(s) stopped"));
    }

    // 2. Port sweep: api + ui ports, plus the workers topology (gateway +
    // per-domain worker range — constants shared with suites::workers while
    // the workers-suite generalization is deferred).
    output::info("Stopping app processes...");
    let mut ports = vec![
        config.manifest.servers.api_port,
        config.manifest.servers.ui_port,
    ];
    ports.push(crate::suites::workers::GATEWAY_PORT);
    ports.extend(crate::suites::workers::worker_ports());
    for port in ports {
        let _ = std::process::Command::new("fuser")
            .arg("-k")
            .arg(format!("{port}/tcp"))
            .status();
    }
    // Remove stale pid files left by the app binary.
    if let Ok(entries) = std::fs::read_dir(&config.root_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&format!("{}-", config.app_binary_name())) && name.ends_with(".pid")
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    if let Some(supabase_dir) = &config.supabase_dir {
        output::info("Stopping Supabase...");
        let _ = std::process::Command::new("npx")
            .arg("supabase")
            .arg("stop")
            .current_dir(supabase_dir)
            .status();
        let mig_dir = supabase_dir.join("supabase/migrations");
        if mig_dir.is_dir() {
            let _ = remove_supabase_links(&mig_dir);
        }
    }

    output::info("Removing generated output...");
    for dir in ["src", "ui", "migrations", "queries", "cornucopia-queries"] {
        let _ = std::fs::remove_dir_all(config.app_dir.join(dir));
    }
    for f in [
        "/tmp/codegraph-ops-app.log",
        "/tmp/codegraph-ops-e2e-app.log",
        "/tmp/codegraph-ops-cli.log",
        "/tmp/codegraph-ops-api-key",
    ] {
        let _ = std::fs::remove_file(f);
    }
    // All SvelteKit preview logs (ui suite + e2e suite variants), which the
    // fixed list above missed.
    let logs = remove_sveltekit_logs(Path::new("/tmp"));
    if logs > 0 {
        output::info(format!("Removed {logs} sveltekit log(s)"));
    }
    // Playwright triage artifacts: KEPT by default (failed-run screenshots/
    // traces stay debuggable); `--deep` opts into their removal.
    if deep {
        output::info("Removing Playwright triage artifacts (test-results/) — --deep");
        let _ = std::fs::remove_dir_all(config.root_dir.join("test-results"));
    }
    output::ok("Clean complete");
}

/// Remove `codegraph-ops-sveltekit*.log` files from `dir` (the /tmp dir in
/// production; injectable for tests). Returns the count removed.
fn remove_sveltekit_logs(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("codegraph-ops-sveltekit") && name.ends_with(".log")
        })
        .filter_map(|e| std::fs::remove_file(e.path()).ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_parses_help() {
        let cmd = Cli::command();
        assert!(cmd.get_subcommands().count() >= 8);
    }

    #[test]
    fn playwright_args_assembly() {
        let cli = Cli::try_parse_from([
            "testkit", "e2e", "--headed", "--grep", "Owner", "--grep", "CRUD",
        ])
        .unwrap();
        let args = build_playwright_args(&cli, &["--retries=2".to_string()]);
        assert_eq!(
            args,
            vec![
                "--headed",
                "--grep",
                "Owner",
                "--grep",
                "CRUD",
                "--retries=2"
            ]
        );
    }

    #[test]
    fn find_manifest_walks_up_from_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(dir.path().join(DEFAULT_MANIFEST), "app_name = \"x\"\n").unwrap();
        let exe_dir = tempfile::tempdir().unwrap();
        assert_eq!(
            find_manifest(&deep, exe_dir.path()),
            Some(dir.path().join(DEFAULT_MANIFEST))
        );
        // The cwd itself also matches when the manifest is right there.
        assert_eq!(
            find_manifest(dir.path(), exe_dir.path()),
            Some(dir.path().join(DEFAULT_MANIFEST))
        );
    }

    #[test]
    fn find_manifest_falls_back_to_exe_dir_then_none() {
        let cwd = tempfile::tempdir().unwrap();
        let exe = tempfile::tempdir().unwrap();
        let exe_deep = exe.path().join("bin/nested");
        std::fs::create_dir_all(&exe_deep).unwrap();
        std::fs::write(exe.path().join(DEFAULT_MANIFEST), "app_name = \"x\"\n").unwrap();
        assert_eq!(
            find_manifest(cwd.path(), &exe_deep),
            Some(exe.path().join(DEFAULT_MANIFEST))
        );
        let other = tempfile::tempdir().unwrap();
        assert_eq!(find_manifest(cwd.path(), other.path()), None);
    }

    #[test]
    fn find_manifest_prefers_cwd_over_exe_dir() {
        let cwd = tempfile::tempdir().unwrap();
        let exe = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join(DEFAULT_MANIFEST), "cwd\n").unwrap();
        std::fs::write(exe.path().join(DEFAULT_MANIFEST), "exe\n").unwrap();
        assert_eq!(
            find_manifest(cwd.path(), exe.path()),
            Some(cwd.path().join(DEFAULT_MANIFEST))
        );
    }

    #[test]
    fn combine_codes_prefers_nonzero_and_max() {
        assert_eq!(combine_codes(None, None), 0);
        assert_eq!(combine_codes(Some(1), None), 1);
        assert_eq!(combine_codes(None, Some(2)), 2);
        assert_eq!(combine_codes(Some(1), Some(1)), 1);
        assert_eq!(combine_codes(Some(1), Some(2)), 2);
        assert_eq!(combine_codes(Some(2), Some(1)), 2);
    }

    #[test]
    fn clean_parses_deep_flag() {
        let plain = Cli::try_parse_from(["testkit", "clean"]).unwrap();
        assert!(matches!(plain.command, Cmd::Clean { deep: false }));
        let deep = Cli::try_parse_from(["testkit", "clean", "--deep"]).unwrap();
        assert!(matches!(deep.command, Cmd::Clean { deep: true }));
    }

    #[test]
    fn global_flags_reuse_and_clear_cache_parse_on_subcommands() {
        let cli = Cli::try_parse_from(["testkit", "api", "--reuse", "--clear-cache"]).unwrap();
        assert!(cli.reuse);
        assert!(cli.clear_cache);
        let cli = Cli::try_parse_from(["testkit", "ui", "--reuse"]).unwrap();
        assert!(cli.reuse && !cli.clear_cache);
    }

    #[test]
    fn no_bundle_defaults_off_and_parses_globally() {
        let cli = Cli::try_parse_from(["testkit", "e2e"]).unwrap();
        assert!(!cli.no_bundle, "bundling on failure is the default");
        let cli = Cli::try_parse_from(["testkit", "api", "--no-bundle"]).unwrap();
        assert!(cli.no_bundle);
        // The bundle subcommand parses standalone.
        let cli = Cli::try_parse_from(["testkit", "bundle"]).unwrap();
        assert!(matches!(cli.command, Cmd::Bundle));
        assert_eq!(subcommand_name(&cli.command), "bundle");
    }

    #[test]
    fn auto_bundle_scoped_to_test_suites() {
        for suite in ["api", "cli", "e2e", "ui", "full", "workers"] {
            assert!(auto_bundles(suite), "{suite} must auto-bundle on failure");
        }
        for suite in ["doctor", "smoke", "quality", "ext", "bundle", "clean"] {
            assert!(!auto_bundles(suite), "{suite} must NOT auto-bundle");
        }
    }

    #[test]
    fn bundle_extra_files_only_include_existing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let results = dir.path().join("results.json");
        std::fs::write(&results, "{}").unwrap();
        let missing = dir.path().join("nope.tsv");
        let cli = Cli::try_parse_from([
            "testkit",
            "api",
            "--results",
            results.to_str().unwrap(),
            "--metrics",
            missing.to_str().unwrap(),
        ])
        .unwrap();
        let extras = bundle_extra_files(&cli);
        assert_eq!(extras, vec![results]);
    }

    #[test]
    fn allow_gen_rev_mismatch_is_a_global_flag_defaulting_off() {
        let cli = Cli::try_parse_from(["testkit", "e2e", "--allow-gen-rev-mismatch"]).unwrap();
        assert!(cli.allow_gen_rev_mismatch);
        let cli = Cli::try_parse_from(["testkit", "workers"]).unwrap();
        assert!(!cli.allow_gen_rev_mismatch);
        // Documented in --help.
        let help = Cli::command().render_help().to_string();
        assert!(
            help.contains("--allow-gen-rev-mismatch"),
            "flag must appear in --help: {help}"
        );
    }

    #[test]
    fn remove_sveltekit_logs_scopes_to_prefix() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "codegraph-ops-sveltekit.log",
            "codegraph-ops-sveltekit-e2e.log",
            "codegraph-ops-app.log",
            "sveltekit-other.log",
        ] {
            std::fs::write(dir.path().join(name), "log").unwrap();
        }
        assert_eq!(remove_sveltekit_logs(dir.path()), 2);
        assert!(!dir.path().join("codegraph-ops-sveltekit.log").exists());
        assert!(!dir.path().join("codegraph-ops-sveltekit-e2e.log").exists());
        assert!(dir.path().join("codegraph-ops-app.log").exists());
        assert!(dir.path().join("sveltekit-other.log").exists());
        // Missing dir is harmless.
        assert_eq!(remove_sveltekit_logs(Path::new("/nonexistent-cg-tmp")), 0);
    }

    #[cfg(unix)]
    #[test]
    fn clean_registry_teardown_kills_leaked_services() {
        use crate::registry::{record_service, ServiceEntry, ServiceRegistry};

        let dir = tempfile::tempdir().unwrap();
        // A detached long-running sleep standing in for a leaked `--keep`
        // server (reparented to init, so it dies and is reaped cleanly).
        let pid = crate::registry::spawn_detached_sleep();
        record_service(
            dir.path(),
            ServiceEntry {
                name: "api".to_string(),
                pid,
                port: 3000,
                health: Some("/health".to_string()),
                started_at: "2026-09-29T12:00:00Z".to_string(),
                profile: Some("debug".to_string()),
                suite: Some("api".to_string()),
            },
        )
        .unwrap();

        let killed = crate::registry::teardown(dir.path());
        assert_eq!(killed, 1, "the leaked service must be killed");
        assert!(!crate::registry::pid_alive(pid), "pid {pid} must be gone");
        assert!(
            !ServiceRegistry::path(dir.path()).exists(),
            "registry file must be removed by clean"
        );
        // Idempotent: a second teardown finds nothing.
        assert_eq!(crate::registry::teardown(dir.path()), 0);
    }
}
