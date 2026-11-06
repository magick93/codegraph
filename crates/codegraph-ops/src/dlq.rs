//! Dead-letter queue surface (issue #611): inspection + requeue over the
//! generated `events_{channel}_dlq` pgmq queues. The DLQ rides the source
//! queue's owner (the one-draining-worker placement invariant), is
//! created lazily on the first dead-letter, and each corpse carries the
//! original envelope plus a `_dlq` failure-metadata object (consumers,
//! errors, attempts, failed_at). Requeue strips `_dlq` and re-publishes
//! to the source queue — at-least-once and unordered relative to live
//! traffic; the generated consumer idempotency keying (#612) makes the
//! re-delivery safe for consumers that opt in.

use crate::config::OpsConfig;
use crate::error::OpsResult;
use crate::output;
use std::time::Duration;

/// The default requeue batch size (one drain cycle's batch at the drain's
/// own `pgmq.read` batch).
pub const DEFAULT_REQUEUE_LIMIT: u32 = 100;

/// A queue name is safe to interpolate into `pgmq.q_{name}` only when it
/// matches the generated naming alphabet (letters, digits, underscore).
fn safe_queue_suffix(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Build the requeue script for one DLQ queue: read up to `limit`
/// messages (30s visibility timeout, the drain's own contract), strip the
/// `_dlq` metadata, re-publish to the source queue, delete from the DLQ.
/// A plpgsql loop so one read feeds both the send and the delete (no
/// double read, no double delivery).
pub fn requeue_sql(dlq_queue: &str, source_queue: &str, limit: u32) -> Result<String, String> {
    for name in [dlq_queue, source_queue] {
        if !safe_queue_suffix(name) {
            return Err(format!(
                "queue name {name:?} is not safe to interpolate (letters, digits, underscore only)"
            ));
        }
    }
    Ok(format!(
        r#"DO $dlq_requeue$
DECLARE
    r RECORD;
    moved INT := 0;
BEGIN
    FOR r IN SELECT msg_id, message FROM pgmq.read('{dlq_queue}', 30, {limit}) LOOP
        PERFORM pgmq.send('{source_queue}', r.message - '_dlq');
        PERFORM pgmq.delete('{dlq_queue}', r.msg_id);
        moved := moved + 1;
    END LOOP;
    RAISE NOTICE 'requeued % message(s) from {dlq_queue} to {source_queue}', moved;
END
$dlq_requeue$;"#
    ))
}

/// Build the depth probe for one DLQ queue (the name came from
/// `pgmq.list_queues()`, sanitized before interpolation).
pub fn depth_sql(dlq_queue: &str) -> Result<String, String> {
    if !safe_queue_suffix(dlq_queue) {
        return Err(format!(
            "queue name {dlq_queue:?} is not safe to interpolate (letters, digits, underscore only)"
        ));
    }
    Ok(format!(
        "SELECT '{dlq_queue}' AS dlq, count(*) FROM pgmq.q_{dlq_queue}"
    ))
}

/// List the DLQ queues (names only — interpolation-safe candidates for
/// the depth probe).
pub fn list_dlq_queues_sql() -> String {
    "SELECT queue_name FROM pgmq.list_queues() WHERE queue_name LIKE '%\\_dlq' ORDER BY queue_name"
        .to_string()
}

/// `testkit dlq`: list DLQ depth per queue (default) or requeue (with
/// `--requeue`) back to the source queues.
pub async fn run(
    config: &OpsConfig,
    requeue: bool,
    channel: Option<&str>,
    limit: u32,
) -> OpsResult<()> {
    let target = &config.api_db;
    let list = crate::db::psql_query(target, &list_dlq_queues_sql()).await?;
    let queues: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if queues.is_empty() {
        output::info("no dead-letter queues exist (nothing has dead-lettered)");
        return Ok(());
    }

    let matched: Vec<&str> = match channel {
        Some(c) => {
            let want = format!("events_{c}_dlq");
            queues.iter().copied().filter(|q| *q == want).collect()
        }
        None => queues.clone(),
    };
    if matched.is_empty() {
        output::warn(&format!(
            "no dead-letter queue matches channel {channel:?} (have: {queues:?})"
        ));
        return Ok(());
    }

    let limit = if limit == 0 {
        DEFAULT_REQUEUE_LIMIT
    } else {
        limit
    };
    for q in matched {
        let depth_sql = depth_sql(q).map_err(crate::error::OpsError::Config)?;
        match crate::db::psql_query(target, &depth_sql).await {
            Ok(depth) => output::info(&format!("{q}: {depth} message(s)")),
            Err(e) => output::warn(&format!("{q}: depth probe failed: {e}")),
        }
        if requeue {
            let source = q.strip_suffix("_dlq").unwrap_or(q);
            let sql =
                requeue_sql(q, source, limit).map_err(|e| crate::error::OpsError::Config(e))?;
            crate::db::psql_query(target, &sql).await?;
            output::info(&format!(
                "  requeued up to {limit} message(s) onto {source}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requeue_sql_targets_the_source_queue_and_strips_dlq_metadata() {
        let sql = requeue_sql("events_orders_dlq", "events_orders", 50).unwrap();
        assert!(
            sql.contains("pgmq.read('events_orders_dlq', 30, 50)"),
            "{sql}"
        );
        assert!(
            sql.contains("pgmq.send('events_orders', r.message - '_dlq')"),
            "{sql}"
        );
        assert!(
            sql.contains("pgmq.delete('events_orders_dlq', r.msg_id)"),
            "{sql}"
        );
        assert!(sql.contains("$dlq_requeue$"), "{sql}");
    }

    #[test]
    fn requeue_sql_rejects_unsafe_queue_names() {
        let err = requeue_sql("events_orders_dlq; DROP TABLE x", "events_orders", 10).unwrap_err();
        assert!(err.contains("not safe"), "{err}");
        let err = requeue_sql("events_orders_dlq", "events_orders'; --", 10).unwrap_err();
        assert!(err.contains("not safe"), "{err}");
    }

    #[test]
    fn depth_sql_interpolates_only_safe_names() {
        assert!(
            depth_sql("events_sales_dlq")
                .unwrap()
                .contains("pgmq.q_events_sales_dlq")
        );
        assert!(depth_sql("bad-name").is_err());
    }

    #[test]
    fn list_sql_matches_only_dlq_queues() {
        let sql = list_dlq_queues_sql();
        assert!(sql.contains("LIKE '%\\_dlq'"), "{sql}");
        assert!(sql.contains("ORDER BY queue_name"), "{sql}");
    }

    #[test]
    fn default_limit_is_the_drain_batch() {
        assert_eq!(DEFAULT_REQUEUE_LIMIT, 100);
        // Keep the timeout contract honest: the requeue read uses the
        // drain's own 30s visibility timeout.
        let sql = requeue_sql("events_a_dlq", "events_a", DEFAULT_REQUEUE_LIMIT).unwrap();
        assert!(sql.contains(", 30, 100)"), "{sql}");
        // The Duration import pins the drain-adjacent semantics.
        let _ = Duration::from_secs(30);
    }
}
