//! Database dialect abstraction for SQL generation.
//!
//! Defines the [`SqlDialect`] trait and implementations for PostgreSQL and SQLite.
//! All DB generators use a dialect instance to produce target-specific SQL.
//!
//! # Adding a new dialect
//!
//! 1. Add a variant to [`DatabaseTarget`]
//! 2. Implement [`SqlDialect`] for the new target
//! 3. Add Tera templates under `templates/db/<target>/`
//! 4. Register the dialect in [`dialect_for_target`]

use std::fmt;

use codegraph_naming::PG_RESERVED;

/// Supported database targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseTarget {
    /// PostgreSQL (default, current behavior)
    Postgres,
    /// SQLite (for environments like Cloudflare D1, Trailbase)
    Sqlite,
}

impl DatabaseTarget {
    /// Parse from a config string. Unknown values default to Postgres.
    pub fn from_config(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "sqlite" => DatabaseTarget::Sqlite,
            _ => DatabaseTarget::Postgres,
        }
    }

    /// Return the template subdirectory name for this dialect.
    pub fn template_dir(&self) -> &'static str {
        match self {
            DatabaseTarget::Postgres => "postgres",
            DatabaseTarget::Sqlite => "sqlite",
        }
    }

    /// All known variants.
    pub fn all() -> &'static [DatabaseTarget] {
        &[DatabaseTarget::Postgres, DatabaseTarget::Sqlite]
    }
}

impl fmt::Display for DatabaseTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseTarget::Postgres => write!(f, "postgres"),
            DatabaseTarget::Sqlite => write!(f, "sqlite"),
        }
    }
}

impl Default for DatabaseTarget {
    fn default() -> Self {
        DatabaseTarget::Postgres
    }
}

// ── SqlDialect trait ─────────────────────────────────────────────────────────

/// A SQL dialect provides type mappings, syntax helpers, and feature flags
/// for a specific database target.
///
/// Every method has a sensible default for PostgreSQL (the primary target).
/// New dialects override only what differs.
pub trait SqlDialect: fmt::Debug + Send + Sync {
    /// Unique name for this dialect (matches [`DatabaseTarget`] string).
    fn name(&self) -> &'static str;

    // ── Type mapping ─────────────────────────────────────────────────────

    /// The SQL type for UUID primary keys.
    fn uuid_type(&self) -> &'static str { "UUID" }

    /// The SQL type for timestamps with time zone.
    fn timestamp_tz_type(&self) -> &'static str { "TIMESTAMPTZ" }

    /// The SQL type for JSON data.
    fn json_type(&self) -> &'static str { "JSONB" }

    /// The SQL type for TEXT.
    fn text_type(&self) -> &'static str { "TEXT" }

    /// The SQL type for BOOLEAN.
    fn boolean_type(&self) -> &'static str { "BOOLEAN" }

    /// The SQL type for INTEGER.
    fn integer_type(&self) -> &'static str { "INTEGER" }

    /// The SQL type for geometry/spatial data.
    fn geometry_type(&self) -> &'static str { "GEOMETRY(Point, 4326)" }

    /// The SQL type for vector/embeddings.
    fn vector_type(&self, dimensions: u32) -> String { format!("vector({})", dimensions) }

    /// Whether arrays are supported natively (PostgreSQL `TEXT[]`).
    fn supports_array_types(&self) -> bool { true }

    /// The array suffix for a type (e.g. `[]` in PostgreSQL).
    fn array_suffix(&self) -> &'static str { "[]" }

    // ── Default expressions ──────────────────────────────────────────────

    /// Default expression for UUID primary keys.
    fn uuid_default(&self) -> &'static str { "gen_random_uuid()" }

    /// Default expression for timestamp columns.
    fn now_default(&self) -> &'static str { "now()" }

    /// Default expression for the null UUID sentinel.
    fn null_uuid_default(&self) -> String {
        "'00000000-0000-0000-0000-000000000000'::UUID".to_string()
    }

    // ── Feature flags ────────────────────────────────────────────────────

    /// Whether the dialect supports database schemas (PostgreSQL schemas).
    fn has_schemas(&self) -> bool { true }

    /// Whether the dialect supports Row-Level Security.
    fn has_rls(&self) -> bool { true }

    /// Whether the dialect supports extensions (`CREATE EXTENSION IF NOT EXISTS`).
    fn has_extensions(&self) -> bool { true }

    /// Whether the dialect supports PL/pgSQL functions.
    fn has_plpgsql(&self) -> bool { true }

    /// Whether the dialect supports `COMMENT ON` statements.
    fn has_comments(&self) -> bool { true }

    /// Whether the dialect supports `DO $$ ... $$` anonymous blocks.
    fn has_do_blocks(&self) -> bool { true }

    /// Whether the dialect supports `CREATE TYPE ... AS ENUM`.
    fn has_enums(&self) -> bool { true }

    /// Whether the dialect supports partial indexes.
    fn has_partial_indexes(&self) -> bool { true }

    /// Whether the dialect supports array columns (TEXT[], INT[]).
    fn has_array_columns(&self) -> bool { true }

    /// Whether the dialect supports full-text search via tsvector/FTS5.
    fn has_fulltext_search(&self) -> bool { true }

    /// Whether the dialect supports pgvector-style embeddings.
    fn has_embeddings(&self) -> bool { true }

    // ── Identifier handling ──────────────────────────────────────────────

    /// Maximum identifier length in bytes.
    fn max_identifier_length(&self) -> usize { 63 }

    /// Quote an identifier for use in SQL.
    fn quote_identifier(&self, name: &str) -> String {
        format!("\"{}\"", name)
    }

    /// Whether identifiers need quoting.
    fn needs_quoting(&self, name: &str) -> bool;

    /// Generate a constraint/index name, truncated to the dialect's max length.
    fn truncate_identifier(&self, name: &str) -> String {
        if name.len() <= self.max_identifier_length() {
            name.to_string()
        } else {
            let hash = fnv1a_64(name.as_bytes());
            let suffix = format!("_{:07x}", hash & 0x0FFF_FFFF);
            let prefix_len = self.max_identifier_length() - suffix.len();
            let prefix = name[..prefix_len].trim_end_matches('_');
            format!("{}{}", prefix, suffix)
        }
    }

    // ── Trigger syntax ───────────────────────────────────────────────────

    /// Whether triggers are defined inline (SQLite) or via a function (PG).
    fn trigger_uses_function(&self) -> bool { true }

    /// Template type for `updated_at` triggers: "plpgsql" or "inline".
    fn trigger_template_style(&self) -> &'static str { "plpgsql" }

    /// Template type for domain event triggers: "pgmq" or "simple_event_table".
    fn event_trigger_style(&self) -> &'static str { "pgmq" }

    // ── Full-text search ─────────────────────────────────────────────────

    /// FTS engine: "tsvector" or "fts5".
    fn fts_engine(&self) -> &'static str { "tsvector" }

    // ── Dialect-specific type coercion ───────────────────────────────────

    /// Convert a PostgreSQL-style type name to the target dialect.
    /// Returns `None` if the type is already dialect-agnostic.
    fn map_pg_type(&self, pg_type: &str) -> Option<String> {
        let upper = pg_type.to_uppercase();
        match upper.as_str() {
            "UUID" => Some(self.uuid_type().to_string()),
            "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => {
                Some(self.timestamp_tz_type().to_string())
            }
            "JSONB" => Some(self.json_type().to_string()),
            "BOOLEAN" => Some(self.boolean_type().to_string()),
            "INTEGER" => Some(self.integer_type().to_string()),
            // Arrays — handled via array_suffix separately
            _ if upper.starts_with("GEOMETRY") || upper.starts_with("GEOGRAPHY") => {
                // Geometry types: keep as-is, SQLite stores as BLOB
                Some(pg_type.to_string())
            }
            _ if upper.starts_with("VECTOR") => {
                // Vector types: keep as-is, SQLite stores as BLOB
                Some(pg_type.to_string())
            }
            _ => None,
        }
    }

    /// Wrap a default expression for a specific column type.
    fn wrap_default(&self, default: &str, _pg_type: &str) -> String {
        default.to_string()
    }
}

// ── PostgreSQL dialect (default) ──────────────────────────────────────────────

/// PostgreSQL dialect — default, current behavior.
#[derive(Debug)]
pub struct PostgresDialect;

impl PostgresDialect {
    pub fn new() -> Self {
        Self
    }
}

impl SqlDialect for PostgresDialect {
    fn name(&self) -> &'static str {
        "postgres"
    }

    fn needs_quoting(&self, name: &str) -> bool {
        PG_RESERVED.contains(&name.to_ascii_lowercase().as_str())
    }

    fn quote_identifier(&self, name: &str) -> String {
        if self.needs_quoting(name) {
            format!("\"{}\"", name)
        } else {
            name.to_string()
        }
    }
}

impl Default for PostgresDialect {
    fn default() -> Self {
        Self
    }
}

// ── SQLite dialect ───────────────────────────────────────────────────────────

/// SQLite dialect.
#[derive(Debug)]
pub struct SqliteDialect;

impl SqliteDialect {
    pub fn new() -> Self {
        Self
    }
}

impl SqlDialect for SqliteDialect {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    // ── Type mapping ─────────────────────────────────────────────────────

    fn uuid_type(&self) -> &'static str { "TEXT" }

    fn timestamp_tz_type(&self) -> &'static str { "TEXT" }

    fn json_type(&self) -> &'static str { "TEXT" }

    fn boolean_type(&self) -> &'static str { "INTEGER" }

    fn vector_type(&self, _dimensions: u32) -> String { "BLOB".to_string() }

    fn supports_array_types(&self) -> bool { false }

    fn array_suffix(&self) -> &'static str { "" }

    // ── Default expressions ──────────────────────────────────────────────

    fn uuid_default(&self) -> &'static str { "" }

    fn now_default(&self) -> &'static str { "(unixepoch())" }

    fn null_uuid_default(&self) -> String {
        "'00000000-0000-0000-0000-000000000000'".to_string()
    }

    // ── Feature flags ────────────────────────────────────────────────────

    fn has_schemas(&self) -> bool { false }

    fn has_rls(&self) -> bool { false }

    fn has_extensions(&self) -> bool { false }

    fn has_plpgsql(&self) -> bool { false }

    fn has_comments(&self) -> bool { false }

    fn has_do_blocks(&self) -> bool { false }

    fn has_enums(&self) -> bool { false }

    fn has_partial_indexes(&self) -> bool { true }

    fn has_array_columns(&self) -> bool { false }

    fn has_fulltext_search(&self) -> bool { true }

    fn has_embeddings(&self) -> bool { false }

    // ── Identifier handling ──────────────────────────────────────────────

    fn max_identifier_length(&self) -> usize { 128 }

    fn needs_quoting(&self, _name: &str) -> bool { false }

    fn quote_identifier(&self, name: &str) -> String { name.to_string() }

    // ── Trigger syntax ───────────────────────────────────────────────────

    fn trigger_uses_function(&self) -> bool { false }

    fn trigger_template_style(&self) -> &'static str { "inline" }

    fn event_trigger_style(&self) -> &'static str { "simple_event_table" }

    // ── Full-text search ─────────────────────────────────────────────────

    fn fts_engine(&self) -> &'static str { "fts5" }

    // ── Type mapping overrides ───────────────────────────────────────────

    fn map_pg_type(&self, pg_type: &str) -> Option<String> {
        let upper = pg_type.to_uppercase();
        // SQLite strict mode: only INTEGER, TEXT, BLOB, REAL
        match upper.as_str() {
            "UUID" | "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE"
            | "JSONB" | "TEXT" | "VARCHAR" => Some("TEXT".to_string()),
            "BOOLEAN" | "INTEGER" | "INT4" | "INT8" | "SMALLINT"
            | "BIGINT" | "SERIAL" | "BIGSERIAL" => Some("INTEGER".to_string()),
            "FLOAT" | "FLOAT8" | "DOUBLE PRECISION" | "REAL" | "NUMERIC"
            | "DECIMAL" => Some("REAL".to_string()),
            _ if upper.starts_with("GEOMETRY")
                || upper.starts_with("GEOGRAPHY")
                || upper.starts_with("VECTOR") => Some("BLOB".to_string()),
            _ => None,
        }
    }

    fn wrap_default(&self, default: &str, _pg_type: &str) -> String {
        // Remove PostgreSQL ::type casts for SQLite
        let cleaned = default.split("::").next().unwrap();
        // Replace gen_random_uuid() with empty (client-generated)
        if cleaned.trim() == "gen_random_uuid()" {
            String::new()
        } else {
            cleaned.to_string()
        }
    }

    fn truncate_identifier(&self, name: &str) -> String {
        // SQLite has a much higher limit, but still truncate to keep things readable
        if name.len() <= self.max_identifier_length() {
            name.to_string()
        } else {
            let hash = fnv1a_64(name.as_bytes());
            let suffix = format!("_{:07x}", hash & 0x0FFF_FFFF);
            let prefix_len = self.max_identifier_length() - suffix.len();
            let prefix = name[..prefix_len].trim_end_matches('_');
            format!("{}{}", prefix, suffix)
        }
    }
}

impl Default for SqliteDialect {
    fn default() -> Self {
        Self
    }
}

// ── Factory ──────────────────────────────────────────────────────────────────

/// Create a dialect instance for the given target.
pub fn dialect_for_target(target: DatabaseTarget) -> Box<dyn SqlDialect> {
    match target {
        DatabaseTarget::Postgres => Box::new(PostgresDialect::new()),
        DatabaseTarget::Sqlite => Box::new(SqliteDialect::new()),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// FNV-1a 64-bit hash for deterministic identifier truncation.
fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_dialect_defaults() {
        let d = PostgresDialect::new();
        assert_eq!(d.name(), "postgres");
        assert_eq!(d.uuid_type(), "UUID");
        assert!(d.has_schemas());
        assert!(d.has_rls());
        assert!(d.has_extensions());
        assert!(d.has_array_columns());
        assert_eq!(d.trigger_template_style(), "plpgsql");
        assert_eq!(d.fts_engine(), "tsvector");
        assert_eq!(d.max_identifier_length(), 63);
    }

    #[test]
    fn test_sqlite_dialect_defaults() {
        let d = SqliteDialect::new();
        assert_eq!(d.name(), "sqlite");
        assert_eq!(d.uuid_type(), "TEXT");
        assert_eq!(d.timestamp_tz_type(), "TEXT");
        assert_eq!(d.json_type(), "TEXT");
        assert_eq!(d.boolean_type(), "INTEGER");
        assert!(!d.has_schemas());
        assert!(!d.has_rls());
        assert!(!d.has_extensions());
        assert!(!d.has_array_columns());
        assert_eq!(d.trigger_template_style(), "inline");
        assert_eq!(d.fts_engine(), "fts5");
        assert_eq!(d.max_identifier_length(), 128);
    }

    #[test]
    fn test_postgres_quote_identifier_reserved() {
        let d = PostgresDialect::new();
        assert_eq!(d.quote_identifier("order"), "\"order\"");
        assert_eq!(d.quote_identifier("select"), "\"select\"");
        assert_eq!(d.quote_identifier("name"), "name");
    }

    #[test]
    fn test_sqlite_quote_identifier_never() {
        let d = SqliteDialect::new();
        assert_eq!(d.quote_identifier("order"), "order");
        assert_eq!(d.quote_identifier("select"), "select");
    }

    #[test]
    fn test_map_pg_type_postgres_identity() {
        let d = PostgresDialect::new();
        assert_eq!(d.map_pg_type("UUID"), Some("UUID".to_string()));
        assert_eq!(d.map_pg_type("TEXT"), None); // already agnostic
    }

    #[test]
    fn test_map_pg_type_sqlite() {
        let d = SqliteDialect::new();
        assert_eq!(d.map_pg_type("UUID"), Some("TEXT".to_string()));
        assert_eq!(d.map_pg_type("TIMESTAMPTZ"), Some("TEXT".to_string()));
        assert_eq!(d.map_pg_type("JSONB"), Some("TEXT".to_string()));
        assert_eq!(d.map_pg_type("BOOLEAN"), Some("INTEGER".to_string()));
        assert_eq!(d.map_pg_type("GEOMETRY(Point, 4326)"), Some("BLOB".to_string()));
        assert_eq!(d.map_pg_type("VECTOR(1536)"), Some("BLOB".to_string()));
    }

    #[test]
    fn test_sqlite_wrap_default_removes_cast() {
        let d = SqliteDialect::new();
        assert_eq!(
            d.wrap_default("'00000000-0000-0000-0000-000000000000'::UUID", "UUID"),
            "'00000000-0000-0000-0000-000000000000'"
        );
    }

    #[test]
    fn test_sqlite_wrap_default_gen_random_uuid() {
        let d = SqliteDialect::new();
        assert_eq!(d.wrap_default("gen_random_uuid()", "UUID"), "");
    }

    #[test]
    fn test_truncate_identifier_within_limit() {
        let d = PostgresDialect::new();
        assert_eq!(d.truncate_identifier("short_name"), "short_name");
    }

    #[test]
    fn test_database_target_from_config() {
        assert_eq!(DatabaseTarget::from_config("postgres"), DatabaseTarget::Postgres);
        assert_eq!(DatabaseTarget::from_config("sqlite"), DatabaseTarget::Sqlite);
        assert_eq!(DatabaseTarget::from_config("unknown"), DatabaseTarget::Postgres);
        assert_eq!(DatabaseTarget::from_config(""), DatabaseTarget::Postgres);
    }

    #[test]
    fn test_database_target_default() {
        assert_eq!(DatabaseTarget::default(), DatabaseTarget::Postgres);
    }

    #[test]
    fn test_dialect_for_target() {
        let pg = dialect_for_target(DatabaseTarget::Postgres);
        assert_eq!(pg.name(), "postgres");

        let sqlite = dialect_for_target(DatabaseTarget::Sqlite);
        assert_eq!(sqlite.name(), "sqlite");
    }
}
