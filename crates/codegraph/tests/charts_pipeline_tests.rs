//! Issue #577 — the chart plane: modeled `ChartDefinition` graph nodes,
//! the registry-wired `UiChartsGenerator` renderer (the org-chart bolt-on
//! is retired), the deprecated `has_orgchart` sugar, and the validation
//! contract (unresolvable bindings are generation errors naming the
//! chart, never bare names in templates).
//!
//! Node-free PR-CI pins: in-memory Grafeo + the real template engine —
//! no node, no DB, no protoc.

use std::fs;

use codegraph_config::config::parse_domain_config_str;
use codegraph_config::{ChartDefinitions, DomainConfig};
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_generate::charts::UiChartsGenerator;
use codegraph_generate::traits::GlobalGenerator;
use codegraph_generate::{ProjectConfig, traits::GeneratedFile};
use codegraph_grafeo::GrafeoEngine;

// ── fixture ─────────────────────────────────────────────────────────────

const DOMAINS_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]
type_suffix = "Type"

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["OrganizationType", "PositionType"]

[charts."org"]
kind = "org"
entity = "Organization"
title = "Org Chart"
route = "org-chart"

[charts."org".org]
hierarchy_field = "parent"
label_fields = ["name"]
detail_fields = []

[charts."headcount"]
kind = "bar"
entity = "PositionType"
route = "headcount"

[charts."headcount".series]
label_field = "department"
value_fields = ["headcount"]
aggregate = "sum"

[charts."total"]
kind = "metric"
entity = "PositionType"

[charts."total".metric]
value_field = "headcount"
aggregate = "count"
label = "Total positions"
"#;

fn org_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "OrganizationType".into(),
        title: "OrganizationType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("common".into()),
        namespace: None,
        rel_path: "common/organization.json".into(),
        pg_type: "uuid".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Organization".into(),
        pg_table_name: "organization".into(),
        api_path_segment: "organization".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn position_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "PositionType".into(),
        title: "PositionType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("common".into()),
        namespace: None,
        rel_path: "common/position.json".into(),
        pg_type: "uuid".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Position".into(),
        pg_table_name: "position".into(),
        api_path_segment: "position".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn prop(name: &str) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: "string".into(),
        description: None,
        format: None,
        is_required: true,
        is_nullable: false,
        is_id: false,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.into(),
        pg_column_type: "TEXT".into(),
        rust_field_name: name.into(),
        rust_field_type: "String".into(),
        sea_orm_type: "String".into(),
        render_strategy: "flat".into(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}

async fn ingest_model(db: &(impl GraphIngestor + GraphQuerier), config: &DomainConfig) {
    db.ingest_schema(&org_schema()).await.expect("org schema");
    db.ingest_schema(&position_schema())
        .await
        .expect("position schema");
    for (title, fields) in [
        ("OrganizationType", vec!["id", "parent", "name"]),
        ("PositionType", vec!["id", "department", "headcount"]),
    ] {
        for field in fields {
            db.ingest_property(title, title, &prop(field))
                .await
                .expect("property");
        }
    }
    codegraph::ingest::api_ingest::ingest_api_model(db, config)
        .await
        .expect("api model");
}

async fn ingest_charts(
    db: &(impl GraphIngestor + GraphQuerier),
    config: &DomainConfig,
) -> codegraph::ingest::chart_ingest::ChartIngestStats {
    codegraph::ingest::chart_ingest::ingest_chart_definitions(db, db, config)
        .await
        .expect("chart ingest")
}

fn domains_toml(extra: &str) -> DomainConfig {
    parse_domain_config_str(&format!("{DOMAINS_TOML}\n{extra}")).expect("domains.toml")
}

fn empty_charts_config() -> DomainConfig {
    parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["OrganizationType"]
"#,
    )
    .expect("domains.toml")
}

async fn run_charts_generator(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    out: &std::path::Path,
) -> Result<Vec<GeneratedFile>, codegraph_generate::error::Error> {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[])?;
    let project = ProjectConfig::default();
    let generator = UiChartsGenerator::new(out);
    generator.generate(db, config, &[], &tera, &project).await
}

// ── graph plane ─────────────────────────────────────────────────────────

#[tokio::test]
async fn chart_definitions_ingest_and_readback_round_trip() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = domains_toml("");
    ingest_model(&engine, &config).await;
    let stats = ingest_charts(&engine, &config).await;

    assert_eq!(stats.charts, 3, "three declared charts");
    assert_eq!(stats.sugar_charts, 0);
    assert_eq!(stats.unresolved_entities, 0);
    assert_eq!(stats.unresolved_fields, 0);

    let charts = engine.get_chart_definitions().await.expect("charts");
    assert_eq!(charts.len(), 3);
    let org = charts.iter().find(|c| c.name == "org").expect("org chart");
    assert_eq!(
        org.entity_title.as_deref(),
        Some("OrganizationType"),
        "bare ref resolves via the Type suffix"
    );
    assert_eq!(org.domain.as_deref(), Some("common"));

    let bindings = engine
        .get_chart_property_bindings()
        .await
        .expect("bindings");
    let org_bindings: Vec<_> = bindings
        .iter()
        .filter(|(chart, ..)| chart == "org")
        .collect();
    assert!(
        org_bindings
            .iter()
            .any(|(_, field, _, role)| field == "parent" && role.as_deref() == Some("hierarchy")),
        "hierarchy binding edge must land: {org_bindings:?}"
    );
    assert!(
        org_bindings
            .iter()
            .any(|(_, field, _, role)| field == "name" && role.as_deref() == Some("label")),
        "label binding edge must land: {org_bindings:?}"
    );
}

#[tokio::test]
async fn has_orgchart_sugar_synthesizes_an_org_chart() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = domains_toml(
        r#"
[domains.common.entity_config.OrganizationType]
has_orgchart = true
hierarchy_field = "parent"
"#,
    );
    ingest_model(&engine, &config).await;
    let stats = ingest_charts(&engine, &config).await;

    assert_eq!(stats.sugar_charts, 1, "the sugar synthesizes one chart");
    let charts = engine.get_chart_definitions().await.expect("charts");
    let synthesized = charts.iter().find(|c| c.name == "organization-org");
    assert!(
        synthesized.is_some(),
        "sugar chart named after the kebab entity: {:?}",
        charts.iter().map(|c| c.name.clone()).collect::<Vec<_>>()
    );
    let synthesized = synthesized.expect("synthesized");
    let payload: codegraph_config::ChartPayload =
        serde_json::from_str(synthesized.config_json.as_deref().unwrap_or_default())
            .expect("payload");
    assert!(matches!(
        payload,
        codegraph_config::ChartPayload::Org(ref cfg)
            if cfg.hierarchy_field.as_deref() == Some("parent")
    ));
}

#[tokio::test]
async fn has_orgchart_sugar_skips_entities_without_a_hierarchy() {
    // The dead generator never emitted for hierarchy-less entities, so
    // the sugar must not start failing their generation.
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]
type_suffix = "Type"

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PositionType"]

[domains.common.entity_config.PositionType]
has_orgchart = true
"#,
    )
    .expect("domains.toml");
    ingest_model(&engine, &config).await;
    let stats = ingest_charts(&engine, &config).await;

    assert_eq!(stats.sugar_charts, 0, "nothing synthesized");
    assert_eq!(stats.sugar_skipped_no_hierarchy, 1, "skipped with a WARN");
    let charts = engine.get_chart_definitions().await.expect("charts");
    assert!(charts.is_empty(), "no chart nodes for the skipped flag");
}

#[tokio::test]
async fn unresolvable_entity_warns_at_ingest_and_fails_generation_by_name() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = parse_domain_config_str(
        r#"
[defaults]
operations = ["list"]
type_suffix = "Type"

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PositionType"]

[charts."ghost"]
kind = "metric"
entity = "GhostType"
[charts."ghost".metric]
value_field = "headcount"
"#,
    )
    .expect("domains.toml");
    ingest_model(&engine, &config).await;
    let stats = ingest_charts(&engine, &config).await;
    assert_eq!(stats.unresolved_entities, 1, "the ghost entity warns");

    let err = run_charts_generator(&engine, &config, std::env::temp_dir().as_path())
        .await
        .expect_err("generation must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("chart 'ghost'") && msg.contains("GhostType"),
        "the error must name the chart and the entity: {msg}"
    );
}

#[tokio::test]
async fn org_without_hierarchy_field_is_a_generation_error_naming_the_fix() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = parse_domain_config_str(
        r#"
[defaults]
operations = ["list"]
type_suffix = "Type"

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["OrganizationType"]

[charts."org"]
kind = "org"
entity = "OrganizationType"
[charts."org".org]
label_fields = ["name"]
"#,
    )
    .expect("domains.toml");
    ingest_model(&engine, &config).await;
    ingest_charts(&engine, &config).await;

    let err = run_charts_generator(&engine, &config, std::env::temp_dir().as_path())
        .await
        .expect_err("generation must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("hierarchy_field") && msg.contains("no self-referencing hierarchy"),
        "the error must name the fix: {msg}"
    );
}

#[tokio::test]
async fn unknown_chart_kind_is_a_hard_config_error() {
    let config = parse_domain_config_str(
        r#"
[defaults]
operations = ["list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[charts."x"]
kind = "treemap"
entity = "Foo"
[charts."x".series]
label_field = "a"
"#,
    );
    assert!(
        config.is_err(),
        "unknown kinds never parse (closed vocabulary)"
    );
    // A kind/payload mismatch parses but is a hard INGEST error.
    let config = parse_domain_config_str(
        r#"
[defaults]
operations = ["list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[charts."x"]
kind = "bar"
entity = "Foo"
[charts."x".series]
label_field = "a"
[charts."x".metric]
value_field = "a"
"#,
    )
    .expect("parses");
    let err = config
        .charts
        .validate()
        .expect_err("kind/payload mismatch is a validation error");
    assert!(err.to_string().contains("does not take"), "{err}");
}

// ── generation plane ────────────────────────────────────────────────────

#[tokio::test]
async fn org_chart_generation_emits_model_driven_routes() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = domains_toml("");
    ingest_model(&engine, &config).await;
    ingest_charts(&engine, &config).await;

    let out = tempfile::tempdir().expect("tempdir");
    let files = run_charts_generator(&engine, &config, out.path())
        .await
        .expect("generation");

    let org_load = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("org-chart/+page.server.ts")
        })
        .expect("org-chart load emitted");
    let org_page = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("org-chart/+page.svelte"))
        .expect("org-chart page emitted");

    // The #574 S1 neutral server contract + model-driven data (no
    // hardcoded domains, no consumer-owned component imports).
    let load = &org_load.content;
    assert!(
        load.contains("import { apiFetch } from '$lib/server/api';"),
        "the load rides the shared helper:\n{load}"
    );
    assert!(
        load.contains("/api/v1/common/organization"),
        "the API path resolves from the API model:\n{load}"
    );
    assert!(
        load.contains("'parent'"),
        "the hierarchy field is model-driven:\n{load}"
    );
    assert!(
        !load.contains("PUBLIC_API_URL") && !load.contains("@crewbase"),
        "retired vocabulary must be gone:\n{load}"
    );

    let page = &org_page.content;
    assert!(
        page.contains("data-chart-kind=\"org\""),
        "kind marker:\n{page}"
    );
    assert!(
        page.contains("data-testid=\"chart-org-chart\""),
        "testid vocabulary:\n{page}"
    );
    assert!(
        !page.contains("@crewbase"),
        "the @crewbase/ui build break must be gone:\n{page}"
    );
}

#[tokio::test]
async fn series_and_metric_kinds_emit_client_side_aggregation() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = domains_toml("");
    ingest_model(&engine, &config).await;
    ingest_charts(&engine, &config).await;

    let out = tempfile::tempdir().expect("tempdir");
    let files = run_charts_generator(&engine, &config, out.path())
        .await
        .expect("generation");

    let bar_load = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("headcount/+page.server.ts")
        })
        .expect("bar load emitted");
    assert!(
        bar_load.content.contains("aggregateSeries"),
        "client-side aggregation:\n{}",
        bar_load.content
    );
    assert!(
        bar_load.content.contains("'sum'") || bar_load.content.contains("case 'sum'"),
        "the declared aggregate lowers:\n{}",
        bar_load.content
    );
    assert!(
        bar_load.content.contains("/api/v1/common/position"),
        "model-driven API path:\n{}",
        bar_load.content
    );

    let bar_page = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("headcount/+page.svelte"))
        .expect("bar page emitted");
    assert!(
        bar_page.content.contains("data-chart-kind=\"bar\""),
        "{}",
        bar_page.content
    );

    let metric_load = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("total/+page.server.ts"))
        .expect("metric load emitted");
    assert!(
        metric_load.content.contains("kind: 'metric'"),
        "{}",
        metric_load.content
    );
}

#[tokio::test]
async fn no_charts_emit_nothing() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = empty_charts_config();
    ingest_model(&engine, &config).await;
    let stats = ingest_charts(&engine, &config).await;
    assert_eq!(stats.charts, 0);

    let out = tempfile::tempdir().expect("tempdir");
    let files = run_charts_generator(&engine, &config, out.path())
        .await
        .expect("generation");
    assert!(
        files.is_empty(),
        "flag-off (no charts) must be byte-identical: {files:?}"
    );
    assert!(fs::read_dir(out.path().join("ui")).is_err(), "no ui output");
}

#[tokio::test]
async fn mapped_component_replaces_builtin_markup() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    let config = domains_toml("");
    ingest_model(&engine, &config).await;
    ingest_charts(&engine, &config).await;

    let mappings: codegraph_config::IfmlComponentMappings = toml::from_str(
        r#"
[[component]]
type = "orgchart"
path = "$lib/components/OrgChart.svelte"
export = "OrgChart"
"#,
    )
    .expect("mappings");

    let out = tempfile::tempdir().expect("tempdir");
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let project = ProjectConfig::default();
    let generator = UiChartsGenerator::new(out.path()).with_mappings(Some(mappings));
    let files = generator
        .generate(&engine, &config, &[], &tera, &project)
        .await
        .expect("generation");

    let org_page = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("org-chart/+page.svelte"))
        .expect("org-chart page emitted");
    let page = &org_page.content;
    assert!(
        page.contains("import OrgChart from '$lib/components/OrgChart.svelte'"),
        "the mapped component is imported:\n{page}"
    );
    assert!(
        page.contains("<OrgChart nodes={data.tree} />"),
        "mapped invocation carries the conventional props:\n{page}"
    );
    assert!(
        !page.contains("chart-tree"),
        "the built-in fallback markup is replaced:\n{page}"
    );

    // Unmapped kinds keep the built-in markup.
    let bar_page = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("headcount/+page.svelte"))
        .expect("bar page emitted");
    assert!(
        bar_page.content.contains("chart-bars"),
        "unmapped kinds keep the fallback:\n{}",
        bar_page.content
    );
}

#[tokio::test]
async fn charts_btree_iteration_is_deterministic() {
    let config = domains_toml("");
    let charts: &ChartDefinitions = &config.charts;
    let names: Vec<&str> = charts.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["headcount", "org", "total"], "BTreeMap order");
}
