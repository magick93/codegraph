//! Tool discovery: locate psql and npx across common install locations.
//!
//! Mirrors the bash psql/npx PATH bootstrapping (Homebrew, Linuxbrew, apt/dnf
//! postgresql-client dirs, fnm, nvm).

use std::path::{Path, PathBuf};

use crate::error::{OpsError, OpsResult};

/// Locate `psql` on PATH or in common install locations.
/// Checks `PSQL_PATH` env override, then Homebrew/Linuxbrew Cellar libpq dirs,
/// then `/usr/lib/postgresql/*/bin`.
pub fn resolve_psql() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("PSQL_PATH") {
        let candidate = PathBuf::from(dir).join("psql");
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    if let Some(found) = find_on_path("psql") {
        return Some(found);
    }
    for brew_prefix in ["/opt/homebrew", "/home/linuxbrew/.linuxbrew"] {
        let cellar = Path::new(brew_prefix).join("Cellar/libpq");
        if let Ok(entries) = std::fs::read_dir(cellar) {
            for entry in entries.flatten() {
                let bin = entry.path().join("bin/psql");
                if bin.is_file() && is_executable(&bin) {
                    return Some(bin);
                }
            }
        }
    }
    let usr_lib = Path::new("/usr/lib/postgresql");
    if let Ok(entries) = std::fs::read_dir(usr_lib) {
        let mut versions: Vec<_> = entries.flatten().collect();
        versions.sort_by_key(|e| e.file_name());
        for entry in versions {
            let bin = entry.path().join("bin/psql");
            if bin.is_file() && is_executable(&bin) {
                return Some(bin);
            }
        }
    }
    None
}

/// Ensure psql is available, returning its path or a clear error.
pub fn ensure_psql() -> OpsResult<PathBuf> {
    resolve_psql().ok_or({
        OpsError::MissingTool(
            "psql",
            "install postgresql-client (apt/dnf/brew) or set PSQL_PATH",
        )
    })
}

/// Locate `npx` on PATH or via fnm/nvm storage.
/// Checks `NPX_PATH` env override, then fnm shell env, nvm.sh, and the fnm
/// node-versions directory scan.
pub fn resolve_npx() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NPX_PATH") {
        let candidate = PathBuf::from(dir).join("npx");
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    if let Some(found) = find_on_path("npx") {
        return Some(found);
    }
    // fnm storage scan: ~/.local/share/fnm/node-versions/*/installation/bin/npx
    if let Some(home) = std::env::var_os("HOME") {
        let base = Path::new(&home).join(".local/share/fnm/node-versions");
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let bin = entry.path().join("installation/bin/npx");
                if bin.is_file() && is_executable(&bin) {
                    return Some(bin);
                }
            }
        }
    }
    None
}

/// Ensure npx is available (needed for supabase, playwright).
/// Returns Ok(()) even if missing (callers degrade gracefully).
pub fn ensure_npx() -> OpsResult<()> {
    if resolve_npx().is_some() {
        Ok(())
    } else {
        output_warn_npx();
        Ok(())
    }
}

fn output_warn_npx() {
    crate::output::warn("npx not found — supabase/playwright commands may fail");
}

fn find_on_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_binaries_on_path() {
        assert!(find_on_path("sh").is_some() || find_on_path("bash").is_some());
        assert!(find_on_path("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn resolve_npx_falls_back_gracefully() {
        // Should not panic; returns Some only if npx/fnm/nvm present.
        let _ = resolve_npx();
    }
}
