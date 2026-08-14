//! Process supervision: spawn managed children with log capture and graceful
//! shutdown (SIGTERM → wait → SIGKILL), mirroring the bash `graceful_kill` +
//! `trap cleanup EXIT` pattern.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Child;

use crate::error::OpsResult;
use crate::output::{info, ok, warn};

/// Outcome of a graceful shutdown attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum ShutdownOutcome {
    /// Process had already exited before shutdown was attempted.
    AlreadyExited,
    /// Process exited after SIGTERM within the grace period.
    Graceful { seconds: u64 },
    /// Process ignored SIGTERM and had to be SIGKILLed.
    ForceKilled { seconds: u64 },
}

/// A spawned child whose stdout+stderr are redirected to a log file and which
/// can be shut down gracefully. Killing on drop guarantees no orphaned
/// processes even on early returns.
pub struct ManagedProcess {
    pub label: String,
    pub log_path: PathBuf,
    child: Option<Child>,
}

impl ManagedProcess {
    /// Spawn `cmd` with stdout+stderr redirected to `log_path` (create/truncate).
    /// stdin null. Returns Err on spawn failure.
    #[allow(unused_mut)]
    pub fn spawn(mut cmd: std::process::Command, label: &str, log_path: &Path) -> OpsResult<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(log_path)?;
        let mut child_cmd = tokio::process::Command::from(cmd);
        child_cmd
            .stdin(Stdio::null())
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        let child = child_cmd.spawn()?;
        Ok(Self {
            label: label.to_string(),
            log_path: log_path.to_path_buf(),
            child: Some(child),
        })
    }

    /// True if the process is still running (checks try_wait).
    pub fn alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Send SIGTERM (via libc::kill on unix), poll child.try_wait() every second
    /// up to `timeout_secs`; on timeout send SIGKILL and wait up to 5 more secs.
    /// Returns ShutdownOutcome describing what happened.
    pub async fn graceful_shutdown(&mut self, timeout_secs: u64) -> ShutdownOutcome {
        let Some(child) = self.child.as_mut() else {
            return ShutdownOutcome::AlreadyExited;
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            self.child = None;
            return ShutdownOutcome::AlreadyExited;
        }
        let pid = child.id().map(|p| p.to_string()).unwrap_or_default();
        #[cfg(unix)]
        send_signal(child, term_signal());
        let grace_secs = if cfg!(unix) { timeout_secs } else { 0 };
        let mut waited: u64 = 0;
        while waited < grace_secs {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            waited += 1;
            if reaped(&mut self.child) {
                ok(format!("{} shut down gracefully after {}s", self.label, waited));
                self.child = None;
                return ShutdownOutcome::Graceful { seconds: waited };
            }
        }
        warn(format!(
            "{} did not shut down within {}s, sending SIGKILL",
            self.label, timeout_secs
        ));
        let Some(child) = self.child.as_mut() else {
            return ShutdownOutcome::AlreadyExited;
        };
        send_signal(child, kill_signal());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if reaped(&mut self.child) {
                warn(format!("{} force-killed after {}s", self.label, timeout_secs));
                self.child = None;
                return ShutdownOutcome::ForceKilled { seconds: timeout_secs };
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        warn(format!("{} (pid {pid}) could not be killed", self.label));
        self.child = None;
        ShutdownOutcome::ForceKilled { seconds: timeout_secs }
    }
}

impl Drop for ManagedProcess {
    /// If still alive, SIGKILL it (best-effort, ignore errors).
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        send_signal(child, kill_signal());
        let _ = child.try_wait();
    }
}

/// RAII guard equivalent of `trap cleanup EXIT` — kills all managed processes
/// unless keep=true.
pub struct Supervisor {
    keep: bool,
    procs: Vec<ManagedProcess>,
}

impl Supervisor {
    pub fn new(keep: bool) -> Self {
        Self {
            keep,
            procs: Vec::new(),
        }
    }

    pub fn add(&mut self, proc: ManagedProcess) {
        self.procs.push(proc);
    }

    /// Remove and return all managed processes (for manual shutdown).
    pub fn take_all(&mut self) -> Vec<ManagedProcess> {
        std::mem::take(&mut self.procs)
    }

    /// Shutdown all managed processes (graceful, 10s timeout each). With
    /// keep=true the processes are intentionally leaked so they survive the
    /// guard, matching `bash --keep`.
    pub async fn shutdown_all(&mut self) {
        if self.keep {
            info("Services still running (--keep)");
            std::mem::forget(std::mem::take(&mut self.procs));
            return;
        }
        let procs = std::mem::take(&mut self.procs);
        for mut proc in procs {
            proc.graceful_shutdown(10).await;
        }
        info("Services stopped");
    }
}

fn reaped(child: &mut Option<Child>) -> bool {
    match child.as_mut() {
        Some(child) => matches!(child.try_wait(), Ok(Some(_))),
        None => true,
    }
}

#[cfg(unix)]
fn send_signal(child: &Child, sig: i32) {
    let Some(pid) = child.id() else {
        return;
    };
    let pid = pid as i32;
    if pid > 0 {
        unsafe {
            libc::kill(pid, sig);
        }
    }
}

#[cfg(not(unix))]
fn send_signal(child: &Child, _sig: i32) {
    let _ = child.start_kill();
}

#[cfg(unix)]
fn term_signal() -> i32 {
    libc::SIGTERM
}

#[cfg(not(unix))]
fn term_signal() -> i32 {
    0
}

#[cfg(unix)]
fn kill_signal() -> i32 {
    libc::SIGKILL
}

#[cfg(not(unix))]
fn kill_signal() -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn graceful_shutdown_terminates_process() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("server.log");
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30");
        let mut proc = ManagedProcess::spawn(cmd, "sleep-test", &log_path).unwrap();
        assert!(proc.alive());
        assert!(log_path.is_file());
        let pid = proc.child.as_ref().unwrap().id().unwrap();
        let outcome = proc.graceful_shutdown(2).await;
        assert!(
            matches!(
                outcome,
                ShutdownOutcome::Graceful { .. } | ShutdownOutcome::ForceKilled { .. }
            ),
            "unexpected outcome: {outcome:?}"
        );
        assert!(!proc.alive());
        assert_ne!(unsafe { libc::kill(pid as i32, 0) }, 0, "pid {pid} still alive");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn drop_kills_process() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("server.log");
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30");
        let proc = ManagedProcess::spawn(cmd, "sleep-test", &log_path).unwrap();
        let pid = proc.child.as_ref().unwrap().id().unwrap();
        drop(proc);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut gone = false;
        while std::time::Instant::now() < deadline {
            if unsafe { libc::kill(pid as i32, 0) } != 0 {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(gone, "pid {pid} still alive after drop");
    }
}
