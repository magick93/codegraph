
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
