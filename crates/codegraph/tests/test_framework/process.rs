//! Low-level process helpers for full-stack integration tests: run commands
//! with timeouts and per-command log files, capture combined output, and
//! grab free TCP ports.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// True when `<name> --version` exits successfully.
pub fn have_tool(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run `program args` in `cwd` with `envs`, streaming stdout+stderr into a
/// per-command log file under `logs_dir`. Kills the child after `timeout`.
pub fn run(
    cwd: &Path,
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
    logs_dir: &Path,
) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path_for(logs_dir, program, args))
        .map_err(|e| e.to_string())?;
    let log_file2 = log_file.try_clone().map_err(|e| e.to_string())?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .envs(envs.iter().copied())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(log_file2))
        .spawn()
        .map_err(|e| format!("spawn {program}: {e}"))?;
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => return Ok(status.success()),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                return Err(format!(
                    "{program} {} timed out after {}s (log: {})",
                    args.join(" "),
                    timeout.as_secs(),
                    log_path_for(logs_dir, program, args).display()
                ));
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Run `program args` in `cwd`, returning (success, combined stdout+stderr).
pub fn capture(cwd: &Path, program: &str, args: &[&str]) -> Result<(bool, String), String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("spawn {program}: {e}"))?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok((out.status.success(), combined))
}

/// Deterministic per-command log path: `{program}-{first_arg}.log` under
/// `logs_dir`, with path-hostile characters flattened.
pub fn log_path_for(logs_dir: &Path, program: &str, args: &[&str]) -> PathBuf {
    let tag = format!(
        "{}-{}",
        Path::new(program)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(program),
        args.first().copied().unwrap_or("run")
    )
    .replace(['/', ':'], "_");
    let _ = fs::create_dir_all(logs_dir);
    logs_dir.join(format!("{tag}.log"))
}

/// Bind an ephemeral TCP port on 127.0.0.1 and return it.
pub fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}
