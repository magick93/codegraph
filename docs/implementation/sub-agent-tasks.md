# Sub-Agent Task Descriptions: SQLite Support

This document describes each phase in enough detail for an autonomous sub-agent
to execute. Each phase is scoped to a specific area with clear inputs, outputs,
and acceptance criteria.

---

## Phase 2: SQLite DDL Templates (all remaining)

**Goal**: Create all remaining SQLite template equivalents under
`crates/codegraph/templates/db/sqlite/`.

**Context**: Templates use Tera syntax. The context variables passed to each
template are identical to the PostgreSQL version — the template itself is
responsible for rendering dialect-specific SQL.

**Template mapping** (create each):

| SQLite template | Based on PG template | Key differences |
|---|---|---|
| `platform_schema.tera` | `db/platform_schema.tera` | No `CREATE SCHEMA`, no `JSONB`, no `TEXT[]`, no `gen_random_uuid()`, no RLS, no `::type` casts, no partial indexes with `IS NOT NULL`, no `COMMENT ON`. Use `TEXT` for JSON, `TEXT` for UUID, `INTEGER` for timestamps (`unixepoch()`), `INTEGER` for booleans. |
| `rbac_roles.tera` | `db/rbac_roles.tera` | Empty or no-op (no PG enum types in SQLite). |
| `api_key_migration.tera` | `db/api_key_migration.tera` | No `pgcrypto` functions. Use `TEXT` for hashed keys and a Rust-level hashing library. No `CREATE ROLE`, no `SECURITY DEFINER`, no `GRANT`. |
| `seed.tera` | `db/seed.tera` | No `auth.users` (Supabase) references, no `gen_random_uuid()`, no `crypt()`, no `gen_salt()`. Use `INSERT OR IGNORE` for upserts. Omit `::type` casts. |
| `workflow_seed.tera` | `db/workflow_seed.tera` | Use `INSERT OR IGNORE` instead of `ON CONFLICT ... DO UPDATE`. No `ARRAY[...]::TEXT[]` — use JSON text instead. No `::JSONB` casts. |
| `report_view.tera` | `db/report_view.tera` | No `information_schema` queries. Simplified view definitions. |
| `deferred_fks.tera` | `db/deferred_fks.tera` | Remove `DO $$ ... $$` wrapper. Use plain `ALTER TABLE ADD FOREIGN KEY`. |
| `pgmq_setup.tera` | `db/pgmq_setup.tera` | Replace pgmq extension with a simple `domain_event` table: `CREATE TABLE IF NOT EXISTS domain_event (id INTEGER PRIMARY KEY AUTOINCREMENT, table_name TEXT NOT NULL, record_id TEXT NOT NULL, event_type TEXT NOT NULL, payload TEXT, created_at INTEGER NOT NULL DEFAULT (unixepoch())) STRICT;` |
| `embedding.tera` | `db/embedding.tera` | Not generated for SQLite (no pgvector support). The DDL generator should skip this. |
| `process_history_view.tera` | `db/process_history_view.tera` | SQLite-compatible view syntax (no `jsonb_build_object()` — use `json_object()`). |

**Tera context variables available** (same as PostgreSQL templates):
- `{{ project }}` — `ProjectConfig` with `database_target` field
- `{{ schema_name }}`, `{{ table_name }}`, `{{ columns }}`, etc.
- `{{ is_tenant_scoped }}`, `{{ has_workflow }}`, etc.

**Acceptance criteria**:
- Each template compiles (valid SQLite syntax when rendered with realistic data)
- Templates don't use PostgreSQL-isms: `::type` casts, `gen_random_uuid()`, `now()`, `JSONB`, `TEXT[]`, `COMMENT ON`, `DO $$`, `plpgsql`, `CREATE EXTENSION`, `ENABLE ROW LEVEL SECURITY`, `SECURITY DEFINER`, `information_schema`
- Templates use SQLite features: `STRICT` mode, `(unixepoch())` for timestamps, `INSERT OR IGNORE`/`INSERT OR REPLACE`, `TEXT` for JSON/UUID, `INTEGER` for booleans

---

## Phase 3: DB Generator Dialect Awareness

**Goal**: Update the Rust generators in `crates/codegraph/src/generate/db/`
to use the `SqlDialect` trait for type mapping and feature gating.

**Files to modify**:

### `ddl.rs`
- Accept a `Box<dyn SqlDialect>` in the constructor (with default Postgres)
- Replace `pg_type` in `ColumnDef` with dialect-mapped types:
  ```rust
  fn map_column_type(&self, dialect: &dyn SqlDialect, col: &ColumnDef) -> String {
      dialect.map_pg_type(&col.pg_type).unwrap_or_else(|| col.pg_type.clone())
  }
  ```
- Conditional output files:
  - Skip RLS files when `!dialect.has_rls()`
  - Skip extension statements when `!dialect.has_extensions()`
  - Skip embedding files when `!dialect.has_embeddings()`
  - Use different default expressions via `dialect.uuid_default()`, `dialect.now_default()`
  - Use `dialect.null_uuid_default()` for the org_id sentinel
- Import `DatabaseTarget` from `super::dialect`
- The `generate()` method's `project` parameter already has `database_target` — use that to create the right dialect

### `entity.rs`
- Accept a `Box<dyn SqlDialect>` in the constructor
- When `!dialect.has_schemas()`:
  - Don't set `schema_name` in entity template context
  - Use `#[sea_orm(primary_key)]` without `auto_increment = false` (let SQLite use its default)
- When dialect is SQLite:
  - Change `sea_orm_type` for timestamps from `"TimestampWithTimeZone"` to `"Timestamp"` or `"String"`
  - Don't emit `column_type = "custom(...)"` for PG range types
  - Change `JsonBinary` attrs to standard `Json` or `Text`

### `codelist.rs`
- Read the template.tera file. If already present, minimal changes needed

### How to pass dialect to generators

In `generate/mod.rs` (the `run_generators_with_opts` function):
1. Extract `database_target` from `build_plan.database_target()` (or default)
2. Create `let dialect = dialect_for_target(database_target);`
3. Pass `dialect` to DB generators via constructor:

```rust
Box::new(
    db::ddl::DdlGenerator::new(output_dir, dialect_for_target(database_target))
        .with_parent_candidates(parent_candidates.clone()),
) as Box<dyn EntityGenerator>,
```

**Acceptance criteria**:
- `cargo test -p codegraph --lib -- generate::db::` passes
- DDL output for PostgreSQL mode is identical to before (no template changes needed for PG)
- When `database_target = "sqlite"`, DDL files use TEXT instead of UUID, no extensions, no RLS, no schemas
- Entity files omit `schema_name` attribute for SQLite

---

## Phase 4: Scaffold Template Dialect Support

**Goal**: Update the generated application scaffold to produce SQLite-compatible
Cargo.toml, main.rs, and middleware.

### `crates/codegraph/templates/scaffold/cargo_toml.tera`
- Make sea-orm feature conditional on `project.database_target`:
  - `"postgres"` → `features = ["sqlx-postgres", "runtime-tokio-rustls", "macros"]`
  - `"sqlite"` → `features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros"]`
- Conditionally exclude pgmq, pgvector, basejump dependencies for SQLite

### `crates/codegraph/templates/scaffold/main.tera`
- Remove/maintain `set_rls_org()` calls for SQLite (they use `set_config()` which is PG-only)
- Different DB connection init for SQLite (file path vs connection URL)

### `crates/codegraph/templates/scaffold/middleware.tera`
- The RLS/session-variable-setting middleware should be conditionally compiled
- For SQLite, the tenant isolation uses `WHERE platform_organization_id = ?` directly

### `crates/codegraph/templates/scaffold/app_state.tera`
- Adjust DB pool type for SQLite (no separate schema connection needed)

**Acceptance criteria**:
- Generated Cargo.toml for SQLite doesn't include `sqlx-postgres`
- Generated main.rs for SQLite doesn't call `set_config()` for RLS
- Generated code compiles for both targets

---

## Phase 5: Workflow Crate Dynamic Backend

**Goal**: Make `crates/codegraph-workflow/` work with both PostgreSQL and SQLite
by replacing hardcoded `DbBackend::Postgres` with dynamic backend detection.

### `Cargo.toml`
- Change sea-orm dependency from hardcoded `sqlx-postgres` to conditional features:
  ```toml
  [features]
  default = ["postgres"]
  postgres = ["sea-orm/sqlx-postgres", "sea-orm/runtime-tokio-rustls"]
  sqlite = ["sea-orm/sqlx-sqlite", "sea-orm/runtime-tokio-rustls"]
  sea-orm = { version = "1", features = ["macros"] }  # base, no backend
  ```

### `src/engine.rs` (19 occurrences of `DbBackend::Postgres`)
1. Replace `Statement::from_sql_and_values(DbBackend::Postgres, sql, params)` with
   `Statement::from_sql_and_values(self.db.backend(), sql, params)`
2. Remove or no-op `set_config('app.organization_id', ...)` calls for SQLite.
   Wrap in a helper:
   ```rust
   async fn set_rls_org(&self, org_id: Uuid) -> Result<()> {
       if self.db.backend() == DbBackend::Postgres {
           // Existing set_config() logic
       }
       // SQLite: RLS is handled at query level via WHERE clauses
       Ok(())
   }
   ```
3. Replace `now()` SQL references with a Rust-side `Utc::now()` passed as parameter,
   or use a dynamic helper:
   ```rust
   fn now_sql(&self) -> &'static str {
       match self.db.backend() {
           DbBackend::Postgres => "now()",
           DbBackend::Sqlite => "datetime('now')",
           _ => "now()",
       }
   }
   ```
4. Replace `'00000000-...'::uuid` with a Rust `Uuid` parameter
5. Handle `TEXT[]` / `Vec<String>` for terminal_states: store as JSON text on SQLite
   and parse with `serde_json`. On PostgreSQL, keep using native array.

### `src/approval.rs` (1 occurrence)
- Same `DbBackend::Postgres` → `self.db.backend()` change

### `src/delegation.rs` (2 occurrences)
- Same pattern

### `src/timer.rs` (2 occurrences)
- Same pattern + conditional `now()` handling

**Acceptance criteria**:
- Workflow crate compiles with `sqlx-sqlite` feature
- All `DbBackend::Postgres` references are replaced
- Engine tests pass for both backends

---

## Phase 6: Global DB Generator Updates

**Goal**: Update non-DDL DB generators to be dialect-aware.

### `platform_schema.rs`
- Accept dialect parameter
- Pass dialect context to the template
- The SQLite template (`db/sqlite/platform_schema.tera`) handles the SQL differences

### `basejump_setup.rs`
- Return empty Vec for SQLite (basejump is PostgreSQL-only)
- Template: `db/sqlite/basejump_setup.tera` — just a comment or empty

### `event_trigger.rs` (PgmqSetupGenerator)
- For SQLite: generate the simple `domain_event` table instead of pgmq queues
- Template: `db/sqlite/pgmq_setup.tera`

### `workflow_seed.rs`
- Pass dialect to template context
- SQLite template handles different UPSERT syntax

### `seed.rs`
- Pass dialect to template context
- SQLite template handles different INSERT syntax

### `report_view.rs`
- Pass dialect to template context
- SQLite template handles different SQL syntax

**Acceptance criteria**:
- `cargo test -p codegraph --lib -- generate::db::` passes
- Database target defaults to Postgres; all existing tests pass unchanged

---

## Phase 7: Verification

**Goal**: Verify backward compatibility and test new functionality.

### Steps
1. Run the existing test suite without `database_target` set — all tests must pass:
   ```bash
   cargo test -p codegraph --lib
   ```
2. Run dialect-specific tests:
   ```bash
   cargo test -p codegraph --lib -- generate::db::dialect
   ```
3. Compare generated output with `database_target = "postgres"` (default) against
   a baseline from the `main` branch — output should be identical:
   ```bash
   git stash && cargo run -- run ... && git stash pop
   ```
4. Run with `database_target = "sqlite"` and verify the output:
   - No `CREATE EXTENSION` statements
   - No RLS policies
   - No PL/pgSQL
   - No `gen_random_uuid()`, `now()` → `(unixepoch())`
   - Types: UUID → TEXT, JSONB → TEXT, TIMESTAMPTZ → TEXT, BOOLEAN → INTEGER
   - Tables use `STRICT` mode
   - Entity files omit `#[sea_orm(schema_name)]`
5. Add integration test: a test profile with `database_target = "sqlite"` that
   validates generated SQL is valid SQLite syntax.
