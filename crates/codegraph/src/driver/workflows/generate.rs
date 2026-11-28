//! Plan-less generation from an already-ingested backend (no schema
//! ingestion): the API model auto-seeds when the graph carries none.

use std::path::{Path, PathBuf};

use crate::driver::config_files::{
    load_domain_config, load_extension_points, load_integration_manifests, load_seed_config,
    load_ui_domains, load_ui_overrides,
};
use crate::driver::diagnostics::print_generation_report;
use crate::driver::generation::{create_tera, effective_ux_rules, load_ifml_component_mappings};
use crate::driver::graph_cache::in_memory_backend;
use crate::driver::post_generation::check_generated_output;
use crate::driver::validation::run_validation;
use crate::error::Result;

/// Generate code from an already-ingested backend (no schema ingestion).
#[allow(clippy::too_many_arguments)]
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn generate(
    config_path: &Path,
    output: &Path,
    extension_points_path: Option<&Path>,
    integration_manifests_paths: &[PathBuf],
    template_dir: &[PathBuf],
    ifml_frameworks: &[String],
    ifml_components: Option<&Path>,
    ifml_design_system: Option<&str>,
    ux_rules: Option<&Path>,
    check: bool,
) -> Result<()> {
    let config = load_domain_config(config_path)?;
    let be = in_memory_backend().await?;

    let ui_overrides = load_ui_overrides(config_path)?;
    let ui_domains = load_ui_domains(config_path)?;

    let tera = create_tera(template_dir, ifml_design_system)?;
    let ext_config = load_extension_points(extension_points_path)?;
    // Integration catalog manifests (issue #552) — see `run`.
    let integration_manifests = load_integration_manifests(integration_manifests_paths)?;

    let api_resources = be
        .querier()
        .get_api_resources()
        .await
        .map_err(crate::error::Error::Graph)?;
    if api_resources.is_empty() {
        println!("No API model nodes found — auto-seeding from domain configuration...");
        let stats = crate::ingest::api_ingest::ingest_api_model(be.ingestor(), &config).await?;
        println!("Auto-seeded: {stats}");
    }

    // generate uses a pre-populated backend; schema base dir is unknown here.
    // Pass an empty path so UiCodelistGenerator skips gracefully.
    run_validation(
        be.querier(),
        &config,
        false,
        false,
        codegraph_generate::db::dialect::DatabaseTarget::default(),
        codegraph_generate::profile::ApiKeyAuthz::default(),
    )
    .await?;
    let ifml_component_mappings =
        load_ifml_component_mappings(ifml_components, ifml_design_system)?;
    // Issue #293: plan-less run (no BuildPlan) — a CLI ux-rules file still
    // reaches generators; ProjectConfig stays None (see `run`).
    let ux_resolved = effective_ux_rules(ux_rules, None)?;
    // Extension-points flag (issue #503): a plan-less run with a parsed
    // config still needs the project-level gate (the AppState wiring), so
    // an explicit ProjectConfig replaces the None default in that case.
    let extension_points_project_config = ext_config.as_ref().map(|_| {
        let mut pc = crate::generate::ProjectConfig::default();
        pc.integration.has_extension_points = true;
        pc
    });
    let report = crate::generate::run_generators_with_opts(crate::generate::GeneratorOpts {
        db: be.querier(),
        config: &config,
        output_dir: output,
        tera: &tera,
        ui_overrides: &ui_overrides,
        ui_domains: &ui_domains,
        schema_base_dir: Path::new(""),
        seed_config: load_seed_config(config_path).as_deref(),
        domain_types_base: None,
        hooks_base: None,
        ext_points: ext_config.as_ref(),
        integration_manifests: integration_manifests.as_ref(),
        build_plan: None,
        profile_name: "default",
        ifml_frameworks: ifml_frameworks.to_vec(),
        ifml_components: ifml_component_mappings.as_ref(),
        ux_rules: ux_resolved,
        project_config: extension_points_project_config.as_ref(),
        emdash_plugins: None,
        domain_config_dir: config_path.parent(),
    })
    .await?;
    print_generation_report(&report);
    if check {
        check_generated_output(output)?;
    }
    Ok(())
}
