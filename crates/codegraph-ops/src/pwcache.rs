//! Playwright transpile-cache hygiene.
//!
//! Playwright transpiles each spec to `/tmp/playwright-transform-cache-{uid}/`
//! and reuses those files across runs. During a real consumer run the cache
//! served STALE transpiled specs after regeneration updated the spec files —
//! Playwright kept executing old code (`Expected pattern` in error contexts
//! matched no file on disk). This module clears the cache so every harness
//! run executes exactly what is on disk.
//!
//! Scoping decision (verified against a local `/tmp/playwright-transform-
//! cache-1000` install, 255 entries): entries are content-hash-addressed —
//! two-hex-char bucket dirs (`00/`, `0b/`, …) containing
//! `<hash>_<hash>_<flattened-filename>.js` (+ `.map`). Reliably deleting only
//! the entries derived from `tests/generated/**` would mean replicating
//! Playwright's internal hash (source path + mtime dependent, varies across
//! Playwright versions) — not trivially implementable. So the WHOLE cache dir
//! is cleared: the cache is disposable by design and rebuilt on the next run.
//!
//! The uid comes from `libc::getuid()` (libc is already a dependency), with a
//! scan over `/tmp/playwright-transform-cache-*` as the fallback for any
//! other suffixes (different uids, sandbox layouts).

use std::path::{Path, PathBuf};

/// Cache directories under `tmp_root`: the current user's
/// `playwright-transform-cache-{uid}` (unix) plus any other
/// `playwright-transform-cache-*` directory found there.
pub fn transform_cache_dirs(tmp_root: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(tmp_root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    #[cfg(unix)]
    let uid_suffix = current_uid().map(|uid| uid.to_string());
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(suffix) = name.strip_prefix("playwright-transform-cache-") else {
            continue;
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        #[cfg(unix)]
        if uid_suffix.as_deref() == Some(suffix) {
            dirs.push(path);
            continue;
        }
        // Fallback: any other cache dir (different uid, sandbox layout).
        dirs.push(path);
    }
    dirs
}

/// Remove every transform-cache directory under `tmp_root`. Returns the
/// number of directories removed. Best-effort: unreadable entries are
/// skipped.
pub fn clear_transform_cache(tmp_root: &Path) -> usize {
    let mut removed = 0usize;
    for dir in transform_cache_dirs(tmp_root) {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => removed += 1,
            Err(e) => crate::output::warn(format!(
                "could not clear Playwright transform cache {}: {e}",
                dir.display()
            )),
        }
    }
    removed
}

/// Clear the transform cache under `/tmp` and report the removal (silent
/// when there was nothing to clear). The pre-Playwright step used by the
/// e2e and ui suites, and the `--clear-cache` global flag.
pub fn clear_and_report() {
    let removed = clear_transform_cache(Path::new("/tmp"));
    if removed > 0 {
        crate::output::info(format!(
            "Cleared Playwright transform cache ({removed} cache dir(s))"
        ));
    }
}

#[cfg(unix)]
fn current_uid() -> Option<u32> {
    // SAFETY: getuid(2) always succeeds.
    Some(unsafe { libc::getuid() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn includes_uid_dir_and_fallbacks_only_once() {
        let tmp = tempfile::tempdir().unwrap();
        let uid = unsafe { libc::getuid() };
        let uid_dir = tmp.path().join(format!("playwright-transform-cache-{uid}"));
        let other_dir = tmp.path().join("playwright-transform-cache-9999");
        std::fs::create_dir_all(uid_dir.join("00")).unwrap();
        std::fs::create_dir_all(&other_dir).unwrap();
        std::fs::write(uid_dir.join("00/abc.js"), "x").unwrap();
        let mut dirs = transform_cache_dirs(tmp.path());
        let mut expected = vec![other_dir, uid_dir];
        dirs.sort();
        expected.sort();
        assert_eq!(dirs, expected);
    }

    #[test]
    fn clear_removes_every_cache_dir_and_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("playwright-transform-cache-1000");
        let b = tmp.path().join("playwright-transform-cache-9999");
        std::fs::create_dir_all(a.join("ab")).unwrap();
        std::fs::write(a.join("ab/spec.js"), "x").unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("keepme.txt"), "not a cache").unwrap();
        assert_eq!(clear_transform_cache(tmp.path()), 2);
        assert!(!a.exists());
        assert!(!b.exists());
        // Idempotent: nothing left to clear.
        assert_eq!(clear_transform_cache(tmp.path()), 0);
    }

    #[test]
    fn non_cache_entries_are_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("playwright-other"), "x").unwrap();
        std::fs::create_dir_all(tmp.path().join("transform-cache-not")).unwrap();
        std::fs::write(tmp.path().join("unrelated.log"), "x").unwrap();
        assert_eq!(clear_transform_cache(tmp.path()), 0);
        assert!(tmp.path().join("playwright-other").is_file());
        assert!(tmp.path().join("transform-cache-not").is_dir());
        assert!(tmp.path().join("unrelated.log").is_file());
    }

    #[test]
    fn missing_tmp_root_is_empty_and_harmless() {
        assert!(transform_cache_dirs(Path::new("/nonexistent-cg-ops-tmp")).is_empty());
        assert_eq!(
            clear_transform_cache(Path::new("/nonexistent-cg-ops-tmp")),
            0
        );
    }
}
