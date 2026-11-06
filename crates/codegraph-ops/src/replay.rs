//! Replay from the event archive (issue #615): `testkit replay`
//! re-publishes archived envelopes matching (event pattern, channel,
//! time range) back onto their source channels. v1 semantics: replay is
//! at-least-once and unordered relative to live traffic — ALL
//! subscribers of the channel re-receive (generated consumer idempotency
//! keying, #612, makes that safe for opted-in consumers). Replayed
//! messages get fresh pgmq msg_ids, preserve `correlation_id`, and gain
//! a `replayed_from` marker carrying the original msg_id.

use crate::config::OpsConfig;
use crate::error::{OpsError, OpsResult};
use crate::output;

/// The default replay batch (one drain cycle's batch).
pub const DEFAULT_REPLAY_LIMIT: u32 = 100;

/// A queue/archive channel name is safe to interpolate into SQL only
/// when it matches the generated naming alphabet.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A `--from`/`--to` timestamp must be ISO-8601-ish before it may ride a
/// SQL literal (postgres parses `YYYY-MM-DD[THH:MM[:SS]]`).
fn safe_timestamp(ts: &str) -> bool {
    !ts.is_empty()
        && ts
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == ':' || c == '.' || c == 'T' || c == 'Z')
}

/// Turn a user event glob (`order*`, `OrderPlaced`) into a SQL LIKE
/// pattern over `platform.event_archive.event` — `%`/`_` are the user's
/// wildcards; everything else is escaped by construction (the pattern is
/// quoted as a single SQL string, so only the two LIKE metacharacters
/// are semantic).
pub fn event_pattern(glob: &str) -> Result<String, String> {
    if glob.chars().any(|c| c == '\'' || c == ';' || c == '\\') {
        return Err(format!(
            "event pattern {glob:?} contains SQL-significant characters"
        ));
    }
    Ok(format!("%{glob}%").replace('*', "%"))
}

/// Build the replay script: read the matching archive rows and
/// re-publish each onto its source channel with the `replayed_from`
/// marker. A plpgsql loop so the SELECT that feeds the send is evaluated
/// once (the fresh pgmq msg_id is assigned by `pgmq.send`; the archive
/// row stays — replay does not consume history).
pub fn replay_sql(
    event_glob: Option<&str>,
    channel: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: u32,
) -> Result<String, String> {
    let mut filters: Vec<String> = Vec::new();
    if let Some(glob) = event_glob {
        let pattern = event_pattern(glob)?;
        filters.push(format!("event LIKE '{pattern}'"));
    }
    if let Some(channel) = channel {
        if !safe_name(channel) {
            return Err(format!("channel {channel:?} is not safe to interpolate"));
        }
        filters.push(format!("channel = 'events_{channel}'"));
    }
    if let Some(ts) = from {
        if !safe_timestamp(ts) {
            return Err(format!("from timestamp {ts:?} is not ISO-8601"));
        }
        filters.push(format!("dispatched_at >= '{ts}'::timestamptz"));
    }
    if let Some(ts) = to {
        if !safe_timestamp(ts) {
            return Err(format!("to timestamp {ts:?} is not ISO-8601"));
        }
        // `to` is an inclusive upper bound.
        filters.push(format!("dispatched_at <= '{ts}'::timestamptz"));
    }
    let where_clause = if filters.is_empty() {
        "TRUE".to_string()
    } else {
        filters.join(" AND ")
    };
    let limit = if limit == 0 {
        DEFAULT_REPLAY_LIMIT
    } else {
        limit
    };
    Ok(format!(
        r#"DO $replay$
DECLARE
    r RECORD;
    replayed INT := 0;
BEGIN
    FOR r IN SELECT msg_id, channel, envelope FROM platform.event_archive
             WHERE {where_clause}
             ORDER BY msg_id
             LIMIT {limit}
    LOOP
        PERFORM pgmq.send(r.channel, r.envelope || jsonb_build_object('replayed_from', r.msg_id));
        replayed := replayed + 1;
    END LOOP;
    RAISE NOTICE 'replayed % archived event(s)', replayed;
END
$replay$;"#
    ))
}

/// `testkit replay [--event GLOB] [--channel NAME] [--from TS] [--to TS]
/// [--limit N]`.
pub async fn run(
    config: &OpsConfig,
    event: Option<&str>,
    channel: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: u32,
) -> OpsResult<()> {
    let sql = replay_sql(event, channel, from, to, limit).map_err(OpsError::Config)?;
    let target = &config.api_db;
    crate::db::psql_query(target, &sql).await?;
    output::info(format!(
        "replayed up to {} archived event(s) onto their source channels",
        if limit == 0 {
            DEFAULT_REPLAY_LIMIT
        } else {
            limit
        }
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_sql_sends_with_the_replayed_from_marker() {
        let sql = replay_sql(Some("OrderPlaced"), Some("orders"), None, None, 25).unwrap();
        assert!(
            sql.contains(
                "pgmq.send(r.channel, r.envelope || jsonb_build_object('replayed_from', r.msg_id))"
            ),
            "{sql}"
        );
        assert!(sql.contains("event LIKE '%OrderPlaced%'"), "{sql}");
        assert!(sql.contains("channel = 'events_orders'"), "{sql}");
        assert!(sql.contains("LIMIT 25"), "{sql}");
        // Correlation is preserved: the envelope rides verbatim (plus the
        // marker) — no rewrite.
        assert!(!sql.contains("correlation_id"), "{sql}");
    }

    #[test]
    fn replay_sql_time_range_filters() {
        let sql = replay_sql(
            None,
            None,
            Some("2026-01-01T00:00:00Z"),
            Some("2026-01-02T00:00:00Z"),
            0,
        )
        .unwrap();
        assert!(
            sql.contains("dispatched_at >= '2026-01-01T00:00:00Z'::timestamptz"),
            "{sql}"
        );
        assert!(
            sql.contains("dispatched_at <= '2026-01-02T00:00:00Z'::timestamptz"),
            "{sql}"
        );
        assert!(sql.contains("LIMIT 100"), "{sql}");
    }

    #[test]
    fn replay_sql_rejects_unsafe_input() {
        assert!(replay_sql(Some("x'; DROP TABLE"), None, None, None, 10).is_err());
        assert!(replay_sql(None, Some("orders; --"), None, None, 10).is_err());
        assert!(replay_sql(None, None, Some("nope"), None, 10).is_err());
    }

    #[test]
    fn replay_sql_without_filters_replays_everything_up_to_the_limit() {
        let sql = replay_sql(None, None, None, None, 0).unwrap();
        assert!(sql.contains("WHERE TRUE"), "{sql}");
    }

    #[test]
    fn event_pattern_wildcards() {
        assert_eq!(event_pattern("order*").unwrap(), "%order%%");
        assert_eq!(event_pattern("OrderPlaced").unwrap(), "%OrderPlaced%");
        assert!(event_pattern("x'").is_err());
    }
}
