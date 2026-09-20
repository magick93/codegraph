//! Repo quality gates (port of scripts/quality-check.sh).
//!
//! Runs cargo test/clippy/fmt against the consumer repo, regenerates the
//! generated app from the manifest, then verifies the app compiles.

use std::path::{Path, PathBuf};

use crate::config::OpsConfig;
use crate::error::{OpsError, OpsResult};
use crate::output;

/// Max chars of command output kept in failure messages.
const TAIL_CHARS: usize = 800;

/// Run the repo quality gates against [`OpsConfig::root_dir`] (consumer repo)
/// and [`OpsConfig::app_dir`] (generated app). The first failing gate aborts
/// with `OpsError::TestFailure`, including a tail of stdout/stderr.
///
/// Gates:
/// 1. `cargo test --workspace` in the repo root.
/// 2. `cargo clippy --workspace -- -D warnings`.
/// 3. `cargo fmt --all -- --check`.
/// 4. Regenerate the app with `cargo run -p {graph_binary} -- run ...`
///    (skipped with a warning when `graph_binary` is unset or neither
///    `mox_files` nor `schemas_dir` provides a model source;
///    only manifest flags with `Some` values are passed).
/// 5. `cargo check` in the generated app directory.
///
/// `extra` names are run as additional `cargo {name} --workspace` gates in
/// the repo root after the standard five.
pub async fn run_quality(config: &OpsConfig, extra: &[String]) -> OpsResult<()> {
    output::section("=== Quality Check ===");

    output::section("1/5 cargo test --workspace");
    run_step(
        "cargo",
        &["test".to_string(), "--workspace".to_string()],
        &config.workspace_root,
    )?;

    output::section("2/5 cargo clippy --workspace -- -D warnings");
    run_step(
        "cargo",
        &[
            "clippy".to_string(),
            "--workspace".to_string(),
            "--".to_string(),
            "-D".to_string(),
            "warnings".to_string(),
        ],
        &config.workspace_root,
    )?;

    output::section("3/5 cargo fmt --all -- --check");
    run_step(
        "cargo",
        &[
            "fmt".to_string(),
            "--all".to_string(),
            "--".to_string(),
            "--check".to_string(),
        ],
        &config.workspace_root,
    )?;

    output::section("4/5 regenerate generated app");
    match regeneration_plan(config) {
        RegenPlan::Run(args) => {
            output::info(format!("cargo {args:?}"));
            run_step("cargo", &args, &config.workspace_root)?;
        }
        RegenPlan::Skip(warning) => output::warn(warning),
    }

    output::section("5/5 cargo check (generated app)");
    run_step("cargo", &["check".to_string()], &config.app_dir)?;

    for name in extra {
        output::section(format!("extra gate: cargo {name} --workspace"));
        run_step(
            "cargo",
            &[name.clone(), "--workspace".to_string()],
            &config.workspace_root,
        )?;
    }

    output::ok("=== All checks passed ===");
    Ok(())
}

/// What quality gate 4 (regenerate generated app) should do.
enum RegenPlan {
    /// Run `cargo {args}` from the workspace root.
    Run(Vec<String>),
    /// Skip regeneration with this warning (remaining gates still run).
    Skip(&'static str),
}

/// Decide gate 4 from the manifest. `mox_files` wins over `schemas_dir`;
/// without a graph binary — or without any model source — regeneration is
/// skipped with a warning, mirroring the previous per-flag warnings.
fn regeneration_plan(config: &OpsConfig) -> RegenPlan {
    let Some(binary) = &config.manifest.graph_binary else {
        return RegenPlan::Skip("graph_binary not set — skipping app regeneration");
    };
    if !config.manifest.mox_files.is_empty() {
        return RegenPlan::Run(generate_mox_args(
            binary,
            &config.manifest.mox_files,
            &config.manifest.domain_config,
            &config.manifest.profile,
            &config.app_dir,
        ));
    }
    match &config.manifest.schemas_dir {
        Some(schemas) => RegenPlan::Run(generate_args(
            binary,
            schemas,
            &config.manifest.classifier,
            &config.manifest.domain_config,
            &config.manifest.profile,
            &config.app_dir,
        )),
        None => {
            RegenPlan::Skip("mox_files not set and schemas_dir not set — skipping app regeneration")
        }
    }
}

/// Build the mox-mode `cargo run -p {graph_binary} -- run ...` argument
/// vector: one `--mox-files` flag per file in manifest order, then the
/// optional `--config`/`--profile`, then `--output`. No `--classifier` —
/// the mox pipeline needs none; a manifest that also sets `schemas_dir`
/// regenerates from mox (schemas_dir/classifier are ignored here).
fn generate_mox_args(
    graph_binary: &str,
    mox_files: &[String],
    domain_config: &Option<PathBuf>,
    profile: &Option<String>,
    app_dir: &Path,
) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "-p".to_string(),
        graph_binary.to_string(),
        "--".to_string(),
        "run".to_string(),
    ];
    for file in mox_files {
        args.push("--mox-files".to_string());
        args.push(file.clone());
    }
    if let Some(cfg) = domain_config {
        args.push("--config".to_string());
        args.push(cfg.to_string_lossy().into_owned());
    }
    if let Some(p) = profile {
        args.push("--profile".to_string());
        args.push(p.clone());
    }
    args.push("--output".to_string());
    args.push(app_dir.to_string_lossy().into_owned());
    args
}

/// Build the `cargo run -p {graph_binary} -- run ...` argument vector for
/// app regeneration. Only manifest flags whose values are `Some` are passed.
fn generate_args(
    graph_binary: &str,
    schemas_dir: &Path,
    classifier: &Option<PathBuf>,
    domain_config: &Option<PathBuf>,
    profile: &Option<String>,
    app_dir: &Path,
) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "-p".to_string(),
        graph_binary.to_string(),
        "--".to_string(),
        "run".to_string(),
        "--schemas".to_string(),
        schemas_dir.to_string_lossy().into_owned(),
    ];
    if let Some(c) = classifier {
        args.push("--classifier".to_string());
        args.push(c.to_string_lossy().into_owned());
    }
    if let Some(cfg) = domain_config {
        args.push("--config".to_string());
        args.push(cfg.to_string_lossy().into_owned());
    }
    if let Some(p) = profile {
        args.push("--profile".to_string());
        args.push(p.clone());
    }
    args.push("--output".to_string());
    args.push(app_dir.to_string_lossy().into_owned());
    args
}

/// Run `{command} {args}` in `dir`, returning stdout on success. On failure
/// returns `OpsError::TestFailure` with a tail of stdout+stderr (max
/// [`TAIL_CHARS`] chars).
fn run_step(command: &str, args: &[String], dir: &Path) -> OpsResult<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| {
            OpsError::Command(format!(
                "failed to spawn {command} in {}: {e}",
                dir.display()
            ))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let tail = tail(&format!("{stdout}\n{stderr}"), TAIL_CHARS);
        return Err(OpsError::TestFailure(format!(
            "{command} {args:?} failed in {} (exit {:?}):\n{tail}",
            dir.display(),
            output.status.code()
        )));
    }
    Ok(stdout.into_owned())
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

    #[test]
    fn generate_args_only_includes_some_flags() {
        let args = generate_args(
            "hr-graph",
            Path::new("schemas"),
            &Some(PathBuf::from("classifier.toml")),
            &None,
            &Some("default".to_string()),
            Path::new("generated-app"),
        );
        assert_eq!(args[0], "run");
        assert!(args.contains(&"hr-graph".to_string()));
        assert!(args.contains(&"classifier.toml".to_string()));
        assert!(!args.contains(&"--config".to_string()));
        assert!(args.contains(&"default".to_string()));
        assert!(args.contains(&"generated-app".to_string()));
    }

    fn quality_manifest() -> codegraph_config::OpsManifest {
        codegraph_config::OpsManifest {
            app_name: "demo-app".into(),
            graph_binary: Some("hr-graph".into()),
            schemas_dir: None,
            mox_files: Vec::new(),
            classifier: None,
            domain_config: None,
            profile: None,
            output_dir: "generated-app".into(),
            ui_dir: None,
            smoke: None,
            api_version: "v1".to_string(),
            servers: Default::default(),
            database: codegraph_config::OpsDatabase {
                api: codegraph_config::OpsDbTarget {
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
            capabilities: Default::default(),
            hurl: None,
            hooks: vec![],
            extensions: vec![],
        }
    }

    fn expect_run(plan: RegenPlan) -> Vec<String> {
        match plan {
            RegenPlan::Run(args) => args,
            RegenPlan::Skip(msg) => panic!("expected Run, got Skip({msg})"),
        }
    }

    fn expect_skip(plan: RegenPlan) -> &'static str {
        match plan {
            RegenPlan::Skip(msg) => msg,
            RegenPlan::Run(args) => panic!("expected Skip, got Run({args:?})"),
        }
    }

    #[test]
    fn generate_mox_args_exact_vector() {
        let args = generate_mox_args(
            "hr-graph",
            &[
                "model/common.mox".to_string(),
                "model/billing.mox".to_string(),
            ],
            &Some(PathBuf::from("domains.toml")),
            &Some("default".to_string()),
            Path::new("generated-app"),
        );
        assert_eq!(
            args,
            vec![
                "run",
                "-p",
                "hr-graph",
                "--",
                "run",
                "--mox-files",
                "model/common.mox",
                "--mox-files",
                "model/billing.mox",
                "--config",
                "domains.toml",
                "--profile",
                "default",
                "--output",
                "generated-app",
            ]
        );
    }

    #[test]
    fn generate_mox_args_without_optionals() {
        let args = generate_mox_args(
            "hr-graph",
            &["model/app.mox".to_string()],
            &None,
            &None,
            Path::new("out"),
        );
        assert!(!args.contains(&"--config".to_string()));
        assert!(!args.contains(&"--profile".to_string()));
        assert!(!args.contains(&"--classifier".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("out"));
    }

    #[test]
    fn regeneration_plan_mox_files_win_over_schemas_dir() {
        let mut manifest = quality_manifest();
        manifest.schemas_dir = Some("schemas".into());
        manifest.classifier = Some("classifier.toml".into());
        manifest.domain_config = Some("domains.toml".into());
        manifest.profile = Some("default".into());
        manifest.mox_files = vec!["model/common.mox".into(), "model/billing.mox".into()];
        let cfg = OpsConfig::from_manifest(manifest, PathBuf::from("/tmp/repo")).unwrap();
        let args = expect_run(regeneration_plan(&cfg));
        assert_eq!(
            args,
            vec![
                "run",
                "-p",
                "hr-graph",
                "--",
                "run",
                "--mox-files",
                "model/common.mox",
                "--mox-files",
                "model/billing.mox",
                "--config",
                "domains.toml",
                "--profile",
                "default",
                "--output",
                "/tmp/repo/generated-app",
            ]
        );
    }

    #[test]
    fn regeneration_plan_legacy_when_only_schemas_dir() {
        let mut manifest = quality_manifest();
        manifest.schemas_dir = Some("schemas".into());
        manifest.classifier = Some("classifier.toml".into());
        manifest.domain_config = Some("domains.toml".into());
        manifest.profile = Some("default".into());
        let cfg = OpsConfig::from_manifest(manifest, PathBuf::from("/tmp/repo")).unwrap();
        let args = expect_run(regeneration_plan(&cfg));
        assert_eq!(
            args,
            vec![
                "run",
                "-p",
                "hr-graph",
                "--",
                "run",
                "--schemas",
                "schemas",
                "--classifier",
                "classifier.toml",
                "--config",
                "domains.toml",
                "--profile",
                "default",
                "--output",
                "/tmp/repo/generated-app",
            ]
        );
    }

    #[test]
    fn regeneration_plan_skips_with_mox_hint_when_neither_source() {
        let manifest = quality_manifest();
        let cfg = OpsConfig::from_manifest(manifest, PathBuf::from("/tmp/repo")).unwrap();
        let msg = expect_skip(regeneration_plan(&cfg));
        assert!(msg.contains("mox_files not set"), "{msg}");
        assert!(msg.contains("schemas_dir not set"), "{msg}");
        assert!(msg.contains("skipping app regeneration"), "{msg}");
    }

    #[test]
    fn regeneration_plan_skips_without_graph_binary() {
        let mut manifest = quality_manifest();
        manifest.graph_binary = None;
        manifest.schemas_dir = Some("schemas".into());
        let cfg = OpsConfig::from_manifest(manifest, PathBuf::from("/tmp/repo")).unwrap();
        let msg = expect_skip(regeneration_plan(&cfg));
        assert!(msg.contains("graph_binary not set"), "{msg}");
    }

    #[test]
    fn generate_args_without_optionals() {
        let args = generate_args(
            "hr-graph",
            Path::new("schemas"),
            &None,
            &None,
            &None,
            Path::new("out"),
        );
        assert!(!args.contains(&"--classifier".to_string()));
        assert!(!args.contains(&"--config".to_string()));
        assert!(!args.contains(&"--profile".to_string()));
        assert!(args.contains(&"--output".to_string()));
    }

    #[test]
    fn run_step_ok_returns_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let out = run_step(
            "sh",
            &["-c".to_string(), "printf hello".to_string()],
            dir.path(),
        )
        .unwrap();
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn run_step_nonzero_is_test_failure_with_tail() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_step(
            "sh",
            &["-c".to_string(), "echo boom >&2; exit 3".to_string()],
            dir.path(),
        )
        .unwrap_err();
        match err {
            OpsError::TestFailure(msg) => {
                assert!(msg.contains("boom"), "tail should include stderr: {msg}");
                assert!(msg.contains("exit 3"), "should include exit code: {msg}");
            }
            other => panic!("expected TestFailure, got {other:?}"),
        }
    }

    #[test]
    fn run_step_missing_binary_is_err_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(run_step("definitely-not-a-real-cg-binary", &[], dir.path()).is_err());
    }

    #[test]
    fn tail_truncates_to_max() {
        let long = "a".repeat(1000);
        let t = tail(&long, 800);
        assert!(t.contains("[truncated 200 chars]"), "{t}");
        assert!(t.ends_with(&"a".repeat(800)));
        let short = "short";
        assert_eq!(tail(short, 800), short);
    }
}
