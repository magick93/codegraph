//! From-clean regeneration for the API suite (issue #796).
//!
//! The api suite regenerates IN PLACE (`stage_generate_build`) and then
//! discovers contracts by listing `hurl.dir`. A file the PREVIOUS generation
//! wrote and this one no longer writes (a renumbered `_actions.hurl`, a
//! removed entity's contract or migration) used to survive and be hurl'd
//! against the new app — the #688/#694 false-green class — unless the
//! consumer authored a `pre_generate` wipe hook.
//!
//! The harness does not own the tree (`hurl.dir` may hold hand-authored
//! contracts and `output_dir = "."` is legal), so it never deletes a file it
//! cannot prove it generated. The proof is the generator's own ownership
//! manifest, `{app_dir}/.codegraph-manifest.json`, which lists every file a
//! generation wrote under that root:
//!
//! 1. [`PriorGeneration::take`] reads the previous manifest's list and
//!    removes the manifest file, so the regeneration writes a FRESH manifest
//!    naming only what it wrote (the generator otherwise MERGES with the
//!    manifest on disk and the difference would be invisible).
//! 2. After a successful regeneration, [`PriorGeneration::sweep`] deletes
//!    exactly `prior − regenerated`: files a previous generation provably
//!    wrote and this one did not. Hand-authored files are never in a
//!    manifest, so they are never candidates.
//! 3. On a failed regeneration, [`PriorGeneration::restore`] puts the prior
//!    manifest back byte-for-byte.
//!
//! Never swept, even when listed: the ops manifest the running suite was
//! loaded from, and the `testkit/` crate (the binary running this code).
//! Entries that are absolute or escape the root are ignored. Boundary: only
//! the `app_dir` root's manifest is consulted — files a generation wrote
//! under a separate `domain_types_base` / `hooks_api_base` root are not
//! swept here.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::error::{OpsError, OpsResult};

/// The generator's ownership manifest filename (mirrors
/// `codegraph_generate::manifest::MANIFEST_FILENAME`; codegraph-ops does not
/// depend on the generator crate).
pub(super) const MANIFEST_FILENAME: &str = ".codegraph-manifest.json";

/// The previous generation's file list, held across one regeneration.
#[derive(Debug)]
pub(super) struct PriorGeneration {
    app_dir: PathBuf,
    /// `None` when no (parseable) prior manifest existed — nothing to sweep.
    prior: Option<Prior>,
}

#[derive(Debug)]
struct Prior {
    entries: BTreeSet<String>,
    raw: String,
}

/// What a sweep did, for the suite log.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SweepReport {
    /// Relative paths removed, sorted.
    pub removed: Vec<String>,
    /// Stale entries skipped because they are protected (ops manifest,
    /// testkit crate).
    pub protected: Vec<String>,
    /// `true` when there was no prior manifest to compare against.
    pub no_prior: bool,
}

impl SweepReport {
    /// One log line: the count and the top-level roots touched.
    pub(super) fn summary(&self) -> String {
        if self.no_prior {
            return format!(
                "regen sweep: no prior {MANIFEST_FILENAME} — nothing to compare, nothing removed"
            );
        }
        let roots: BTreeSet<&str> = self
            .removed
            .iter()
            .map(|p| p.split('/').next().unwrap_or(p.as_str()))
            .collect();
        let mut line = format!(
            "regen sweep: removed {} stale file(s) a previous generation wrote",
            self.removed.len()
        );
        if !roots.is_empty() {
            line.push_str(&format!(
                " (roots: {})",
                roots.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
        if !self.protected.is_empty() {
            line.push_str(&format!(
                "; kept {} protected: {}",
                self.protected.len(),
                self.protected.join(", ")
            ));
        }
        line
    }
}

impl PriorGeneration {
    /// Snapshot the prior manifest and remove it so the regeneration writes
    /// a fresh one. A missing or unparseable manifest yields an empty
    /// snapshot (nothing is ever swept on a guess).
    pub(super) fn take(app_dir: &Path) -> OpsResult<Self> {
        let path = app_dir.join(MANIFEST_FILENAME);
        let prior = match std::fs::read_to_string(&path) {
            Ok(raw) => match parse_generated(&raw) {
                Some(entries) => {
                    std::fs::remove_file(&path)?;
                    Some(Prior { entries, raw })
                }
                None => None,
            },
            Err(_) => None,
        };
        Ok(Self {
            app_dir: app_dir.to_path_buf(),
            prior,
        })
    }

    /// Put the prior manifest back (a failed regeneration must not lose the
    /// ownership record). A manifest the failed run managed to write is
    /// replaced.
    pub(super) fn restore(self) -> OpsResult<()> {
        if let Some(prior) = self.prior {
            std::fs::write(self.app_dir.join(MANIFEST_FILENAME), prior.raw)?;
        }
        Ok(())
    }

    /// Delete `prior − regenerated`, skipping `protected` relative paths
    /// (exact files, or directory prefixes ending in `/`).
    pub(super) fn sweep(self, protected: &[String]) -> OpsResult<SweepReport> {
        let Some(prior) = self.prior else {
            return Ok(SweepReport {
                no_prior: true,
                ..SweepReport::default()
            });
        };
        let current_raw =
            std::fs::read_to_string(self.app_dir.join(MANIFEST_FILENAME)).map_err(|e| {
                OpsError::TestFailure(format!(
                    "regen sweep: the regeneration wrote no {MANIFEST_FILENAME} under {} ({e}) — \
                     refusing to sweep without the new ownership record",
                    self.app_dir.display()
                ))
            })?;
        let current = parse_generated(&current_raw).ok_or_else(|| {
            OpsError::TestFailure(format!(
                "regen sweep: the regenerated {MANIFEST_FILENAME} is unparseable — refusing to sweep"
            ))
        })?;
        let mut report = SweepReport::default();
        for rel in prior.entries.difference(&current) {
            if !is_contained_relative(rel) {
                continue;
            }
            if is_protected(rel, protected) {
                report.protected.push(rel.clone());
                continue;
            }
            let path = self.app_dir.join(rel);
            if path.is_file() {
                std::fs::remove_file(&path)?;
                remove_empty_parents(&self.app_dir, &path);
                report.removed.push(rel.clone());
            }
        }
        Ok(report)
    }
}

/// The relative paths the harness must never sweep: the ops manifest it was
/// loaded from (when it lives under `app_dir`) and the `testkit/` crate.
pub(super) fn protected_paths(app_dir: &Path, manifest_path: &Path) -> Vec<String> {
    let mut protected = vec!["testkit/".to_string()];
    let app = app_dir
        .canonicalize()
        .unwrap_or_else(|_| app_dir.to_path_buf());
    let manifest = manifest_path
        .canonicalize()
        .unwrap_or_else(|_| manifest_path.to_path_buf());
    if let Ok(rel) = manifest.strip_prefix(&app) {
        protected.push(rel.to_string_lossy().replace('\\', "/"));
    }
    protected
}

fn parse_generated(raw: &str) -> Option<BTreeSet<String>> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let list = value.get("generated")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    )
}

/// A manifest entry the sweep may act on: relative, no `..`, no root.
fn is_contained_relative(rel: &str) -> bool {
    !rel.is_empty()
        && rel != MANIFEST_FILENAME
        && Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn is_protected(rel: &str, protected: &[String]) -> bool {
    protected.iter().any(|p| {
        if let Some(dir) = p.strip_suffix('/') {
            rel == dir || rel.starts_with(p.as_str())
        } else {
            rel == p
        }
    })
}

/// Remove directories the sweep emptied, up to (not including) `root`.
fn remove_empty_parents(root: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) {
            break;
        }
        if std::fs::remove_dir(d).is_err() {
            break; // not empty (or gone) — stop climbing
        }
        dir = d.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn manifest(entries: &[&str]) -> String {
        serde_json::json!({
            "root": ".",
            "generated": entries,
            "generatedBy": "codegraph test",
            "codegraphCommit": "test",
        })
        .to_string()
    }

    /// A generation that writes `files` and a FRESH manifest naming them —
    /// what the real generator does once the prior manifest is out of the
    /// way.
    fn generate(root: &Path, files: &[&str]) {
        for f in files {
            write(root, f, "regenerated");
        }
        write(root, MANIFEST_FILENAME, &manifest(files));
    }

    #[test]
    fn regeneration_sweeps_only_files_a_previous_generation_wrote() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Previous generation: two contracts, a migration, the ops manifest,
        // the testkit crate.
        let previous = [
            "hurl/10_core_person.hurl",
            "hurl/11_core_person_actions.hurl",
            "migrations/000500_core_gone.sql",
            "src/lib.rs",
            "codegraph-ops.toml",
            "testkit/src/main.rs",
        ];
        for f in previous {
            write(root, f, "old");
        }
        write(root, MANIFEST_FILENAME, &manifest(&previous));
        // Hand-authored files never appear in a manifest.
        write(root, "hurl/90_hand_authored.hurl", "mine");

        let prior = PriorGeneration::take(root).unwrap();
        assert!(
            !root.join(MANIFEST_FILENAME).exists(),
            "the prior manifest is moved out of the generator's merge path"
        );
        // This generation renumbers the actions contract, drops an entity,
        // and (profile without `ops`) does not rewrite codegraph-ops.toml.
        generate(
            root,
            &[
                "hurl/10_core_person.hurl",
                "hurl/12_core_person_actions.hurl",
                "src/lib.rs",
            ],
        );
        let report = prior
            .sweep(&protected_paths(root, &root.join("codegraph-ops.toml")))
            .unwrap();

        assert_eq!(
            report.removed,
            vec![
                "hurl/11_core_person_actions.hurl".to_string(),
                "migrations/000500_core_gone.sql".to_string(),
            ]
        );
        assert!(!root.join("hurl/11_core_person_actions.hurl").exists());
        assert!(
            !root.join("migrations").exists(),
            "an emptied dir is removed"
        );
        assert!(root.join("hurl/90_hand_authored.hurl").exists());
        assert!(root.join("hurl/12_core_person_actions.hurl").exists());
        assert!(
            root.join("codegraph-ops.toml").exists(),
            "the suite's own manifest stays"
        );
        assert!(
            root.join("testkit/src/main.rs").exists(),
            "the running testkit stays"
        );
        assert_eq!(
            report.protected,
            vec![
                "codegraph-ops.toml".to_string(),
                "testkit/src/main.rs".to_string()
            ]
        );
        let line = report.summary();
        assert!(line.contains("removed 2 stale file(s)"), "{line}");
        assert!(line.contains("(roots: hurl, migrations)"), "{line}");
    }

    #[test]
    fn no_prior_manifest_sweeps_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "hurl/10_old.hurl", "unknown provenance");
        let prior = PriorGeneration::take(root).unwrap();
        generate(root, &["hurl/20_new.hurl"]);
        let report = prior.sweep(&[]).unwrap();
        assert!(report.no_prior);
        assert!(report.removed.is_empty());
        assert!(
            root.join("hurl/10_old.hurl").exists(),
            "never delete on a guess"
        );
        assert!(report.summary().contains("nothing removed"));
    }

    #[test]
    fn escaping_or_absolute_entries_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("app");
        std::fs::create_dir_all(&root).unwrap();
        write(tmp.path(), "outside.txt", "not ours");
        write(
            &root,
            MANIFEST_FILENAME,
            &manifest(&["../outside.txt", "/etc/hostname"]),
        );
        let prior = PriorGeneration::take(&root).unwrap();
        generate(&root, &[]);
        let report = prior.sweep(&[]).unwrap();
        assert!(report.removed.is_empty());
        assert!(tmp.path().join("outside.txt").exists());
    }

    #[test]
    fn failed_regeneration_restores_the_prior_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let original = manifest(&["hurl/10_a.hurl"]);
        write(root, MANIFEST_FILENAME, &original);
        let prior = PriorGeneration::take(root).unwrap();
        prior.restore().unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join(MANIFEST_FILENAME)).unwrap(),
            original
        );
    }

    #[test]
    fn sweep_refuses_without_a_regenerated_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "hurl/10_a.hurl", "old");
        write(root, MANIFEST_FILENAME, &manifest(&["hurl/10_a.hurl"]));
        let prior = PriorGeneration::take(root).unwrap();
        let err = prior.sweep(&[]).unwrap_err();
        assert!(err.to_string().contains("refusing to sweep"), "{err}");
        assert!(root.join("hurl/10_a.hurl").exists());
    }
}
