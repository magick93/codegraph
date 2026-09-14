//! npm project helpers for full-stack integration tests: hash-gated
//! installs, svelte-kit sync, typecheck (`run_check`) and build
//! (`run_build`) invocations with shared command logging.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::process::{self, capture};

/// Marker file inside node_modules storing the package.json hash the
/// install corresponds to. Name kept gate-branded for warm-cache
/// continuity with existing checkouts.
const PKG_HASH_FILE: &str = ".ifml-gate-pkg-hash";

/// An npm-managed project directory plus the log directory its commands
/// write to.
pub struct NodeProject {
    pub root: PathBuf,
    pub logs_dir: PathBuf,
}

impl NodeProject {
    pub fn new(root: impl Into<PathBuf>, logs_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            logs_dir: logs_dir.into(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn npm(&self, args: &[&str], timeout: Duration) -> Result<bool, String> {
        process::run(&self.root, "npm", args, &[], timeout, &self.logs_dir)
    }

    pub fn npx(
        &self,
        args: &[&str],
        envs: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<bool, String> {
        process::run(&self.root, "npx", args, envs, timeout, &self.logs_dir)
    }

    pub fn package_json_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        fs::read_to_string(self.root.join("package.json"))
            .unwrap_or_default()
            .hash(&mut hasher);
        hasher.finish()
    }

    /// Hash-gated `npm install`: skips when node_modules already matches the
    /// current package.json hash.
    pub fn ensure_install(&self) -> Result<(), String> {
        if !process::have_tool("npm") {
            return Err("npm not found".to_string());
        }
        if !process::have_tool("npx") {
            return Err("npx not found".to_string());
        }
        let node_modules = self.root.join("node_modules");
        let hash_file = node_modules.join(PKG_HASH_FILE);
        let hash = self.package_json_hash().to_string();
        if node_modules.is_dir()
            && fs::read_to_string(&hash_file).is_ok_and(|stored| stored.trim() == hash)
        {
            return Ok(());
        }
        let ok = self.npm(
            &["install", "--no-audit", "--no-fund"],
            Duration::from_secs(900),
        )?;
        if !ok {
            return Err(format!(
                "npm install failed (see {})",
                self.logs_dir.display()
            ));
        }
        fs::create_dir_all(&node_modules).map_err(|e| e.to_string())?;
        fs::write(&hash_file, hash).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// `npx svelte-kit sync` (180s).
    pub fn sync_sveltekit(&self) -> Result<(), String> {
        let ok = self.npx(&["svelte-kit", "sync"], &[], Duration::from_secs(180))?;
        if !ok {
            return Err(format!(
                "svelte-kit sync failed (see {})",
                self.logs_dir.display()
            ));
        }
        Ok(())
    }

    /// `npx svelte-check --tsconfig ./tsconfig.json`, captured.
    pub fn run_check(&self) -> Result<(bool, String), String> {
        capture(
            &self.root,
            "npx",
            &["svelte-check", "--tsconfig", "./tsconfig.json"],
        )
    }

    /// `npx vite build` (600s).
    pub fn run_build(&self) -> Result<bool, String> {
        self.npx(&["vite", "build"], &[], Duration::from_secs(600))
    }
}
