//! Generated consumer idempotency at the pipeline level (issue #612):
//! with `event_idempotency` on, the drain's dispatch path keys each
//! delivery on (pgmq msg_id, consumer) through
//! `platform.event_consumption`, and the migration band carries the
//! dedupe table. Flag-off emits none of it. Node-free PR-CI pins over
//! `run_generators_with_opts`; the runtime redelivery semantics ride the
//! `events_runtime_tests` lane.

use std::collections::BTreeMap;
use std::path::Path;

use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    EvtChannelNode, EvtEventField, EvtEventNode, EvtModelGraph, EvtSubscriptionNode, SchemaNode,
};

fn config() -> codegraph_config::DomainConfig {
    codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
"#,
    )
    .unwrap()
}

fn entity_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: "sales/json/OrderType.json".to_string(),
        title: "OrderType".to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some("sales".to_string()),
        rel_path: "sales/json/OrderType.json".to_string(),
        pg_type: "JSONB".to_string(),
        rust_type: "Order".to_string(),
        sea_orm_type: "Json".to_string(),
        rust_type_name: "Order".to_string(),
        pg_table_name: "order".to_string(),
        api_path_segment: "orders".to_string(),
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

fn field(name: &str, type_json: serde_json::Value, resolved: Option<&str>) -> EvtEventField {
    EvtEventField {
        name: name.to_string(),
        type_json,
        resolved_title: resolved.map(|t| t.to_string()),
    }
}

fn primitive_string_json() -> serde_json::Value {
    serde_json::json!({"type": "primitive", "value": "string"})
}

fn orders_evt_model() -> EvtModelGraph {
    EvtModelGraph {
        source_path: "orders.evt".to_string(),
        events: vec![EvtEventNode {
            source_path: "orders.evt".to_string(),
            name: "OrderPlaced".to_string(),
            version: Some("1.0.0".to_string()),
            fields: vec![field("order_id", primitive_string_json(), None)],
            cloud_events: None,
            ordinal: 0,
        }],
        channels: vec![EvtChannelNode {
            source_path: "orders.evt".to_string(),
            name: "orders".to_string(),
            publishes: vec!["OrderPlaced".to_string()],
            scope: None,
            ordinal: 0,
        }],
        subscriptions: vec![EvtSubscriptionNode {
            source_path: "orders.evt".to_string(),
            name: "auditing".to_string(),
            events: vec!["OrderPlaced".to_string()],
            consumer: "audit-worker".to_string(),
            when: None,
            delivery: None,
            ordinal: 0,
        }],
    }
}

async fn db_with(model: Option<EvtModelGraph>) -> codegraph_core::mock::MockEngine {
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(entity_schema())
        .build();
    if let Some(model) = model {
        db.ingest_evt_model(&model).await.expect("ingest evt model");
    }
    db
}

async fn run_monolith(idempotency: bool) -> BTreeMap<String, String> {
    let plan = codegraph::profile::BuildPlan {
        entity_form: false,
        ui_live_queries: false,
        ifml_tanstack_forms: false,
        entity_generators: vec![],
        domain_generators: vec![],
        global_generators: vec![
            "evt_events".to_string(),
            "pgmq_setup".to_string(),
            "webhook_dispatch".to_string(),
            "platform_schema".to_string(),
        ],
        post_gen_scripts: vec![],
        ifml_frameworks: vec![],
        template_pack_path: None,
        database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
        has_atproto: false,
        atproto_tenancy: "shared_pds".to_string(),
        has_fern: false,
        has_emdash: false,
        fern_sdk_languages: vec![],
        persistence_provider: codegraph::profile::PersistenceProvider::SeaOrm,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Monolith,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        event_envelope: codegraph::generate::events::EnvelopeVariant::Native,
        event_idempotency: idempotency,
        event_archive: false,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        entity_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    };
    let mut project = codegraph::generate::ProjectConfig::default();
    project.codegen.event_idempotency = idempotency;
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir).unwrap();
    let output_dir = tempfile::TempDir::new().unwrap();
    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &db_with(Some(orders_evt_model())).await,
            config: &config(),
            output_dir: output_dir.path(),
            tera: &tera,
            ui_overrides: &Default::default(),
            ui_domains: &Default::default(),
            schema_base_dir: Path::new(""),
            domain_types_base: None,
            hooks_base: None,
            ext_points: None,
            integration_manifests: None,
            seed_config: None,
            build_plan: Some(&plan),
            profile_name: "default",
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .expect("monolith run generates");
    assert!(
        !report.has_errors(),
        "monolith run should have no errors: {}",
        report.summary()
    );
    let mut files = BTreeMap::new();
    for f in &report.files {
        let rel = f
            .path
            .strip_prefix(output_dir.path())
            .unwrap_or(&f.path)
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(rel, f.content.clone());
    }
    files
}

fn read<'a>(files: &'a BTreeMap<String, String>, suffix: &str) -> &'a str {
    files
        .iter()
        .find(|(k, _)| k.ends_with(suffix))
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| {
            panic!(
                "{suffix} missing (have: {:?})",
                files.keys().collect::<Vec<_>>()
            )
        })
}

/// Flag-on: the dedupe table migrates, the dispatchers carry the msg_id,
/// and every consumer dispatch is guarded insert-first.
#[tokio::test]
async fn event_idempotency_keys_consumer_deliveries() {
    let files = run_monolith(true).await;

    let consumers = read(&files, "src/events/consumers.rs");
    for needle in [
        "async fn mark_consumed(",
        "INSERT INTO platform.event_consumption (msg_id, consumer, processed_at)",
        "ON CONFLICT DO NOTHING",
        "msg_id: i64",
    ] {
        assert!(consumers.contains(needle), "missing {needle}:\n{consumers}");
    }

    let platform = read(&files, "0005_platform_schema.sql");
    assert!(
        platform.contains("CREATE TABLE IF NOT EXISTS platform.event_consumption"),
        "{platform}"
    );

    let dispatch = read(&files, "src/webhook_dispatch.rs");
    assert!(
        dispatch.contains("dispatch_registered(\n                    self.db.clone(),\n                    message.clone(),\n                    msg_id,"),
        "the drain passes the pgmq msg_id:\n{dispatch}"
    );
}

/// Flag-off: zero idempotency surface anywhere in the tree.
#[tokio::test]
async fn event_idempotency_flag_off_emits_nothing() {
    let files = run_monolith(false).await;
    let consumers = read(&files, "src/events/consumers.rs");
    assert!(!consumers.contains("event_consumption"), "{consumers}");
    assert!(!consumers.contains("msg_id"), "{consumers}");
    let platform = read(&files, "0005_platform_schema.sql");
    assert!(!platform.contains("event_consumption"), "{platform}");
    let dispatch = read(&files, "src/webhook_dispatch.rs");
    assert!(
        dispatch.contains("dispatch_registered(\n                    self.db.clone(),\n                    message.clone(),\n                )"),
        "flag-off keeps the no-msg_id call shape:\n{dispatch}"
    );
}
