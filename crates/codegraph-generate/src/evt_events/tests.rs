//! Generation tests for the `evt_events` generator (issues #454/#455,
//! Phases 3-4), including the byte-identity goldens that pin the flag-off
//! rendering of every touched template.

use std::path::Path;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    EvtChannelNode, EvtEventField, EvtEventNode, EvtModelGraph, EvtSubscriptionNode, SchemaNode,
};
use tempfile::TempDir;

use crate::evt_events::EvtEventsGenerator;
use crate::project_config::ProjectConfig;
use crate::render_template_with_project;
use crate::scaffold::generator::ScaffoldContext;
use crate::template_engine::create_tera;
use crate::traits::GlobalGenerator;
use crate::webhook::dispatch::WebhookDispatchGenerator;
use crate::webhook::endpoint_api::WebhookEndpointApiGenerator;

// ── fixtures ───────────────────────────────────────────────────────────

fn config() -> DomainConfig {
    toml::from_str(
        r#"
[domains.sales]
label = "Sales"
schema_dir = "schemas/sales"
postgres_schema = "sales"
"#,
    )
    .expect("config parses")
}

fn tera() -> tera::Tera {
    create_tera(Path::new("")).expect("embedded templates")
}

fn codelist_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: "sales/json/OrderStatusType.json".to_string(),
        title: "OrderStatusType".to_string(),
        description: None,
        schema_type: "string".to_string(),
        classification: "codelist".to_string(),
        domain: Some("sales".to_string()),
        rel_path: "sales/json/OrderStatusType.json".to_string(),
        pg_type: "TEXT".to_string(),
        rust_type: "OrderStatus".to_string(),
        sea_orm_type: "Text".to_string(),
        rust_type_name: "OrderStatus".to_string(),
        pg_table_name: "order_status".to_string(),
        api_path_segment: String::new(),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
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

fn class_json(name: &str) -> serde_json::Value {
    serde_json::json!({"type": "class", "value": {"package": "app", "name": name}})
}

/// The `orders.evt` shape: a versioned event (primitive + enum-resolved
/// field), an unversioned two-primitive event, one channel publishing
/// both, and two subscriptions to different consumers.
fn orders_evt_model() -> EvtModelGraph {
    EvtModelGraph {
        source_path: "orders.evt".to_string(),
        events: vec![
            EvtEventNode {
                source_path: "orders.evt".to_string(),
                name: "OrderPlaced".to_string(),
                version: Some("1.2.0".to_string()),
                fields: vec![
                    field("order_id", primitive_string_json(), None),
                    field(
                        "status",
                        class_json("OrderStatusType"),
                        Some("OrderStatusType"),
                    ),
                ],
                ordinal: 0,
            },
            EvtEventNode {
                source_path: "orders.evt".to_string(),
                name: "OrderCancelled".to_string(),
                version: None,
                fields: vec![
                    field("reason", primitive_string_json(), None),
                    field("note", primitive_string_json(), None),
                ],
                ordinal: 1,
            },
        ],
        channels: vec![EvtChannelNode {
            source_path: "orders.evt".to_string(),
            name: "orders".to_string(),
            publishes: vec!["OrderPlaced".to_string(), "OrderCancelled".to_string()],
            ordinal: 0,
        }],
        subscriptions: vec![
            EvtSubscriptionNode {
                source_path: "orders.evt".to_string(),
                name: "auditing".to_string(),
                events: vec!["OrderPlaced".to_string()],
                consumer: "audit-worker".to_string(),
                ordinal: 0,
            },
            EvtSubscriptionNode {
                source_path: "orders.evt".to_string(),
                name: "shipping".to_string(),
                events: vec!["OrderCancelled".to_string()],
                consumer: "shipping-fanout".to_string(),
                ordinal: 1,
            },
        ],
    }
}

async fn db_with(model: Option<EvtModelGraph>) -> codegraph_core::mock::MockEngine {
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(codelist_schema())
        .build();
    if let Some(model) = model {
        db.ingest_evt_model(&model).await.expect("ingest evt model");
    }
    db
}

fn project() -> ProjectConfig {
    ProjectConfig::default()
}

fn sqlite_project() -> ProjectConfig {
    let mut project = ProjectConfig::default();
    project.database.database_target = crate::db::dialect::DatabaseTarget::Sqlite;
    project
}

/// Generate and return the emitted files keyed by file name.
async fn generate_events(
    db: &codegraph_core::mock::MockEngine,
    project: &ProjectConfig,
) -> Vec<(String, String)> {
    let dir = TempDir::new().unwrap();
    let generator = EvtEventsGenerator::new(dir.path());
    let files = generator
        .generate(db, &config(), &[], &tera(), project)
        .await
        .expect("evt_events generates");
    files
        .into_iter()
        .map(|f| {
            (
                f.path.file_name().unwrap().to_string_lossy().into_owned(),
                f.content.clone(),
            )
        })
        .collect()
}

fn content_of<'a>(files: &'a [(String, String)], name: &str) -> &'a str {
    files
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, c)| c.as_str())
        .unwrap_or_else(|| panic!("{name} not emitted"))
}

// ── the full fixture: contracts + emit + consumers + mod ───────────────

#[tokio::test]
async fn orders_fixture_emits_contracts_with_typed_payloads_and_version_const() {
    let db = db_with(Some(orders_evt_model())).await;
    let files = generate_events(&db, &project()).await;
    let contracts = content_of(&files, "contracts.rs");

    assert!(
        contracts.contains("pub struct OrderPlacedPayload"),
        "{contracts}"
    );
    assert!(contracts.contains("pub order_id: String,"));
    assert!(contracts.contains("pub status: OrderStatus,"));
    assert!(
        contracts.contains("pub const ORDER_PLACED_VERSION: &str = \"1.2.0\";"),
        "versioned event pins its const"
    );
    assert!(contracts.contains("pub struct OrderCancelledPayload"));
    assert!(contracts.contains("pub reason: String,"));
    assert!(contracts.contains("pub note: String,"));
    assert!(
        !contracts.contains("ORDER_CANCELLED_VERSION"),
        "unversioned events carry no const"
    );
    assert!(
        contracts.contains("use crate::codelist::OrderStatus;"),
        "enum-resolved fields import from the codelist module"
    );
}

#[tokio::test]
async fn orders_fixture_emits_channel_enum_and_pgmq_publish_fn() {
    let db = db_with(Some(orders_evt_model())).await;
    let files = generate_events(&db, &project()).await;
    let emit = content_of(&files, "emit.rs");

    assert!(emit.contains("pub enum OrdersEvent"), "{emit}");
    assert!(emit.contains("OrderPlaced(OrderPlacedPayload),"));
    assert!(emit.contains("OrderCancelled(OrderCancelledPayload),"));
    assert!(emit.contains("pub async fn publish_orders("));
    assert!(
        emit.contains("pgmq.send('events_orders'"),
        "the publish statement targets the channel's queue: {emit}"
    );
    // The envelope keys the drain and the subscription matcher rely on.
    assert!(emit.contains("\"event\": event.event_name(),"));
    assert!(emit.contains("\"version\": event.event_version(),"));
    assert!(emit.contains("\"channel\": \"orders\","));
    assert!(emit.contains("\"payload\": event.payload(),"));
    assert!(emit.contains("\"event_type\": event.event_name(),"));
    assert!(emit.contains("\"domain\": event.domain(),"));
    assert!(emit.contains("\"occurred_at\": chrono::Utc::now().to_rfc3339(),"));
}

#[tokio::test]
async fn orders_fixture_emits_handler_traits_and_the_consumer_registry() {
    let db = db_with(Some(orders_evt_model())).await;
    let files = generate_events(&db, &project()).await;
    let consumers = content_of(&files, "consumers.rs");

    assert!(
        consumers.contains("pub struct AuditWorkerContext<'a>"),
        "{consumers}"
    );
    assert!(consumers.contains("pub trait AuditWorkerHandler"));
    assert!(consumers.contains("async fn on_order_placed("));
    assert!(consumers.contains("pub struct ShippingFanoutContext<'a>"));
    assert!(consumers.contains("pub trait ShippingFanoutHandler"));
    assert!(consumers.contains("async fn on_order_cancelled("));
    assert!(consumers.contains("pub fn dispatch_audit_worker("));
    assert!(consumers.contains("pub fn dispatch_shipping_fanout("));
    assert!(consumers.contains("pub fn register_audit_worker("));
    assert!(consumers.contains("pub fn register_shipping_fanout("));
    assert!(
        consumers.contains("pub(crate) fn dispatch_registered("),
        "the drain's entry point"
    );
    assert!(
        consumers
            .contains("use crate::events::contracts::{OrderCancelledPayload, OrderPlacedPayload};"),
        "dispatchers reference the typed payloads: {consumers}"
    );
}

#[tokio::test]
async fn events_mod_names_sources_and_payloads() {
    let db = db_with(Some(orders_evt_model())).await;
    let files = generate_events(&db, &project()).await;
    let events_mod = content_of(&files, "mod.rs");

    assert!(events_mod.contains("pub mod contracts;"), "{events_mod}");
    assert!(events_mod.contains("pub mod emit;"));
    assert!(events_mod.contains("pub mod consumers;"));
    assert!(
        events_mod.contains("`orders.evt`"),
        "source files are named"
    );
    assert!(events_mod.contains("OrderPlacedPayload"));
    assert!(events_mod.contains("OrderCancelledPayload"));
}

// ── gating: the acceptance contract ────────────────────────────────────

#[tokio::test]
async fn empty_graph_emits_zero_files() {
    let db = db_with(None).await;
    let files = generate_events(&db, &project()).await;
    assert!(
        files.is_empty(),
        "no .evt input ⇒ zero files, got: {files:?}"
    );
}

// ── sqlite: unbound channels emit no publish surface ───────────────────

#[tokio::test]
async fn sqlite_unbound_channel_emits_no_publish_fns() {
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(codelist_schema())
        .build();
    db.ingest_evt_model(&EvtModelGraph {
        source_path: "orders.evt".to_string(),
        events: vec![EvtEventNode {
            source_path: "orders.evt".to_string(),
            name: "OrderPlaced".to_string(),
            version: None,
            fields: vec![field("note", primitive_string_json(), None)],
            ordinal: 0,
        }],
        channels: vec![EvtChannelNode {
            source_path: "orders.evt".to_string(),
            name: "orders".to_string(),
            publishes: vec!["OrderPlaced".to_string()],
            ordinal: 0,
        }],
        subscriptions: vec![EvtSubscriptionNode {
            source_path: "orders.evt".to_string(),
            name: "auditing".to_string(),
            events: vec!["OrderPlaced".to_string()],
            consumer: "audit-worker".to_string(),
            ordinal: 0,
        }],
    })
    .await
    .expect("ingest evt model");

    // Binding rule: sqlite leaves the channel unbound (warned by the IR).
    let arch = crate::events::EventArchitecture::from_graph(
        &db,
        &*crate::db::dialect::dialect_for_target(crate::db::dialect::DatabaseTarget::Sqlite),
        &config(),
    )
    .await
    .expect("architecture builds");
    assert_eq!(arch.bound_channels().count(), 0);

    // Contracts + consumers are transport-free and still emitted; emit.rs
    // carries no publish surface.
    let files = generate_events(&db, &sqlite_project()).await;
    assert!(!files.is_empty(), "transport-free modules still emit");
    let contracts = content_of(&files, "contracts.rs");
    assert!(contracts.contains("pub struct OrderPlacedPayload"));
    let emit = content_of(&files, "emit.rs");
    assert!(
        !emit.contains("pgmq.send"),
        "no publish fns on sqlite: {emit}"
    );
    assert!(!emit.contains("pub async fn publish_"));
    assert!(emit.contains("pub struct EventContext"));
    let consumers = content_of(&files, "consumers.rs");
    assert!(consumers.contains("pub trait AuditWorkerHandler"));
}

// ── subscription resolution ────────────────────────────────────────────

#[tokio::test]
async fn subscription_with_unknown_events_skips_its_consumer() {
    let mut model = orders_evt_model();
    model.subscriptions.push(EvtSubscriptionNode {
        source_path: "orders.evt".to_string(),
        name: "ghosting".to_string(),
        events: vec!["GhostEvent".to_string()],
        consumer: "ghost-worker".to_string(),
        ordinal: 2,
    });
    let db = db_with(Some(model)).await;
    let files = generate_events(&db, &project()).await;
    let consumers = content_of(&files, "consumers.rs");
    assert!(
        !consumers.contains("GhostWorker"),
        "unresolvable subscription events skip the consumer: {consumers}"
    );
    assert!(
        consumers.contains("pub trait AuditWorkerHandler"),
        "resolvable consumers are unaffected"
    );
}

#[tokio::test]
async fn duplicate_consumer_subscriptions_emit_one_handler_trait() {
    let mut model = orders_evt_model();
    model.subscriptions.push(EvtSubscriptionNode {
        source_path: "orders.evt".to_string(),
        name: "auditing-2".to_string(),
        events: vec!["OrderCancelled".to_string()],
        consumer: "audit-worker".to_string(),
        ordinal: 2,
    });
    let db = db_with(Some(model)).await;
    let files = generate_events(&db, &project()).await;
    let consumers = content_of(&files, "consumers.rs");
    assert_eq!(
        consumers.matches("pub trait AuditWorkerHandler").count(),
        1,
        "one handler trait per consumer, not per subscription: {consumers}"
    );
}

// ── webhook dispatch drain gating ──────────────────────────────────────

#[tokio::test]
async fn dispatch_generator_renders_typed_branch_when_events_present() {
    let db = db_with(Some(orders_evt_model())).await;
    let dir = TempDir::new().unwrap();
    let files = WebhookDispatchGenerator::new(dir.path())
        .generate(&db, &config(), &[], &tera(), &project())
        .await
        .expect("dispatch generates");
    let content = &files[0].content;

    assert!(
        content.contains("crate::events::consumers::dispatch_registered(&self.db, &message) == 0"),
        "the delete condition extends to consumer dispatches: {content}"
    );
    assert!(
        content.contains("message.get(\"event\").is_some()"),
        "typed envelopes are detected by the `event` key"
    );
    assert!(
        content.contains("message[\"event\"].as_str().unwrap_or(\"\")"),
        "the declared event name drives event_type"
    );
    assert!(
        content.contains("message[\"event_type\"].as_str().unwrap_or(\"\")"),
        "the trigger-envelope path is preserved"
    );
}

#[tokio::test]
async fn dispatch_template_without_evt_ctx_renders_untyped_only() {
    let out = render_template_with_project(
        &tera(),
        "webhook/dispatch.tera",
        &serde_json::json!({}),
        &project(),
    )
    .unwrap();
    assert!(out.contains("let event_type = message[\"event_type\"].as_str().unwrap_or(\"\");"));
    assert!(!out.contains("dispatch_registered"));
    assert!(!out.contains("is_typed_envelope"));
}

// ── webhook subscription vocabulary gating ─────────────────────────────

#[tokio::test]
async fn api_endpoints_render_vocabulary_validation_when_events_present() {
    let db = db_with(Some(orders_evt_model())).await;
    let dir = TempDir::new().unwrap();
    let files = WebhookEndpointApiGenerator::new(dir.path())
        .generate(&db, &config(), &[], &tera(), &project())
        .await
        .expect("api endpoints generate");
    let endpoints = &files[0].content;

    assert!(
        endpoints.contains("DECLARED.contains(&event_type.as_str())"),
        "{endpoints}"
    );
    assert!(
        endpoints
            .contains("\"OrderCancelled\", \"OrderPlaced\", \"created\", \"updated\", \"deleted\"")
    );
    assert!(
        endpoints.contains("unknown event_type"),
        "rejections use the existing error-response shape"
    );
}

// ── goldens: flag-off rendering is byte-identical ──────────────────────

/// The pre-#455 `webhook/dispatch.tera` (verbatim).
const DISPATCH_PRE_EVT: &str = include!("../evt_golden/dispatch_pre_evt.rs");
/// The pre-#455 `webhook/api_endpoints.tera` (verbatim).
const API_ENDPOINTS_PRE_EVT: &str = include!("../evt_golden/api_endpoints_pre_evt.rs");
/// The pre-#455 `scaffold/lib.tera` (verbatim).
const LIB_PRE_EVT: &str = include!("../evt_golden/lib_pre_evt.rs");
/// The pre-#455 `scaffold/cargo_toml.tera` (verbatim).
const CARGO_TOML_PRE_EVT: &str = include!("../evt_golden/cargo_toml_pre_evt.rs");

fn render_pre_evt(
    template_name: &str,
    golden: &str,
    ctx: &impl serde::Serialize,
    project: &ProjectConfig,
) -> String {
    let mut baseline = tera::Tera::default();
    baseline.add_raw_template("pre_evt", golden).unwrap();
    let expected = render_template_with_project(&baseline, "pre_evt", ctx, project).unwrap();
    let current = render_template_with_project(&tera(), template_name, ctx, project).unwrap();
    assert_eq!(
        current, expected,
        "flag-off {template_name} rendering changed"
    );
    expected
}

#[test]
fn dispatch_template_flag_off_is_byte_identical_to_pre_evt() {
    render_pre_evt(
        "webhook/dispatch.tera",
        DISPATCH_PRE_EVT,
        &serde_json::json!({}),
        &project(),
    );
}

#[test]
fn api_endpoints_template_flag_off_is_byte_identical_to_pre_evt() {
    render_pre_evt(
        "webhook/api_endpoints.tera",
        API_ENDPOINTS_PRE_EVT,
        &serde_json::json!({}),
        &project(),
    );
}

fn scaffold_ctx(has_events: bool) -> ScaffoldContext {
    ScaffoldContext {
        app_name: "sales-app".to_string(),
        grant_schemas: vec!["common".to_string(), "sales".to_string()],
        domains: vec![],
        codegraph_workflow_path: String::new(),
        type_contracts_path: String::new(),
        domain_types_path: String::new(),
        hooks_api_path: String::new(),
        extensions_path: String::new(),
        app_config_path: String::new(),
        decision_engine_path: String::new(),
        has_webhooks: true,
        has_reports: false,
        has_grpc: false,
        has_atproto: false,
        has_fern: false,
        has_cli: false,
        has_test_gen: false,
        has_auth_rate_limit: false,
        has_admin_cli: false,
        has_labels: false,
        has_seed: false,
        has_events,
        seed_crate_path: String::new(),
        migration_strategy: "sea-orm".to_string(),
    }
}

#[test]
fn scaffold_lib_and_cargo_toml_flag_off_are_byte_identical_to_pre_evt() {
    let project = project();
    // has_events: false skips the key in the serialized context entirely —
    // the exact pre-#455 context shape.
    let ctx = scaffold_ctx(false);
    render_pre_evt("scaffold/lib.tera", LIB_PRE_EVT, &ctx, &project);
    render_pre_evt(
        "scaffold/cargo_toml.tera",
        CARGO_TOML_PRE_EVT,
        &ctx,
        &project,
    );
}

#[test]
fn scaffold_templates_render_events_module_when_has_events() {
    let project = project();
    let ctx = scaffold_ctx(true);
    let lib = render_template_with_project(&tera(), "scaffold/lib.tera", &ctx, &project).unwrap();
    assert!(lib.contains("pub mod events;"), "{lib}");
    let cargo =
        render_template_with_project(&tera(), "scaffold/cargo_toml.tera", &ctx, &project).unwrap();
    assert!(cargo.contains("anyhow = \"1\""), "{cargo}");
}
