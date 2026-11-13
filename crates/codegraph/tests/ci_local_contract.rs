//! `just ci-local` contract (issue #628): the root justfile's `ci-local`
//! recipe must mirror the CI matrix (`.github/workflows/ci.yml`) — one
//! command that runs what CI runs, so "green locally" means "green on CI"
//! modulo runner-only steps.
//!
//! The drift direction enforced here is CI → recipe: every non-exempt
//! `run:` step in ci.yml must appear in the justfile. CI gains a step, the
//! contract fails until the recipe mirrors it — the wrapper-drift
//! discipline (#348) applied to CI itself.
//!
//! Exempt steps (runner-only, documented): package installs (`sudo …`),
//! release downloads (`curl …`), and pure tool version probes
//! (`hurl --version`). Extra justfile entries (local conveniences, gates
//! wired locally ahead of CI) are allowed — only the CI → recipe direction
//! is a hard contract.
//!
//! ```text
//! cargo test -p codegraph --test ci_local_contract
//! ```

use std::path::Path;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn read(file: &str) -> String {
    std::fs::read_to_string(workspace_root().join(file))
        .unwrap_or_else(|e| panic!("{file} must exist: {e}"))
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every `run:` step in ci.yml, inline and block scalars, as normalized
/// command text.
fn ci_run_steps() -> Vec<String> {
    let ci = read(".github/workflows/ci.yml");
    let mut steps = Vec::new();
    let mut lines = ci.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(inline) = trimmed.strip_prefix("- run: ") {
            let cmd = inline.trim().to_string();
            if cmd == "|" || cmd == ">" {
                // Block scalar: collect the more-indented body.
                let mut body = Vec::new();
                while let Some(next) = lines.peek() {
                    let next_trim = next.trim_start();
                    if next_trim.is_empty()
                        || next.trim_start().len() > line.trim_start().len()
                            && !next_trim.starts_with("- ")
                    {
                        body.push(next_trim.to_string());
                        lines.next();
                    } else {
                        break;
                    }
                }
                steps.push(body.join(" "));
            } else {
                steps.push(cmd);
            }
        }
    }
    steps.into_iter().map(|s| normalize(&s)).collect()
}

fn is_runner_only(step: &str) -> bool {
    // Package installs, release downloads, and version probes never run
    // locally — the recipe degrades to a skip WARN instead (see justfile).
    step.starts_with("sudo ")
        || step.starts_with("curl ")
        || step == "hurl --version"
        || step
            .split(" ")
            .any(|part| part.starts_with("sudo ") || part.starts_with("curl "))
}

#[test]
fn justfile_exists_at_workspace_root() {
    let justfile = workspace_root().join("justfile");
    assert!(
        justfile.exists(),
        "a root justfile must exist — `just ci-local` is the one-command CI \
         mirror (issue #627/#628 pairing: the discipline needs version parity \
         AND command parity to work at all)"
    );
}

#[test]
fn every_ci_run_step_appears_in_the_justfile() {
    let justfile = normalize(&read("justfile"));
    let mut missing = Vec::new();
    for step in ci_run_steps() {
        if is_runner_only(&step) {
            continue;
        }
        if !justfile.contains(&step) {
            missing.push(step);
        }
    }
    assert!(
        missing.is_empty(),
        "ci.yml gained steps the justfile does not mirror (issue #628 drift \
         contract): {missing:?} — add each to the matching `ci-local` leg \
         (lint/test/audit/ops) in the same PR"
    );
}

#[test]
fn ci_local_recipe_runs_the_full_matrix() {
    let justfile = read("justfile");
    for leg in ["lint", "test", "audit", "ops"] {
        assert!(
            justfile.contains(&format!("\n{leg}:")),
            "the `{leg}` leg recipe must exist"
        );
    }
    let recipe_line = justfile
        .lines()
        .find(|line| line.trim().starts_with("ci-local:") || line.trim() == "ci-local:")
        .unwrap_or_else(|| {
            panic!(
                "the justfile must declare a `ci-local` recipe — the issue \
                 names the command literally (`just ci-local`, #628)"
            )
        });
    for leg in ["lint", "test", "audit", "ops"] {
        assert!(
            recipe_line.contains(leg),
            "`just ci-local` must run the FULL matrix — `{leg}` is missing \
             from `{recipe_line}` (issue #628: one command, whole matrix; \
             per-gate tooling gaps degrade to WARN + skip)"
        );
    }
    assert!(
        justfile.contains("default: ci-local"),
        "bare `just` must alias the full matrix too"
    );
}

#[test]
fn test_leg_mirrors_ci_debuginfo_discipline() {
    let justfile = normalize(&read("justfile"));
    assert!(
        justfile.contains("-C debuginfo=0"),
        "the test leg must set RUSTFLAGS=-C debuginfo=0 — AGENTS.md mandates \
         local full-suite runs mirror CI's flag (without it every test binary \
         links full debuginfo and the target dir explodes)"
    );
}
