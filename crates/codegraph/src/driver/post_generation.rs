//! Post-generation steps: the profile plan's scripts and the opt-in
//! `--check` compile gate (issue #336).

use std::path::{Path, PathBuf};

use crate::error::Result;

/// Run the profile plan's post-generation scripts (issue #336 era): one
/// `sh -c` per script, streamed, first failure aborts the run.
pub(super) async fn run_post_gen_scripts(
    build_plan: Option<&crate::profile::BuildPlan>,
    no_post_gen: bool,
) -> Result<()> {
    // Run post-generation scripts from the profile plan.
    if let Some(plan) = build_plan
        && !no_post_gen
        && !plan.post_gen_scripts.is_empty()
    {
        println!("\nPost-generation scripts:");
        for (section, scripts) in &plan.post_gen_scripts {
            for cmd in scripts {
                println!("  [{section}] {cmd}");
                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .status()
                    .map_err(|e| {
                        crate::error::Error::Config(format!(
                            "failed to run post_gen script [{section}] {cmd}: {e}"
                        ))
                    })?;
                if !status.success() {
                    let code = status.code().unwrap_or(-1);
                    return Err(crate::error::Error::Config(format!(
                        "post_gen script [{section}] {cmd} failed (exit {code})"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// How many trailing `cargo check` output lines a gate failure carries.
const CHECK_FAILURE_TAIL_LINES: usize = 50;

/// Post-generate compile gate (issue #336): `cargo check --manifest-path
/// <output>/Cargo.toml` so a run that emitted non-compiling code exits
/// non-zero instead of leaving the failure to the consumer's first build.
///
/// Output streams live: every cargo line prints as it arrives (`[check]`
/// prefix, stderr + stdout); on failure the last
/// [`CHECK_FAILURE_TAIL_LINES`] lines ride the returned error so the run
/// exits non-zero with the rustc errors visible. Degrades to a WARN + skip
/// (never fails the run) when `cargo` is absent or the output carries no
/// `Cargo.toml` (e.g. UI-only generation).
pub fn check_generated_output(output: &Path) -> Result<()> {
    let manifest_path = output.join("Cargo.toml");
    if !manifest_path.exists() {
        eprintln!(
            "WARN: --check requested but no Cargo.toml at {} — compile gate skipped",
            manifest_path.display()
        );
        return Ok(());
    }
    let cargo = match find_cargo() {
        Some(cargo) => cargo,
        None => {
            eprintln!(
                "WARN: --check requested but `cargo` was not found ($CARGO unset, not on $PATH) — compile gate skipped"
            );
            return Ok(());
        }
    };
    println!(
        "Compile gate: cargo check --manifest-path {}",
        manifest_path.display()
    );

    let mut child = std::process::Command::new(&cargo)
        .arg("check")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            crate::error::Error::Config(format!(
                "failed to spawn cargo check ({}): {e}",
                cargo.display()
            ))
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        crate::error::Error::Config("cargo check stdout not captured".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        crate::error::Error::Config("cargo check stderr not captured".to_string())
    })?;

    // Pump stderr on a thread so both streams print live as they arrive;
    // stdout drains inline on the calling thread.
    use std::io::BufRead as _;
    let stderr_thread = std::thread::spawn(move || {
        let mut collected = String::new();
        for line in std::io::BufReader::new(stderr)
            .lines()
            .map_while(std::result::Result::ok)
        {
            eprintln!("[check] {line}");
            collected.push_str(&line);
            collected.push('\n');
        }
        collected
    });
    let mut stdout_collected = String::new();
    for line in std::io::BufReader::new(stdout)
        .lines()
        .map_while(std::result::Result::ok)
    {
        println!("[check] {line}");
        stdout_collected.push_str(&line);
        stdout_collected.push('\n');
    }
    let stderr_collected = stderr_thread.join().unwrap_or_default();

    let status = child
        .wait()
        .map_err(|e| crate::error::Error::Config(format!("failed to wait for cargo check: {e}")))?;
    if !status.success() {
        let code = status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        let combined = format!("{stdout_collected}{stderr_collected}");
        let tail = last_lines(&combined, CHECK_FAILURE_TAIL_LINES);
        return Err(crate::error::Error::Config(format!(
            "compile gate failed: cargo check exited with {code} — last {} lines:\n{tail}",
            CHECK_FAILURE_TAIL_LINES
        )));
    }
    println!("Compile gate passed.");
    Ok(())
}

/// Resolve the cargo binary: `$CARGO` when set (also how `cargo test`
/// environments expose it), else a `$PATH` scan. `None` = unavailable (the
/// gate degrades to a WARN + skip).
fn find_cargo() -> Option<PathBuf> {
    if let Some(cargo) = std::env::var_os("CARGO").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(cargo));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("cargo"))
        .find(|candidate| candidate.is_file())
}

/// Last `max` lines of `s` (line-based, UTF-8 safe).
fn last_lines(s: &str, max: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(max);
    lines[start..].join("\n")
}
