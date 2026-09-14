//! Build + boot the generated axum server for full-stack integration tests:
//! `cargo build` with an isolated `CARGO_TARGET_DIR`, a `ManagedProcess`
//! spawn, and a `/health` readiness wait.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use codegraph_ops::proc::ManagedProcess;

use super::process;

/// A booted server: keep alive until dropped, expose its bound port.
pub struct BootedServer {
    _proc: ManagedProcess,
    pub port: u16,
}

/// `cargo build --manifest-path <manifest>` run from `workspace_root` with
/// an isolated `CARGO_TARGET_DIR` so generated-app builds never contend with
/// the host workspace's lock. Returns the built binary path.
pub fn build_app(
    manifest: &Path,
    workspace_root: &Path,
    target_dir: &Path,
    logs_dir: &Path,
    bin_name: &str,
) -> Result<PathBuf, String> {
    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
    let target_dir = target_dir.to_string_lossy().to_string();
    let manifest = manifest.to_string_lossy().to_string();
    let ok = process::run(
        workspace_root,
        "cargo",
        &["build", "--manifest-path", &manifest],
        &[("CARGO_TARGET_DIR", target_dir.as_str())],
        Duration::from_secs(1200),
        logs_dir,
    )?;
    if !ok {
        return Err(format!(
            "cargo build of the generated app failed (see {}/cargo-build.log)",
            logs_dir.display()
        ));
    }
    let bin = Path::new(&target_dir).join("debug").join(bin_name);
    if !bin.is_file() {
        return Err(format!("expected server binary at {}", bin.display()));
    }
    Ok(bin)
}

/// Spawn `bin` (with `envs` plus an auto-allocated `BIND_ADDR`) as a managed
/// process and wait up to 60s for `/health`. On failure the server log tail
/// is included in the error.
pub async fn boot_server(
    bin: &Path,
    envs: &[(&str, &str)],
    name: &str,
    logs_dir: &Path,
) -> Result<BootedServer, String> {
    let port = process::free_port();
    let log = logs_dir.join(format!("{name}.log"));
    let mut cmd = Command::new(bin);
    cmd.env("BIND_ADDR", format!("127.0.0.1:{port}"))
        .envs(envs.iter().copied());
    let mut proc =
        ManagedProcess::spawn(cmd, name, &log).map_err(|e| format!("spawn server: {e}"))?;

    let url = format!("http://127.0.0.1:{port}/health");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if !proc.alive() {
            let tail = fs::read_to_string(&log).unwrap_or_default();
            let tail: String = tail.lines().rev().take(30).collect::<Vec<_>>().join("\n");
            return Err(format!(
                "server exited before /health became ready. log tail:\n{tail}"
            ));
        }
        let reachable = tokio::process::Command::new("curl")
            .args(["-sf", "--max-time", "2", &url])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if reachable {
            return Ok(BootedServer { _proc: proc, port });
        }
        if Instant::now() >= deadline {
            let tail = fs::read_to_string(&log).unwrap_or_default();
            let tail: String = tail.lines().rev().take(30).collect::<Vec<_>>().join("\n");
            proc.graceful_shutdown(2).await;
            return Err(format!(
                "server /health not ready within 60s. log tail:\n{tail}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
