//! Shared preflight checks used by the suites before their long stages.
//!
//! Each check fails fast with an actionable message: a blocked port or a
//! stale app binary used to surface only after minutes of migrating/building
//! (or worse, as baffling test failures deep inside the suite).

use std::path::Path;
use std::time::SystemTime;

use crate::error::{OpsError, OpsResult};

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

/// Fail when `binary` is missing or older than the newest source file under
/// `{app_dir}/src` — a stale binary would silently test an app that doesn't
/// match the current generator output (this is exactly how mixed-profile
/// trees produced baffling "intermittent" failures).
pub fn ensure_binary_fresh(app_dir: &Path, binary: &Path) -> OpsResult<()> {
    if !binary.is_file() {
        return Err(OpsError::TestFailure(format!(
            "no binary at {} — run with build or --rebuild",
            binary.display()
        )));
    }
    let newest_src = newest_mtime(&app_dir.join("src"));
    let binary_mtime = binary.metadata().and_then(|m| m.modified()).ok();
    let stale = matches!((binary_mtime, newest_src), (Some(b), Some(s)) if b < s);
    if stale {
        return Err(OpsError::TestFailure(format!(
            "binary {} is older than {} — the suite would test stale code; \
             rebuild (drop --skip-build) first",
            binary.display(),
            app_dir.join("src").display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_port_free_accepts_a_free_port() {
        // Discover a free port by binding an ephemeral listener, then release
        // it for the check (tiny race, acceptable in a unit test).
        let port = {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            listener.local_addr().unwrap().port()
        };
        ensure_port_free(port).unwrap();
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
