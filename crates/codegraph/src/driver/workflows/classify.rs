//! The `classify` report: ingest the domain-plane model sources exactly as
//! `run` does (quietly), run the shared classification algorithm, and
//! format the entity/VO decisions instead of applying them.

use std::path::{Path, PathBuf};

use crate::driver::config_files::{load_classifier_config, load_domain_config, load_ui_overrides};
use crate::driver::graph_cache::in_memory_backend;
use crate::driver::ingestion::{PassLog, classification, models};
use crate::driver::outcome::ClassifyFormat;
use crate::driver::sources::ModelSources;
use crate::error::Result;

/// Classify all schemas and show entity/VO decisions. Mox files, when
/// provided, are ingested first: their schemas bypass the classifier and
/// show up in the report with the `override:source=mox` reason.
/// `schemas` is optional when `mox_files` is provided (mox-first, issue
/// #231); the classifier config is only loaded when `classifier_path` is
/// given (an empty config is used otherwise, mirroring `run`).
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn classify(
    schemas: Option<&Path>,
    classifier_path: Option<&Path>,
    config_path: &Path,
    domain_filter: Option<&str>,
    format: ClassifyFormat,
    mox_files: &[PathBuf],
    rosetta_files: &[PathBuf],
) -> Result<()> {
    if schemas.is_none() && mox_files.is_empty() && rosetta_files.is_empty() {
        return Err(crate::error::Error::Config(
            "no model source: provide --mox-files or --rosetta-files (primary) or --schemas"
                .to_string(),
        ));
    }
    let domain_config = load_domain_config(config_path)?;
    let classifier_config = load_classifier_config(classifier_path)?;
    let be = in_memory_backend().await?;
    let ui_overrides = load_ui_overrides(config_path)?;

    // Mox-first: mox-authored schemas enter the report as
    // `override:source=mox` and shadow same-titled JSON schemas. Imported
    // schema files ingest through the JSON pipeline and their aliases wire
    // after the schema pass (issue #230) so the report sees the same model
    // the run does — it runs the run's own domain-plane passes.
    let sources = ModelSources {
        schemas,
        classifier: classifier_path,
        mox_files,
        rosetta_files,
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_files: &[],
        openapi_files: &[],
    };
    models::ingest_models(
        &be,
        &domain_config,
        &classifier_config,
        &ui_overrides,
        &sources,
        PassLog::Quiet,
    )
    .await?;

    let results = classification::classify_domains(&be, &domain_config, &classifier_config).await?;

    match format {
        ClassifyFormat::Table => crate::classify::output::format_table(&results, domain_filter),
        ClassifyFormat::Json => crate::classify::output::format_json(&results),
    }

    Ok(())
}
