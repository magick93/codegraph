//! Events-plane runtime gate (issues #611/#612/#615 — the DB-backed
//! lane): assembles the GENERATED events module + drain verbatim into a
//! scratch crate (sea_orm leg) and runs it against real Postgres with a
//! FUNCTIONAL pgmq stand-in (real queue tables with msg_id/read_ct/vt
//! semantics — CI's postgres:15 service has no pgmq extension; the
//! staged-migrations pattern is the authz_contract precedent):
//!
//! - DLQ fate: a poison consumer cannot starve its queue — after the
//!   attempt budget the message lands in `events_orders_dlq` with the
//!   `_dlq` failure metadata, the source queue drains, and the sibling
//!   consumer was delivered exactly once across the redelivery window
//!   (generated idempotency keying).
//! - Archive + replay: the delivered envelope lands in
//!   `platform.event_archive`; the harness replay verb re-publishes it
//!   with a fresh msg_id + `replayed_from` marker, and the consumer
//!   re-runs (fresh identity → no dedupe skip).
//!
//! Needs Postgres (`DATABASE_URL`, default
//! `postgres://postgres:postgres@localhost:5432/postgres`) + cargo.
//! Run: `cargo test -p codegraph --test events_runtime_tests -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::path::Path;

use codegraph_core::traits::GraphIngestor;
use codegraph_ops::db::psql_exec;
use codegraph_ops::pg::PgTarget;

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

fn entity_schema() -> codegraph_core::types::SchemaNode {
    codegraph_core::types::SchemaNode {
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

fn orders_evt_model() -> codegraph_core::types::EvtModelGraph {
    use codegraph_core::types::{EvtChannelNode, EvtEventField, EvtEventNode, EvtSubscriptionNode};
    let field = |name: &str| EvtEventField {
        name: name.to_string(),
        type_json: serde_json::json!({"type": "primitive", "value": "string"}),
        resolved_title: None,
    };
    codegraph_core::types::EvtModelGraph {
        source_path: "orders.evt".to_string(),
        events: vec![EvtEventNode {
            source_path: "orders.evt".to_string(),
            name: "OrderPlaced".to_string(),
            version: Some("1.0.0".to_string()),
            fields: vec![field("order_id")],
            ordinal: 0,
        }],
        channels: vec![EvtChannelNode {
            source_path: "orders.evt".to_string(),
            name: "orders".to_string(),
            publishes: vec!["OrderPlaced".to_string()],
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
                name: "fulfillment".to_string(),
                events: vec!["OrderPlaced".to_string()],
                consumer: "ship-worker".to_string(),
                ordinal: 1,
            },
        ],
    }
}

/// A FUNCTIONAL pgmq stand-in for plain Postgres (CI's postgres:15 has
/// no pgmq extension): real queue tables with pgmq's msg_id/read_ct/vt
/// contract, so the drain's read/delete/dead-letter SQL behaves as it
/// does against real pgmq.
const PGMQ_FUNCTIONAL_SHIM: &str = r#"
-- Functional pgmq stand-in (no extension available).
CREATE SCHEMA IF NOT EXISTS pgmq;

DO $roles$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_user') THEN
    CREATE ROLE app_user NOLOGIN;
  END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'api_key') THEN
    CREATE ROLE api_key NOLOGIN;
  END IF;
END $roles$;

CREATE OR REPLACE FUNCTION pgmq.create(p_name text) RETURNS void
AS $shim$ BEGIN
  EXECUTE format('CREATE TABLE IF NOT EXISTS pgmq.q_%I (msg_id BIGSERIAL PRIMARY KEY, read_ct INT DEFAULT 0, enqueued_at TIMESTAMPTZ DEFAULT now(), vt TIMESTAMPTZ NOT NULL DEFAULT now(), message JSONB)', p_name);
END $shim$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq.send(p_queue text, p_msg jsonb) RETURNS bigint
AS $shim$ DECLARE v_id bigint; BEGIN
  EXECUTE format('INSERT INTO pgmq.q_%I (message) VALUES ($1) RETURNING msg_id', p_queue) INTO v_id USING p_msg;
  RETURN v_id;
END $shim$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq.read(p_queue text, p_vt int, p_qty int)
RETURNS TABLE (msg_id bigint, read_ct int, message jsonb)
AS $shim$ BEGIN
  RETURN QUERY EXECUTE format(
    'UPDATE pgmq.q_%I SET vt = now() + make_interval(secs => $1), read_ct = read_ct + 1 WHERE msg_id IN (SELECT msg_id FROM pgmq.q_%I WHERE vt <= now() ORDER BY msg_id LIMIT $2) RETURNING msg_id, read_ct, message', p_queue, p_queue)
    USING p_vt, p_qty;
END $shim$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq.delete(p_queue text, p_msg bigint) RETURNS void
AS $shim$ BEGIN
  EXECUTE format('DELETE FROM pgmq.q_%I WHERE msg_id = $1', p_queue) USING p_msg;
END $shim$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq.list_queues() RETURNS TABLE (queue_name text)
AS $shim$ SELECT substring(tablename FROM 3) FROM pg_tables WHERE schemaname = 'pgmq' AND tablename LIKE 'q\_%'; $shim$ LANGUAGE sql;

CREATE OR REPLACE FUNCTION emit_domain_event() RETURNS TRIGGER
AS $shim$ BEGIN RETURN COALESCE(NEW, OLD); END $shim$ LANGUAGE plpgsql;

GRANT USAGE ON SCHEMA pgmq TO app_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA pgmq TO app_user;
"#;

fn scratch_cargo_toml() -> String {
    r#"
[package]
name = "events_runtime_harness"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
sea-orm = { version = "2", features = ["sqlx-postgres", "runtime-tokio-rustls"] }
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["rt"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = "0.4"
anyhow = "1"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tracing = "0.1"
tracing-subscriber = "0.3"
metrics = "0.24"
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
fastrand = "2"

[workspace]
"#
    .to_string()
}

fn harness_main() -> String {
    r#"
mod events;
mod webhook_dispatch;

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

use events::consumers::{
    AuditWorkerHandler, AuditWorkerContext, ShipWorkerHandler, ShipWorkerContext,
};
use events::emit::{publish_orders, EventContext, OrdersEvent};

/// The DLQ driver: the audit consumer FAILS while the harness poison
/// flag is up (a SQL-gated handler — the test controls the fate without
/// regenerating); after the replay it succeeds and records the delivery.
struct Poison {
    db: sea_orm::DatabaseConnection,
}
#[async_trait::async_trait]
impl AuditWorkerHandler for Poison {
    async fn on_order_placed(
        &self,
        _ctx: &AuditWorkerContext<'_>,
        _event: &events::contracts::OrderPlacedPayload,
    ) -> anyhow::Result<()> {
        // Consumers must NEVER panic through the drain (a panic kills
        // the dispatch task) — fail closed instead.
        let poison: bool = match self
            .db
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT poison FROM harness_control".to_string(),
            ))
            .await
        {
            Ok(Some(row)) => row.try_get_by_index(0).unwrap_or(true),
            _ => true,
        };
        if poison {
            anyhow::bail!("poison consumer fails while the flag is up (issue #611 driver)")
        }
        use sea_orm::ConnectionTrait;
        self.db
            .execute_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                "INSERT INTO scratch_log (id, consumer, payload) VALUES ('audit-replay', 'audit-worker', '{}')".to_string(),
            ))
            .await?;
        Ok(())
    }
}

/// The sibling consumer: ALWAYS succeeds and records the delivery — the
/// generated idempotency keying must hold it to exactly one run across
/// the poison-driven redelivery window.
struct Ship {
    db: sea_orm::DatabaseConnection,
}
#[async_trait::async_trait]
impl ShipWorkerHandler for Ship {
    async fn on_order_placed(
        &self,
        _ctx: &ShipWorkerContext<'_>,
        event: &events::contracts::OrderPlacedPayload,
    ) -> anyhow::Result<()> {
        let payload = serde_json::to_string(event).unwrap_or_default().replace('\'', "''");
        let id = format!("ship-{}", uuid::Uuid::new_v4());
        use sea_orm::ConnectionTrait;
        self.db
            .execute_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                format!("INSERT INTO scratch_log (id, consumer, payload) VALUES ('{id}', 'ship-worker', '{payload}')"),
            ))
            .await?;
        Ok(())
    }
}

async fn scalar(db: &sea_orm::DatabaseConnection, sql: &str) -> i64 {
    let row = db
        .query_one_raw(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            sql.to_string(),
        ))
        .await
        .expect("scalar query");
    match row {
        Some(r) => r.try_get_by_index::<i64>(0).expect("scalar i64"),
        None => 0,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let db_url = std::env::var("HARNESS_DATABASE_URL").expect("HARNESS_DATABASE_URL");
    let db = sea_orm::Database::connect(&db_url).await?;

    // Harness tables: the poison flag + the delivery log.
    db.execute_raw(Statement::from_string(
        DatabaseBackend::Postgres,
        "CREATE TABLE IF NOT EXISTS harness_control (poison BOOLEAN NOT NULL DEFAULT true)".to_string(),
    ))
    .await?;
    db.execute_raw(Statement::from_string(
        DatabaseBackend::Postgres,
        "INSERT INTO harness_control (poison) VALUES (true)".to_string(),
    ))
    .await?;
    db.execute_raw(Statement::from_string(
        DatabaseBackend::Postgres,
        "CREATE TABLE IF NOT EXISTS scratch_log (id TEXT PRIMARY KEY, consumer TEXT NOT NULL, payload JSONB NOT NULL, at TIMESTAMPTZ NOT NULL DEFAULT now())".to_string(),
    ))
    .await?;

    // Sibling consumers: ship-worker ALWAYS succeeds (the idempotency
    // probe); audit-worker poisons until the flag drops.
    events::consumers::register_audit_worker(std::sync::Arc::new(Poison { db: db.clone() }));
    events::consumers::register_ship_worker(std::sync::Arc::new(Ship { db: db.clone() }));

    // Publish one native envelope.
    let ctx = EventContext {
        tenant_id: uuid::Uuid::new_v4(),
        correlation_id: uuid::Uuid::new_v4(),
        traceparent: None,
    };
    publish_orders(
        &db,
        OrdersEvent::OrderPlaced(events::contracts::OrderPlacedPayload {
            order_id: "order-runtime-1".to_string(),
        }),
        &ctx,
    )
    .await?;

    // Drain for up to 90s. The poison message redelivers until the
    // attempt budget exhausts it into `events_orders_dlq`; the archive
    // row lands when the DLQ move deletes it; the harness then replays
    // (the ops-harness verb's own SQL) and the audit consumer re-runs.
    let dispatcher = webhook_dispatch::WebhookDispatcher::new(db.clone())
        .map_err(|e| anyhow::anyhow!("dispatcher init: {e}"))?;
    let shutdown = tokio_util::sync::CancellationToken::new();
    tokio::spawn({
        let shutdown = shutdown.clone();
        async move { dispatcher.run(shutdown).await }
    });

    let replay_sql = std::env::var("HARNESS_REPLAY_SQL").unwrap_or_default();
    let mut replayed = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(85);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        // Force the visibility timeout open between cycles: redelivery is
        // exactly the at-least-once condition under test (the drain's own
        // read sets vt 30s).
        use sea_orm::ConnectionTrait;
        db.execute_raw(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            "UPDATE pgmq.q_events_orders SET vt = now() - interval '1 second'",
        ))
        .await?;
        // The DLQ queue exists lazily (created on the first dead-letter)
        // — force its visibility timeout only once it does.
        db.execute_raw(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            "DO $$ BEGIN \
             IF to_regclass('pgmq.q_events_orders_dlq') IS NOT NULL THEN \
               UPDATE pgmq.q_events_orders_dlq SET vt = now() - interval '1 second'; \
             END IF; END $$;",
        ))
        .await?;

        let audit = scalar(&db, "SELECT count(*) FROM scratch_log WHERE consumer = 'audit-worker'").await;
        let ship = scalar(&db, "SELECT count(*) FROM scratch_log WHERE consumer = 'ship-worker'").await;
        let dlq = scalar(
            &db,
            "SELECT count(*) FROM pgmq.list_queues() WHERE queue_name = 'events_orders_dlq'",
        )
        .await;
        let archive = scalar(&db, "SELECT count(*) FROM platform.event_archive WHERE event = 'OrderPlaced'").await;
        let src = scalar(&db, "SELECT count(*) FROM pgmq.q_events_orders").await;
        let read_ct = scalar(
            &db,
            "SELECT COALESCE(read_ct, 0)::bigint FROM pgmq.q_events_orders LIMIT 1",
        )
        .await;
        let queues = scalar(&db, "SELECT count(*) FROM pgmq.list_queues()").await;
        eprintln!("HARNESS audit={audit} ship={ship} dlq={dlq} archive={archive} replayed={replayed} src={src} read_ct={read_ct} queues={queues}");

        if dlq >= 1 && archive >= 1 && !replayed && !replay_sql.is_empty() {
            // The poison message dead-lettered — drop the flag, then
            // replay: the re-published envelope's audit delivery must
            // now succeed (fresh msg_id → no dedupe skip), while the
            // ship-worker's replay delivery is a SECOND record.
            db.execute_raw(sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Postgres,
                "UPDATE harness_control SET poison = false".to_string(),
            ))
            .await?;
            replayed = true;
            db.execute_raw(sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Postgres,
                replay_sql.clone(),
            ))
            .await?;
            eprintln!("HARNESS replayed");
            continue;
        }

        if audit >= 1 && ship >= 2 && dlq >= 1 {
            eprintln!("HARNESS done");
            break;
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("runtime deadline exceeded: audit={audit} ship={ship} dlq={dlq} archive={archive} replayed={replayed}");
        }
    }
    shutdown.cancel();
    Ok(())
}
"#
    .to_string()
}

#[tokio::test]
#[ignore = "needs Postgres (DATABASE_URL) + cargo; the test-ops-integration lane"]
async fn poison_messages_dead_letter_and_delivered_events_replay() {
    // ── 0. Postgres (skip when unavailable) ──
    let base_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());
    let Ok(base) = PgTarget::from_url(&base_url) else {
        eprintln!("skipping: DATABASE_URL is not a postgres URL");
        return;
    };
    if codegraph_ops::db::psql_query(&base, "SELECT 1")
        .await
        .is_err()
    {
        eprintln!("skipping: postgres unreachable at {base_url}");
        return;
    }

    let db_name = format!(
        "evt_rt_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    psql_exec(&base, &format!("CREATE DATABASE {db_name};"))
        .await
        .expect("create db");
    let mut target = base.clone();
    target.db = db_name.clone();

    // Force-drop on any exit path.
    let result = run_scenarios(&target).await;
    match &result {
        Ok(()) => {
            let _ = psql_exec(
                &base,
                &format!("DROP DATABASE IF EXISTS {db_name} WITH (FORCE);"),
            )
            .await;
        }
        Err(e) => eprintln!("keeping {db_name} for inspection: {e}"),
    }
    result.expect("runtime scenarios");
}

async fn run_scenarios(target: &PgTarget) -> anyhow::Result<()> {
    // ── 1. Generate the plane (flags ON) ──
    let engine = codegraph_core::mock::MockEngine::builder()
        .with_schema(entity_schema())
        .build();
    engine
        .ingest_evt_model(&orders_evt_model())
        .await
        .expect("ingest evt model");
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
        event_idempotency: true,
        event_archive: true,
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
    project.codegen.event_idempotency = true;
    project.codegen.event_archive = true;
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir).unwrap();
    let out = tempfile::TempDir::new().unwrap();
    let report =
        codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
            db: &engine,
            config: &config(),
            output_dir: out.path(),
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
            ifml_frameworks: vec![],
            ifml_components: None,
            ux_rules: None,
            project_config: Some(&project),
            emdash_plugins: None,
            domain_config_dir: None,
        })
        .await
        .expect("generation");
    assert!(
        !report.has_errors(),
        "generation errors: {}",
        report.summary()
    );

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for f in &report.files {
        let rel = f
            .path
            .strip_prefix(out.path())
            .unwrap_or(&f.path)
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(rel, f.content.clone());
    }

    // ── 2. Migrations: the functional pgmq shim + the generated platform
    //    schema (carries event_consumption + event_archive under the flags) ──
    psql_exec(target, PGMQ_FUNCTIONAL_SHIM)
        .await
        .map_err(|e| anyhow::anyhow!("pgmq shim: {e}"))?;
    // The generated pgmq_setup migration creates the domain + channel
    // queues (the shim above supplies the functions it calls) and the
    // real plpgsql trigger envelope.
    let pgmq_setup = files
        .iter()
        .find(|(k, _)| k.ends_with("0003_pgmq_setup.sql"))
        .map(|(_, v)| v.clone())
        .expect("pgmq setup migration")
        // The shim supplies the pgmq functions; the extension line does
        // not apply on plain Postgres.
        .replace("CREATE EXTENSION IF NOT EXISTS pgmq;", "-- pgmq shimmed");
    psql_exec(target, &pgmq_setup)
        .await
        .map_err(|e| anyhow::anyhow!("pgmq setup: {e}"))?;
    // The platform RLS policies reference the BaseJump org helper — the
    // drain runs as the owner (RLS-exempt) so a minimal session-var
    // projection suffices for the harness.
    psql_exec(
        target,
        "CREATE OR REPLACE FUNCTION public.get_current_org_id() RETURNS uuid \
         AS $$ SELECT NULLIF(current_setting('app.organization_id', true), '')::uuid $$ \
         LANGUAGE sql STABLE",
    )
    .await
    .map_err(|e| anyhow::anyhow!("org helper: {e}"))?;
    let platform = files
        .iter()
        .find(|(k, _)| k.ends_with("0005_platform_schema.sql"))
        .map(|(_, v)| v.clone())
        .expect("platform schema migration");
    psql_exec(target, &platform)
        .await
        .map_err(|e| anyhow::anyhow!("platform schema: {e}"))?;
    psql_exec(
        target,
        "CREATE TABLE IF NOT EXISTS scratch_log (
            id TEXT PRIMARY KEY,
            consumer TEXT NOT NULL,
            payload JSONB NOT NULL,
            at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .await
    .map_err(|e| anyhow::anyhow!("scratch log: {e}"))?;

    // ── 3. Assemble the scratch crate from the generated files ──
    // Persisted under target/ so a failed harness compile stays
    // inspectable.
    let crate_dir = std::path::PathBuf::from(
        std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()),
    )
    .join("events-runtime-harness");
    let _ = std::fs::remove_dir_all(&crate_dir);
    std::fs::create_dir_all(&crate_dir).unwrap();
    let src = crate_dir.join("src");
    std::fs::create_dir_all(src.join("events")).unwrap();
    std::fs::write(crate_dir.as_path().join("Cargo.toml"), scratch_cargo_toml()).unwrap();
    std::fs::write(src.join("main.rs"), harness_main()).unwrap();
    for file in ["contracts.rs", "emit.rs", "consumers.rs", "mod.rs"] {
        std::fs::write(
            src.join("events").join(file),
            files
                .get(&format!("src/events/{file}"))
                .ok_or_else(|| anyhow::anyhow!("missing src/events/{file}"))?,
        )
        .unwrap();
    }
    std::fs::write(
        src.join("webhook_dispatch.rs"),
        files
            .get("src/webhook_dispatch.rs")
            .ok_or_else(|| anyhow::anyhow!("missing src/webhook_dispatch.rs"))?,
    )
    .unwrap();

    // The replay SQL the harness executes once the archive row lands
    // (the ops-harness verb's own builder — fresh msg_id + replayed_from
    // marker, correlation preserved).
    let replay_sql =
        codegraph_ops::replay::replay_sql(Some("OrderPlaced"), Some("orders"), None, None, 5)
            .map_err(|e| anyhow::anyhow!("replay sql: {e}"))?;

    // ── 4. Run the harness ──
    let output = tokio::process::Command::new("cargo")
        .args(["run", "--quiet"])
        .current_dir(crate_dir.as_path())
        .env("HARNESS_DATABASE_URL", target_url(target))
        .env("HARNESS_REPLAY_SQL", &replay_sql)
        .output()
        .await
        .expect("scratch cargo run");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        anyhow::bail!("harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
    }

    // ── 5. Assertions over the real database ──
    let scalar = |sql: String| {
        let target = target.clone();
        async move {
            codegraph_ops::db::psql_query(&target, &sql)
                .await
                .map(|v| v.trim().to_string())
        }
    };
    // Idempotency: the SUCCEEDING sibling ran EXACTLY twice across the
    // poison-driven redelivery window (once live, once replayed) — the
    // generated dedupe skipped the 5+ redeliveries.
    let ship = scalar("SELECT count(*) FROM scratch_log WHERE consumer = 'ship-worker'".into())
        .await
        .map_err(|e| anyhow::anyhow!("ship count: {e}"))?;
    assert_eq!(
        ship, "2",
        "ship-worker ran {ship} times (want 2: live + replay)"
    );
    // The poison consumer recovered after the flag dropped and handled
    // the replayed delivery.
    let audit = scalar("SELECT count(*) FROM scratch_log WHERE consumer = 'audit-worker'".into())
        .await
        .map_err(|e| anyhow::anyhow!("audit count: {e}"))?;
    assert_eq!(
        audit, "1",
        "audit consumer ran {audit} times (want 1: the replayed delivery)"
    );
    // DLQ: the poison message dead-lettered with the failure metadata.
    let dlq = codegraph_ops::db::psql_query(
        target,
        "SELECT message FROM pgmq.q_events_orders_dlq WHERE message->'_dlq' IS NOT NULL LIMIT 1",
    )
    .await
    .map_err(|e| anyhow::anyhow!("dlq row: {e}"))?;
    assert!(
        dlq.contains("poison consumer fails while the flag is up"),
        "DLQ metadata: {dlq}"
    );
    assert!(dlq.contains("OrderPlaced"), "DLQ envelope: {dlq}");
    assert!(dlq.contains("attempts"), "DLQ attempts: {dlq}");
    // Archive: BOTH delivered envelopes archived at the delete site (the
    // DLQ-moved original + the replayed message that was then delivered).
    let archived =
        scalar("SELECT count(*) FROM platform.event_archive WHERE event = 'OrderPlaced'".into())
            .await
            .map_err(|e| anyhow::anyhow!("archive count: {e}"))?;
    assert_eq!(archived, "2", "archive rows: {archived}");
    // Idempotency ledger: the ship-worker's two successful deliveries
    // (live + replay); the audit-worker's failed deliveries RELEASED
    // their rows (the ledger records successes, never attempts).
    let ledger = scalar(
        "SELECT count(*) FROM platform.event_consumption WHERE consumer = 'ship-worker'".into(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("ship ledger: {e}"))?;
    assert_eq!(ledger, "2", "ship ledger rows: {ledger}");
    let audit_ledger = scalar(
        "SELECT count(*) FROM platform.event_consumption WHERE consumer = 'audit-worker'".into(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("audit ledger: {e}"))?;
    assert_eq!(audit_ledger, "1", "audit ledger rows: {audit_ledger}");
    eprintln!(
        "runtime scenarios green: ship={ship} audit={audit} ledger={ledger} archive={archived}"
    );
    Ok(())
}

fn target_url(target: &PgTarget) -> String {
    format!(
        "postgres://{}:{}@{}:{}/{}",
        target.user, target.password, target.host, target.port, target.db
    )
}
