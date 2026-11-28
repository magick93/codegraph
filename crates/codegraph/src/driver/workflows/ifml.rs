//! IFML-only UI generation: ingest IFML DSL files (+ optional schemas) and
//! emit framework routes/navigation for the requested frameworks. No
//! entity, DDL, API, or UI-scaffold generators run, and validation is
//! skipped. Shares the IFML ingestion, entity classification, API-model
//! pass and generation preparation with the full pipeline.

use std::collections::HashSet;

use crate::driver::args::IfmlGenerateArgs;
use crate::driver::config_files::{load_classifier_config, load_domain_config, load_ui_overrides};
use crate::driver::diagnostics::print_generation_report;
use crate::driver::generation::{
    create_tera, effective_design_system, effective_ux_rules, load_ifml_component_mappings,
};
use crate::driver::graph_cache::in_memory_backend;
use crate::driver::ingestion::{classification, designs, ifml};
use crate::driver::profile::resolve_ifml_profile;
use crate::error::Result;

/// IFML-only UI generation (see the module docs).
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn ifml_generate(args: IfmlGenerateArgs<'_>) -> Result<()> {
    let IfmlGenerateArgs {
        config_path,
        output,
        ifml_files,
        schemas,
        classifier,
        frameworks,
        profiles_config_path,
        template_dir,
        ifml_components,
        ifml_design_system,
        ux_rules,
    } = args;

    if ifml_files.is_empty() {
        return Err(crate::error::Error::Config(
            "no IFML files provided: --ifml-files is required".to_string(),
        ));
    }
    if schemas.is_some() && classifier.is_none() {
        return Err(crate::error::Error::Config(
            "--classifier is required when --schemas is provided".to_string(),
        ));
    }

    let be = in_memory_backend().await?;
    let domain_config = load_domain_config(config_path)?;

    // Pass 1: ingest + classify schemas when provided (entity resolution
    // enrichment for IFML data bindings). `classifier` is guaranteed Some
    // here by the guard above.
    if let (Some(schemas_dir), Some(classifier_path)) = (schemas, classifier) {
        let classifier_config = load_classifier_config(Some(classifier_path))?;

        let empty_entities = HashSet::new();
        let ui_overrides = load_ui_overrides(config_path)?;
        let ingest_result = crate::ingest::async_ingest::ingest_schemas(
            be.ingestor(),
            schemas_dir,
            &classifier_config,
            &empty_entities,
            &ui_overrides,
            &domain_config.defaults.type_suffix,
        )
        .await?;
        println!(
            "Pass 1 complete: {} schemas ingested",
            ingest_result.schemas_created
        );

        // Auto-classify + Pass 2: reclassify with entity flags.
        classification::apply_entity_classification(&be, &domain_config, &classifier_config)
            .await?;
    }

    // Pass 1b: ingest IFML DSL files through the checker gate (issue
    // #591 — the same front door the primary run path uses).
    ifml::ingest_ifml_files(&be, ifml_files).await?;

    // Pass 1c: ingest API model from domain configuration.
    designs::ingest_api_model(&be, &domain_config).await?;

    let frameworks: Vec<String> = if frameworks.is_empty() {
        vec!["svelte".to_string()]
    } else {
        frameworks.to_vec()
    };

    let (build_plan, mut project_config) =
        resolve_ifml_profile(profiles_config_path.as_deref(), &frameworks, &domain_config)?;

    let design_system = effective_design_system(ifml_design_system, build_plan.as_ref());
    let tera = create_tera(template_dir, design_system)?;
    let ifml_component_mappings = load_ifml_component_mappings(ifml_components, design_system)?;

    // Issue #293: the IFML-only path renders through ProjectConfig, so the
    // resolved rules ride there for the route/e2e templates.
    project_config.ux.ux = effective_ux_rules(ux_rules, build_plan.as_ref())?;

    let report = crate::generate::run_ifml_generators(
        be.querier(),
        &domain_config,
        output,
        &tera,
        &frameworks,
        build_plan.as_ref(),
        &project_config,
        ifml_component_mappings.as_ref(),
    )
    .await?;

    print_generation_report(&report);
    println!("Done.");
    Ok(())
}
