//! The design/interaction/event/deployment ingestion passes (driver passes
//! 1b–1f): IFML, `.ddd`, `.evt`, `.deploy`, the config-defined API model,
//! modeled charts, and OpenAPI. They run AFTER the domain-plane passes so
//! every reference resolves against the full ingested title set; `.deploy`
//! compiles against its own `.ddd`/`.ifml` imports.

use codegraph_backend::Backend;

use crate::driver::sources::ModelSources;
use crate::error::Result;

/// The interaction/design/event/deployment/API-model/chart/OpenAPI ingest
/// passes (driver passes 1b–1f). Skipped entirely by the caller when the
/// graph cache reused the persisted graph.
pub(in crate::driver) async fn run_design_ingest_passes(
    be: &Backend,
    domain_config: &codegraph_config::config::DomainConfig,
    sources: &ModelSources<'_>,
) -> Result<()> {
    // Pass 1b: Ingest IFML DSL files (if provided) through the shared
    // front door (issue #591, see `ingestion::ifml`).
    if !sources.ifml_files.is_empty() {
        super::ifml::ingest_ifml_files(be, sources.ifml_files).await?;
    }

    // Pass 1c: Ingest pherolang .ddd design files (issue #449): compiled
    // against their imported .mox domains and landed as the DDD design
    // plane. Runs AFTER the schema passes so design classes resolve
    // against the full ingested title set; a compile error is a hard
    // error (the RosettaModel precedent).
    if !sources.ddd_files.is_empty() {
        println!("Pass 1c: {} ddd files to ingest", sources.ddd_files.len());
        let ddd_stats = crate::ingest::ddd_ingest::ingest_ddd_files(
            be.ingestor(),
            be.querier(),
            sources.ddd_files,
            domain_config,
        )
        .await?;
        println!("Pass 1c complete: {ddd_stats}");
    }

    // Pass 1d: Ingest pherolang .evt event-contract files (issue #454):
    // compiled against their imported .mox domains and landed as the
    // events plane. Runs AFTER the schema passes so payload field types
    // resolve against the full ingested title set; a compile error is a
    // hard error (the DddModel precedent).
    if !sources.evt_files.is_empty() {
        println!("Pass 1d: {} evt files to ingest", sources.evt_files.len());
        let evt_stats = crate::ingest::evt_ingest::ingest_evt_files(
            be.ingestor(),
            be.querier(),
            sources.evt_files,
            domain_config,
        )
        .await?;
        println!("Pass 1d complete: {evt_stats}");
    }

    // Pass 1e: Ingest pherolang .deploy deployment files (pherolang #49):
    // compiled against their imported .ddd designs and .ifml flows and
    // landed as the deployment plane. A compile error is a hard error
    // (the EvtModel precedent).
    if !sources.deploy_files.is_empty() {
        println!(
            "Pass 1e: {} deploy files to ingest",
            sources.deploy_files.len()
        );
        let deploy_stats =
            crate::ingest::deploy_ingest::ingest_deploy_files(be.ingestor(), sources.deploy_files)
                .await?;
        println!("Pass 1e complete: {deploy_stats}");
    }

    // Pass 1c: Ingest API model from domain configuration
    ingest_api_model(be, domain_config).await?;

    // Pass 1f: Ingest modeled charts from the `[charts]` config table
    // (issue #577). Runs AFTER the schema passes so entity refs resolve
    // against the full ingested title set; the deprecated
    // `has_orgchart` sugar synthesizes org entries here (WARN).
    if !domain_config.charts.is_empty()
        || crate::ingest::chart_ingest::has_orgchart_sugar(domain_config)
    {
        let chart_stats = crate::ingest::chart_ingest::ingest_chart_definitions(
            be.ingestor(),
            be.querier(),
            domain_config,
        )
        .await?;
        println!("Pass 1f complete: {chart_stats}");
    }

    // Pass 1d: Ingest OpenAPI spec files (if provided)
    if !sources.openapi_files.is_empty() {
        println!(
            "Pass 1d: {} OpenAPI files to ingest",
            sources.openapi_files.len()
        );
        for openapi_path in sources.openapi_files {
            let stats =
                crate::ingest::openapi_ingest::ingest_openapi_file(be.ingestor(), openapi_path)
                    .await?;
            println!("  ingested {}: {stats}", openapi_path.display());
        }
    }
    Ok(())
}

/// Pass 1c: ingest the API model from the domain configuration — shared by
/// the full pipeline and the IFML-only path.
pub(in crate::driver) async fn ingest_api_model(
    be: &Backend,
    domain_config: &codegraph_config::config::DomainConfig,
) -> Result<()> {
    let api_stats =
        crate::ingest::api_ingest::ingest_api_model(be.ingestor(), domain_config).await?;
    println!("Pass 1c complete: {api_stats}");
    Ok(())
}
