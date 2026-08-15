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
///    (skipped with a warning when `graph_binary`/`schemas_dir` are unset;
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
        &config.root_dir,
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
        &config.root_dir,
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
        &config.root_dir,
    )?;

    output::section("4/5 regenerate generated app");
    match (&config.manifest.graph_binary, &config.manifest.schemas_dir) {
        (Some(binary), Some(schemas)) => {
            let args = generate_args(
                binary,
                schemas,
                &config.manifest.classifier,
                &config.manifest.domain_config,
                &config.manifest.profile,
                &config.app_dir,
            );
            output::info(format!("cargo {args:?}"));
            run_step("cargo", &args, &config.root_dir)?;
        }
        (None, _) => output::warn("graph_binary not set — skipping app regeneration"),
        (_, None) => output::warn("schemas_dir not set — skipping app regeneration"),
    }

    output::section("5/5 cargo check (generated app)");
    run_step("cargo", &["check".to_string()], &config.app_dir)?;

    for name in extra {
        output::section(format!("extra gate: cargo {name} --workspace"));
        run_step(
            "cargo",
            &[name.clone(), "--workspace".to_string()],
            &config.root_dir,
        )?;
    }

    output::ok("=== All checks passed ===");
    Ok(())
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
