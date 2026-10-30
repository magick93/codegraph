//! Events plane under the workers topology (issue #505): the typed
//! consumers, publish surface, and drain wiring route into the owning
//! per-domain worker crates, the drain's delete condition extends to the
//! consumer registry, and flag-off (no `.evt`) worker output is
//! byte-identical. Node-free PR-CI pins over `run_generators_with_opts`.

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

/// The `orders.evt` shape: one channel (colliding with the domain name)
/// publishing two events, two subscriptions.
fn orders_evt_model() -> EvtModelGraph {
    EvtModelGraph {
        source_path: "orders.evt".to_string(),
        events: vec![
            EvtEventNode {
                source_path: "orders.evt".to_string(),
                name: "OrderPlaced".to_string(),
                version: Some("1.0.0".to_string()),
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
                fields: vec![field("reason", primitive_string_json(), None)],
                ordinal: 1,
            },
        ],
        channels: vec![EvtChannelNode {
            source_path: "orders.evt".to_string(),
            name: "orders".to_string(),
            publishes: vec!["OrderPlaced".to_string(), "OrderCancelled".to_string()],
            ordinal: 0,
        }],
        subscriptions: vec![EvtSubscriptionNode {
            source_path: "orders.evt".to_string(),
            name: "auditing".to_string(),
            events: vec!["OrderPlaced".to_string()],
            consumer: "audit-worker".to_string(),
            ordinal: 0,
        }],
    }
}

async fn db_with(model: Option<EvtModelGraph>) -> codegraph_core::mock::MockEngine {
    let db = codegraph_core::mock::MockEngine::builder()
        .with_schema(entity_schema())
        .with_schema(codelist_schema())
        .build();
    if let Some(model) = model {
        db.ingest_evt_model(&model).await.expect("ingest evt model");
    }
    db
}

fn workers_plan() -> codegraph::profile::BuildPlan {
    codegraph::profile::BuildPlan {
        entity_generators: vec![],
        domain_generators: vec![],
        global_generators: vec!["worker_scaffold".to_string()],
        post_gen_scripts: vec![],
        ifml_frameworks: vec![],
        template_pack_path: None,
        database_target: codegraph::generate::db::dialect::DatabaseTarget::Postgres,
        has_atproto: false,
        atproto_tenancy: "shared_pds".to_string(),
        has_fern: false,
        has_emdash: false,
        fern_sdk_languages: vec![],
        persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
        dto_key_casing: "snake".to_string(),
        deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        dependency_strategy: codegraph::profile::DependencyStrategy::Rev,
        namespace_layout: false,
        ux_rules: false,
        expr_ir: false,
        ifml_e2e_auth: false,
        ifml_remote_functions: false,
        testid_module: false,
        structural_gates: false,
        public_operations_rls: false,
        features: toml::Table::new(),
    }
}

fn workers_project() -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig {
        deployment: codegraph::generate::DeploymentConfig {
            deployment_topology: codegraph::profile::DeploymentTopology::Workers,
        },
        database: codegraph::generate::DatabaseConfig {
            persistence_provider: codegraph::profile::PersistenceProvider::Cornucopia,
            ..Default::default()
        },
        ..Default::default()
    }
}

async fn run_workers(db: &codegraph_core::mock::MockEngine) -> BTreeMap<String, String> {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir).unwrap();
    let output_dir = tempfile::TempDir::new().unwrap();
    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db,
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
            build_plan: Some(&workers_plan()),
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&workers_project()),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .expect("workers run generates");
    assert!(
        !report.has_errors(),
        "workers run should have no errors: {}",
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

// ── the typed events plane under workers ────────────────────────────────

#[tokio::test]
async fn events_plane_routes_into_the_owning_worker_crate() {
    let db = db_with(Some(orders_evt_model())).await;
    let files = run_workers(&db).await;

    // The events plane lives in the worker crate, not the output root.
    for file in ["mod.rs", "contracts.rs", "emit.rs", "consumers.rs"] {
        read(&files, &format!("workers/sales/src/events/{file}"));
    }
    assert!(
        !files.keys().any(|k| k.starts_with("src/events/")),
        "no output-root events plane under workers: {:?}",
        files
            .keys()
            .filter(|k| k.contains("events"))
            .collect::<Vec<_>>()
    );

    // The worker crate declares the module and carries the gated deps.
    let lib = read(&files, "workers/sales/src/lib.rs");
    assert!(lib.contains("pub mod events;"), "{lib}");
    let cargo = read(&files, "workers/sales/Cargo.toml");
    assert!(cargo.contains("anyhow = \"1\""), "{cargo}");

    // The cornucopia shapes: DbClient typing, idempotent registration,
    // per-dispatch client acquisition.
    let consumers = read(&files, "workers/sales/src/events/consumers.rs");
    assert!(!consumers.contains("DatabaseConnection"), "{consumers}");
    assert!(consumers.contains("use crate::db_client::{ClientSource, DbClient};"));
    assert!(
        consumers
            .contains("pub(crate) async fn dispatch_registered(\n    source: &dyn ClientSource,"),
        "{consumers}"
    );
    assert!(consumers.contains("pub trait AuditWorkerHandler"));
    let emit = read(&files, "workers/sales/src/events/emit.rs");
    assert!(emit.contains("client: &DbClient,"), "{emit}");
    assert!(emit.contains("SELECT pgmq.send($1, $2)"));

    // The drain: the scheduled handler exists (has_events adds the cron),
    // the dispatch module carries the typed delete condition, and the
    // wrangler gains the drain cron.
    let worker = read(&files, "workers/sales/src/worker.rs");
    assert!(worker.contains("#[event(scheduled)]"), "{worker}");
    assert!(
        worker.contains("crate::webhook_dispatch::dispatch_cycle(&state, \"sales\""),
        "{worker}"
    );
    let dispatch = read(&files, "workers/sales/src/webhook_dispatch.rs");
    assert!(
        dispatch.contains(
            "crate::events::consumers::dispatch_registered(&*state.client_source, message.clone())"
        ),
        "{dispatch}"
    );
    assert!(dispatch.contains(", typed_ok"), "{dispatch}");
    assert!(dispatch.contains("if ok && typed_ok {"), "{dispatch}");
    let wrangler = read(&files, "workers/sales/wrangler.toml");
    assert!(wrangler.contains("*/1 * * * *"), "{wrangler}");
    // The native loop drains too.
    let main = read(&files, "workers/sales/src/main.rs");
    assert!(main.contains("mod events;"), "{main}");
    assert!(main.contains("WebhookDispatcher::new"), "{main}");
}

// ── flag-off: no .evt ⇒ byte-identical workers output ───────────────────

#[tokio::test]
async fn no_evt_keeps_worker_output_free_of_the_events_plane() {
    let db_on = db_with(Some(orders_evt_model())).await;
    let db_off = db_with(None).await;
    let files_on = run_workers(&db_on).await;
    let files_off = run_workers(&db_off).await;

    assert!(
        !files_off.keys().any(|k| k.contains("events")),
        "no events surface without .evt: {:?}",
        files_off
            .keys()
            .filter(|k| k.contains("event"))
            .collect::<Vec<_>>()
    );
    let lib_off = read(&files_off, "workers/sales/src/lib.rs");
    assert!(!lib_off.contains("pub mod events;"), "{lib_off}");
    // No webhooks and no events ⇒ no dispatch module at all (the
    // has_webhooks || has_events gate).
    assert!(
        !files_off.keys().any(|k| k.ends_with("webhook_dispatch.rs")),
        "no dispatch module without webhooks/events"
    );

    // The common worker files are byte-identical across the runs — the
    // events plane adds files and gates exactly these worker surfaces
    // (the module declaration, the native entry, the gated dep, the
    // scheduled handler, the cron, and the module index).
    const GATED: [&str; 6] = [
        "workers/sales/Cargo.toml",
        "workers/sales/src/lib.rs",
        "workers/sales/src/main.rs",
        "workers/sales/src/mod.rs",
        "workers/sales/src/worker.rs",
        "workers/sales/wrangler.toml",
    ];
    let mut differing = Vec::new();
    for (name, content_off) in &files_off {
        if GATED.contains(&name.as_str()) {
            continue;
        }
        match files_on.get(name) {
            Some(content_on) if content_on == content_off => {}
            _ => differing.push(name.clone()),
        }
    }
    assert!(
        differing.is_empty(),
        "flag-off files must be byte-identical; these differ: {differing:?}"
    );

    // And the gated worker surfaces differ by exactly the events gating.
    assert!(read(&files_on, "workers/sales/src/lib.rs").contains("pub mod events;"));
    assert!(!lib_off.contains("pub mod events;"));
}
