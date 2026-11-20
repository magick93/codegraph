//! The FLAG-OFF review-fixture regeneration recipe (issue #747).
//!
//! The committed `crates/review/generated-candidate/` tree is owned by
//! `grafeo_candidate_inspect_output` (`grafeo_e2e_tests/full_generation.rs`):
//! a PR-CI test that regenerates the fixture IN PLACE with the default-opts,
//! flag-OFF pipeline on every full test run. This module reproduces that
//! recipe exactly — same ingest (`setup_grafeo`, included verbatim via
//! `#[path]` so the two cannot drift), same generator call
//! (`run_generators_with_domain_types_base` with default UI configs, default
//! `ProjectConfig`, domain-types/hooks redirected to temp dirs), same
//! post-pipeline `RepositoryImplEmitter` emit for `CandidateType` —
//! parameterized over the output directory so the #747 drift pin can run it
//! into a scratch directory while the in-place helper refreshes the
//! committed tree with the same bytes.
//!
//! What this recipe deliberately does NOT do:
//! - no flag threading: `ProjectConfig::default()` means every codegen
//!   feature (entity_remote_functions, entity_form, ui_live_queries, …)
//!   stays OFF — the committed fixture is the flag-OFF tree, and the #747
//!   scope decision keeps it that way (the flag-ON planes belong to the
//!   nightly `entity_codegen_gate`, which regenerates the same fixture
//!   in place with the shared flag-ON recipe when IT runs);
//! - no destructive wipe: unlike the inspect test (which
//!   `remove_dir_all`s the fixture first), the maintenance caller must
//!   preserve the checked-in content the flag-OFF pipeline does not
//!   produce — `codegraph-ops.toml` + `testkit/` (the `review_api_suite`
//!   live gate reads them) and consumer-authored `ui/forms/`. The
//!   pipeline's own `clean_generated_output` sweeps only entity-scoped
//!   output, so those extras survive an in-place run untouched.
//!
//! Node-free and DB-free: the ingest is an in-memory Grafeo engine and no
//! npm/node step is involved.

// Included verbatim so the ingest half of this recipe is LITERALLY the
// code `grafeo_candidate_inspect_output` runs — the two recipes cannot
// drift. The sibling functions this binary does not call
// (`generate_full_app`, the factory helpers) are dead here; they are live
// in their own binary.
#[allow(dead_code)]
#[path = "grafeo_e2e_tests/setup.rs"]
mod inspect_setup;

use std::path::Path;

use codegraph::generate::ProjectConfig;
use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
use codegraph::generate::template_engine::create_tera;

/// The fixture's path inside the repository (in-place maintenance target).
pub fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("review")
        .join("generated-candidate")
}

/// The flag-OFF recipe, caller-chosen output directory.
///
/// The emitted bytes do not depend on the output directory: every input is
/// `CARGO_MANIFEST_DIR`-relative, the `RepositoryImplEmitter` output embeds
/// no path, and the pipeline writes output-relative paths only — so a
/// scratch-dir run is byte-identical to an in-place one (verified by the
/// drift pin itself, which compares exactly these two).
pub async fn regenerate_flag_off_fixture_into(output_dir: &Path) -> Result<(), String> {
    // 1. Ingest — the same `setup_grafeo()` call the inspect test makes
    //    (tests/fixtures JSON schemas through the classifier; entity names
    //    from domains.toml; no rosetta, no auto-classify — the flag-OFF
    //    recipe predates both and the fixture has no rosetta-only surface).
    let (engine, config) = inspect_setup::setup_grafeo().await;

    // 2. The pipeline — default-opts, flag-OFF. Domain-types and hooks
    //    output is redirected to temp dirs so the fixture schemas (only
    //    common/compensation/recruiting/events/rsvp) do not corrupt real
    //    workspace source files.
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"))
        .map_err(|e| format!("tera: {e}"))?;
    let domain_types_tmp = tempfile::tempdir().map_err(|e| format!("domain-types tempdir: {e}"))?;
    let hooks_tmp = tempfile::tempdir().map_err(|e| format!("hooks tempdir: {e}"))?;
    let report = codegraph::generate::run_generators_with_domain_types_base(
        &engine,
        &config,
        output_dir,
        &tera,
        &Default::default(),
        &Default::default(),
        Path::new(""),
        domain_types_tmp.path(),
        hooks_tmp.path(),
    )
    .await
    .map_err(|e| format!("flag-OFF fixture regeneration: {e}"))?;
    if report.has_errors() {
        let detail = report
            .errors
            .iter()
            .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
            .take(10)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "flag-OFF fixture regeneration reported errors:\n{detail}"
        ));
    }

    // 3. The repository impl — emitted by the inspect test after the
    //    pipeline (not part of run_generators' template flow).
    let emitter = RepositoryImplEmitter;
    let repo_code = emitter
        .emit(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .map_err(|e| format!("repository impl emit: {e}"))?;
    let repo_dir = output_dir
        .join("src")
        .join("domain")
        .join("recruiting")
        .join("candidate");
    std::fs::create_dir_all(&repo_dir).map_err(|e| format!("mkdir {repo_dir:?}: {e}"))?;
    std::fs::write(repo_dir.join("repository_impl.rs"), repo_code)
        .map_err(|e| format!("write repository_impl.rs: {e}"))?;

    Ok(())
}

/// Refresh the committed fixture in place with the flag-OFF recipe.
pub async fn regenerate_flag_off_fixture() -> Result<(), String> {
    regenerate_flag_off_fixture_into(&fixture_dir()).await
}
