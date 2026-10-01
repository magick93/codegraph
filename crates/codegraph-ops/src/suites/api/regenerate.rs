//! Regeneration, build and check helpers for the API suite.

use std::process::Command;

use crate::config::OpsConfig;
use crate::error::{OpsError, OpsResult};
use crate::proc::run_streaming;

use super::capture::tail_lines;

/// Build the `cargo run -p {graph} -- run ...` argument vector for the regen
/// stage. Only manifest flags whose values are `Some` are passed — including
/// the manifest `profile`, without which regeneration could never exercise a
/// non-default provider.
pub(super) fn regenerate_args(config: &OpsConfig, graph_binary: &str) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "-p".to_string(),
        graph_binary.to_string(),
        "--".to_string(),
        "run".to_string(),
    ];
    if let Some(schemas) = &config.manifest.schemas_dir {
        args.push("--schemas".to_string());
        args.push(schemas.to_string_lossy().into_owned());
    }
    if let Some(classifier) = &config.manifest.classifier {
        args.push("--classifier".to_string());
        args.push(classifier.to_string_lossy().into_owned());
    }
    if let Some(cfg) = &config.manifest.domain_config {
        args.push("--config".to_string());
        args.push(cfg.to_string_lossy().into_owned());
    }
    if let Some(profile) = &config.manifest.profile {
        args.push("--profile".to_string());
        args.push(profile.clone());
    }
    args.push("--output".to_string());
    args.push(config.app_dir.to_string_lossy().into_owned());
    args
}

/// Regenerate the app via the graph binary; returns captured combined output.
///
/// The child's exit status is the success signal — a substring scan used to
/// stand in for it and reported SIGKILLed/panicked regens as "✓ Templates
/// regenerated" (no literal "error" anywhere) while "0 errors" read as a
/// failure. On non-zero exit the error carries the output tail.
/// Fail when the generation report carries errors ("Generated N files |
/// E errors | …" and/or the driver's "completed with errors" notice). Silent
/// template breakage otherwise shrinks the generated tree — and the test
/// coverage — without anyone noticing.
pub(super) fn assert_generation_clean(gen_output: &str) -> OpsResult<()> {
    let errored = gen_output.contains("Generation completed with errors")
        || regex_free_has_error_count(gen_output);
    if errored {
        return Err(OpsError::TestFailure(
            "generation reported errors (entities were skipped) — fix the templates or pass \
             --allow-gen-errors"
                .into(),
        ));
    }
    Ok(())
}

/// True when a "Generated N files | E errors | W warnings" summary line shows
/// a non-zero error count.
pub(super) fn regex_free_has_error_count(gen_output: &str) -> bool {
    for line in gen_output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Generated ") {
            for segment in rest.split('|') {
                let segment = segment.trim();
                if let Some(count) = segment.strip_suffix("errors") {
                    let count = count.trim();
                    if count.parse::<u32>().map(|c| c > 0).unwrap_or(false) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub(super) fn regenerate(
    config: &OpsConfig,
    graph_binary: &str,
    release: bool,
) -> OpsResult<String> {
    let mut args = regenerate_args(config, graph_binary);
    if release {
        // Build/run the graph binary in the requested profile; a stale debug
        // binary otherwise silently regenerates with old generator code.
        args.insert(1, "--release".to_string());
    }
    let mut cmd = Command::new("cargo");
    cmd.args(&args).current_dir(&config.root_dir);
    // Streams live under [generate] — a 56-minute regen is never silent.
    let out = run_streaming(&mut cmd, "generate")?;
    let combined = out.captured;
    if !out.status.success() {
        return Err(OpsError::Command(format!(
            "`cargo run -p {graph_binary} -- run` failed with {}: \n{}",
            out.status,
            tail_lines(&combined, 20)
        )));
    }
    Ok(combined)
}

/// `cargo build` inside the generated app. Exports `CORNUCOPIA_DATABASE_URL`
/// for the cornucopia provider (its `build.rs` connects to Postgres at build
/// time). `Ok` only on a clean exit; `Err` carries the output tail.
pub(in crate::suites) fn cargo_build_in(config: &OpsConfig, release: bool) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    if release {
        cmd.arg("--release");
    }
    if let Some((key, value)) = cornucopia_db_env(config) {
        cmd.env(key, value);
    }
    // Build in the generated app dir (the app resolves its own workspace).
    cmd.current_dir(&config.app_dir);
    match run_streaming(&mut cmd, "build") {
        Ok(out) if out.status.success() => {
            // A successful build is a freshness statement: pre_generate
            // clean hooks may have wiped + fully regenerated src (fresh
            // mtimes on byte-identical files), and cargo skips the relink
            // when nothing changed — leaving the binary's mtime older than
            // src even though the bytes match. Touch it so mtime-based
            // freshness checks reflect the build that just succeeded.
            let binary = config
                .app_dir
                .join("target")
                .join(if release { "release" } else { "debug" })
                .join(config.app_binary_name());
            if binary.is_file() {
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&binary)
                    .and_then(|f| {
                        f.set_times(
                            std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()),
                        )
                    });
            }
            Ok(())
        }
        Ok(out) => Err(tail_lines(&out.captured, 20)),
        Err(e) => Err(format!("failed to spawn cargo: {e}")),
    }
}

/// `cargo check` inside the generated app. `Ok` only when the check exits
/// successfully without a `^error` diagnostic; `Err` carries the output tail.
pub(super) fn cargo_check_in(config: &OpsConfig) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("check").current_dir(&config.app_dir);
    if let Some((key, value)) = cornucopia_db_env(config) {
        cmd.env(key, value);
    }
    match run_streaming(&mut cmd, "check") {
        Ok(out) => {
            if out.status.success() && !out.captured.contains("^error") {
                Ok(())
            } else {
                Err(tail_lines(&out.captured, 20))
            }
        }
        Err(e) => Err(format!("failed to spawn cargo: {e}")),
    }
}

/// Build/runtime env for the cornucopia persistence provider: the generated
/// app's `cornucopia-queries/build.rs` connects to Postgres at BUILD time via
/// `CORNUCOPIA_DATABASE_URL`, so every cargo invocation touching the app
/// workspace and every app-binary spawn must carry it. Returns `None` (sets
/// nothing) for other providers — harmless for sea_orm.
/// Build-time env for the cornucopia persistence provider: its `build.rs`
/// connects to Postgres to compile the SQL-first repositories. `(key, value)`
/// is empty when the provider isn't cornucopia.
pub(super) fn cornucopia_db_env(config: &OpsConfig) -> Option<(String, String)> {
    if config.manifest.capabilities.persistence_provider == "cornucopia" {
        Some(("CORNUCOPIA_DATABASE_URL".to_string(), config.api_db.url()))
    } else {
        None
    }
}

/// Whether the release binary is the one to use.
pub(super) fn is_release_binary(config: &OpsConfig) -> bool {
    let release = config
        .app_dir
        .join("target/release")
        .join(config.app_binary_name());
    release.is_file()
}
