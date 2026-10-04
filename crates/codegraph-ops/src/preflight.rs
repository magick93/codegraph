//! Shared preflight checks used by the suites before their long stages.
//!
//! Each check fails fast with an actionable message: a blocked port or a
//! stale app binary used to surface only after minutes of migrating/building
//! (or worse, as baffling test failures deep inside the suite).

use std::path::Path;
use std::time::SystemTime;

use crate::error::{OpsError, OpsResult};
use crate::output;

/// Verify `port` can be bound. An [`OpsError`] naming the port and the likely
/// cause is returned when another process holds it — the api suite once
/// burned ten minutes before failing on an unrelated process holding :3000,
/// with the real error buried in the app log.
pub fn ensure_port_free(port: u16) -> OpsResult<()> {
    match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(listener) => drop(listener),
        Err(e) => {
            return Err(OpsError::TestFailure(format!(
                "port {port} is not free ({e}) — another process is likely bound to it; \
                 free the port (e.g. `fuser -k {port}/tcp`) and rerun"
            )));
        }
    }
    Ok(())
}

/// Outcome of the registry-aware port preflight
/// ([`ensure_port_available`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortOutcome {
    /// Nothing was listening.
    Free,
    /// A registry-known prior server still holds the port and `--reuse` was
    /// given: the caller must skip booting its own instance. Documented
    /// caveat: the harness assumes the running service serves the CURRENT
    /// build — regenerate/rebuild still happen, but the old server keeps
    /// serving whatever it was started with.
    Reused { name: String },
    /// A registry-known prior server was killed; the caller boots its own
    /// instance normally.
    TookOver { name: String, pid: u32 },
}

impl PortOutcome {
    /// True when a prior server is being reused and the caller must skip its
    /// own boot + health wait (the service is already answering).
    pub fn reused(&self) -> bool {
        matches!(self, PortOutcome::Reused { .. })
    }
}

/// Registry-aware port preflight: free ports pass through; an occupied port
/// is resolved through the advisory services registry
/// (`.testkit/services.json`):
///
/// - registry-known, live occupant + `--reuse` → [`PortOutcome::Reused`]
///   (caller skips booting its own instance);
/// - registry-known, live occupant, default → takeover: SIGTERM → grace →
///   SIGKILL on the recorded pid, entry removed, [`PortOutcome::TookOver`];
/// - anything else (unknown or pid-reused occupant) → the same actionable
///   error as before (unchanged contract).
pub fn ensure_port_available(root_dir: &Path, port: u16, reuse: bool) -> OpsResult<PortOutcome> {
    ensure_port_available_with(root_dir, port, reuse, &crate::registry::kill_service)
}

/// [`ensure_port_available`] with an injectable killer (unit tests pass a
/// stub or the real [`crate::registry::kill_service`]).
pub fn ensure_port_available_with(
    root_dir: &Path,
    port: u16,
    reuse: bool,
    kill_by_pid: &dyn Fn(u32) -> bool,
) -> OpsResult<PortOutcome> {
    if ensure_port_free(port).is_ok() {
        return Ok(PortOutcome::Free);
    }
    let registry = crate::registry::ServiceRegistry::load(root_dir).pruned();
    match registry.service_on_port(port) {
        Some(entry) if reuse => {
            output::warn(format!(
                "reusing {} (pid {}, started {}) on port {port} (--reuse) — \
                 it may serve a STALE build; the harness will not boot its own instance",
                entry.name, entry.pid, entry.started_at
            ));
            Ok(PortOutcome::Reused {
                name: entry.name.clone(),
            })
        }
        Some(entry) => {
            output::info(format!(
                "port {port} held by registry-known {} (pid {}, started {}) — taking over",
                entry.name, entry.pid, entry.started_at
            ));
            if kill_by_pid(entry.pid) {
                let _ = crate::registry::remove_service(root_dir, entry.pid);
                output::ok(format!(
                    "took over port {port} (stopped {} pid {})",
                    entry.name, entry.pid
                ));
                Ok(PortOutcome::TookOver {
                    name: entry.name.clone(),
                    pid: entry.pid,
                })
            } else {
                Err(OpsError::TestFailure(format!(
                    "port {port} is held by {} (pid {}) and the registry takeover failed — \
                     free the port manually (e.g. `fuser -k {port}/tcp`) and rerun",
                    entry.name, entry.pid
                )))
            }
        }
        // Unknown occupant (or a stale entry whose pid died and got reused):
        // the pre-existing actionable error, unchanged.
        None => ensure_port_free(port).map(|_| PortOutcome::Free),
    }
}

/// Newest `modified` timestamp across `.rs` files under `dir` (None when the
/// tree is missing or unreadable). Used by [`ensure_binary_fresh`].
pub fn newest_mtime(dir: &Path) -> Option<SystemTime> {
    fn walk(dir: &Path, newest: &mut Option<SystemTime>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, newest)?;
            } else if path.extension().is_some_and(|e| e == "rs") {
                let mtime = entry.metadata()?.modified()?;
                if newest.is_none_or(|n| mtime > n) {
                    *newest = Some(mtime);
                }
            }
        }
        Ok(())
    }
    let mut newest = None;
    walk(dir, &mut newest).ok()?;
    newest
}

/// Verify the generated output tree is complete enough to test: `src/` must
/// contain `.rs` files and `migrations/` must exist and be non-empty.
///
/// The api suite once ran against a tree whose `src/` and `migrations/` had
/// been wiped by an interrupted regeneration, booted a stale binary anyway,
/// and reported 15 baffling failures (`missing api_key`, `no RLS migration
/// files`, blanket 401s). This check turns that state into a fast, actionable
/// error instead.
pub fn ensure_output_tree(app_dir: &Path) -> OpsResult<()> {
    let src = app_dir.join("src");
    if newest_mtime(&src).is_none() {
        return Err(OpsError::TestFailure(format!(
            "generated output tree is incomplete: {} is missing or has no .rs files — \
             run generate first (e.g. `cargo run -p <graph_binary> -- run --output {}`)",
            src.display(),
            app_dir.display()
        )));
    }
    let migrations = app_dir.join("migrations");
    let has_migrations = std::fs::read_dir(&migrations)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if !has_migrations {
        return Err(OpsError::TestFailure(format!(
            "generated output tree is incomplete: {} is missing or empty — \
             run generate first; testing now would pair a stale binary with wiped \
             migrations (baffling api_key / RLS failures)",
            migrations.display()
        )));
    }
    Ok(())
}

/// Fail when `binary` is missing or older than the newest source file under
/// `{app_dir}/src` — a stale binary would silently test an app that doesn't
/// match the current generator output (this is exactly how mixed-profile
/// trees produced baffling "intermittent" failures).
///
/// A missing `{app_dir}/src` is also a hard error: without sources there is
/// nothing to compare against, and a passing check here historically let the
/// suite boot a binary that no longer matched the (wiped) tree.
pub fn ensure_binary_fresh(app_dir: &Path, binary: &Path) -> OpsResult<()> {
    if !binary.is_file() {
        return Err(OpsError::TestFailure(format!(
            "no binary at {} — run with build or --rebuild",
            binary.display()
        )));
    }
    let src_dir = app_dir.join("src");
    let newest_src = newest_mtime(&src_dir);
    let Some(newest_src) = newest_src else {
        return Err(OpsError::TestFailure(format!(
            "no generated sources under {} — run generate first; \
             binary freshness cannot be established",
            src_dir.display()
        )));
    };
    let binary_mtime = binary.metadata().and_then(|m| m.modified()).ok();
    let stale = matches!(binary_mtime, Some(b) if b < newest_src);
    if stale {
        return Err(OpsError::TestFailure(format!(
            "binary {} is older than {} — the suite would test stale code; \
             rebuild (drop --skip-build) first",
            binary.display(),
            src_dir.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{ServiceEntry, ServiceRegistry, record_service};

    /// A live ServiceEntry for `port` whose pid is this test process (always
    /// alive; only safe with the reuse path or an injected killer).
    fn own_pid_entry(name: &str, port: u16) -> ServiceEntry {
        ServiceEntry {
            name: name.to_string(),
            pid: std::process::id(),
            port,
            health: Some("/health".to_string()),
            started_at: "2026-09-29T12:00:00Z".to_string(),
            profile: Some("debug".to_string()),
            suite: Some("api".to_string()),
        }
    }

    #[test]
    fn ensure_port_free_accepts_a_free_port() {
        // Discover a free port by binding an ephemeral listener, then release
        // it for the check. The release→recheck window is racy (parallel
        // tests churn ephemeral ports too), so retry until a port survives.
        for _ in 0..20 {
            let port = {
                let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                listener.local_addr().unwrap().port()
            };
            if ensure_port_free(port).is_ok() {
                return;
            }
        }
        panic!("no free port found for the check");
    }

    #[test]
    fn ensure_port_available_free_port() {
        let dir = tempfile::tempdir().unwrap();
        let port = {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            listener.local_addr().unwrap().port()
        };
        assert_eq!(
            ensure_port_available(dir.path(), port, false).unwrap(),
            PortOutcome::Free
        );
    }

    #[test]
    fn ensure_port_available_unknown_occupant_keeps_actionable_error() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        // No registry entry: the historical error, unchanged.
        let err = ensure_port_available(dir.path(), port, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&port.to_string()), "{msg}");
        assert!(msg.contains("not free"), "{msg}");
        // Same for a corrupt registry (advisory: read as empty + warn).
        let path = ServiceRegistry::path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "}{ broken").unwrap();
        let err = ensure_port_available(dir.path(), port, false).unwrap_err();
        assert!(err.to_string().contains("not free"));
    }

    #[test]
    fn ensure_port_available_reuse_skips_takeover() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        record_service(dir.path(), own_pid_entry("api", port)).unwrap();
        // Killer must never be called on the reuse path — a panicking stub
        // proves it.
        let outcome =
            ensure_port_available_with(dir.path(), port, true, &|_pid| panic!("must not kill"))
                .unwrap();
        assert_eq!(
            outcome,
            PortOutcome::Reused {
                name: "api".to_string()
            }
        );
        assert!(outcome.reused());
    }

    #[cfg(unix)]
    #[test]
    fn ensure_port_available_takeover_kills_registered_pid() {
        let dir = tempfile::tempdir().unwrap();
        // The port is held by this test's listener, but the registry records
        // a real, killable detached `sleep` — the unit-level stand-in for a
        // leaked `--keep` server (the real production killer is exercised).
        let pid = crate::registry::spawn_detached_sleep();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut entry = own_pid_entry("api", port);
        entry.pid = pid;
        record_service(dir.path(), entry).unwrap();

        let outcome = ensure_port_available(dir.path(), port, false).unwrap();
        assert_eq!(
            outcome,
            PortOutcome::TookOver {
                name: "api".to_string(),
                pid
            }
        );
        assert!(!crate::registry::pid_alive(pid), "sleep must be dead");
        // The takeover removed the consumed registry entry.
        let registry = ServiceRegistry::load(dir.path());
        assert!(
            !registry.services.iter().any(|e| e.pid == pid),
            "entry must be pruned after takeover"
        );
    }

    #[test]
    fn ensure_port_available_failed_takeover_is_actionable() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        // Own-pid entry survives the dead-pid prune; the injected killer
        // fails without harming anyone.
        record_service(dir.path(), own_pid_entry("api", port)).unwrap();
        let err = ensure_port_available_with(dir.path(), port, false, &|_pid| false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("takeover failed"), "{msg}");
        assert!(msg.contains("fuser -k"), "{msg}");
    }

    #[test]
    fn ensure_port_free_names_port_when_already_bound() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let err = ensure_port_free(port).unwrap_err();
        assert!(matches!(err, OpsError::TestFailure(_)), "got {err:?}");
        let msg = err.to_string();
        assert!(
            msg.contains(&port.to_string()),
            "error must name the port: {msg}"
        );
        assert!(
            msg.contains("another process"),
            "error must name the likely cause: {msg}"
        );
    }

    #[test]
    fn newest_mtime_tracks_rs_files_and_ignores_others() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(newest_mtime(dir.path()), None);
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(src.join("notes.txt"), "not rust").unwrap();
        let nested = src.join("deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("lib.rs"), "pub fn f() {}").unwrap();
        let newest = newest_mtime(dir.path()).unwrap();
        // All files were written "now"; the newest mtime must be recent.
        let elapsed = newest.elapsed().unwrap();
        assert!(
            elapsed.as_secs() < 60,
            "unexpected newest mtime: {newest:?}"
        );
    }

    #[test]
    fn newest_mtime_returns_none_for_missing_dir() {
        assert_eq!(newest_mtime(Path::new("/nonexistent/src-tree")), None);
    }

    #[test]
    fn ensure_binary_fresh_rejects_missing_binary() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("target/debug/app");
        let err = ensure_binary_fresh(dir.path(), &binary).unwrap_err();
        assert!(err.to_string().contains("no binary at"));
    }

    #[test]
    fn ensure_binary_fresh_rejects_missing_src() {
        // The wiped-tree incident: a binary exists but src/ is gone. The
        // check must fail loudly instead of passing vacuously.
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("app");
        std::fs::write(&binary, "binary").unwrap();
        let err = ensure_binary_fresh(dir.path(), &binary).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no generated sources"), "got {msg}");
        assert!(msg.contains("run generate first"), "got {msg}");
    }

    fn complete_tree(dir: &tempfile::TempDir) {
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("migrations")).unwrap();
        std::fs::write(dir.path().join("migrations/0001_init.sql"), "SELECT 1;").unwrap();
    }

    #[test]
    fn ensure_output_tree_accepts_complete_tree() {
        let dir = tempfile::tempdir().unwrap();
        complete_tree(&dir);
        ensure_output_tree(dir.path()).unwrap();
    }

    #[test]
    fn ensure_output_tree_rejects_missing_src() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("migrations")).unwrap();
        std::fs::write(dir.path().join("migrations/0001_init.sql"), "SELECT 1;").unwrap();
        let err = ensure_output_tree(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("incomplete"), "got {msg}");
        assert!(msg.contains("src"), "got {msg}");
        assert!(msg.contains("run generate first"), "got {msg}");
    }

    #[test]
    fn ensure_output_tree_rejects_src_without_rs_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/notes.txt"), "not rust").unwrap();
        std::fs::create_dir_all(dir.path().join("migrations")).unwrap();
        std::fs::write(dir.path().join("migrations/0001_init.sql"), "SELECT 1;").unwrap();
        let err = ensure_output_tree(dir.path()).unwrap_err();
        assert!(err.to_string().contains("incomplete"), "got {err}");
    }

    #[test]
    fn ensure_output_tree_rejects_missing_migrations() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        let err = ensure_output_tree(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("incomplete"), "got {msg}");
        assert!(msg.contains("migrations"), "got {msg}");
    }

    #[test]
    fn ensure_output_tree_rejects_empty_migrations() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("migrations")).unwrap();
        let err = ensure_output_tree(dir.path()).unwrap_err();
        assert!(err.to_string().contains("migrations"), "got {err}");
    }

    #[test]
    fn ensure_binary_fresh_rejects_stale_binary() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let binary = dir.path().join("app");
        std::fs::write(&binary, "binary").unwrap();
        // Binary predates the newest src file → stale.
        let file = std::fs::File::options().write(true).open(&binary).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH).unwrap();
        drop(file);
        std::fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        let err = ensure_binary_fresh(dir.path(), &binary).unwrap_err();
        assert!(err.to_string().contains("older than"), "got {err}");
    }

    #[test]
    fn ensure_binary_fresh_accepts_current_binary() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        let binary = dir.path().join("app");
        std::fs::write(&binary, "binary").unwrap();
        // Force the binary strictly newer than src (same-second mtimes would
        // compare equal, which counts as fresh — `b < s`).
        let file = std::fs::File::options().write(true).open(&binary).unwrap();
        file.set_modified(SystemTime::now()).unwrap();
        drop(file);
        ensure_binary_fresh(dir.path(), &binary).unwrap();
    }
}
