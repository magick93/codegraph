//! Review-fixture drift pin (issue #747).
//!
//! The committed `crates/review/generated-candidate/` tree is maintained by
//! `grafeo_candidate_inspect_output` — the PR-CI flag-OFF default-opts test
//! that regenerates the fixture IN PLACE on every full test run — but until
//! #747 nothing held the COMMITTED bytes against the generators: every
//! template change that shipped without a fixture regen widened the drift
//! silently and surfaced later as an unexplained dirty tree (the #433
//! catch-up class; #747 itself accumulated the #691 `error.rs`
//! constructors and the #700 child-tenancy migration rewrites as a
//! 17-file backlog).
//!
//! The pin runs the SAME flag-OFF recipe
//! (`review_fixture_flag_off_regen::regenerate_flag_off_fixture_into` —
//! the `grafeo_candidate_inspect_output` pipeline, ingest included via
//! `#[path]`) into a scratch directory and byte-compares every produced
//! file against the COMMITTED bytes, materialized with `git archive HEAD`.
//! Comparing against git rather than the working tree is deliberate: it
//! makes the pin hermetic against the very maintenance it pins — the
//! inspect test (and the in-place helper below) rewrite the fixture under
//! the running test's feet; the pin holds the COMMIT, so local dirt never
//! matters and the next un-regenerated template change fails in PR CI with
//! the differing files named. Pin recipe == in-place recipe is the point:
//! after any full test run (which rewrites the fixture in place with this
//! exact recipe) the committed tree and the pin agree, so the oscillation
//! that motivated #747 is over.
//!
//! Node-free and DB-free: the recipe ingests into an in-memory Grafeo
//! engine and no npm/node step is involved.
//!
//! Three artifact classes are compared by consistency rather than bytes:
//! - the run manifests (merge + if-absent semantics make byte equality
//!   impossible across a scratch-dir vs maintained-tree run — see
//!   `MANIFEST_FILES`);
//! - `testkit/Cargo.toml` renders its `codegraph-ops` path dep relative to
//!   wherever the recipe ran, so the pin normalizes that one line and
//!   instead asserts the committed spelling is the fixture-relative
//!   `../../../codegraph-ops` (machine paths must never reach the
//!   committed bytes — see `OUTPUT_DIR_RELATIVE_FILES`);
//! - the deliberate checked-in content the flag-OFF pipeline does not
//!   produce (`committed_extra`): consumer-authored `ui/forms/` and the
//!   retired-era trees (`api-contracts/`, `svelte/`, `e2e/`). Removing any
//!   of those is a deliberate decision — shrink `committed_extra` in the
//!   same commit.
//!
//! Maintenance: `regenerate_committed_fixture_in_place` (ignored) rewrites
//! the committed fixture with the same recipe, so the fixture and the pin
//! can never disagree about what "regenerated" means.

#[allow(dead_code)]
#[path = "review_fixture_flag_off_regen.rs"]
mod review_fixture_flag_off_regen;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The fixture's path inside the repository — the `git archive` prefix and
/// the root the comparison walks.
const FIXTURE_PREFIX: &str = "crates/review/generated-candidate";

/// Run-scoped manifests. `.codegraph-manifest.json` MERGES with any
/// manifest already on disk (sequential profile runs accumulate, per the
/// manifest module docs) and omits files a run SKIPPED because they
/// already existed (the if-absent scaffold sources) — so a scratch-dir
/// run's manifest can never be byte-identical to a maintained tree's.
/// They are compared by consistency instead of bytes: both sides must
/// parse, carry the same `generatedBy`, and the committed manifests must
/// not name files that no longer exist (dangling claims = stale cleanup
/// debt).
const MANIFEST_FILES: &[&str] = &[
    ".codegraph-manifest.json",
    "migrations/.codegraph-manifest.json",
];

/// Cap each failure category at this many named files so a drifted run
/// stays readable; the count line always carries the true total.
const MAX_REPORTED: usize = 20;

/// Committed fixture content the flag-OFF recipe deliberately does NOT
/// produce (they must never be reported as stale):
/// - `ui/forms/` is consumer-authored fixture content — the
///   `entity_codegen_gate` treats it the same way (reported, not
///   asserted);
/// - `api-contracts/`, `svelte/`, `e2e/` are retired-era trees the fixture
///   still carries. The current flag-OFF pipeline does not produce them
///   and #747's scope deliberately does NOT delete them (the fixture
///   tracks the flag-OFF recipe with the modest delta the mandate names —
///   era cleanup is a separate decision).
///
/// Note `codegraph-ops.toml` + `testkit/` are NOT here: the ops generator
/// IS in the flag-OFF plan and the recipe produces them (only
/// `testkit/Cargo.toml` needs special byte handling — see
/// `OUTPUT_DIR_RELATIVE_FILES`).
fn committed_extra(rel: &str) -> bool {
    rel.starts_with("ui/forms/")
        || rel.starts_with("api-contracts/")
        || rel.starts_with("svelte/")
        || rel.starts_with("e2e/")
}

/// Files whose bytes legitimately depend on WHERE the recipe ran, so a
/// scratch-dir pin run cannot byte-match the committed (in-place) bytes:
/// `testkit/Cargo.toml` renders the `codegraph-ops` path dep relative to
/// the output dir (`ops::resolve_ops_path`): in place it is
/// `../../../codegraph-ops`, from a scratch dir it is a long relative walk
/// back to this checkout. Everything else in the file must match, and the
/// committed dep must be exactly the fixture-relative spelling — that
/// assertion is what keeps machine-specific paths out of the committed
/// bytes (#747).
const OUTPUT_DIR_RELATIVE_FILES: &[&str] = &["testkit/Cargo.toml"];

/// The one line that legitimately differs between a scratch-dir run and
/// the committed tree, and the committed spelling it must carry.
const OPS_DEP_LINE_PREFIX: &str = "codegraph-ops = { path = ";
const COMMITTED_OPS_DEP_LINE: &str = "codegraph-ops = { path = \"../../../codegraph-ops\" }";

/// Compare one file, applying the output-dir-relative normalization for
/// [`OUTPUT_DIR_RELATIVE_FILES`]. Returns `Some(detail)` on a real drift.
fn files_differ(rel: &str, fresh: &Path, committed: &Path) -> Option<String> {
    let fresh_bytes = fs::read(fresh).unwrap_or_default();
    let committed_bytes = fs::read(committed).unwrap_or_default();
    if !OUTPUT_DIR_RELATIVE_FILES.contains(&rel) {
        return (fresh_bytes != committed_bytes).then(|| "bytes differ".to_string());
    }
    // Normalize: drop the differing dep line from both sides, compare the
    // remainder byte-for-byte, then assert the committed spelling.
    let norm = |bytes: &[u8]| -> Vec<String> {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter(|line| !line.starts_with(OPS_DEP_LINE_PREFIX))
            .map(str::to_string)
            .collect()
    };
    if norm(&fresh_bytes) != norm(&committed_bytes) {
        return Some("non-dep lines drifted".into());
    }
    let committed_text = String::from_utf8_lossy(&committed_bytes);
    if !committed_text
        .lines()
        .any(|line| line == COMMITTED_OPS_DEP_LINE)
    {
        return Some(format!(
            "the committed codegraph-ops dep is not the fixture-relative spelling \
             (`{COMMITTED_OPS_DEP_LINE}`) — machine-specific paths must not leak into \
             the committed bytes"
        ));
    }
    None
}

/// Every file under `root`, as forward-slash paths relative to `root`,
/// sorted for deterministic comparison and reporting.
fn walk_files(root: &Path) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    walk_inner(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_inner(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_inner(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

/// Materialize the COMMITTED fixture bytes into `dest` via
/// `git archive HEAD`, run at the worktree root (`git rev-parse
/// --show-toplevel` — pathspecs are CWD-limited and the test binary runs
/// with the package directory as CWD). Works from any checkout/worktree
/// (including the shallow single-commit fetch CI uses: the tip commit's
/// full tree is present by definition). The extracted tree lands at
/// `dest/{FIXTURE_PREFIX}`.
fn materialize_committed_fixture(dest: &Path) -> Result<PathBuf, String> {
    let toplevel = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|e| format!("spawn git rev-parse: {e}"))?
            .stdout,
    )
    .map_err(|e| format!("git rev-parse output: {e}"))?
    .trim()
    .to_string();
    if toplevel.is_empty() {
        return Err("git rev-parse --show-toplevel returned nothing (not a git checkout?)".into());
    }
    let archive = dest.join("fixture.tar");
    let output = Command::new("git")
        .current_dir(&toplevel)
        .args(["archive", "--format=tar", "-o"])
        .arg(&archive)
        .arg("HEAD")
        .arg(FIXTURE_PREFIX)
        .output()
        .map_err(|e| format!("spawn git archive: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git archive HEAD -- {FIXTURE_PREFIX} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let output = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(dest)
        .output()
        .map_err(|e| format!("spawn tar: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "tar -xf {} failed: {}",
            archive.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let root = dest.join(FIXTURE_PREFIX);
    if !root.is_dir() {
        return Err(format!(
            "git archive did not materialize {FIXTURE_PREFIX} under {}",
            dest.display()
        ));
    }
    Ok(root)
}

/// One failure category, capped at [`MAX_REPORTED`] named files.
fn category(label: &str, files: &[String]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let mut report = format!("\n{label} ({} total):\n", files.len());
    for rel in files.iter().take(MAX_REPORTED) {
        report.push_str(&format!("  - {rel}\n"));
    }
    if files.len() > MAX_REPORTED {
        report.push_str(&format!("  … and {} more\n", files.len() - MAX_REPORTED));
    }
    report
}

/// A parsed `.codegraph-manifest.json` (only the fields the pin checks).
#[derive(serde::Deserialize)]
struct FixtureManifest {
    #[serde(rename = "generatedBy")]
    generated_by: String,
    #[serde(default)]
    generated: Vec<String>,
}

/// Parse a run manifest; any parse failure is a drift finding (the
/// manifests are generated artifacts — a malformed one is template drift).
fn parse_manifest(path: &Path) -> Result<FixtureManifest, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// The manifest consistency checks (see [`MANIFEST_FILES`]): same
/// generator identity both sides, and no dangling claims in the committed
/// manifests — every listed file must exist in the committed tree.
/// Manifest `generated` entries are relative to the MANIFEST'S OWN
/// directory (the root manifest's entries are fixture-relative; the
/// migrations manifest's entries are migrations-relative).
fn check_manifests(
    fresh_root: &Path,
    committed_root: &Path,
    committed_files: &HashSet<String>,
) -> Vec<String> {
    let mut findings = Vec::new();
    for rel in MANIFEST_FILES {
        let fresh = parse_manifest(&fresh_root.join(rel));
        let committed = parse_manifest(&committed_root.join(rel));
        let prefix = match rel.rfind('/') {
            Some(idx) => format!("{}/", &rel[..idx]),
            None => String::new(),
        };
        match (fresh, committed) {
            (Ok(fresh), Ok(committed)) => {
                if fresh.generated_by != committed.generated_by {
                    findings.push(format!(
                        "{rel}: generatedBy drifted (fresh `{}`, committed `{}`)",
                        fresh.generated_by, committed.generated_by
                    ));
                }
                for entry in &committed.generated {
                    let claimed = format!("{prefix}{entry}");
                    if !committed_files.contains(&claimed) {
                        findings.push(format!(
                            "{rel}: lists `{claimed}` which no longer exists \
                             (stale claim — regenerate the fixture)"
                        ));
                    }
                }
            }
            (Err(e), _) | (_, Err(e)) => findings.push(e),
        }
    }
    findings
}

/// The pin: the committed fixture must be EXACTLY what the flag-OFF recipe
/// produces on the current generators — every generated file present with
/// identical bytes, and no committed file the current generators no longer
/// produce (beyond the deliberate checked-in extras). Fix on failure: run
/// the ignored `regenerate_committed_fixture_in_place` helper and commit
/// the tree.
#[tokio::test]
async fn committed_fixture_matches_the_current_generators() {
    let fresh = tempfile::tempdir().expect("scratch regen dir");
    review_fixture_flag_off_regen::regenerate_flag_off_fixture_into(fresh.path())
        .await
        .expect("the flag-OFF fixture regeneration must succeed with no generation errors");

    let committed_tmp = tempfile::tempdir().expect("committed-tree scratch dir");
    let committed_root = materialize_committed_fixture(committed_tmp.path())
        .expect("materialize the committed fixture from git");

    let fresh_files = walk_files(fresh.path()).expect("walk the fresh tree");
    let committed_files = walk_files(&committed_root).expect("walk the committed tree");

    let committed_set: HashSet<String> = committed_files.iter().cloned().collect();
    let fresh_set: HashSet<String> = fresh_files.iter().cloned().collect();

    let mut missing: Vec<String> = Vec::new();
    let mut modified: Vec<String> = Vec::new();
    for rel in &fresh_files {
        if MANIFEST_FILES.contains(&rel.as_str()) {
            continue; // compared by consistency, not bytes (see MANIFEST_FILES)
        }
        if !committed_set.contains(rel) {
            missing.push(rel.clone());
        } else if let Some(detail) =
            files_differ(rel, &fresh.path().join(rel), &committed_root.join(rel))
        {
            modified.push(format!("{rel} ({detail})"));
        }
    }
    let stale: Vec<String> = committed_files
        .iter()
        .filter(|rel| {
            !MANIFEST_FILES.contains(&rel.as_str())
                && !fresh_set.contains(*rel)
                && !committed_extra(rel)
        })
        .cloned()
        .collect();

    let manifest_findings = check_manifests(fresh.path(), &committed_root, &committed_set);

    let mut report = format!(
        "{}{}{}",
        category(
            "generated file missing from the committed fixture",
            &missing
        ),
        category(
            "generated file whose bytes differ from the committed bytes",
            &modified
        ),
        category(
            "committed file the current generators no longer produce",
            &stale
        ),
    );
    if !manifest_findings.is_empty() {
        report.push_str(&format!(
            "\nrun-manifest consistency ({} total):",
            manifest_findings.len()
        ));
        for finding in manifest_findings.iter().take(MAX_REPORTED) {
            report.push_str(&format!("\n  - {finding}"));
        }
        if manifest_findings.len() > MAX_REPORTED {
            report.push_str(&format!(
                "\n  … and {} more\n",
                manifest_findings.len() - MAX_REPORTED
            ));
        }
        report.push('\n');
    }
    assert!(
        report.is_empty(),
        "the committed review fixture drifted from the current generators.\
         \nFix: run `cargo test -p codegraph --test review_fixture_drift_tests -- --ignored \
         --nocapture regenerate_committed_fixture_in_place`, review the diff, and commit the \
         regenerated tree with an enumerated message.{report}"
    );
}

/// Maintenance helper (ignored): rewrite the committed fixture in place
/// from the current generators. This is THE way to refresh the fixture —
/// the drift pin compares against exactly this output, so the fixture and
/// the pin can never disagree about what "regenerated" means.
///
/// Unlike `grafeo_candidate_inspect_output` this does NOT wipe the tree:
/// the checked-in content the flag-OFF pipeline does not produce
/// (consumer-authored `ui/forms/`, the retired-era trees) survives,
/// because the pipeline's own `clean_generated_output` sweeps only
/// entity-scoped output. Files a template change RETIRES elsewhere (hurl
/// renumbering, retired UI routes) are named by the pin's "no longer
/// produce" category — delete them by hand, then re-run this helper so
/// the run manifests list exactly the surviving tree.
#[tokio::test]
#[ignore = "maintenance: rewrites crates/review/generated-candidate in place (issue #747)"]
async fn regenerate_committed_fixture_in_place() {
    review_fixture_flag_off_regen::regenerate_flag_off_fixture()
        .await
        .expect("fixture regeneration must succeed");
}
