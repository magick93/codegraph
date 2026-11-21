//! File-length contract (issue #751).
//!
//! Every `crates/*/src/**/*.rs` file must stay under [`MAX_LINES`] lines,
//! and every `crates/*/templates/**/*.tera` template must stay under
//! [`MAX_TEMPLATE_LINES`] lines. Clippy has no file-length lint
//! (`too_many_lines` is per-function), so the contract is a plain Rust
//! test: it runs in the standard CI `test` job with zero extra tooling.
//! When a file grows past the limit the fix is a split, not a larger
//! number.
//!
//! [`ALLOWED_OVER_LIMIT`] and [`ALLOWED_TEMPLATES_OVER_LIMIT`] list the
//! files that were over the limit when each half of the contract landed;
//! each entry is a tracked follow-up (see issue #751's follow-up list,
//! and #778 for the template half). A stale entry — a file now under the
//! limit — FAILS this test, so the lists can only shrink.
//! Integration-test directories (`crates/*/tests/`) are deliberately out
//! of scope: the contract guards hand-maintained production source, and
//! the largest offenders there are exercised by their own suites. So are
//! nested fixture `src` trees (`crates/review/generated-candidate/**`,
//! `crates/*/tests/fixtures/**`) — those are generator output.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// Maximum lines per source file.
const MAX_LINES: usize = 2_000;

/// Maximum lines per template file. Templates are denser than Rust source
/// (one control-flow block per line is common), so the budget is lower.
const MAX_TEMPLATE_LINES: usize = 500;

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

/// Templates over [`MAX_TEMPLATE_LINES`] when the template half of the
/// contract landed (issue #778). Sorted by path, and stale entries fail
/// [`templates_stay_under_the_line_limit`] exactly like the Rust list.
///
/// There are two template roots in this workspace: `codegraph-generate`
/// (the pipeline's templates, embedded at build time) and `codegraph`
/// (the `init` scaffold root, `init/commands.rs`). `admin_ts.tera` exists
/// in BOTH and is over the limit in both, so each entry is listed
/// separately.
const ALLOWED_TEMPLATES_OVER_LIMIT: &[(&str, &str)] = &[
    (
        // TEMPORARY — issue #778. Wave 4 of the handler refactor splits
        // this template by responsibility; that PR must delete this entry
        // (the stale-entry rule makes forgetting it a test failure).
        "crates/codegraph-generate/templates/api/handler.tera",
        "CRUD/service-op/tree handlers in one template; split per handler arm (#778)",
    ),
    (
        "crates/codegraph-generate/templates/api/media_route.tera",
        "media upload/presign/display/remove handlers; split per route",
    ),
    (
        "crates/codegraph-generate/templates/ddd/command.tera",
        "command plane; split per execution kind",
    ),
    (
        "crates/codegraph-generate/templates/emdash/admin_ts.tera",
        "emdash admin UI; split per panel",
    ),
    (
        "crates/codegraph-generate/templates/ifml/svelte/page.tera",
        "IFML page rendering; split per component kind",
    ),
    (
        "crates/codegraph-generate/templates/ifml/svelte/page_tanstack.tera",
        "TanStack form variant; split per component kind",
    ),
    (
        "crates/codegraph-generate/templates/scaffold/server.tera",
        "app server wiring; split per plane",
    ),
    (
        "crates/codegraph-generate/templates/scaffold/worker_middleware.tera",
        "worker middleware stack; split per layer",
    ),
    (
        "crates/codegraph-generate/templates/ui/list_page.tera",
        "entity list page; split per list mode",
    ),
    (
        "crates/codegraph-generate/templates/ui/test/page.tera",
        "POM page class; split per page kind",
    ),
    (
        "crates/codegraph-generate/templates/webhook/api_endpoints.tera",
        "SeaORM webhook API; split per endpoint",
    ),
    (
        "crates/codegraph-generate/templates/webhook/dispatch.tera",
        "SeaORM dispatcher; split drain/deliver",
    ),
    (
        "crates/codegraph-generate/templates/webhook/dispatch_worker.tera",
        "cornucopia worker dispatcher; split drain/deliver",
    ),
    (
        "crates/codegraph/templates/emdash/admin_ts.tera",
        "init-scaffold copy of the emdash admin UI; split per panel",
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

/// Every `.tera` file under `crates/<crate>/templates`, recursively. The
/// roots are discovered shallowly (one `templates/` per direct child of
/// `crates/`) so both the generator root and the `init` scaffold root are
/// covered; no `target/` directory can appear under `templates`.
fn template_files(root: &Path) -> Vec<PathBuf> {
    let crates_dir = root.join("crates");
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let templates = entry.path().join("templates");
            if templates.is_dir() {
                stack.push(templates);
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
            } else if path.extension().and_then(|e| e.to_str()) == Some("tera") {
                files.push(path);
            }
        }
    }
    files
}

/// Enforce `max_lines` over `files`, allowing the paths in `allowed`.
///
/// `fix_hint` completes the actionable violation message ("split it into
/// modules" / "split the template into responsibility fragments").
/// Returns every violation rather than panicking so each half of the
/// contract can assert with its own header.
fn budget_violations(
    root: &Path,
    files: &[PathBuf],
    max_lines: usize,
    allowed: &[(&str, &str)],
    fix_hint: &str,
) -> Vec<String> {
    let allowed_set: HashSet<&str> = allowed.iter().map(|(p, _)| *p).collect();
    let mut satisfied_entries: BTreeSet<&str> = BTreeSet::new();
    let mut violations: Vec<String> = Vec::new();

    for file in files {
        let rel = file
            .strip_prefix(root)
            .expect("walked files live under the workspace root")
            .to_string_lossy()
            .replace('\\', "/");
        let lines = line_count(file);
        if lines <= max_lines {
            continue;
        }
        if allowed_set.contains(rel.as_str()) {
            continue;
        }
        violations.push(format!(
            "{rel}: {lines} lines (limit {max_lines}) — {fix_hint}"
        ));
    }

    // The allow-list can only shrink: an entry whose file now fits is
    // stale and must be removed.
    for (path, _) in allowed {
        let abs = root.join(path);
        if !abs.exists() {
            violations.push(format!(
                "allow-list entry {path} does not exist — remove the stale entry"
            ));
            continue;
        }
        let lines = line_count(&abs);
        if lines <= max_lines {
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

    violations
}

#[test]
fn src_files_stay_under_the_line_limit() {
    let root = workspace_root();
    let files = src_rust_files(&root);
    // Baseline 497 when the contract landed (crate-level `src` trees
    // only, see the module docs for the fixture-tree exclusions).
    assert!(
        files.len() > 400,
        "walked only {} source files under {}/crates — did the workspace root resolve wrong?",
        files.len(),
        root.display()
    );

    let violations = budget_violations(
        &root,
        &files,
        MAX_LINES,
        ALLOWED_OVER_LIMIT,
        "split it into modules",
    );
    assert!(
        violations.is_empty(),
        "file-length contract violated (limit {MAX_LINES}):\n{}",
        violations.join("\n")
    );
}

#[test]
fn templates_stay_under_the_line_limit() {
    let root = workspace_root();
    let files = template_files(&root);
    // Baseline 384 when the template half landed (both template roots).
    assert!(
        files.len() > 300,
        "walked only {} templates under {}/crates — did the workspace root resolve wrong?",
        files.len(),
        root.display()
    );

    let violations = budget_violations(
        &root,
        &files,
        MAX_TEMPLATE_LINES,
        ALLOWED_TEMPLATES_OVER_LIMIT,
        "split the template into responsibility fragments",
    );
    assert!(
        violations.is_empty(),
        "template file-length contract violated (limit {MAX_TEMPLATE_LINES}):\n{}",
        violations.join("\n")
    );
}
