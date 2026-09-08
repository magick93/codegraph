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
