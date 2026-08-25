//! File-ownership manifests (`.codegraph-manifest.json`).
//!
//! After a generation run, a `.codegraph-manifest.json` is written at each
//! output root — the `--output` directory plus any `domain_types_base` /
//! `hooks_api_base` roots. The manifest lists every file codegen wrote under
//! that root (relative paths, sorted, deduped), merged with any manifest that
//! was already on disk so sequential profile runs into the same root
//! (fullstack then e2e) accumulate a complete picture instead of clobbering
//! each other.
//!
//! `scripts/verify-generated.sh --guard` consumes these manifests to prove no
//! hand-written file has slipped into a generated tree. The manifest file
//! itself is never listed inside itself (the guard special-cases it).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::traits::GeneratedFile;

/// Name of the ownership manifest emitted at each output root.
pub const MANIFEST_FILENAME: &str = ".codegraph-manifest.json";

/// A file-ownership manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Human-readable root this manifest describes (normalized absolute path).
    pub root: String,
    /// Files codegen owns under `root`, relative to `root`, forward-slash
    /// separated, sorted, deduped. Never includes `MANIFEST_FILENAME`.
    pub generated: Vec<String>,
    #[serde(rename = "generatedBy")]
    pub generated_by: String,
    /// Git commit of the codegraph-atproto checkout that produced this tree.
    /// Consumers use it to pin the generator source before verifying drift
    /// (CI checks out this rev so a fresh regen is byte-comparable).
    #[serde(rename = "codegraphCommit")]
    pub codegraph_commit: String,
}

impl Manifest {
    fn new(root: &Path, generated: Vec<String>, codegraph_rev: &str) -> Self {
        Self {
            root: root.to_string_lossy().into_owned(),
            generated,
            generated_by: format!("codegraph v{}", env!("CARGO_PKG_VERSION")),
            codegraph_commit: codegraph_rev.to_string(),
        }
    }
}

/// Resolve a path to a canonical absolute form.
///
/// Relative paths are resolved against the current working directory and
/// symlinks are resolved where the path exists (e.g. `/tmp` → `/private/tmp`
/// on macOS). Falls back to the joined absolute path when the file does not
/// exist yet.
pub fn absolutize(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    joined.canonicalize().unwrap_or(joined)
}

/// Emit `.codegraph-manifest.json` at every output root.
///
/// `roots` is the list of output roots for this run (the main `--output` dir
/// plus any domain-types/hooks-api bases). `written` is the file list of this
/// run's generation report — every `write_output` call mirrors the file into
/// `report.files`, so that Vec is the complete record of what was written.
/// `codegraph_rev` is the generator-source git commit pinned into each
/// manifest (e.g. `ProjectConfig::codegraph_rev`); it lets drift/CI reproduce
/// the exact generator state the committed tree was produced at.
pub fn emit_manifests(
    roots: &[&Path],
    written: &[GeneratedFile],
    codegraph_rev: &str,
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let root = absolutize(root);
        if !seen.insert(root.clone()) {
            continue;
        }
        emit_manifest(&root, written, codegraph_rev)?;
    }
    Ok(())
}

fn emit_manifest(root: &Path, written: &[GeneratedFile], codegraph_rev: &str) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let manifest_path = root.join(MANIFEST_FILENAME);

    // Merge with a previously-written manifest so sequential profile runs
    // into the same root accumulate rather than clobber each other's files.
    let mut entries: BTreeSet<String> = read_existing_entries(&manifest_path);

    for file in written {
        let abs = absolutize(&file.path);
        // Only list files that actually exist after generation. Generators can
        // report a file and later clean it up (e.g. the app-root codelist
        // re-export generator deletes the enum files the domain-types codelist
        // generator just wrote) — such files must not appear in the manifest.
        if !abs.is_file() {
            continue;
        }
        if let Ok(rel) = abs.strip_prefix(root) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !rel.is_empty() && rel != MANIFEST_FILENAME {
                entries.insert(rel);
            }
        }
    }

    let manifest = Manifest::new(root, entries.into_iter().collect(), codegraph_rev);
    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&manifest_path, format!("{json}\n"))?;
    Ok(())
}

fn read_existing_entries(manifest_path: &Path) -> BTreeSet<String> {
    let Ok(content) = std::fs::read_to_string(manifest_path) else {
        return BTreeSet::new();
    };
    match serde_json::from_str::<Manifest>(&content) {
        Ok(m) => m.generated.into_iter().collect(),
        Err(e) => {
            tracing::warn!(
                "manifest {} unparseable ({e}); starting fresh",
                manifest_path.display()
            );
            BTreeSet::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str) -> GeneratedFile {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
        GeneratedFile {
            path,
            content: String::new(),
        }
    }

    #[test]
    fn emit_manifests_lists_relative_paths_sorted_and_deduped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let files = vec![
            write(root, "src/lib.rs"),
            write(root, "src/entity/mod.rs"),
            write(root, "src/entity/mod.rs"), // duplicate
            write(root, "migrations/0001_x.sql"),
        ];
        emit_manifests(&[root], &files, "abcdef0123456789").unwrap();

        let json = std::fs::read_to_string(root.join(MANIFEST_FILENAME)).unwrap();
        let m: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            m.generated,
            vec!["migrations/0001_x.sql", "src/entity/mod.rs", "src/lib.rs"]
        );
        assert!(m.generated_by.starts_with("codegraph v"));
        assert_eq!(m.codegraph_commit, "abcdef0123456789");
        assert_eq!(m.root, absolutize(root).to_string_lossy());
    }

    #[test]
    fn emit_manifests_merges_with_existing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // First run writes src/lib.rs.
        emit_manifests(&[root], &[write(root, "src/lib.rs")], "rev-1").unwrap();
        // Second run (e.g. a different profile into the same root) writes
        // only src/extra.rs; the manifest must keep both.
        emit_manifests(&[root], &[write(root, "src/extra.rs")], "rev-1").unwrap();

        let m: Manifest = serde_json::from_str(
            &std::fs::read_to_string(root.join(MANIFEST_FILENAME)).unwrap(),
        )
        .unwrap();
        assert_eq!(m.generated, vec!["src/extra.rs", "src/lib.rs"]);
        // The merge keeps the current run's rev (sequential profile runs use
        // the same generator checkout).
        assert_eq!(m.codegraph_commit, "rev-1");
    }

    #[test]
    fn emit_manifests_skips_manifest_file_itself() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // A pathological run that reports the manifest path as a written file.
        let path = root.join(MANIFEST_FILENAME);
        std::fs::write(&path, b"x").unwrap();
        let files = vec![
            GeneratedFile {
                path: path.clone(),
                content: String::new(),
            },
            write(root, "src/main.rs"),
        ];
        emit_manifests(&[root], &files, "rev-2").unwrap();

        let m: Manifest = serde_json::from_str(
            &std::fs::read_to_string(root.join(MANIFEST_FILENAME)).unwrap(),
        )
        .unwrap();
        assert_eq!(m.generated, vec!["src/main.rs"]);
    }

    #[test]
    fn emit_manifests_dedupes_nested_roots() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        let files = vec![
            write(root, "top.rs"),
            write(&nested, "inner.rs"),
        ];
        // Passing both root and nested: nested is filtered out because root
        // already covers it, but files under nested are still listed relative
        // to root.
        emit_manifests(&[root, &nested], &files, "rev-3").unwrap();

        let m: Manifest = serde_json::from_str(
            &std::fs::read_to_string(root.join(MANIFEST_FILENAME)).unwrap(),
        )
        .unwrap();
        assert_eq!(m.generated, vec!["nested/inner.rs", "top.rs"]);
        assert_eq!(m.codegraph_commit, "rev-3");
    }
}
