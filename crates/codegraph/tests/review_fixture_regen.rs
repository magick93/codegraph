//! Regenerate the committed review fixture with the CURRENT generators
//! (issue #709) — the CI lane's dedicated regen-only target.
//!
//! `review_api_suite` couples regeneration with the full live suite
//! (provision → build → `testkit api`), so wiring the api suite into CI
//! through it would pay the whole suite for a lane whose FIRST job is to
//! fail fast on template drift. This target is the regeneration half alone:
//! same recipe (the default-profile flag-ON plan over
//! `tests/fixtures/{schemas,classifier.toml,domains.toml}`), same
//! cross-process `FixtureGate`, same from-clean sweep — NONE of the DB,
//! build or hurl work.
//!
//! Committed-tree drift afterwards is intentional: the lane runs against the
//! freshly regenerated tree (`--skip-build` consumes it), and the PR-CI
//! drift pin (`review_fixture_drift_tests`) holds the committed bytes to the
//! flag-OFF recipe separately.
//!
//! ```text
//! cargo test -p codegraph --test review_fixture_regen -- --ignored --nocapture
//! ```

#[path = "review_api_suite_regen.rs"]
mod review_api_suite_regen;

/// Regenerate the fixture in place, from clean, under the fixture gate.
#[tokio::test]
#[ignore = "regenerates the committed review fixture in place (CI api lane, \
            issue #709); run explicitly — it writes ~2k files and takes \
            minutes"]
async fn regenerate_committed_review_fixture_in_place() {
    let gate = review_api_suite_regen::FixtureGate::acquire()
        .unwrap_or_else(|e| panic!("acquire fixture gate: {e}"));
    gate.provision()
        .await
        .unwrap_or_else(|e| panic!("regenerate the review fixture: {e}"));
}
