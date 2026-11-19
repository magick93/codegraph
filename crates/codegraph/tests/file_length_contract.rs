//! File-length contract (issue #751).
//!
//! Every `crates/*/src/**/*.rs` file must stay under [`MAX_LINES`] lines.
//! Clippy has no file-length lint (`too_many_lines` is per-function), so
//! the contract is a plain Rust test: it runs in the standard CI `test`
//! job with zero extra tooling. When a file grows past the limit the fix
//! is a module split, not a larger number.
//!
//! [`ALLOWED_OVER_LIMIT`] lists the files that were over the limit when
//! the contract landed; each entry is a tracked follow-up (see issue
//! #751's follow-up list). A stale entry — a file now under the limit —
//! FAILS this test, so the list can only shrink. Integration-test
//! directories (`crates/*/tests/`) are deliberately out of scope: the
//! contract guards hand-maintained production source, and the largest
//! offenders there are exercised by their own suites.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// Maximum lines per source file.
const MAX_LINES: usize = 2_000;

/// Files over [`MAX_LINES`] when the contract landed. When one of these
/// is split under the limit, its entry MUST be removed in the same PR —
/// a stale entry fails [`src_files_stay_under_the_line_limit`].
const ALLOWED_OVER_LIMIT: &[(&str, &str)] = &[
    (
        "crates/codegraph-generate/src/ifml/route_generator/tests.rs",
        "IFML route-generator unit tests; split per fixture family",
    ),
    (
        "crates/codegraph-generate/src/ifml/route_generator/context.rs",
        "build_page_context + friends; split per component kind",
    ),
    (
        "crates/codegraph/src/init/commands.rs",
        "init/doctor/add-domain command surface; split per subcommand",
    ),
    (
        "crates/codegraph/src/driver.rs",
        "driver::run pipeline; split per pass",
    ),
    (
        "crates/codegraph-generate/src/ddd/body_lower.rs",
        "#662 statement-body lowering; split stmt/expr lowering",
    ),
    (
        "crates/codegraph-generate/src/scaffold/worker.rs",
        "workers-topology scaffold; split scaffold/deploy-derivation",
    ),
    (
        "crates/codegraph-generate/src/ui/page.rs",
        "entity UI page contexts; split per page kind",
    ),
];

/// `CARGO_MANIFEST_DIR` for this test is `crates/codegraph`; the
/// workspace root is two levels up.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .expect("CARGO_MANIFEST_DIR sits two levels below the workspace root")
}

/// Every `.rs` file under `crates/<crate>/src`, recursively. Crate
/// discovery is shallow (one `src/` per direct child of `crates/`);
/// everything below a crate's `src` tree is walked in full (no
/// `target/` directory can appear under `src`).
fn src_rust_files(root: &Path) -> Vec<PathBuf> {
    let crates_dir = root.join("crates");
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let src = entry.path().join("src");
            if src.is_dir() {
                stack.push(src);
            }
        }
    }
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files
}

fn line_count(path: &Path) -> usize {
    std::fs::read(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .into_iter()
        .filter(|&b| b == b'\n')
        .count()
}

#[test]
fn src_files_stay_under_the_line_limit() {
    let root = workspace_root();
    let files = src_rust_files(&root);
    assert!(
        files.len() > 700,
        "walked only {} source files under {}/crates — did the workspace root resolve wrong?",
        files.len(),
        root.display()
    );

    let allowed: HashSet<&str> = ALLOWED_OVER_LIMIT.iter().map(|(p, _)| *p).collect();
    let mut satisfied_entries: BTreeSet<&str> = BTreeSet::new();
    let mut violations: Vec<String> = Vec::new();

    for file in &files {
        let rel = file
            .strip_prefix(&root)
            .expect("walked files live under the workspace root")
            .to_string_lossy()
            .replace('\\', "/");
        let lines = line_count(file);
        if lines <= MAX_LINES {
            continue;
        }
        if allowed.contains(rel.as_str()) {
            continue;
        }
        violations.push(format!(
            "{rel}: {lines} lines (limit {MAX_LINES}) — split it into modules"
        ));
    }

    // The allow-list can only shrink: an entry whose file now fits is
    // stale and must be removed.
    for (path, _) in ALLOWED_OVER_LIMIT {
        let abs = root.join(path);
        if !abs.exists() {
            violations.push(format!(
                "allow-list entry {path} does not exist — remove the stale entry"
            ));
            continue;
        }
        let lines = line_count(&abs);
        if lines <= MAX_LINES {
            satisfied_entries.insert(*path);
        }
    }
    if !satisfied_entries.is_empty() {
        violations.push(
            satisfied_entries
                .into_iter()
                .map(|p| format!("allow-list entry {p} is now under the limit — remove it"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    assert!(
        violations.is_empty(),
        "file-length contract violated (limit {MAX_LINES}):\n{}",
        violations.join("\n")
    );
}
