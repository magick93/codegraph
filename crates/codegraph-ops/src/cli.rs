//! CLI entry point for the ops harness (clap).
//!
//! Subcommands: `api`, `cli`, `e2e`, `ui`, `full`, `clean`, `smoke`,
//! `quality`, `ext <name>`. Global flags: `--config`, `--keep`,
//! `--skip-build`, `--skip-generate`, `--release`, `--verbose`, `--metrics`,
//! `--headed`, `--grep`.
//!
//! The generated `testkit` binary wraps `codegraph_ops::cli::main()`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::OpsConfig;
use crate::error::OpsError;
use crate::output;
use crate::suites::api::{run_api, ApiArgs};
use crate::suites::cli::{run_cli, CliArgs};
use crate::suites::e2e::{run_e2e, E2eArgs};
use crate::suites::quality::run_quality;
use crate::suites::smoke::{run_smoke, SmokeArgs};
use crate::suites::ui::{run_ui, UiArgs};

const DEFAULT_MANIFEST: &str = "codegraph-ops.toml";

#[derive(Parser)]
#[command(
    name = "testkit",
    about = "Test & deploy harness for codegraph-generated apps",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    command: Cmd,

    /// Manifest file (default: codegraph-ops.toml in the current dir).
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Leave services running after tests.
    #[arg(long, global = true)]
    keep: bool,

    /// Skip generate + build (reuse existing output).
    #[arg(long, global = true)]
    skip_build: bool,

    /// Skip generation only.
    #[arg(long, global = true)]
    skip_generate: bool,

    /// Build in release mode.
    #[arg(long, global = true)]
    release: bool,

    /// Show server logs on failure.
    #[arg(long, global = true)]
    verbose: bool,

    /// Append stage timings to FILE (TSV).
    #[arg(long, global = true, value_name = "FILE")]
    metrics: Option<PathBuf>,

    /// Show the browser (Playwright).
    #[arg(long, global = true)]
    headed: bool,

    /// Filter Playwright tests by pattern (repeatable).
    #[arg(long, global = true, value_name = "PATTERN")]
    grep: Vec<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// API integration tests (preflight, migrate, hurl, curl smoke, RLS...).
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
    Cli,
    /// Full E2E: Supabase -> generate -> build -> Playwright.
    E2e {
        /// Extra args passed through to Playwright.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// UI-only Playwright runner (requires the API running).
    Ui {
        /// Extra args passed through to Playwright.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Run the API suite then the E2E suite.
    Full,
    /// Stop services and remove generated output.
    Clean,
    /// Smoke-test a remote deployment.
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
    Quality {
        /// Extra cargo gates to run (e.g. `doc`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Run a test extension (from the manifest or trait registry).
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

    let manifest_path = cli
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MANIFEST));
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
                metrics_file: cli.metrics.as_ref().map(|p| p.display().to_string()),
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
                    metrics_file: None,
                };
                if let Err(e) = run_api(&config, &args).await {
                    return report_error("api", e);
                }
            }
            let args = CliArgs {
                skip_build: cli.skip_build,
                verbose: cli.verbose,
            };
            run_cli(&config, &args).await
        }
        Cmd::E2e { extra } => {
            let args = E2eArgs {
                keep: cli.keep,
                skip_build: cli.skip_build,
                skip_generate: cli.skip_generate,
                release: cli.release,
                headed: cli.headed,
                playwright_args: build_playwright_args(&cli, extra),
            };
            output::bold("Running end-to-end tests");
            run_e2e(&config, &args).await
        }
        Cmd::Ui { extra } => {
            let args = UiArgs {
                keep: cli.keep,
                headed: cli.headed,
                playwright_args: build_playwright_args(&cli, extra),
            };
            output::bold("Running UI Playwright tests");
            run_ui(&config, &args).await
        }
        Cmd::Full => {
            output::bold("Running full test suite (API + E2E)");
            let api_args = ApiArgs {
                keep: cli.keep,
                skip_build: cli.skip_build,
                skip_generate: cli.skip_generate,
                migrate: true,
                rebuild: false,
                regen: false,
                release: cli.release,
                metrics_file: cli.metrics.as_ref().map(|p| p.display().to_string()),
            };
            if let Err(e) = run_api(&config, &api_args).await {
                return report_error("full", e);
            }
            println!();
            output::bold("════════════════════════════════════════════");
            println!();
            let e2e_args = E2eArgs {
                keep: cli.keep,
                skip_build: cli.skip_build,
                skip_generate: cli.skip_generate,
                release: cli.release,
                headed: cli.headed,
                playwright_args: build_playwright_args(&cli, &[]),
            };
            run_e2e(&config, &e2e_args).await
        }
        Cmd::Clean => {
            cmd_clean(&config).await;
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

    match result {
        Ok(()) => {
            if let Some(path) = &cli.metrics {
                let _ = config
                    .metrics
                    .append_tsv(path, subcommand_name(&cli.command));
            } else {
                config.metrics.print_summary();
            }
            0
        }
        Err(e) => report_error(subcommand_name(&cli.command), e),
    }
}

fn subcommand_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Api { .. } => "api",
        Cmd::Cli => "cli",
        Cmd::E2e { .. } => "e2e",
        Cmd::Ui { .. } => "ui",
        Cmd::Full => "full",
        Cmd::Clean => "clean",
        Cmd::Smoke { .. } => "smoke",
        Cmd::Quality { .. } => "quality",
        Cmd::Ext { .. } => "ext",
    }
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
    match &e {
        OpsError::TestFailure(_) => {
            output::fail(format!("{subcommand}: {e}"));
            1
        }
        _ => {
            output::fail(format!("{subcommand}: {e}"));
            e.exit_code()
        }
    }
}

/// Stop services and remove generated output (mirrors bash `cmd_clean`).
async fn cmd_clean(config: &OpsConfig) {
    use crate::migrate::remove_supabase_links;

    output::info("Stopping app processes...");
    for port in [
        config.manifest.servers.api_port,
        config.manifest.servers.ui_port,
    ] {
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
    output::ok("Clean complete");
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
}
