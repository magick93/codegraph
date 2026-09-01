//! End-to-end tests for the API metamodel graph round-trip (issue #79).
//!
//! Regression: `ingest_edge` matched API-metamodel endpoints by `name` using
//! synthetic ids ("ar:<name>", "ao:<name>", ...) that never existed as node
//! properties, so HasOperation/UsesPipeline/etc. edges were silently never
//! created. With no edges, `get_api_operations` returned empty and
//! `resolve_entity_operations` degraded to config defaults (full CRUD),
//! emitting PUT/DELETE for append-only entities.

use codegraph::generate::api::api_model::{normalized_resource_name, resolve_entity_operations};
use codegraph::ingest::api_ingest::ingest_api_model;
use codegraph_config::config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_grafeo::GrafeoEngine;

fn compliance_config() -> DomainConfig {
    toml::from_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.compliance]
label = "Compliance"
schema_dir = "compliance"
postgres_schema = "compliance"
entities = ["Screening Result", "Document"]

[domains.compliance.entity_config."Screening Result"]
operations = ["create", "read", "list"]

[domains.compliance.entity_config.Document]
"#,
    )
    .unwrap()
}

/// The graph must contain exactly the configured operations: HasOperation
/// edges connect ApiResource → ApiOperation for append-only restrictions.
#[tokio::test]
async fn api_operations_ingested_and_queryable() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = compliance_config();
    ingest_api_model(&engine, &config).await.unwrap();

    let resource_name = normalized_resource_name("Screening Result");
    let ops = engine
        .get_api_operations(&resource_name)
        .await
        .unwrap()
        .into_iter()
        .map(|op| op.kind)
        .collect::<Vec<_>>();

    assert_eq!(ops.len(), 3, "restricted ops: {ops:?}");
    for kind in ["create", "read", "list"] {
        assert!(ops.iter().any(|k| k == kind), "missing {kind} in {ops:?}");
    }
    assert!(!ops.iter().any(|k| k == "update"), "no update in {ops:?}");
    assert!(!ops.iter().any(|k| k == "delete"), "no delete in {ops:?}");
}

/// Unrestricted entities get the configured defaults through the graph too.
#[tokio::test]
async fn default_operations_ingested_for_unrestricted_entity() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = compliance_config();
    ingest_api_model(&engine, &config).await.unwrap();

    let ops = engine
        .get_api_operations("Document")
        .await
        .unwrap()
        .into_iter()
        .map(|op| op.kind)
        .collect::<Vec<_>>();
    assert_eq!(ops.len(), 5, "full CRUD expected: {ops:?}");
}

/// UsesPipeline edges must connect HttpEndpoints to the default pipeline so
/// the router can resolve endpoint middleware.
#[tokio::test]
async fn endpoints_bind_to_default_pipeline() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = compliance_config();
    ingest_api_model(&engine, &config).await.unwrap();

    let endpoints = engine.get_http_endpoints().await.unwrap();
    assert!(!endpoints.is_empty(), "endpoints ingested");

    let pipeline = engine
        .get_pipeline_for_endpoint(&endpoints[0].path_template)
        .await
        .unwrap()
        .expect("endpoint must bind to a pipeline");
    let middleware = pipeline.middleware.unwrap_or_default();
    assert!(middleware.contains(&"auth".to_string()));
}

/// Full precedence chain against a real backend: explicit config operations
/// win; the graph agrees with the config here.
#[tokio::test]
async fn resolve_entity_operations_prefers_explicit_config() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = compliance_config();
    ingest_api_model(&engine, &config).await.unwrap();

    let ops = resolve_entity_operations(&engine, &config, "compliance", "ScreeningResult").await;
    assert_eq!(ops, vec!["create", "read", "list"]);
}

/// Without explicit config operations the graph model is authoritative.
#[tokio::test]
async fn resolve_entity_operations_uses_graph_when_config_has_none() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let config = compliance_config();
    ingest_api_model(&engine, &config).await.unwrap();

    // "Document" has an entity config but no explicit operations list.
    let mut ops = resolve_entity_operations(&engine, &config, "compliance", "Document").await;
    ops.sort();
    assert_eq!(ops, vec!["create", "delete", "list", "read", "update"]);
}
