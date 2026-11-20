//! The review fixture's cross-process gate and per-recipe produced-file
//! ledgers (issue #694).
//!
//! `crates/review/generated-candidate/` is a persistent, CHECKED-IN tree that
//! FOUR test paths regenerate in place, from TWO different recipes:
//!
//! | path | recipe |
//! |---|---|
//! | `entity_codegen_gate`, `review_api_suite`, `rules_guard_api_gate` | flag-ON (`review_api_suite_regen`) |
//! | `grafeo_candidate_inspect_output` (PR CI), `review_fixture_flag_off_regen`'s in-place helper | flag-OFF |
//!
//! The #694 rule — regenerate from clean or key discovery on content — needs
//! two properties, and neither is `--test-threads=1`:
//!
//! 1. **Serialized** — every mutator takes [`FixtureGate`], an exclusive
//!    cross-process file lock. `--test-threads=1` only serializes one binary;
//!    the lock covers all four call paths, any thread count, and a subset run.
//! 2. **From clean** — [`sweep_produced`] deletes the files the previous
//!    generation of the SAME recipe wrote. The set comes from that recipe's
//!    own generation report, not a hand-kept allowlist, so committed extras
//!    and gate-owned stubs can never appear in a ledger and are never deleted.
//!
//! The sweep is deliberately PER RECIPE, and one ledger per recipe is recorded
//! so the two sets are visible side by side. The recipes are NOT subsets of one
//! another, so a cross-recipe sweep is destructive rather than clean: the
//! flag-OFF recipe produces 65 paths the flag-ON recipe neither produces nor
//! restores — `testkit/`, `codegraph-ops.toml`, `api-contracts/`, `svelte/`,
//! `e2e/` (the flag-ON plan strips the `ops` generator precisely because the
//! checked-in `codegraph-ops.toml` carries hand-written sections the
//! `review_api_suite` gate asserts on). Sweeping the flag-OFF ledger from a
//! flag-ON provision would therefore delete `testkit/` + `codegraph-ops.toml`
//! and leave the tree unbuildable for the live gate. Flag-switch residue is
//! covered by the other end instead: `grafeo_candidate_inspect_output` (the
//! flag-OFF origin, PR CI) wipes the tree WHOLESALE before regenerating, and
//! the flag-ON gates' documented hand-written sweeps (the retired CRUD stores)
//! own what a pre-#694 checkout left behind.
//!
//! Ledgers are still recorded for BOTH recipes: that is what makes each
//! recipe's from-clean true (the flag-OFF maintenance helper now sweeps its own
//! retired files instead of relying on a hand-deletion note), and it is the
//! input a future distinct-directory design would need.
//!
//! The remaining boundary, stated so nobody reads it as more than it is: a
//! ledger covers exactly the files a previous run of that recipe produced. A
//! tree produced before #694 has no ledgers, and a hand-authored file is never
//! in one.
//!
//! Included by path from four test binaries; `#[allow(dead_code)]` at each
//! include site because no single binary uses every item.

use std::path::{Component, Path, PathBuf};

/// Which recipe a ledger belongs to. One file per recipe, so a flag-OFF run
/// never overwrites the flag-ON recipe's recollection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    FlagOn,
    FlagOff,
}

impl Recipe {
    fn ledger_file(self) -> &'static str {
        match self {
            Recipe::FlagOn => "produced-flag-on.txt",
            Recipe::FlagOff => "produced-flag-off.txt",
        }
    }
}

/// Every recipe, for callers that need to enumerate the recorded sets (each
/// one sweeps only its own; see [`sweep_produced`]).
pub const RECIPES: [Recipe; 2] = [Recipe::FlagOn, Recipe::FlagOff];

/// Run state for the review-fixture mutators: the lock and the ledgers. Lives
/// in the workspace `target/` — never inside the committed fixture (machine
/// state must not reach the tracked bytes) and never in
/// `crates/codegraph/target/` (issue #683a: that is not the workspace target
/// dir).
///
/// Resolved from this file's location (`<repo>/crates/codegraph/tests/…`), so
/// it is independent of `CARGO_MANIFEST_DIR` of the including binary.
fn state_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/review-fixture-gate")
}

fn lock_path() -> PathBuf {
    state_dir().join("gate.lock")
}

fn ledger_path(recipe: Recipe) -> PathBuf {
    state_dir().join(recipe.ledger_file())
}

/// The exclusive, cross-process gate over the review fixture.
///
/// Held for a test's whole body: the gates regenerate AND build in the same
/// tree, so serializing only the regeneration would leave a build reading a
/// tree another test is rewriting. Dropping it releases the lock (the OS also
/// releases it if the process dies).
pub struct FixtureGate {
    file: std::fs::File,
}

impl FixtureGate {
    /// Block until this process is the only review-fixture mutator.
    ///
    /// `std::fs::File::lock` is `flock(2)` on unix: a second handle — even in
    /// the SAME process — blocks, which is what serializes two
    /// `#[tokio::test]` functions of one binary without `--test-threads=1`.
    /// The wait is a plain blocking syscall by design (a lock is not
    /// cancellable), and it ANNOUNCES itself: the heavy gates can queue for
    /// 10-20 minutes, and a silent block is indistinguishable from a hang.
    pub fn acquire() -> Result<Self, String> {
        let dir = state_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let path = lock_path();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("open gate lock {}: {e}", path.display()))?;
        println!(
            "gate: waiting for the review-fixture gate {} — four test paths share \
             this tree under one lock (issue #694); a queued wait can be minutes",
            path.display()
        );
        file.lock()
            .map_err(|e| format!("acquire gate lock {}: {e}", path.display()))?;
        println!("gate: review-fixture gate acquired");
        Ok(Self { file })
    }
}

impl Drop for FixtureGate {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Delete the files this recipe's previous generation produced, and any
/// directory the removals leave empty. Returns how many files it removed.
///
/// Per recipe by design (see the module docs: the recipes are not subsets, so a
/// cross-recipe sweep deletes files the current recipe cannot restore).
///
/// A missing ledger (first run, a fresh checkout, or a tree produced before
/// #694) means there is nothing to sweep — correct: the committed tree is the
/// source of truth until this recipe has recorded a generation of its own.
pub fn sweep_produced(output_dir: &Path, recipe: Recipe) -> usize {
    let Ok(ledger) = std::fs::read_to_string(ledger_path(recipe)) else {
        return 0;
    };
    sweep_listed(output_dir, &ledger)
}

/// [`sweep_produced`] over an explicit ledger body (pure over the filesystem,
/// so the #694 rule is unit-testable without a 20-minute gate).
pub fn sweep_listed(output_dir: &Path, ledger: &str) -> usize {
    let mut removed = 0usize;
    let mut touched_dirs: Vec<PathBuf> = Vec::new();
    for rel in ledger.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some(rel_path) = safe_relative(rel) else {
            // A ledger written by something else must never steer a delete
            // outside the fixture.
            continue;
        };
        let path = output_dir.join(&rel_path);
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
            // `Path::new("a.txt").parent()` is `Some("")` — never treat the
            // fixture root as a removable directory.
            if let Some(parent) = rel_path.parent()
                && !parent.as_os_str().is_empty()
            {
                touched_dirs.push(parent.to_path_buf());
            }
        }
    }
    // Deepest first so a whole retired tree collapses rather than leaving a
    // chain of empty directories for the next generation to inherit.
    touched_dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
    touched_dirs.dedup();
    for dir in &touched_dirs {
        let path = output_dir.join(dir);
        // `remove_dir` refuses non-empty directories: a dir still holding
        // committed or gate-owned content stays.
        let _ = std::fs::remove_dir(&path);
    }
    removed
}

/// Record one recipe's produced set (absolute paths inside `output_dir`, or
/// already-relative names). Atomic (temp + rename), so a killed run cannot
/// truncate the ledger and make the next sweep under-delete.
pub fn record_produced(
    output_dir: &Path,
    recipe: Recipe,
    produced: &[PathBuf],
) -> Result<(), String> {
    let path = ledger_path(recipe);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    record_to(&path, output_dir, produced)
}

fn record_to(path: &Path, output_dir: &Path, produced: &[PathBuf]) -> Result<(), String> {
    let text = ledger_text(output_dir, produced);
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))
}

/// The ledger body for a produced set: fixture-relative, sorted, de-duplicated,
/// `/`-separated, one per line. Paths outside the fixture are dropped.
pub fn ledger_text(output_dir: &Path, produced: &[PathBuf]) -> String {
    let mut rels: Vec<String> = produced
        .iter()
        .filter_map(|path| relative_inside(output_dir, path))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    rels.sort();
    rels.dedup();
    format!("{}\n", rels.join("\n"))
}

/// A produced path as a fixture-relative path, or `None` when it lives
/// outside the fixture (a hooks/domain-types dir redirected to a tempdir).
fn relative_inside(output_dir: &Path, path: &Path) -> Option<PathBuf> {
    let rel = if path.is_absolute() {
        path.strip_prefix(output_dir).ok()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    safe_relative(&rel.to_string_lossy()).map(|_| rel)
}

/// A ledger entry is only ever a plain relative path inside the fixture.
pub fn safe_relative(rel: &str) -> Option<PathBuf> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return None;
    }
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    /// The from-clean rule: the previous generation's files go, everything the
    /// recipe did not write (committed extras, gate-owned stubs) stays, and a
    /// directory the sweep empties is pruned.
    #[test]
    fn the_sweep_deletes_only_the_previous_generations_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/domain/x/mod.rs");
        write(root, "hurl/11_payroll_pay_run_actions.hurl");
        write(root, "ui/forms/text.svelte"); // consumer-authored
        write(root, "testkit/Cargo.toml"); // committed extra
        write(root, "codegraph-ops.toml"); // committed manifest

        let ledger = "src/domain/x/mod.rs\nhurl/11_payroll_pay_run_actions.hurl\n";
        assert_eq!(sweep_listed(root, ledger), 2);

        assert!(!root.join("src/domain/x/mod.rs").exists());
        assert!(!root.join("hurl/11_payroll_pay_run_actions.hurl").exists());
        assert!(!root.join("hurl").exists(), "an emptied dir is pruned");
        assert!(!root.join("src/domain/x").exists());
        assert!(root.join("ui/forms/text.svelte").is_file());
        assert!(root.join("testkit/Cargo.toml").is_file());
        assert!(root.join("codegraph-ops.toml").is_file());
    }

    /// A ledger entry that escapes the fixture (a hand-edited or foreign
    /// ledger) is ignored — never a delete outside the tree.
    #[test]
    fn the_sweep_refuses_paths_outside_the_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().parent().unwrap().join("outside-694.txt");
        std::fs::write(&outside, "keep me").unwrap();

        let root = tmp.path().join("fixture");
        std::fs::create_dir_all(&root).unwrap();
        let ledger = format!("../outside-694.txt\n{}\n", outside.to_string_lossy());
        assert_eq!(sweep_listed(&root, &ledger), 0);
        assert!(
            outside.is_file(),
            "a path outside the fixture is never removed"
        );
        let _ = std::fs::remove_file(&outside);
    }

    /// A top-level ledger entry must never remove the fixture root itself
    /// (`Path::new("a.txt").parent()` is `Some("")`).
    #[test]
    fn the_sweep_never_prunes_the_fixture_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("fixture");
        std::fs::create_dir_all(&root).unwrap();
        write(&root, "Cargo.toml");
        assert_eq!(sweep_listed(&root, "Cargo.toml\n"), 1);
        assert!(root.is_dir(), "the fixture root must survive the sweep");
    }

    /// The ledger round-trips: recorded paths are what the next sweep removes,
    /// sorted and de-duplicated, relativized against the fixture.
    #[test]
    fn the_ledger_round_trips_a_produced_set() {
        let root = Path::new("/fixture");
        let produced = vec![
            PathBuf::from("/fixture/src/b.rs"),
            PathBuf::from("/fixture/src/a.rs"),
            PathBuf::from("/fixture/src/a.rs"),
            PathBuf::from("/elsewhere/hooks.rs"),
        ];
        assert_eq!(ledger_text(root, &produced), "src/a.rs\nsrc/b.rs\n");
        assert_eq!(safe_relative("src/a.rs"), Some(PathBuf::from("src/a.rs")));
        assert_eq!(safe_relative("/etc/passwd"), None);
        assert_eq!(safe_relative("../escape.rs"), None);
    }

    /// Recording replaces (never appends) and leaves no temp file behind, so a
    /// killed run cannot truncate the ledger into an under-deleting sweep.
    #[test]
    fn recording_replaces_the_ledger_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("fixture");
        std::fs::create_dir_all(&root).unwrap();
        let ledger = tmp.path().join("produced.txt");

        let first = vec![root.join("src/a.rs"), root.join("src/b.rs")];
        record_to(&ledger, &root, &first).unwrap();
        assert_eq!(
            std::fs::read_to_string(&ledger).unwrap(),
            "src/a.rs\nsrc/b.rs\n"
        );
        assert!(!ledger.with_extension("tmp").exists());

        // A smaller set REPLACES the previous one (a merge would sweep files
        // the current generation still owns).
        record_to(&ledger, &root, &first[..1]).unwrap();
        assert_eq!(std::fs::read_to_string(&ledger).unwrap(), "src/a.rs\n");
        assert!(!ledger.with_extension("tmp").exists());
    }
}
