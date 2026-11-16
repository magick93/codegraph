//! Storage garbage collection (issue #631 R7): `testkit storage --gc`.
//!
//! The generated media plane already cleans up on DELETE (the handler-site
//! block recomputing the overwrite-in-place keys); `--gc` is the ops-side
//! backstop for the orphan classes a code path can still produce (the app
//! killed between the byte write and the reference persist; historical
//! rows predating the hook). Self-discovering over SQL — no manifest
//! coupling: a media child table is any table carrying BOTH a `url` and a
//! `mime_type` column (the MediaReference shape); the referenced-key set is
//! the union of their `url` columns; orphans are `storage.objects` rows
//! outside that set.
//!
//! v1 semantics (recorded): the sweep deletes `storage.objects` catalog
//! rows over psql (the dlq/replay verb precedent). On hosted Supabase the
//! backing bytes are removed by the storage service's own reconciliation;
//! self-hosted deployments converge on their next storage restart. A
//! REST-based byte sweep (service-key DELETE per orphan) is the recorded
//! follow-up.

use crate::config::OpsConfig;
use crate::error::OpsResult;
use crate::output;

/// A bucket id is safe to interpolate into SQL only when it matches the
/// generated bucket naming alphabet (letters, digits, `-`, `_`, `.`).
fn safe_bucket(bucket: &str) -> bool {
    !bucket.is_empty()
        && bucket
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The media child-table discovery: tables carrying BOTH `url` and
/// `mime_type` columns (the MediaReference child-table shape — the model's
/// `url_field`/`mime_field` defaults or their configured spellings).
pub fn discover_media_tables_sql() -> String {
    r#"SELECT c.table_schema, c.table_name
FROM information_schema.columns c
WHERE c.column_name = 'url'
GROUP BY c.table_schema, c.table_name
HAVING bool_or(EXISTS (
    SELECT 1 FROM information_schema.columns c2
    WHERE c2.table_schema = c.table_schema
      AND c2.table_name = c.table_name
      AND c2.column_name = 'mime_type'
))"#
    .to_string()
}

/// The one-shot gc script. `collect = true` deletes the orphans; otherwise
/// it only reports the count. `bucket` restricts the sweep (all buckets
/// when absent).
pub fn gc_sql(bucket: Option<&str>, collect: bool) -> Result<String, String> {
    if let Some(b) = bucket
        && !safe_bucket(b)
    {
        return Err(format!(
            "bucket name {b:?} is not safe to interpolate (letters, digits, '-', '_', '.' only)"
        ));
    }
    let bucket_filter = bucket
        .map(|b| format!(" AND o.bucket_id = '{b}'"))
        .unwrap_or_default();
    let disposition = if collect {
        format!(
            "    DELETE FROM storage.objects o\n    WHERE TRUE{bucket_filter}\n      AND NOT EXISTS (SELECT 1 FROM _media_gc_refs refs WHERE refs.key = o.name);"
        )
    } else {
        "    RAISE NOTICE 'storage gc: % orphaned object(s) (report only — pass --gc to delete)', orphan_count;\n    RETURN;".to_string()
    };
    Ok(format!(
        r#"DO $media_gc$
DECLARE
    r RECORD;
    orphan_count INT := 0;
    deleted_count INT := 0;
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = 'storage') THEN
        RAISE WARNING 'storage gc: no storage schema (plain Postgres?) — nothing to collect';
        RETURN;
    END IF;
    CREATE TEMP TABLE IF NOT EXISTS _media_gc_refs (key TEXT);
    TRUNCATE _media_gc_refs;
    FOR r IN
        SELECT c.table_schema, c.table_name
        FROM information_schema.columns c
        WHERE c.column_name = 'url'
        GROUP BY c.table_schema, c.table_name
        HAVING bool_or(EXISTS (
            SELECT 1 FROM information_schema.columns c2
            WHERE c2.table_schema = c.table_schema
              AND c2.table_name = c.table_name
              AND c2.column_name = 'mime_type'
        ))
    LOOP
        EXECUTE format('INSERT INTO _media_gc_refs SELECT url FROM %I.%I WHERE url IS NOT NULL',
                       r.table_schema, r.table_name);
    END LOOP;
    SELECT count(*) INTO orphan_count FROM storage.objects o
    WHERE TRUE{bucket_filter}
      AND NOT EXISTS (SELECT 1 FROM _media_gc_refs refs WHERE refs.key = o.name);
    IF orphan_count = 0 THEN
        RAISE NOTICE 'storage gc: no orphaned objects';
        RETURN;
    END IF;
{disposition}
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RAISE NOTICE 'storage gc: deleted % orphaned object(s)', deleted_count;
    DROP TABLE IF EXISTS _media_gc_refs;
END
$media_gc$;"#,
        bucket_filter = bucket_filter,
        disposition = disposition,
    ))
}

/// `testkit storage [--gc] [--bucket B]`: report orphaned media objects
/// (default) or collect them (`--gc`).
pub async fn run(config: &OpsConfig, collect: bool, bucket: Option<&str>) -> OpsResult<()> {
    let target = &config.api_db;
    let sql = gc_sql(bucket, collect).map_err(crate::error::OpsError::Config)?;
    let out = crate::db::psql_query(target, &sql).await?;
    for line in out.lines().filter(|l| !l.trim().is_empty()) {
        output::info(line.trim());
    }
    if !collect {
        output::info("report only — pass --gc to delete the orphans");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gc_sql_discovers_media_child_tables_and_counts_orphans() {
        let sql = gc_sql(None, false).unwrap();
        assert!(
            sql.contains("c.column_name = 'url'") && sql.contains("c2.column_name = 'mime_type'"),
            "the MediaReference shape discovery: {sql}"
        );
        assert!(
            sql.contains("SELECT url FROM %I.%I WHERE url IS NOT NULL"),
            "the referenced-key union: {sql}"
        );
        assert!(
            sql.contains("WHERE TRUE\n      AND NOT EXISTS"),
            "no bucket filter when absent: {sql}"
        );
        assert!(
            sql.contains("report only — pass --gc to delete"),
            "default is a report: {sql}"
        );
        assert!(!sql.contains("DELETE FROM storage.objects"), "{sql}");
    }

    #[test]
    fn collect_deletes_and_the_bucket_filter_interpolates() {
        let sql = gc_sql(Some("person-media"), true).unwrap();
        assert!(sql.contains("DELETE FROM storage.objects o"), "{sql}");
        assert!(sql.contains("AND o.bucket_id = 'person-media'"), "{sql}");
        assert!(
            sql.contains("GET DIAGNOSTICS deleted_count = ROW_COUNT"),
            "{sql}"
        );
    }

    #[test]
    fn unsafe_bucket_names_are_rejected() {
        assert!(gc_sql(Some("person-media'; DROP TABLE x"), false).is_err());
        assert!(gc_sql(Some(""), false).is_err());
        assert!(gc_sql(Some("person_media.v2"), false).is_ok());
    }

    #[test]
    fn plain_postgres_fails_soft() {
        let sql = gc_sql(None, false).unwrap();
        assert!(
            sql.contains("RAISE WARNING 'storage gc: no storage schema"),
            "{sql}"
        );
    }
}
