//! Advisory services registry: `.testkit/services.json` under the manifest
//! root (`config.root_dir`).
//!
//! Whenever the harness leaves a server running (api server, e2e app server,
//! vite preview), it records `{name, pid, port, health, started_at, profile,
//! suite}` here. `clean` kills by registry pid (with the `fuser -k` sweep as
//! the fallback for unknown orphans), and the port preflight consults the
//! registry to offer takeover (default) or `--reuse` for a prior server.
//!
//! The registry is ADVISORY by contract: a stale, missing, or corrupt file
//! must never break a run. Reads are best-effort (corrupt JSON → empty +
//! warning), dead pids are pruned via `kill(pid, 0)`, and every mutating
//! helper tolerates failure (callers warn at most).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::OpsResult;
use crate::output;

/// Registry lives at `{root_dir}/.testkit/services.json`.
const REGISTRY_DIR: &str = ".testkit";
const REGISTRY_FILE: &str = "services.json";

/// One registered service left running by a previous harness run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// Logical service name ("api", "ui").
    pub name: String,
    pub pid: u32,
    pub port: u16,
    /// Health path probed for readiness ("/health"), when the service has one.
    #[serde(default)]
    pub health: Option<String>,
    /// RFC 3339 start timestamp.
    pub started_at: String,
    /// Build profile the server was started with ("release"/"debug").
    #[serde(default)]
    pub profile: Option<String>,
    /// Suite that left the service running ("api", "e2e", "ui").
    #[serde(default)]
    pub suite: Option<String>,
}

/// The whole registry document: `{"services": [...]}`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRegistry {
    #[serde(default)]
    pub services: Vec<ServiceEntry>,
}

impl ServiceRegistry {
    /// Path of the registry file for a manifest root.
    pub fn path(root_dir: &Path) -> PathBuf {
        root_dir.join(REGISTRY_DIR).join(REGISTRY_FILE)
    }

    /// Best-effort load. Missing file → empty registry; corrupt JSON →
    /// warning + empty registry (advisory contract: never break a run).
    pub fn load(root_dir: &Path) -> Self {
        let path = Self::path(root_dir);
        match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                output::warn(format!(
                    "could not read services registry {}: {e}",
                    path.display()
                ));
                Self::default()
            }
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(registry) => registry,
                Err(e) => {
                    output::warn(format!(
                        "ignoring corrupt services registry {}: {e}",
                        path.display()
                    ));
                    Self::default()
                }
            },
        }
    }

    /// Persist (create `.testkit/` as needed).
    pub fn save(&self, root_dir: &Path) -> OpsResult<()> {
        let path = Self::path(root_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| crate::error::OpsError::Config(format!("registry encode failed: {e}")))?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Drop entries whose pid is no longer alive (stale `--keep` leftovers).
    pub fn pruned(mut self) -> Self {
        self.services.retain(|e| pid_alive(e.pid));
        self
    }

    /// First entry registered on `port`, if any.
    pub fn service_on_port(&self, port: u16) -> Option<&ServiceEntry> {
        self.services.iter().find(|e| e.port == port)
    }

    /// Insert `entry`, replacing any existing entry for the same (name, port).
    pub fn add(&mut self, entry: ServiceEntry) {
        self.services
            .retain(|e| !(e.name == entry.name && e.port == entry.port));
        self.services.push(entry);
    }

    /// Remove the entry with `pid` (if present). Returns true when removed.
    pub fn remove_by_pid(&mut self, pid: u32) -> bool {
        let before = self.services.len();
        self.services.retain(|e| e.pid != pid);
        self.services.len() != before
    }
}

/// Record `entry` (load → add → save). Advisory: errors are returned for the
/// caller to warn about.
pub fn record_service(root_dir: &Path, entry: ServiceEntry) -> OpsResult<()> {
    let mut registry = ServiceRegistry::load(root_dir);
    registry.add(entry);
    registry.save(root_dir)
}

/// Remove the entry for `pid` (load → remove → save when changed).
pub fn remove_service(root_dir: &Path, pid: u32) -> OpsResult<()> {
    let mut registry = ServiceRegistry::load(root_dir);
    if !registry.remove_by_pid(pid) {
        return Ok(());
    }
    registry.save(root_dir)
}

/// Kill every registered service (SIGTERM → grace → SIGKILL) and delete the
/// registry file. Returns the number of services successfully killed. Used by
/// `clean`; unknown orphans keep falling to its `fuser -k` port sweep.
pub fn teardown(root_dir: &Path) -> usize {
    let registry = ServiceRegistry::load(root_dir).pruned();
    let mut killed = 0usize;
    for entry in &registry.services {
        output::info(format!(
            "stopping registered service {} (pid {}, port {})",
            entry.name, entry.pid, entry.port
        ));
        if kill_service(entry.pid) {
            killed += 1;
        } else {
            output::warn(format!(
                "could not stop {} (pid {}) — it may still hold port {}",
                entry.name, entry.pid, entry.port
            ));
        }
    }
    let path = ServiceRegistry::path(root_dir);
    if path.is_file() {
        let _ = std::fs::remove_file(&path);
    }
    // Remove the `.testkit` dir too when the registry was its only content.
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir(dir);
    }
    killed
}

/// True when `pid` refers to a live process. `kill(pid, 0)` performs the
/// existence check without signalling; `EPERM` (process owned by someone
/// else) still counts as alive. On Linux a ZOMBIE also answers `kill(pid, 0)`
/// but is not a running service (e.g. a `full` run taking over its own
/// `api --keep` child after SIGTERM) — zombies read as dead.
#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    // SAFETY: kill(2) with signal 0 is a pure existence/permission check.
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        #[cfg(target_os = "linux")]
        if is_zombie(pid) {
            return false;
        }
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Linux only: true when `/proc/{pid}/stat` reports process state `Z`.
#[cfg(all(unix, target_os = "linux"))]
fn is_zombie(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|stat| {
            // Format: `pid (comm) S ...` — the state letter follows the comm
            // (which may itself contain parens, hence rfind).
            match stat.rfind(')') {
                Some(idx) => stat[idx + 2..].starts_with('Z'),
                None => false,
            }
        })
        .unwrap_or(false)
}

/// Non-unix platforms are outside the harness's supported supervision paths;
/// entries are never pruned there (advisory contract errs on the safe side).
#[cfg(not(unix))]
pub fn pid_alive(_pid: u32) -> bool {
    true
}

/// Stop a registered service by pid: SIGTERM, poll up to 3s, then SIGKILL and
/// poll 1s more. True when the process is gone. Unlike
/// [`crate::proc::ManagedProcess`] this is not our child (a leaked `--keep`
/// server reparented to init), so liveness is polled via `kill(pid, 0)`.
#[cfg(unix)]
pub fn kill_service(pid: u32) -> bool {
    let raw = pid as i32;
    if raw <= 0 || !pid_alive(pid) {
        return true;
    }
    // SAFETY: signal delivery to a known pid.
    unsafe {
        libc::kill(raw, libc::SIGTERM);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if !pid_alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    unsafe {
        libc::kill(raw, libc::SIGKILL);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        if !pid_alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    !pid_alive(pid)
}

/// Non-unix: no signal-based teardown is implemented.
#[cfg(not(unix))]
pub fn kill_service(_pid: u32) -> bool {
    false
}

/// Test helper: spawn a long-running `sleep` that is NOT our child — `sh`
/// backgrounds it and exits, orphaning the sleep to init, exactly like a
/// leaked `--keep` server whose harness has exited. (Our own children would
/// linger as zombies until `wait()`, and zombies answer `kill(pid, 0)`.)
#[cfg(all(unix, test))]
pub(crate) fn spawn_detached_sleep() -> u32 {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 30 >/dev/null 2>&1 & echo $!")
        .output()
        .expect("spawn sh");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("background pid from sh")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, pid: u32, port: u16) -> ServiceEntry {
        ServiceEntry {
            name: name.to_string(),
            pid,
            port,
            health: Some("/health".to_string()),
            started_at: "2026-09-29T12:00:00Z".to_string(),
            profile: Some("release".to_string()),
            suite: Some("api".to_string()),
        }
    }

    #[test]
    fn round_trip_write_read() {
        let dir = tempfile::tempdir().unwrap();
        record_service(dir.path(), entry("api", 4242, 3000)).unwrap();
        record_service(dir.path(), entry("ui", 5151, 5173)).unwrap();
        let loaded = ServiceRegistry::load(dir.path());
        assert_eq!(loaded.services.len(), 2);
        let api = loaded.service_on_port(3000).unwrap();
        assert_eq!(api.name, "api");
        assert_eq!(api.pid, 4242);
        assert_eq!(api.health.as_deref(), Some("/health"));
        assert_eq!(api.suite.as_deref(), Some("api"));
        assert_eq!(api.profile.as_deref(), Some("release"));
        // Documented on-disk shape: {root}/.testkit/services.json.
        assert_eq!(
            ServiceRegistry::path(dir.path()),
            dir.path().join(".testkit/services.json")
        );
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ServiceRegistry::load(dir.path());
        assert!(registry.services.is_empty());
    }

    #[test]
    fn load_corrupt_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = ServiceRegistry::path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json at all").unwrap();
        let registry = ServiceRegistry::load(dir.path());
        assert!(
            registry.services.is_empty(),
            "corrupt registry must read as empty"
        );
    }

    #[test]
    fn pruned_drops_dead_pids_keeps_live() {
        // A process that exits immediately: its pid is dead after wait().
        let mut exited = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .spawn()
            .unwrap();
        let dead_pid = exited.id();
        exited.wait().unwrap();
        let registry = ServiceRegistry {
            services: vec![
                entry("dead", dead_pid, 3000),
                entry("self", std::process::id(), 5173),
            ],
        };
        let pruned = registry.pruned();
        assert_eq!(pruned.services.len(), 1);
        assert_eq!(pruned.services[0].name, "self");
    }

    #[test]
    fn record_replaces_same_name_and_port() {
        let dir = tempfile::tempdir().unwrap();
        record_service(dir.path(), entry("api", 1111, 3000)).unwrap();
        record_service(dir.path(), entry("api", 2222, 3000)).unwrap();
        record_service(dir.path(), entry("ui", 3333, 5173)).unwrap();
        let loaded = ServiceRegistry::load(dir.path());
        assert_eq!(loaded.services.len(), 2);
        assert_eq!(loaded.service_on_port(3000).unwrap().pid, 2222);
    }

    #[test]
    fn remove_service_by_pid() {
        let dir = tempfile::tempdir().unwrap();
        record_service(dir.path(), entry("api", 4242, 3000)).unwrap();
        remove_service(dir.path(), 4242).unwrap();
        assert!(ServiceRegistry::load(dir.path()).services.is_empty());
        // Removing an unknown pid is a no-op, not an error.
        remove_service(dir.path(), 9999).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn kill_service_terminates_sleeping_process() {
        let pid = spawn_detached_sleep();
        assert!(pid_alive(pid));
        assert!(kill_service(pid), "sleep must die on SIGTERM");
        assert!(!pid_alive(pid), "pid {pid} must be gone");
        // Killing an already-dead pid reports success (nothing to do).
        assert!(kill_service(pid));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pid_alive_treats_zombies_as_dead() {
        // An exited-but-unreaped child is a zombie: kill(pid, 0) answers 0,
        // yet it is not a running service. While the zombie exists (before
        // our wait()), pid_alive must read it as dead.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut zombie = false;
        for _ in 0..100 {
            if is_zombie(pid) {
                zombie = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if zombie {
            assert!(!pid_alive(pid), "zombie must read as dead");
        }
        child.wait().unwrap();
    }
}
