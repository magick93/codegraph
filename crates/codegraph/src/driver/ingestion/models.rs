//! The domain-plane ingestion passes: mox-first, then rosetta, then JSON
//! Schema, then the #230 alias wiring (driver passes 1/1b/1a).
//!
//! Ordering is load-bearing: every title a pass bridges joins the skip-set
//! the later passes honour, so mox wins over rosetta and both win over a
//! same-titled JSON schema; imported-schema aliases resolve only once both
//! the imported files and the `--schemas` pass have landed.

use std::collections::HashSet;

use codegraph_backend::Backend;

use super::PassLog;
use crate::driver::sources::ModelSources;
use crate::error::Result;

/// Ingest the domain-plane model sources. `run` reports each pass; the
/// `classify` report runs the SAME passes quietly so its stdout stays the
/// classification report alone (the JSON format is machine output).
pub(in crate::driver) async fn ingest_models(
    be: &Backend,
    domain_config: &codegraph_config::config::DomainConfig,
    classifier_config: &codegraph_classifier::config::ClassifierConfig,
    ui_overrides: &codegraph_config::UiOverrideConfig,
    sources: &ModelSources<'_>,
    log: PassLog,
) -> Result<()> {
    // Pass 1: Ingest pherolang .mox domain sources FIRST (mox-first, issue
    // #231): vocabularies with facets, class operations, derived features,
    // and schema/property nodes for mox-authored classes and enums. The
    // bridged class titles win schema-title conflicts because the schema
    // pass below skips every title created here. Diagnostics warn and never
    // fail the run — except missing/invalid `import schema` targets, which
    // are a hard error (issue #230): the lowered model's structural
    // references depend on them.
    let mut mox_covered: HashSet<String> = HashSet::new();
    let mut mox_stats = crate::ingest::mox_ingest::MoxIngestStats::default();
    let mut pending_alias_refs: Vec<crate::ingest::mox_ingest::PendingAliasRef> = Vec::new();
    if !sources.mox_files.is_empty() {
        if log.reports() {
            println!("Pass 1: {} mox files to ingest", sources.mox_files.len());
        }
        let outcome = crate::ingest::mox_ingest::ingest_mox_files(
            be.ingestor(),
            be.querier(),
            sources.mox_files,
            domain_config,
            &domain_config.defaults.type_suffix,
        )
        .await?;
        if log.reports() {
            println!("Pass 1 complete: {}", outcome.stats);
        }
        mox_covered.extend(outcome.stats.bridged_titles.iter().cloned());
        mox_stats = outcome.stats;
        pending_alias_refs = outcome.pending_alias_refs;

        // Pass 1 (imports): the .mox model's imported JSON schema files go
        // through the same pipeline as --schemas, BEFORE the schema pass so
        // their titles join the skip-set and a file passed both ways
        // ingests exactly once (issue #230).
        if !outcome.imported_files.is_empty() {
            let imported = crate::ingest::async_ingest::ingest_imported_schemas(
                be.ingestor(),
                &outcome.imported_files,
                classifier_config,
                ui_overrides,
                &domain_config.defaults.type_suffix,
            )
            .await?;
            if log.reports() {
                println!(
                    "Pass 1 imports: {} schema files ingested",
                    imported.ingested.schemas_created
                );
            }
            mox_covered.extend(imported.titles);
        }
    }

    // Pass 1b: Rosetta (Rune DSL) data-plane bridge (issues #256, #257).
    // Runs alongside mox and BEFORE the JSON schema pass: bridged titles
    // join the skip-set so a same-titled JSON schema never duplicates the
    // node. Mox keeps primary position (rosetta wins over JSON by passing
    // first; mox wins over rosetta). Rosetta provenance rides `origin` (not
    // mox's `source`), so the classifier auto-scores bridged types.
    if !sources.rosetta_files.is_empty() {
        if log.reports() {
            println!(
                "Pass 1b: {} rosetta files to ingest",
                sources.rosetta_files.len()
            );
        }
        let outcome = crate::ingest::rosetta_ingest::ingest_rosetta_files(
            be.ingestor(),
            be.querier(),
            sources.rosetta_files,
            domain_config,
            &domain_config.defaults.type_suffix,
        )
        .await?;
        if log.reports() {
            println!("Pass 1b complete: {}", outcome.stats);
        }
        mox_covered.extend(outcome.stats.bridged_titles.iter().cloned());
    }

    // Pass 1a: Ingest JSON schemas (no entity classification). Optional
    // under mox-first: skipped entirely when only .mox files are provided,
    // and titles already created from .mox are skipped within the pass.
    if let Some(schemas_dir) = sources.schemas {
        let empty_entities = HashSet::new();
        let ingest_result = crate::ingest::async_ingest::ingest_schemas_with_skips(
            be.ingestor(),
            schemas_dir,
            classifier_config,
            &empty_entities,
            ui_overrides,
            &domain_config.defaults.type_suffix,
            &mox_covered,
        )
        .await?;
        if log.reports() {
            println!(
                "Pass 1a: {} schemas ingested",
                ingest_result.schemas_created
            );
        }
    }

    // Pass 1a (alias wiring): imported-schema aliases resolve now that both
    // the imported files and the --schemas pass have run — exact title
    // match, then title + type suffix; misses warn and skip the edge
    // (issue #230).
    if !pending_alias_refs.is_empty() {
        crate::ingest::mox_ingest::wire_alias_refs(
            be.ingestor(),
            be.querier(),
            &pending_alias_refs,
            &domain_config.defaults.type_suffix,
            &mut mox_stats,
        )
        .await?;
        if log.reports() {
            println!(
                "Pass 1a wiring: {} aliases resolved, {} unresolved",
                mox_stats.resolved_aliases, mox_stats.unresolved_aliases
            );
        }
    }
    Ok(())
}
