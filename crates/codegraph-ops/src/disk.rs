//! Filesystem capacity and size probes (std + libc only, no new deps).
//!
//! The expanded doctor (#358) reports free space on the three filesystems the
//! harness writes to (manifest root, app `target/`, temp dir) and the size of
//! the Playwright transform cache. A real consumer run exhausted the /tmp
//! quota (agent scratch + playwright artifacts + build logs) and lost hours
//! with no early signal — these probes are that early signal.

use std::path::Path;

/// Free bytes available to unprivileged users on the filesystem holding
/// `path` (`statvfs` `f_bavail × f_frsize`). `None` when the path does not
/// exist, is unreadable, or the platform has no statvfs (non-unix).
#[cfg(unix)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs writes only into `stat`; `c_path` outlives the call.
    let mut stat = unsafe { std::mem::zeroed::<libc::statvfs>() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(unix))]
pub fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

/// Total size in bytes of every file under `dir` (recursive). Best-effort:
/// unreadable entries and missing dirs count as 0.
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += dir_size(&path);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

/// Human-readable byte count (`1.2 GB`, `345.6 MB`, `12 B`).
pub fn human_bytes(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_bytes_reports_positive_space_for_real_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let free = free_bytes(dir.path()).expect("statvfs on a tempdir must work on unix");
        assert!(free > 0, "expected free space, got {free}");
    }

    #[test]
    fn free_bytes_is_none_for_missing_paths() {
        assert_eq!(free_bytes(Path::new("/nonexistent-cg-ops-disk")), None);
    }

    #[test]
    fn dir_size_sums_files_recursively() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/one"), "12345").unwrap();
        std::fs::write(dir.path().join("a/b/two"), "1234567890").unwrap();
        assert_eq!(dir_size(dir.path()), 15);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }

    #[test]
    fn human_bytes_uses_friendly_units() {
        assert_eq!(human_bytes(12), "12 B");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
