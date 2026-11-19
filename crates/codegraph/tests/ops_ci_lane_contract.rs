//! The honest ops CI lane contract (issues #708 + #709).
//!
//! `test-ops-integration` exists to exercise the full generated platform
//! band (basejump + pgmq + extensions), but it used to provision a vanilla
//! `postgres:15` service — the two band tests skipped themselves, and the
//! job stayed green while the band it exists to protect was never applied
//! (issue #708). These node-free pins hold the fix:
//!
//! 1. the job's service image IS the Supabase postgres image (the one the
//!    repo's own gates run) — never a bare `postgres:<n>`;
//! 2. the job exports `CODEGRAPH_REQUIRE_SUPABASE=1`, so a missing
//!    capability FAILS naming the image instead of silently skipping;
//! 3. the two band tests (and the idempotency twin) route their
//!    compatibility probe through the SHARED fail-closed gate in
//!    `codegraph_ops::supabase` — no second predicate that could drift;
//! 4. the justfile's `ops-db-integration` recipe mirrors the gate env.
//!
//! ```text
//! cargo test -p codegraph --test ops_ci_lane_contract
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

/// The `test-ops-integration:` job block of ci.yml (from the job key to the
/// next same-indent job key).
fn ci_ops_job_block() -> String {
    let ci = read(".github/workflows/ci.yml");
    let mut block = String::new();
    let mut in_job = false;
    for line in ci.lines() {
        if line.starts_with("  test-ops-integration:") {
            in_job = true;
        } else if in_job && line.starts_with("  ") && !line.starts_with("    ") {
            // Next top-level job (two-space key) ends the block.
            break;
        }
        if in_job {
            block.push_str(line);
            block.push('\n');
        }
    }
    assert!(
        block.contains("runs-on"),
        "ci.yml must still declare the test-ops-integration job (issue #708 \
         pins its service image; the block parser found nothing)"
    );
    block
}

/// The lane must provision the Supabase postgres image — the image that
/// provides `auth` / pgmq / pg_tle / http, which converts the two band-test
/// skips into real coverage (issue #708). A bare `postgres:<n>` service
/// makes the lane lie again.
#[test]
fn test_ops_integration_provisions_the_supabase_image_service() {
    let block = ci_ops_job_block();
    let image = block
        .lines()
        .find_map(|l| l.trim().strip_prefix("image: "))
        .map(str::trim)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    assert!(
        !image.is_empty(),
        "the test-ops-integration job must declare a service image"
    );
    assert!(
        !image.starts_with("postgres:"),
        "the test-ops-integration service must not provision a bare \
         postgres:<n> image (issue #708: the band tests silently skip on \
         it) — got `{image}`"
    );
    assert!(
        image.contains("supabase/postgres"),
        "the test-ops-integration service image must be the Supabase \
         postgres image (public.ecr.aws/supabase/postgres:17.6.1.132, the \
         same image ifml-gate.yml / entity-gate.yml / the gate harness use \
         — issue #708); got `{image}`"
    );
}

/// The fail-closed gate must be ACTIVE in the lane: the job exports
/// `CODEGRAPH_REQUIRE_SUPABASE=1` so a missing capability is a FAIL naming
/// the image, not a skip. Without the export the contract's teeth are
/// decorative.
#[test]
fn test_ops_integration_exports_the_require_supabase_gate() {
    let block = ci_ops_job_block();
    let exported = block
        .lines()
        .any(|l| l.trim().starts_with("CODEGRAPH_REQUIRE_SUPABASE:") && l.contains('1'));
    assert!(
        exported,
        "the test-ops-integration job must export CODEGRAPH_REQUIRE_SUPABASE: \
         \"1\" (issue #708: with the env set, a missing Supabase capability \
         fails naming the image instead of silently skipping the band tests)"
    );
}

/// The band tests must NOT carry a private compatibility predicate: both
/// skip sites route through the shared fail-closed gate in
/// `codegraph_ops::supabase` (issue #708 — one probe, three consumers; a
/// second predicate is the drift this workstream exists to kill).
#[test]
fn the_band_tests_use_the_shared_fail_closed_gate() {
    for file in [
        "crates/codegraph-ops/tests/db_integration.rs",
        "crates/codegraph/tests/idempotency_tests.rs",
    ] {
        let text = read(file);
        assert!(
            text.contains("codegraph_ops::supabase"),
            "{file} must route its Supabase compatibility probe through \
             codegraph_ops::supabase (issue #708: the shared gate fails \
             closed under CODEGRAPH_REQUIRE_SUPABASE and skips otherwise) — \
             a private `supabase_compatible` is the silent-skip class"
        );
        assert!(
            !text.contains("fn supabase_compatible"),
            "{file} must not carry a private supabase_compatible predicate \
             (issue #708: the shared gate in codegraph_ops::supabase is the \
             one probe)"
        );
    }
}

/// The justfile's ops lane mirrors the gate env (the `ci_local_contract`
/// drift discipline applied to the env var — the recipe probes the target
/// and exports the flag when it serves the band).
#[test]
fn the_justfile_ops_recipe_mirrors_the_gate_env() {
    let justfile = read("justfile");
    let recipe = justfile
        .split("ops-db-integration:")
        .nth(1)
        .unwrap_or("")
        .lines()
        .take(30)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !recipe.is_empty(),
        "the justfile must declare an ops-db-integration recipe"
    );
    assert!(
        recipe.contains("CODEGRAPH_REQUIRE_SUPABASE"),
        "the justfile ops-db-integration recipe must mirror the \
         CODEGRAPH_REQUIRE_SUPABASE gate env (issue #708: probe-conditional — \
         export it when the target serves the band so local `just ci-local` \
         is fail-closed exactly like CI)"
    );
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The real `testkit api` lane (issue #709): the ops harness's own api
/// suite was validated by nothing in CI (`grep -rn testkit
/// .github/workflows/` found nothing). ci.yml must build the committed
/// review fixture app and run its testkit against the Supabase-image
/// service (shape B — the stack is already up, the suite skips stack
/// startup), and the justfile must mirror the steps (the
/// `ci_local_contract` drift discipline).
#[test]
fn the_testkit_api_lane_is_wired_into_ci_and_mirrored_in_the_justfile() {
    let ci = normalize(&read(".github/workflows/ci.yml"));
    let api_step =
        "cargo run -p testkit -- api --skip-build --allow-gen-rev-mismatch";
    let build_step =
        "cargo build --manifest-path crates/review/generated-candidate/Cargo.toml";
    assert!(
        ci.contains(api_step),
        "ci.yml must run the ops harness's own api suite (`{api_step}`, \
         issue #709: the harness's api suite was validated by nothing in CI)"
    );
    assert!(
        ci.contains(build_step),
        "ci.yml must build the review fixture app before the api suite \
         (`--skip-build` skips generate+build; the binary must exist)"
    );
    let justfile = normalize(&read("justfile"));
    assert!(
        justfile.contains(api_step) && justfile.contains(build_step),
        "the justfile must mirror the testkit api lane steps (issue #709: \
         every non-runner-only CI step appears in the justfile)"
    );
}
