//! The pre-generation validation surface. A broken model or config never
//! half-generates: every gate here runs before any tree is written.

use codegraph_backend::Backend;

use crate::error::Result;

/// The pre-generation validation surface: ValidationPass (topology-aware,
/// issue #663), the #450 function-binding checks, and the #625 media
/// config validation. A broken config never half-generates.
pub(super) async fn run_validation_gates(
    be: &Backend,
    domain_config: &codegraph_config::config::DomainConfig,
    build_plan: Option<&crate::profile::BuildPlan>,
) -> Result<()> {
    run_validation(
        be.querier(),
        domain_config,
        build_plan.is_some_and(|plan| {
            plan.deployment_topology == codegraph_generate::profile::DeploymentTopology::Workers
        }),
        build_plan.is_some_and(|plan| plan.events_producer),
        build_plan
            .map(|plan| plan.database_target())
            .unwrap_or_default(),
        build_plan
            .map(|plan| plan.api_key_authz)
            .unwrap_or_default(),
    )
    .await?;

    // Function bindings (issue #450): unknown function names are hard
    // config errors naming the offender; `sql` bindings are refused on
    // the sqlite dialect. No-op when the `[functions]` table is absent.
    let binding_errors = crate::validate::check_function_bindings(
        be.querier(),
        domain_config,
        &build_plan
            .map(|p| p.database_target().to_string())
            .unwrap_or_else(|| "postgres".to_string()),
    )
    .await;
    if !binding_errors.is_empty() {
        return Err(crate::error::Error::Config(binding_errors.join("; ")));
    }
    // Issue #337: every HTTP-exposed function's signature must resolve to
    // generated Rust types — a broken binding never half-generates.
    let binding_errors = codegraph_generate::ddd::functions_api::check_exposed_function_types(
        be.querier(),
        domain_config,
    )
    .await;
    if !binding_errors.is_empty() {
        return Err(crate::error::Error::Config(binding_errors.join("; ")));
    }

    // Media storage (issue #625 PR-A): provider capability matrix, key
    // template, and graph resolution are hard config errors naming the
    // domain — a broken media config never half-generates. No-op when no
    // domain declares [media].
    codegraph_generate::media::validate_media_configs(be.querier(), domain_config).await?;
    Ok(())
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(super) async fn run_validation(
    querier: &dyn codegraph_core::traits::GraphQuerier,
    config: &codegraph_config::DomainConfig,
    workers_topology: bool,
    events_producer: bool,
    database_target: codegraph_generate::db::dialect::DatabaseTarget,
    api_key_authz: codegraph_generate::profile::ApiKeyAuthz,
) -> Result<()> {
    let issues = crate::validate::ValidationPass::run_with_options(
        querier,
        config,
        crate::validate::ValidationOptions {
            workers_topology,
            events_producer,
            database_target,
            api_key_authz,
        },
    )
    .await;
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.severity, crate::validate::Severity::Error))
        .collect();
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("  ERR  {} [{}]: {}", e.entity, e.check, e.message);
        }
        return Err(crate::error::Error::Validation(format!(
            "{} validation errors found",
            errors.len()
        )));
    }
    for w in issues
        .iter()
        .filter(|i| matches!(i.severity, crate::validate::Severity::Warning))
    {
        eprintln!("  WARN {} [{}]: {}", w.entity, w.check, w.message);
    }
    Ok(())
}
