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
    assert!(consumers.contains("pub async fn dispatch_audit_worker("));
    assert!(consumers.contains("pub async fn dispatch_shipping_fanout("));
    assert!(consumers.contains("pub fn register_audit_worker("));
    assert!(consumers.contains("pub fn register_shipping_fanout("));
    assert!(
        consumers.contains("pub(crate) async fn dispatch_registered(\n    db: DatabaseConnection,"),
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
        content.contains(
            "crate::events::consumers::dispatch_registered(\n                    self.db.clone(),"
        ),
        "the delete condition extends to consumer dispatches: {content}"
    );
    assert!(
        content.contains("let typed_ok = outcomes.iter().all(|outcome| outcome.ok);"),
        "the terminal condition is all-outcomes-ok (issue #611): {content}"
    );
    assert!(
        content.contains("message.get(\"event\").is_some()"),
        "typed envelopes are detected by the `event` key"
    );
    // Issue #609: CloudEvents envelopes key the name as `type` — the
    // typed branch normalizes across both shapes.
    assert!(
        content.contains("message.get(\"type\").is_some()"),
        "CloudEvents envelopes are detected by the `type` key"
    );
    assert!(
        content.contains(".or_else(|| message.get(\"type\").and_then(|v| v.as_str()))"),
        "the event name normalizes across both envelope shapes"
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
/// The pre-#505 `events/consumers.tera` (verbatim — the sea_orm branch).
const CONSUMERS_PRE_WORKERS: &str = include!("../evt_golden/consumers_pre_workers.rs");
/// The pre-#505 `events/emit.tera` (verbatim — the sea_orm branch).
const EMIT_PRE_WORKERS: &str = include!("../evt_golden/emit_pre_workers.rs");

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
        has_function_events: false,
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

// ── #505 Leg 1: provider-parameterized events plane ─────────────────────

/// The sea_orm branch of the provider-parameterized consumers/emit
/// templates renders byte-identically to the pre-#505 templates (the
/// monolith output contract).
#[tokio::test]
async fn consumers_and_emit_templates_flag_off_are_byte_identical_to_pre_workers() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let consumers = super::consumers_context_for(&arch, None);
    let emit = super::emit_context(&arch);
    render_pre_evt(
        "events/consumers.tera",
        CONSUMERS_PRE_WORKERS,
        &consumers,
        &project(),
    );
    render_pre_evt("events/emit.tera", EMIT_PRE_WORKERS, &emit, &project());
}

/// The cornucopia branch types the plane on the persistence-agnostic
/// `DbClient`/`ClientSource` handle: zero sea_orm references, a
/// per-consumer client acquisition in `dispatch_registered`, and the
/// idempotent registration guard workers need.
#[tokio::test]
async fn consumers_and_emit_templates_render_cornucopia_shapes() {
    let db = db_with(Some(orders_evt_model())).await;
    let mut project = project();
    project.database.persistence_provider = crate::profile::PersistenceProvider::Cornucopia;
    let files = generate_events(&db, &project).await;

    let consumers = content_of(&files, "consumers.rs");
    assert!(
        !consumers.contains("DatabaseConnection"),
        "no sea_orm types on the cornucopia branch: {consumers}"
    );
    assert!(consumers.contains("use crate::db_client::{ClientSource, DbClient};"));
    assert!(consumers.contains("pub struct AuditWorkerContext<'a>"));
    assert!(consumers.contains("pub db: &'a DbClient,"));
    assert!(consumers.contains("pub trait AuditWorkerHandler"));
    assert!(
        consumers
            .contains("pub(crate) async fn dispatch_registered(\n    source: &dyn ClientSource,"),
        "the drain's entry point acquires per-dispatch clients: {consumers}"
    );
    assert!(
        consumers.contains("let Ok(db) = source.client().await else {"),
        "client acquisition failures count as consumer failures: {consumers}"
    );
    assert!(
        consumers.contains("if guard.iter().any(|c| c.name == \"audit-worker\") {"),
        "registration is idempotent for worker isolates: {consumers}"
    );
    let emit = content_of(&files, "emit.rs");
    assert!(
        !emit.contains("DatabaseConnection"),
        "no sea_orm types on the cornucopia branch: {emit}"
    );
    assert!(emit.contains("use crate::db_client::DbClient;"));
    assert!(emit.contains("client: &DbClient,"));
    assert!(
        emit.contains("SELECT pgmq.send($1, $2)"),
        "the cornucopia publish is parameterized (no escaping workaround): {emit}"
    );
}

// ── #505 Leg 2: events under the workers topology ───────────────────────

fn generation_order_sales() -> Vec<crate::GenerationEntry> {
    vec![crate::GenerationEntry {
        schema_title: "OrderStatusType".to_string(),
        domain: "sales".to_string(),
        pg_schema: "sales".to_string(),
        is_cyclic: false,
    }]
}

/// Workers runs are cornucopia (the topology×provider rule); the events
/// plane is typed on the DbClient handle there.
fn cornucopia_project() -> ProjectConfig {
    let mut project = project();
    project.database.persistence_provider = crate::profile::PersistenceProvider::Cornucopia;
    project
}

/// Under the workers topology the events plane routes into the owning
/// worker crate: the output root carries nothing, the placement puts the
/// channel queue on the payload-domain worker, and only placed consumers
/// are emitted.
#[tokio::test]
async fn workers_topology_emits_events_into_worker_crates() {
    let db = db_with(Some(orders_evt_model())).await;
    let dir = TempDir::new().unwrap();
    let generator = EvtEventsGenerator::new(dir.path()).with_workers_topology(true);
    let files = generator
        .generate(
            &db,
            &config(),
            &generation_order_sales(),
            &tera(),
            &cornucopia_project(),
        )
        .await
        .expect("workers evt_events generates");

    let rels: Vec<String> = files
        .iter()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    assert!(
        rels.iter()
            .all(|p| !p.contains("src/events") || p.contains("workers/sales/src/events")),
        "no output-root events plane: {rels:?}"
    );
    let any = |needle: &str| rels.iter().any(|p| p.contains(needle));
    for file in ["mod.rs", "contracts.rs", "emit.rs", "consumers.rs"] {
        assert!(
            any(&format!("workers/sales/src/events/{file}")),
            "the sales worker carries {file}: {rels:?}"
        );
    }

    // The consumer rides the queue owner.
    let consumers = &files
        .iter()
        .find(|f| f.path.ends_with("consumers.rs"))
        .unwrap()
        .content;
    assert!(
        consumers.contains("pub trait AuditWorkerHandler"),
        "audit-worker subscribes to OrderPlaced (on events_orders): {consumers}"
    );
    assert!(
        consumers.contains("pub trait ShippingFanoutHandler"),
        "shipping-fanout subscribes to OrderCancelled, also published on events_orders: {consumers}"
    );

    // Monolith emission is unchanged by the topology flag.
    let monolith = EvtEventsGenerator::new(TempDir::new().unwrap().path());
    let root_files = monolith
        .generate(
            &db,
            &config(),
            &generation_order_sales(),
            &tera(),
            &project(),
        )
        .await
        .expect("monolith evt_events generates");
    assert!(
        root_files
            .iter()
            .all(|f| !f.path.to_string_lossy().contains("workers/")),
        "monolith emission never touches workers/: {root_files:?}"
    );
}

/// A channel whose queue has no owner (not a domain, ambiguous payload
/// domains, no deploy override) is a generation error naming the fix.
#[tokio::test]
async fn workers_topology_unplaceable_channel_is_a_hard_error() {
    let mut model = orders_evt_model();
    // A second channel whose events span no resolvable domain (the
    // OrderCancelled event is all-primitive in the fixture).
    model.channels.push(EvtChannelNode {
        source_path: "orders.evt".to_string(),
        name: "pings".to_string(),
        publishes: vec!["OrderCancelled".to_string()],
        ordinal: 1,
    });
    let db = db_with(Some(model)).await;
    let dir = TempDir::new().unwrap();
    let generator = EvtEventsGenerator::new(dir.path()).with_workers_topology(true);
    let err = generator
        .generate(
            &db,
            &config(),
            &generation_order_sales(),
            &tera(),
            &cornucopia_project(),
        )
        .await
        .expect_err("the unplaceable channel must fail generation");
    let message = err.to_string();
    assert!(message.contains("`pings'"), "{message}");
    assert!(message.contains("events_pings"), "{message}");
    assert!(message.contains("kind = \"queue\""), "{message}");
}

// ── #609: the CloudEvents envelope variant ──────────────────────────────

/// The envelope-variant mapping table is pinned: CloudEvents 1.0
/// structured JSON with the native identity keys carried as CE extension
/// attributes (nothing in the native envelope is lost).
#[test]
fn envelope_variant_parses_strictly() {
    use crate::events::EnvelopeVariant;
    assert_eq!(EnvelopeVariant::default(), EnvelopeVariant::Native);
    assert_eq!(EnvelopeVariant::Native.as_str(), "native");
    assert_eq!(EnvelopeVariant::CloudEvents.as_str(), "cloudevents");
    assert_eq!(
        EnvelopeVariant::from_config("native").unwrap(),
        EnvelopeVariant::Native
    );
    assert_eq!(
        EnvelopeVariant::from_config("cloudevents").unwrap(),
        EnvelopeVariant::CloudEvents
    );
    let err = EnvelopeVariant::from_config("ce").unwrap_err().to_string();
    assert!(err.contains("event_envelope"), "{err}");
    assert!(err.contains("cloudevents"), "{err}");
}

/// Under `event_envelope = "cloudevents"` the publish fns build the CE
/// 1.0 structured envelope: `type`/`source`/`id`/`time`/`datacontenttype`
/// with the payload under `data`, and the native identity keys as CE
/// extension attributes (`version`/`channel`/`domain`/`correlationid`/
/// `tenantid`). The native mirror key `event_type` is gone — matching
/// normalizes instead.
#[tokio::test]
async fn emit_template_renders_the_cloudevents_envelope() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let emit = super::emit_context(&arch);
    let mut project = project();
    project.codegen.event_envelope = "cloudevents".to_string();
    let rendered =
        render_template_with_project(&tera(), "events/emit.tera", &emit, &project).unwrap();
    for key in [
        "\"specversion\": \"1.0\"",
        "\"type\": event.event_name()",
        "\"source\": ",
        "\"id\": uuid::Uuid::new_v4()",
        "\"time\": chrono::Utc::now().to_rfc3339()",
        "\"datacontenttype\": \"application/json\"",
        "\"data\": event.payload()",
        "\"correlationid\": ctx.correlation_id",
        "\"tenantid\": ctx.tenant_id",
        "\"version\": event.event_version()",
        "\"channel\": \"{{ channel.name }}\"",
    ] {
        let key = key.replace("{{ channel.name }}", "orders");
        assert!(
            rendered.contains(&key),
            "CE envelope missing {key}:\n{rendered}"
        );
    }
    assert!(
        !rendered.contains("\"event_type\""),
        "CE envelope must not carry the native event_type mirror"
    );
}

/// The native envelope is untouched by the CE branch (the canaries pin
/// the byte-identity; this pin asserts the axis exists in the template
/// SOURCE and that the native rendering keeps the native key set).
#[tokio::test]
async fn emit_template_branches_on_the_envelope_variant() {
    let source = include_str!("../../templates/events/emit.tera");
    assert!(
        source.contains("project.event_envelope"),
        "emit.tera lost the envelope-variant discriminator"
    );
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let emit = super::emit_context(&arch);
    let rendered =
        render_template_with_project(&tera(), "events/emit.tera", &emit, &project()).unwrap();
    assert!(
        rendered.contains("\"event_type\": event.event_name()"),
        "native envelope lost the event_type mirror:\n{rendered}"
    );
    assert!(
        !rendered.contains("ce_source"),
        "native rendering must not carry the CE accessor"
    );
}

/// Envelope normalization: `dispatch_registered` and the per-consumer
/// dispatchers resolve the event name and payload through the shared
/// `event_name_of`/`payload_of` helpers, which read both the native
/// (`event`/`payload`) and CloudEvents (`type`/`data`) key sets — a
/// consumer receiving mixed producer shapes routes correctly.
#[tokio::test]
async fn consumers_template_normalizes_both_envelope_shapes() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let consumers = super::consumers_context_for(&arch, None);
    let rendered =
        render_template_with_project(&tera(), "events/consumers.tera", &consumers, &project())
            .unwrap();
    for needle in [
        "fn event_name_of(",
        "fn payload_of(",
        "event_name_of(envelope)",
        "payload_of(envelope)",
        ".get(\"type\")",
        ".get(\"data\")",
    ] {
        assert!(
            rendered.contains(needle),
            "consumers.tera lost the normalization helper {needle}:\n{rendered}"
        );
    }
}

/// #612: with `event_idempotency` on, `dispatch_registered` keys each
/// delivery on (pgmq msg_id, consumer) — insert-first into
/// `platform.event_consumption`, skip when the row already exists — so
/// an at-least-once redelivery (visibility timeout < drain cron) runs
/// the handler once. Fail-open: a dedupe-write error still delivers.
#[tokio::test]
async fn consumers_template_guards_dispatches_with_idempotency_keying() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let consumers = super::consumers_context_for(&arch, None);
    let mut project = project();
    project.codegen.event_idempotency = true;
    let rendered =
        render_template_with_project(&tera(), "events/consumers.tera", &consumers, &project)
            .unwrap();
    for needle in [
        "fn mark_consumed(",
        "INSERT INTO platform.event_consumption",
        "ON CONFLICT DO NOTHING",
        "msg_id: i64",
        "mark_consumed(&db, msg_id,",
    ] {
        assert!(
            rendered.contains(needle),
            "idempotency guard missing {needle}:\n{rendered}"
        );
    }
    // The doc discipline flips under the flag: keying is GENERATED, the
    // handler duty narrows.
    assert!(
        rendered.contains("Idempotency keying is generated"),
        "{rendered}"
    );
}

/// Flag-off: no dedupe surface at all — the msg_id parameter is absent,
/// `mark_consumed` is not emitted, and the pre-#612 rendering (the
/// re-blessed golden) stays byte-identical.
#[tokio::test]
async fn consumers_template_flag_off_has_no_idempotency_surface() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let consumers = super::consumers_context_for(&arch, None);
    let rendered =
        render_template_with_project(&tera(), "events/consumers.tera", &consumers, &project())
            .unwrap();
    assert!(!rendered.contains("event_consumption"), "{rendered}");
    assert!(!rendered.contains("mark_consumed"), "{rendered}");
    assert!(!rendered.contains("msg_id"), "{rendered}");
}

/// #612: the dedupe table rides the platform schema migration when the
/// flag is on (created BEFORE the blanket app_user grants so the drain's
/// app_user pool can insert), and is absent flag-off.
#[test]
fn platform_schema_carries_the_event_consumption_table_under_the_flag() {
    let mut project = project();
    project.codegen.event_idempotency = true;
    let rendered = render_template_with_project(
        &tera(),
        "db/platform_schema.tera",
        &serde_json::json!({}),
        &project,
    )
    .unwrap();
    assert!(
        rendered.contains("CREATE TABLE IF NOT EXISTS platform.event_consumption"),
        "{rendered}"
    );
    assert!(
        rendered.contains("PRIMARY KEY (msg_id, consumer)"),
        "{rendered}"
    );
    // Table creation precedes the blanket grant in the file order.
    let table_pos = rendered.find("event_consumption").expect("table present");
    let grant_pos = rendered
        .find("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA platform")
        .expect("blanket grant present");
    assert!(
        table_pos < grant_pos,
        "table must precede the app_user grant"
    );

    let rendered_off = render_template_with_project(
        &tera(),
        "db/platform_schema.tera",
        &serde_json::json!({}),
        &ProjectConfig::default(),
    )
    .unwrap();
    assert!(
        !rendered_off.contains("event_consumption"),
        "{rendered_off}"
    );
}

/// #611: `dispatch_registered` reports PER-CONSUMER outcomes (not a bare
/// failure count) — the DLQ fate needs the failed consumer names and
/// error texts for the dead-letter metadata, and #613's counters label
/// by consumer. Per-consumer dispatch returns Result so the error text
/// survives.
#[tokio::test]
async fn consumers_report_per_consumer_outcomes() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let consumers = super::consumers_context_for(&arch, None);
    let rendered =
        render_template_with_project(&tera(), "events/consumers.tera", &consumers, &project())
            .unwrap();
    for needle in [
        "pub struct ConsumerOutcome {",
        "pub consumer: &'static str,",
        "pub ok: bool,",
        "pub error: Option<String>,",
        ") -> Vec<ConsumerOutcome> {",
        "ConsumerOutcome {",
        "-> Result<(), String> {",
    ] {
        assert!(
            rendered.contains(needle),
            "outcomes missing {needle}:\n{rendered}"
        );
    }
}

/// #611: the drain dead-letters a message whose consumers keep failing
/// once `read_ct` reaches the delivery policy's attempt budget — lazy
/// `pgmq.create` of `{queue}_dlq`, send with the `_dlq` failure metadata
/// attached, delete from the source queue. "Moved to DLQ" is terminal
/// for the delete condition, so a poison message cannot block its queue
/// (or — by the one-draining-worker placement invariant — its sibling
/// consumers) forever.
#[tokio::test]
async fn dispatch_dead_letters_permanently_failing_messages() {
    // The dispatch template renders through the webhook generator; pin
    // the emitted drain via a direct template render with the typed ctx.
    let ctx = serde_json::json!({ "evt_events": true });
    let rendered =
        render_template_with_project(&tera(), "webhook/dispatch.tera", &ctx, &project()).unwrap();
    for needle in [
        "SELECT msg_id, read_ct, message FROM pgmq.read(",
        "dead_letter(
                        &self.db,",
        "\"_dlq\"",
        "\"attempts\": read_ct",
        "format!(\"{queue}_dlq\")",
        "SELECT pgmq.create($1) WHERE NOT EXISTS",
        "(typed_ok || moved_to_dlq)",
    ] {
        assert!(
            rendered.contains(needle),
            "DLQ fate missing {needle}:\n{rendered}"
        );
    }
}

/// #613: the publish context captures W3C trace context, and the envelope
/// carries an additive `traceparent` key (null when the caller had none —
/// the `event_type` precedent: consumers ignore unknown keys). Both
/// envelope variants carry it.
#[tokio::test]
async fn emit_template_carries_the_traceparent_key() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let emit = super::emit_context(&arch);
    let rendered =
        render_template_with_project(&tera(), "events/emit.tera", &emit, &project()).unwrap();
    assert!(
        rendered.contains("pub traceparent: Option<String>,"),
        "EventContext captures trace context:\n{rendered}"
    );
    assert!(
        rendered.contains("\"traceparent\": ctx.traceparent.as_deref()"),
        "native envelope carries traceparent:\n{rendered}"
    );
    let mut ce = project();
    ce.codegen.event_envelope = "cloudevents".to_string();
    let rendered = render_template_with_project(&tera(), "events/emit.tera", &emit, &ce).unwrap();
    assert!(
        rendered.contains("\"traceparent\": ctx.traceparent.as_deref()"),
        "CE envelope carries traceparent as an extension attribute:\n{rendered}"
    );
}

/// #613: the drain instruments the delivery path — the five events
/// counters plus the three webhook outcome counters, exposed through the
/// existing `/metrics` Prometheus recorder (the `metrics` dep is
/// unconditional in the generated app). Labels are NAMES only (queue,
/// event, consumer, endpoint) — never payload fields. W3C trace context
/// propagates: the webhook POST rides the envelope's `traceparent` as an
/// unsigned transport header (beside the body-only HMAC signature).
#[tokio::test]
async fn dispatch_instruments_the_delivery_path() {
    let ctx = serde_json::json!({ "evt_events": true });
    let rendered =
        render_template_with_project(&tera(), "webhook/dispatch.tera", &ctx, &project()).unwrap();
    for needle in [
        "metrics::counter!(\"events_received\"",
        "metrics::counter!(\"events_processed\"",
        "metrics::counter!(\"events_retried\"",
        "metrics::counter!(\"events_failed\"",
        "metrics::counter!(\"events_dead_lettered\"",
        "metrics::counter!(\"webhook_delivered\"",
        "metrics::counter!(\"webhook_retry_scheduled\"",
        "metrics::counter!(\"webhook_endpoint_deactivated\"",
        // traceparent propagation (unsigned header beside the signature)
        ".header(\"traceparent\",",
    ] {
        assert!(
            rendered.contains(needle),
            "delivery metrics missing {needle}:\n{rendered}"
        );
    }
    // Redaction: the counters label by identity names only.
    for banned in ["\"payload\" =>", "payload.get(\"payload\") =>"] {
        assert!(
            !rendered.contains(banned),
            "counter labels must never carry payload data: {banned}"
        );
    }
}

// ── #614: per-event JSON Schema emission ────────────────────────────────

/// The rex-primitive → JSON Schema type mapping (2020-12).
#[test]
fn event_schema_primitive_type_mapping() {
    use crate::events::docs::json_type_of;
    assert_eq!(
        json_type_of("String"),
        serde_json::json!({"type": "string"})
    );
    assert_eq!(json_type_of("bool"), serde_json::json!({"type": "boolean"}));
    assert_eq!(json_type_of("i64"), serde_json::json!({"type": "integer"}));
    assert_eq!(json_type_of("f64"), serde_json::json!({"type": "number"}));
    assert_eq!(
        json_type_of("uuid::Uuid"),
        serde_json::json!({"type": "string", "format": "uuid"})
    );
    assert_eq!(
        json_type_of("chrono::NaiveDate"),
        serde_json::json!({"type": "string", "format": "date"})
    );
    assert_eq!(
        json_type_of("chrono::NaiveDateTime"),
        serde_json::json!({"type": "string", "format": "date-time"})
    );
    assert_eq!(
        json_type_of("rust_decimal::Decimal"),
        serde_json::json!({"type": "number"})
    );
    // Unmapped (serde_json::Value) → permissive, never silently strict.
    assert_eq!(json_type_of("serde_json::Value"), serde_json::json!({}));
}

/// One payload schema + one envelope wrapper schema per declared event,
/// emitted under `schemas/events/`. Properties ride declaration order
/// (the wire artifact's field order); resolved-but-undocumented schema
/// fields stay permissive (`{"type": "object"}`) — never silently
/// strict; the payload `$ref` targets resolve within the emitted set.
#[tokio::test]
async fn event_schemas_emit_per_declared_event() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let docs =
        crate::events::docs::document_artifacts(&arch, crate::events::EnvelopeVariant::Native);
    let mut paths: Vec<&str> = docs.iter().map(|d| d.rel_path.as_str()).collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "docs/asyncapi.json",
            "schemas/events/OrderCancelled.Envelope.json",
            "schemas/events/OrderCancelled.json",
            "schemas/events/OrderPlaced.Envelope.json",
            "schemas/events/OrderPlaced.json",
        ],
        "{paths:?}"
    );

    let placed = docs
        .iter()
        .find(|d| d.rel_path == "schemas/events/OrderPlaced.json")
        .unwrap();
    let doc: serde_json::Value = serde_json::from_str(&placed.content).unwrap();
    assert_eq!(
        doc["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(doc["type"], "object");
    assert_eq!(doc["properties"]["order_id"]["type"], "string");
    // Resolved schema-typed field: permissive object (no static DTO
    // schemas are published yet), with the resolution recorded.
    assert_eq!(doc["properties"]["status"]["type"], "object");
    assert!(
        placed.content.contains("\"order_id\"")
            && placed.content.find("\"order_id\"").unwrap()
                < placed.content.find("\"status\"").unwrap(),
        "declaration order preserved: {}",
        placed.content
    );

    let envelope = docs
        .iter()
        .find(|d| d.rel_path == "schemas/events/OrderPlaced.Envelope.json")
        .unwrap();
    let doc: serde_json::Value = serde_json::from_str(&envelope.content).unwrap();
    assert_eq!(doc["properties"]["payload"]["$ref"], "OrderPlaced.json");
    for key in [
        "event",
        "version",
        "channel",
        "payload",
        "event_type",
        "domain",
        "tenant_id",
        "occurred_at",
        "correlation_id",
        "traceparent",
    ] {
        assert!(
            doc["properties"].get(key).is_some(),
            "envelope schema missing {key}: {}",
            envelope.content
        );
    }
    assert!(
        doc["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("event"))
    );
}

/// Flag-off: an empty architecture emits zero document artifacts.
#[test]
fn event_schemas_emit_nothing_without_events() {
    let arch = crate::events::EventArchitecture::default();
    let docs =
        crate::events::docs::document_artifacts(&arch, crate::events::EnvelopeVariant::Native);
    assert!(docs.is_empty(), "{docs:?}");
}

/// Pipeline leg: the emitted tree carries the schema documents and every
/// payload `$ref` resolves to a sibling file.
#[tokio::test]
async fn event_schemas_flow_through_the_pipeline() {
    let db = db_with(Some(orders_evt_model())).await;
    let dir = TempDir::new().unwrap();
    let generator = EvtEventsGenerator::new(dir.path());
    let files = generator
        .generate(&db, &config(), &[], &tera(), &project())
        .await
        .expect("schemas generate");
    let names: Vec<String> = files
        .iter()
        .filter_map(|f| {
            f.path
                .strip_prefix(dir.path())
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    for expected in [
        "schemas/events/OrderPlaced.json",
        "schemas/events/OrderPlaced.Envelope.json",
    ] {
        assert!(names.iter().any(|n| n == expected), "{names:?}");
    }
    // Every payload $ref resolves within the emitted set.
    for f in files
        .iter()
        .filter(|f| f.path.extension().map(|e| e == "json").unwrap_or(false))
    {
        let doc: serde_json::Value = serde_json::from_str(&f.content).unwrap();
        if let Some(r) = doc["properties"]["payload"]["$ref"].as_str() {
            let sibling = f.path.parent().unwrap().join(r);
            assert!(
                files.iter().any(|f| f.path == sibling),
                "$ref {r} unresolved for {:?}",
                f.path
            );
        }
    }
}

// ── #610: AsyncAPI 3.0.0 document ───────────────────────────────────────

/// One AsyncAPI 3.0.0 document from the architecture: channels keyed by
/// their pgmq queue (the collapse rule falls out naturally), messages
/// referencing the #614 JSON Schemas externally (no duplication),
/// publish operations per channel, subscribe operations per
/// subscription with the envelope documented. Deterministic by
/// construction.
#[tokio::test]
async fn asyncapi_document_pins_the_graph_plane() {
    let db = db_with(Some(orders_evt_model())).await;
    let arch = super::architecture_for(&db, &project(), &config())
        .await
        .expect("architecture builds");
    let doc = crate::events::docs::asyncapi_document(&arch, crate::events::EnvelopeVariant::Native);
    assert_eq!(doc.rel_path, "docs/asyncapi.json");
    let v: serde_json::Value = serde_json::from_str(&doc.content).unwrap();
    assert_eq!(v["asyncapi"], "3.0.0");
    assert_eq!(v["info"]["title"], "Events");

    // Channels keyed by queue: the DSL channel queue + the per-domain
    // trigger queue (the fixture's channel name collides with the
    // domain — one entry under the collapsed queue name).
    assert!(
        v["channels"].get("events_orders").is_some(),
        "{}",
        doc.content
    );
    assert!(
        v["channels"]["events_orders"]["bindings"]["x-pgmq"]["queue"] == "events_orders",
        "{}",
        doc.content
    );
    // Messages: declared events on the channel, payload external refs.
    assert!(
        v["components"]["messages"]["OrderPlaced"]["payload"]["$ref"]
            == "../schemas/events/OrderPlaced.json",
        "{}",
        doc.content
    );
    // Operations: publish per channel, subscribe per subscription with
    // the consumer as the operationId.
    assert!(
        v["operations"].get("publishEventsOrders").is_some(),
        "{}",
        doc.content
    );
    assert!(
        v["operations"].get("subscribeAuditing").is_some(),
        "{}",
        doc.content
    );
    assert!(
        v["operations"]["subscribeAuditing"]["messages"][0]["$ref"]
            == "#/components/messages/OrderPlaced",
        "messages: {}",
        v["operations"]["subscribeAuditing"],
    );
    // The envelope is documented (native variant).
    assert!(
        doc.content.contains("x-envelope") && doc.content.contains("native"),
        "{}",
        doc.content
    );

    // Deterministic: two builds byte-equal.
    let again =
        crate::events::docs::asyncapi_document(&arch, crate::events::EnvelopeVariant::Native);
    assert_eq!(doc.content, again.content);

    // The CE variant documents the CloudEvents envelope.
    let ce =
        crate::events::docs::asyncapi_document(&arch, crate::events::EnvelopeVariant::CloudEvents);
    assert!(ce.content.contains("CloudEvents 1.0"), "{}", ce.content);
}

/// Pipeline leg: `docs/asyncapi.json` rides the .evt-only set.
#[tokio::test]
async fn asyncapi_document_flows_through_the_pipeline() {
    let db = db_with(Some(orders_evt_model())).await;
    let dir = TempDir::new().unwrap();
    let generator = EvtEventsGenerator::new(dir.path());
    let files = generator
        .generate(&db, &config(), &[], &tera(), &project())
        .await
        .expect("docs generate");
    assert!(
        files.iter().any(|f| f.path.ends_with("docs/asyncapi.json")),
        "{:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}

/// #615: the archive substrate migrates under `event_archive` — created
/// BEFORE the blanket grants (the drain's app_user pool writes it) — and
/// the drain archives each DELIVERED envelope at the delete site with a
/// retention sweep on every drain cycle. Flag-off: neither appears.
#[test]
fn platform_schema_carries_the_event_archive_table_under_the_flag() {
    let mut project = project();
    project.codegen.event_archive = true;
    let rendered = render_template_with_project(
        &tera(),
        "db/platform_schema.tera",
        &serde_json::json!({}),
        &project,
    )
    .unwrap();
    assert!(
        rendered.contains("CREATE TABLE IF NOT EXISTS platform.event_archive"),
        "{rendered}"
    );
    let table_pos = rendered.find("platform.event_archive").unwrap();
    let grant_pos = rendered
        .find("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA platform")
        .unwrap();
    assert!(
        table_pos < grant_pos,
        "archive table must precede the grant"
    );

    let rendered_off = render_template_with_project(
        &tera(),
        "db/platform_schema.tera",
        &serde_json::json!({}),
        &ProjectConfig::default(),
    )
    .unwrap();
    assert!(!rendered_off.contains("event_archive"), "{rendered_off}");
}

/// #615: the drain archives delivered envelopes (on-dispatch — only
/// delivered events are replayable) and sweeps past-retention rows every
/// cycle; replayed markers are the drain-agnostic side (the ops verb).
#[tokio::test]
async fn dispatch_archives_delivered_envelopes_under_the_flag() {
    let mut project = project();
    project.codegen.event_archive = true;
    let ctx = serde_json::json!({ "evt_events": true });
    let rendered =
        render_template_with_project(&tera(), "webhook/dispatch.tera", &ctx, &project).unwrap();
    for needle in [
        "INSERT INTO platform.event_archive",
        "ON CONFLICT (msg_id) DO NOTHING",
        "DELETE FROM platform.event_archive",
        "'30 days'",
    ] {
        assert!(
            rendered.contains(needle),
            "archive missing {needle}:\n{rendered}"
        );
    }
    let rendered_off = render_template_with_project(
        &tera(),
        "webhook/dispatch.tera",
        &ctx,
        &ProjectConfig::default(),
    )
    .unwrap();
    assert!(!rendered_off.contains("event_archive"), "{rendered_off}");
}

/// #615: the worker drain archives + sweeps on both dispatch-cycle legs
/// (native and wasm) under the flag.
#[tokio::test]
async fn worker_drain_archives_delivered_envelopes_under_the_flag() {
    let mut project = project();
    project.codegen.event_archive = true;
    let ctx = serde_json::json!({
        "has_events": true,
        "name": "sales",
        "queue_max_retries": 5,
        "extra_event_queues": [],
    });
    let rendered =
        render_template_with_project(&tera(), "webhook/dispatch_worker.tera", &ctx, &project)
            .unwrap();
    for needle in [
        "INSERT INTO platform.event_archive",
        "DELETE FROM platform.event_archive",
    ] {
        assert!(
            rendered.contains(needle),
            "worker archive missing {needle}:\n{rendered}"
        );
    }
    let rendered_off = render_template_with_project(
        &tera(),
        "webhook/dispatch_worker.tera",
        &ctx,
        &ProjectConfig::default(),
    )
    .unwrap();
    assert!(!rendered_off.contains("event_archive"), "{rendered_off}");
}
