//! Committed fixture locks must track the workspace's dependency resolution
//! (issue #841).
//!
//! `crates/codegraph/tests/fixtures/workers_gate/Cargo.lock` is a real,
//! committed lockfile for the fixture's OWN workspace (the WS2 workers live
//! gate's regeneration step runs `cargo run -p gate-graph` there). Cargo does
//! not repair a locked package that conflicts with a newly required one — not
//! even without `--locked` — so a stale lock fails at RESOLUTION instead of
//! re-resolving. That is exactly what happened when the workspace moved to
//! `phero-* @ 958acd0b` + `xstate-core @ da0c804d` (pherolang#387/#388, issue
//! #815): the new graph pulls `oxc_allocator 0.77` → `bumpalo =3.19.0`, while
//! the fixture lock still pinned `bumpalo 3.20.3` through the OLD phero rev, so
//! `cargo metadata --locked` exited 101 and the nightly Workers Gate was red
//! from 2026-11-25.
//!
//! The nightly lane is too slow to be the only guard: this test is the
//! node-free PR-CI pin. It reads the two lockfiles as text (`Cargo.lock` is
//! TOML) and asserts
//!
//! 1. every git-sourced package present in BOTH locks resolves to the SAME
//!    commit — generic, so a phero/restingstate/sigil/decision-model pin move
//!    is caught by rev and not by package name;
//! 2. `bumpalo` agrees — the concrete package whose version conflict made the
//!    lock unsatisfiable. Comparing one transitive package would be brittle on
//!    its own (any `cargo update` can move it), which is why it rides beside
//!    the general rev rule: the failure mode this pins is "the lock is
//!    unsatisfiable", and `bumpalo` is the package that proves it.
//!
//! ```text
//! cargo test -p codegraph --test workers_gate_fixture_lock_tests
//! ```
//!
//! Fixing a failure: in the fixture workspace run `cargo update` (or the
//! targeted `cargo update -p <pkg> --precise <ver>`) and commit the
//! regenerated `Cargo.lock` in the SAME PR that moves the pin.

use std::path::{Path, PathBuf};

/// The fixture lock this contract guards.
fn fixture_lock_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("workers_gate")
        .join("Cargo.lock")
}

/// The workspace lock the fixture must agree with.
fn workspace_lock_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("Cargo.lock"))
        .expect("crates/codegraph must sit two levels under the workspace root")
}

/// One `[[package]]` entry of a lockfile.
struct LockedPackage {
    name: String,
    version: String,
    source: Option<String>,
}

impl LockedPackage {
    /// The resolved VCS commit of a git-sourced package (the `#<sha>`
    /// fragment cargo records), falling back to the declared `rev=` when the
    /// source carries no fragment. `None` for registry/path packages.
    fn git_commit(&self) -> Option<&str> {
        let source = self.source.as_deref()?;
        if !source.starts_with("git+") {
            return None;
        }
        if let Some((_, fragment)) = source.rsplit_once('#') {
            return Some(fragment);
        }
        source.split_once("rev=").map(|(_, rev)| rev)
    }
}

/// Read a lockfile into its package list.
///
/// A `Cargo.lock` is TOML, so this is a plain parse — no cargo invocation, no
/// build, no network, and no shared `target/` lock.
fn read_lock(path: &Path) -> Vec<LockedPackage> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    let table: toml::Table = text
        .parse()
        .unwrap_or_else(|e| panic!("{} must parse as TOML: {e}", path.display()));
    let packages = table
        .get("package")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("{} must carry a `[[package]]` array", path.display()));
    packages
        .iter()
        .filter_map(|entry| {
            Some(LockedPackage {
                name: entry.get("name")?.as_str()?.to_string(),
                version: entry.get("version")?.as_str()?.to_string(),
                source: entry
                    .get("source")
                    .and_then(toml::Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

fn fixture_lock() -> Vec<LockedPackage> {
    read_lock(&fixture_lock_path())
}

fn workspace_lock() -> Vec<LockedPackage> {
    read_lock(&workspace_lock_path())
}

/// A package's single entry by name (a lock can hold two majors of one crate,
/// so lookups that expect one version use this).
fn single_by_name<'a>(packages: &'a [LockedPackage], name: &str) -> &'a LockedPackage {
    let mut found = packages.iter().filter(|p| p.name == name);
    let first = found
        .next()
        .unwrap_or_else(|| panic!("`{name}` must appear in the lock under test"));
    assert!(
        found.next().is_none(),
        "`{name}` appears more than once in the lock — the comparison below assumes one entry"
    );
    first
}

/// Refresh the fixture lock in the PR that moves the pherolang/restingstate
/// pin.
///
/// Every git dependency the fixture shares with the workspace must resolve to
/// the same commit. A mismatch means the fixture lock predates a pin move.
#[test]
fn fixture_lock_git_revisions_match_the_workspace_pin() {
    let fixture = fixture_lock();
    let workspace = workspace_lock();

    let workspace_git: Vec<(&str, &str)> = workspace
        .iter()
        .filter_map(|p| p.git_commit().map(|commit| (p.name.as_str(), commit)))
        .collect();
    let mut compared = 0usize;
    let mut mismatches = Vec::new();
    for package in &fixture {
        let Some(fixture_commit) = package.git_commit() else {
            continue;
        };
        let Some((_, workspace_commit)) =
            workspace_git.iter().find(|(name, _)| *name == package.name)
        else {
            continue;
        };
        compared += 1;
        if fixture_commit != *workspace_commit {
            mismatches.push(format!(
                "  {} {}: fixture lock -> {fixture_commit}, workspace lock -> {workspace_commit}",
                package.name, package.version
            ));
        }
    }

    // Vacuity guard: the fixture workspace depends on `codegraph`, which
    // depends on `phero-driver`, so its git closure is never empty. If this
    // ever trips, the comparison below silently stopped covering anything.
    let has_phero_driver = fixture.iter().any(|p| p.name == "phero-driver");
    assert!(
        compared >= 10 && has_phero_driver,
        "expected the fixture lock to share the workspace's git closure (compared \
         {compared} package(s), phero-driver present: {has_phero_driver}); the fixture \
         lock was not read as expected"
    );

    assert!(
        mismatches.is_empty(),
        "\n{} fixture lock git revision(s) disagree with the workspace lock:\n{}\n\n\
         Fix: refresh the fixture lock in the PR that moves the pherolang/restingstate \
         pin — `cd crates/codegraph/tests/fixtures/workers_gate && cargo update` (or a \
         targeted `cargo update -p <pkg> --precise <ver>`) and commit the regenerated \
         Cargo.lock (issue #841).\n",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// The fixture lock must resolve `bumpalo` to the workspace's version.
///
/// This is the concrete package that made the post-#815 lock unsatisfiable:
/// `oxc_allocator 0.77` (via `xstate-parser`) requires `bumpalo =3.19.0`,
/// while the stale fixture lock held 3.20.3 for `grafeo-common`.
#[test]
fn fixture_lock_resolves_bumpalo_from_the_workspace() {
    let fixture_packages = fixture_lock();
    let workspace_packages = workspace_lock();
    let fixture = single_by_name(&fixture_packages, "bumpalo");
    let workspace = single_by_name(&workspace_packages, "bumpalo");
    assert_eq!(
        fixture.version, workspace.version,
        "\nbumpalo disagrees: fixture lock resolves {}, the workspace resolves {}.\n\n\
         Fix: refresh the fixture lock in the PR that moves the pherolang/restingstate \
         pin — `cd crates/codegraph/tests/fixtures/workers_gate && cargo update -p bumpalo \
         --precise {}` (issue #841). A stale entry makes `cargo metadata --locked` fail \
         with \"failed to select a version for `bumpalo`\".\n",
        fixture.version, workspace.version, workspace.version
    );
}
