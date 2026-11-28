//! The full pipeline: load config → open the (optionally cached) graph →
//! ingest models, designs, then classify → prepare generation → validate →
//! generate → post-generation. This module orchestrates; every stage lives
//! in its own driver module.

use std::path::{Path, PathBuf};

use crate::driver::args::RunArgs;
use crate::driver::config_files::{
    load_classifier_config, load_domain_config, load_seed_config, load_ui_domains,
    load_ui_overrides,
};
use crate::driver::diagnostics::{
    print_generate_perf, print_generation_report, print_ingest_perf, print_model_source_notices,
};
use crate::driver::generation::{GenerationPrep, prepare_generation};
use crate::driver::graph_cache::{compute_inputs_hash, open_graph_backend, persist_fresh_graph};
use crate::driver::ingestion::{PassLog, classification, designs, models};
use crate::driver::outcome::RunOutcome;
use crate::driver::post_generation::{check_generated_output, run_post_gen_scripts};
use crate::driver::profile::resolve_run_profile;
use crate::driver::sources::ModelSources;
use crate::driver::validation::run_validation_gates;
use crate::error::Result;

/// Run the full pipeline: ingest + classify + generate.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn run(args: RunArgs<'_>) -> Result<()> {
    run_with_graph_cache(args, None).await.map(|_| ())
}

/// [`run`] with an optional persisted-graph cache directory (issue #275).
/// When `graph_cache` is set, the run computes an inputs hash over all
/// model/config inputs plus the graph format version; on a match with the
/// stored hash the persisted graph is reopened and re-ingestion is skipped
/// entirely (an INFO notice goes to stderr). Otherwise the model is
/// ingested into a fresh persistent engine which is checkpointed and
/// recorded for the next run.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn run_with_graph_cache(
    args: RunArgs<'_>,
    graph_cache: Option<&Path>,
) -> Result<RunOutcome> {
    let RunArgs {
        schemas,
        classifier,
        config_path,
        output,
        extension_points_path,
        integration_manifests,
        profile_name,
        variant,
        profiles_config_path,
        no_post_gen,
        template_dir,
        ifml_files,
        openapi_files,
        mox_files,
        rosetta_files,
        ddd_files,
        evt_files,
        deploy_files,
        ifml_framework,
        ifml_components,
        ifml_design_system,
        ux_rules,
        codegraph_rev,
        check,
    } = args;

    // Model-source guard: at least one of schemas / mox files / rosetta
    // files is required, and schemas need a classifier config.
    if schemas.is_none() && mox_files.is_empty() && rosetta_files.is_empty() {
        return Err(crate::error::Error::Config(
            "no model source: provide --mox-files or --rosetta-files (primary) or --schemas"
                .to_string(),
        ));
    }
    if schemas.is_some() && classifier.is_none() {
        return Err(crate::error::Error::Config(
            "--classifier is required when --schemas is provided".to_string(),
        ));
    }
    print_model_source_notices(
        schemas.is_some(),
        !mox_files.is_empty(),
        !rosetta_files.is_empty(),
    );

    let mut domain_config = load_domain_config(config_path)?;

    let config_dir = config_path.parent();
    let sources = ModelSources {
        schemas,
        classifier,
        mox_files,
        rosetta_files,
        ddd_files,
        evt_files,
        deploy_files,
        ifml_files,
        openapi_files,
    };
    let inputs_hash = compute_inputs_hash(
        graph_cache,
        &sources,
        config_path,
        &domain_config,
        config_dir,
    )?;

    let (be, graph_cache_reused) = open_graph_backend(graph_cache, inputs_hash.as_deref()).await?;

    let classifier_config = load_classifier_config(classifier)?;
    let ui_overrides = load_ui_overrides(config_path)?;
    let ui_domains = load_ui_domains(config_path)?;

    let profiles_path = profiles_config_path.unwrap_or_else(|| PathBuf::from("profiles.toml"));
    let (build_plan, mut project_config, domain_types_base_path) = resolve_run_profile(
        &profiles_path,
        profile_name,
        variant,
        ifml_framework,
        codegraph_rev.as_deref(),
        output,
        &domain_config,
    )?;

    // All ingestion passes are skipped when the graph cache reused the
    // persisted graph (inputs unchanged): the graph already holds the
    // model. Generation and validation still run on the reused graph.
    if !graph_cache_reused {
        models::ingest_models(
            &be,
            &domain_config,
            &classifier_config,
            &ui_overrides,
            &sources,
            PassLog::Report,
        )
        .await?;
        designs::run_design_ingest_passes(&be, &domain_config, &sources).await?;
        classification::run_classification_passes(
            &be,
            &domain_config,
            &classifier_config,
            config_dir,
            &project_config,
        )
        .await?;
    }

    // Synthetic bounded contexts for the dependency domains (issue #276):
    // generation reads the foreign faces from the graph; the entries carry
    // no local model config. Runs on both the fresh and the cache-reused
    // path (domain_config is re-parsed every run).
    crate::ingest::dependencies::inject_dependency_domains(&mut domain_config);

    if let Some(cache_dir) = graph_cache.filter(|_| !graph_cache_reused)
        && let Some(hash) = inputs_hash.as_deref()
    {
        persist_fresh_graph(&be, cache_dir, hash)?;
    }

    // Perf counters (issues #469/#473): one ingest summary line after the
    // ingest passes, one validate+generate line after generation — both only
    // when CODEGRAPH_PERF_COUNTERS is set.
    print_ingest_perf();
    let perf_generate_start = codegraph_grafeo::perf::perf_snapshot();

    let GenerationPrep {
        tera,
        ext_config,
        integration_manifests,
        ux_resolved,
        ifml_component_mappings,
    } = prepare_generation(
        template_dir,
        ifml_design_system,
        build_plan.as_ref(),
        ifml_components,
        extension_points_path,
        integration_manifests,
        ux_rules,
        &mut project_config,
    )?;

    run_validation_gates(&be, &domain_config, build_plan.as_ref()).await?;

    let report = crate::generate::run_generators_with_opts(crate::generate::GeneratorOpts {
        db: be.querier(),
        config: &domain_config,
        output_dir: output,
        tera: &tera,
        ui_overrides: &ui_overrides,
        ui_domains: &ui_domains,
        // mox-only runs have no schemas dir; an empty path keeps the UI
        // codelist generator's schema-dir lookups graceful (see `generate`).
        schema_base_dir: schemas.unwrap_or_else(|| Path::new("")),
        seed_config: load_seed_config(config_path).as_deref(),
        domain_types_base: domain_types_base_path.as_deref(),
        hooks_base: None,
        ext_points: ext_config.as_ref(),
        integration_manifests: integration_manifests.as_ref(),
        build_plan: build_plan.as_ref(),
        profile_name,
        ifml_frameworks: ifml_framework.to_vec(),
        ifml_components: ifml_component_mappings.as_ref(),
        ux_rules: ux_resolved,
        project_config: Some(&project_config),
        emdash_plugins: None,
        domain_config_dir: config_path.parent(),
    })
    .await?;

    print_generation_report(&report);
    print_generate_perf(&perf_generate_start);

    run_post_gen_scripts(build_plan.as_ref(), no_post_gen).await?;

    // Post-generate compile gate (issue #336): strictly opt-in via `--check`
    // — flag off ⇒ no cargo invocation, no output change.
    if check {
        check_generated_output(output)?;
    }

    println!("Done.");
    Ok(RunOutcome {
        graph_cache_reused,
        inputs_hash,
    })
}
