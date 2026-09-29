//! Process supervision: spawn managed children with log capture and graceful
//! shutdown (SIGTERM → wait → SIGKILL), mirroring the bash `graceful_kill` +
//! `trap cleanup EXIT` pattern. Also the streaming one-shot runner
//! ([`run_streaming`]) that replaces every silent `Command::output()` spawn.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};

use tokio::process::Child;

use crate::error::{OpsError, OpsResult};
use crate::output::{self, info, ok, warn};

/// Trailing combined-output lines kept for failure contexts.
pub const TAIL_LINES: usize = 50;

/// Outcome of a streamed one-shot command.
#[derive(Debug)]
pub struct RunOutput {
    /// Process exit status. Spawn succeeded — check [`RunOutput::success`]
    /// yourself; parsers often need the output of FAILED runs too (hurl
    /// excerpts, Playwright tallies).
    pub status: ExitStatus,
    /// Last [`TAIL_LINES`] lines of combined stdout+stderr — the failure
    /// context callers attach to errors (always, never verbose-gated). Raw
    /// child lines, same shape as [`RunOutput::captured`].
    pub tail: String,
    /// Full combined stdout+stderr with RAW child lines (no label prefix), so
    /// the post-run parsers (generated-files scan, hurl summaries, Playwright
    /// tallies) keep matching exactly what the child printed.
    pub captured: String,
}

impl RunOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }
}

/// Stream one command to completion: each stdout/stderr line is forwarded to
/// the harness output prefixed with `label` (e.g. `[build] Compiling foo`)
/// AND captured. `captured` feeds the usual post-run parsers; `tail` holds
/// the last [`TAIL_LINES`] lines for failure contexts. Non-UTF8 output is
/// lossy-decoded. Blocking (std::thread readers) — drop-in replacement for
/// the `Command::output()` spawns it replaces.
pub fn run_streaming(cmd: &mut Command, label: &str) -> OpsResult<RunOutput> {
    run_streaming_inner(cmd, label, false)
}

/// Quiet variant for stages whose output is pure noise at default verbosity
/// (rustc dependency-compilation spam, browser downloads): capture only, no
/// per-line echo — unless `--verbose` is set. The stage-start/duration lines
/// around the call (caller info lines, `Metrics::begin`/`end`) still print.
pub fn run_streaming_quiet(cmd: &mut Command, label: &str) -> OpsResult<RunOutput> {
    run_streaming_inner(cmd, label, true)
}

/// Per-line echo policy: quiet stages stay silent at default verbosity and
/// stream when `--verbose` is on; loud stages always stream.
fn should_echo(quiet: bool) -> bool {
    !quiet || output::is_verbose()
}

/// `[label] line` — the streaming prefix keeping interleaved stage output
/// attributable to its stage.
fn prefix_line(label: &str, line: &str) -> String {
    format!("[{label}] {line}")
}

/// Human-readable `{program} {args…}` for stage-start lines and spawn errors.
fn command_repr(cmd: &Command) -> String {
    let mut parts = vec![cmd.get_program().to_string_lossy().into_owned()];
    parts.extend(cmd.get_args().map(|a| a.to_string_lossy().into_owned()));
    parts.join(" ")
}

struct Capture {
    lines: Vec<String>,
    ring: VecDeque<String>,
}

fn run_streaming_inner(cmd: &mut Command, label: &str, quiet: bool) -> OpsResult<RunOutput> {
    let cmdline = command_repr(cmd);
    let echo = should_echo(quiet);
    if !quiet {
        output::stream_start(&prefix_line(label, &format!("$ {cmdline}")));
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| OpsError::Command(format!("failed to spawn {label} (`{cmdline}`): {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| OpsError::Command(format!("{label}: stdout pipe missing")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| OpsError::Command(format!("{label}: stderr pipe missing")))?;
    let capture = Arc::new(Mutex::new(Capture {
        lines: Vec::new(),
        ring: VecDeque::new(),
    }));
    let out_handle = drain(stdout, label.to_string(), echo, Arc::clone(&capture));
    let err_handle = drain(stderr, label.to_string(), echo, Arc::clone(&capture));
    let status = child.wait()?;
    let _ = out_handle.join();
    let _ = err_handle.join();
    let cap = capture.lock().expect("stream capture poisoned");
    Ok(RunOutput {
        status,
        tail: cap
            .ring
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
        captured: cap.lines.join("\n"),
    })
}

/// Read `stream` to EOF line by line: capture the RAW line (parsers depend on
/// the child's own output shape), ring-buffer the last [`TAIL_LINES`] for
/// failure tails, and echo it label-prefixed when `echo`.
fn drain<R: Read + Send + 'static>(
    stream: R,
    label: String,
    echo: bool,
    capture: Arc<Mutex<Capture>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let mut line = String::from_utf8_lossy(&bytes).into_owned();
                    while line.ends_with('\n') || line.ends_with('\r') {
                        line.pop();
                    }
                    let prefixed = prefix_line(&label, &line);
                    {
                        let mut cap = capture.lock().expect("stream capture poisoned");
                        cap.lines.push(line.clone());
                        if cap.ring.len() >= TAIL_LINES {
                            cap.ring.pop_front();
                        }
                        cap.ring.push_back(line);
                    }
                    if echo {
                        output::stream_line(&prefixed);
                    }
                }
            }
        }
    })
}

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
                ok(format!(
                    "{} shut down gracefully after {}s",
                    self.label, waited
                ));
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
                warn(format!(
                    "{} force-killed after {}s",
                    self.label, timeout_secs
                ));
                self.child = None;
                return ShutdownOutcome::ForceKilled {
                    seconds: timeout_secs,
                };
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        warn(format!("{} (pid {pid}) could not be killed", self.label));
        self.child = None;
        ShutdownOutcome::ForceKilled {
            seconds: timeout_secs,
        }
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

    #[test]
    fn prefix_line_formats_label() {
        assert_eq!(
            prefix_line("build", "Compiling foo"),
            "[build] Compiling foo"
        );
        assert_eq!(prefix_line("hook:pgmq", "patched"), "[hook:pgmq] patched");
    }

    #[test]
    fn should_echo_follows_verbose_policy() {
        // Loud stages always stream; quiet stages only under --verbose.
        assert!(should_echo(false));
        assert_eq!(should_echo(true), output::is_verbose());
    }

    #[cfg(unix)]
    #[test]
    fn run_streaming_captures_stdout_and_stderr_raw() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo hello; echo world >&2"]);
        let out = run_streaming(&mut cmd, "build").unwrap();
        assert!(out.success());
        assert!(out.captured.contains("hello"), "{:?}", out.captured);
        assert!(out.captured.contains("world"), "{:?}", out.captured);
        // Raw lines: parsers strip child-shaped prefixes, never `[label]`.
        assert!(
            out.captured.lines().all(|l| !l.starts_with('[')),
            "{:?}",
            out.captured
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_streaming_ring_buffer_caps_tail() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "for i in $(seq 1 200); do echo line$i; done"]);
        let out = run_streaming(&mut cmd, "gen").unwrap();
        assert_eq!(out.captured.lines().count(), 200);
        assert!(
            out.tail.lines().count() <= TAIL_LINES,
            "tail kept {} lines",
            out.tail.lines().count()
        );
        // Exactly the LAST TAIL_LINES lines survive.
        assert!(out.tail.lines().next().unwrap_or_default() == "line151");
        assert!(out.tail.lines().last().unwrap_or_default() == "line200");
        assert!(!out.tail.contains("line150"));
    }

    #[cfg(unix)]
    #[test]
    fn run_streaming_lossy_decodes_non_utf8() {
        let mut cmd = Command::new("sh");
        // Octal escapes: POSIX printf has no \x escapes.
        cmd.args(["-c", "printf 'caf\\351 \\377\\n'"]);
        let out = run_streaming(&mut cmd, "bin").unwrap();
        assert!(out.captured.contains("caf"), "{:?}", out.captured);
        assert!(
            out.captured.contains('\u{FFFD}'),
            "invalid bytes must lossy-decode: {:?}",
            out.captured
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_streaming_failure_keeps_tail_and_status() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo boom; exit 3"]);
        let out = run_streaming(&mut cmd, "hook:x").unwrap();
        assert!(!out.success());
        assert_eq!(out.status.code(), Some(3));
        // Failure tails stay available for unconditional attachment.
        assert!(out.tail.contains("boom"), "{:?}", out.tail);
        assert!(out.captured.contains("boom"));
    }

    #[cfg(unix)]
    #[test]
    fn run_streaming_quiet_still_captures() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo noisy-compilation"]);
        let out = run_streaming_quiet(&mut cmd, "install").unwrap();
        assert!(out.success());
        assert_eq!(out.captured.trim(), "noisy-compilation");
        assert!(out.tail.contains("noisy-compilation"));
    }

    #[test]
    fn run_streaming_missing_binary_is_command_error() {
        let mut cmd = Command::new("definitely-not-a-real-cg-binary-xyz");
        let err = run_streaming(&mut cmd, "x").unwrap_err();
        assert!(matches!(err, OpsError::Command(_)), "got {err:?}");
        assert!(err
            .to_string()
            .contains("definitely-not-a-real-cg-binary-xyz"));
    }

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
        assert_ne!(
            unsafe { libc::kill(pid as i32, 0) },
            0,
            "pid {pid} still alive"
        );
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
