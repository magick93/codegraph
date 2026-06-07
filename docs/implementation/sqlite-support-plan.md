# SQLite Support — Implementation Plan

## Overview

Add SQLite as a configurable database target alongside PostgreSQL. The default
remains PostgreSQL — existing projects (hr-specs, etc.) are unaffected.

## Architecture

```
profiles.toml                    database_target from features
     │                                   │
     ▼                                   ▼
    BuildPlan ─────────────────► ProjectConfig.database_target
     │                                   │
     ▼                                   ▼
  DB Generators ─────────────► SqlDialect trait (dialect.rs)
     │                                   │
     ▼                                   ▼
  Tera templates                  {{ project.database_target }}
  (db/postgres/*.tera          or sqlite templates selected
   vs db/sqlite/*.tera)           at Tera init
```

**Key files created:**

| File | Purpose |
|------|---------|
| `crates/codegraph/src/generate/db/dialect.rs` | `SqlDialect` trait + `PostgresDialect` + `SqliteDialect` |
| `templates/db/sqlite/table.tera` | SQLite DDL for CREATE TABLE |
| `templates/db/sqlite/entity.tera` | SeaORM entity for SQLite (no schema_name) |
| `templates/db/sqlite/trigger.tera` | SQLite inline trigger syntax |
| `templates/db/sqlite/rls.tera` | Empty (RLS doesn't exist in SQLite) |
| `templates/db/sqlite/fts.tera` | FTS5 virtual table instead of tsvector |
| `templates/db/sqlite/embedding.tera` | Not generated (SQLite has no pgvector) |
| `templates/db/sqlite/domain_event_trigger.tera` | Simple event table insert |
| `templates/db/sqlite/pgmq_setup.tera` | Simple event queue table |
| `templates/db/sqlite/platform_schema.tera` | SQLite-compatible platform schema |
| `templates/db/sqlite/rbac_roles.tera` | Empty (no PG enums in SQLite) |
| `templates/db/sqlite/api_key_migration.tera` | SQLite-compatible API keys |
| `templates/db/sqlite/seed.tera` | SQLite-compatible seed data |
| `templates/db/sqlite/workflow_seed.tera` | SQLite-compatible UPSERT |
| `templates/db/sqlite/codelist.tera` | SQLite-compatible codelist table |
| `templates/db/sqlite/report_view.tera` | SQLite-compatible views |
| `templates/db/sqlite/deferred_fks.tera` | SQLite-compatible FK addition |

**Key files modified:**

| File | Change |
|------|--------|
| `crates/codegraph/src/generate/db/mod.rs` | Add `pub mod dialect` |
| `crates/codegraph/src/generate/mod.rs` | Add `database_target` to `ProjectConfig` |
| `crates/codegraph/src/profile.rs` | Parse `database_target` from features, add to `BuildPlan` |
| `crates/codegraph/src/main.rs` | Pass `database_target` to `ProjectConfig` |
| `crates/codegraph/src/generate/db/ddl.rs` | Accept dialect, use it for type mapping |
| `crates/codegraph/src/generate/db/entity.rs` | Accept dialect, adjust SeaORM attrs |
| `crates/codegraph/src/generate/db/platform_schema.rs` | Accept dialect context |
| `crates/codegraph/src/generate/db/basejump_setup.rs` | Skip for SQLite |
| `crates/codegraph/src/generate/db/event_trigger.rs` | Skip pgmq for SQLite |
| `crates/codegraph/src/generate/db/workflow_seed.rs` | Accept dialect context |
| `crates/codegraph/src/generate/db/seed.rs` | Accept dialect context |
| `crates/codegraph/src/generate/db/report_view.rs` | Accept dialect context |
| `crates/codegraph/templates/scaffold/cargo_toml.tera` | Conditional sea-orm feature |
| `crates/codegraph/templates/scaffold/main.tera` | No set_rls_org() for SQLite |
| `crates/codegraph/templates/scaffold/middleware.tera` | Different DB auth pattern |
| `crates/codegraph-workflow/Cargo.toml` | Optional sqlx-sqlite feature |
| `crates/codegraph-workflow/src/engine.rs` | Dynamic DbBackend |
| `crates/codegraph-workflow/src/approval.rs` | Dynamic DbBackend |
| `crates/codegraph-workflow/src/delegation.rs` | Dynamic DbBackend |
| `crates/codegraph-workflow/src/timer.rs` | Dynamic DbBackend |

## Phases

### Phase 1: Core dialect abstraction
- Create `dialect.rs` with `SqlDialect` trait, `PostgresDialect`, `SqliteDialect`
- Add `database_target` to `BuildPlan`, `ProjectConfig`, profile parsing
- Create dialect integration test

### Phase 2: SQLite DDL templates
- Create `templates/db/sqlite/` directory with all SQLite equivalents
- Each template mirrors a PostgreSQL template but emits SQLite-compatible SQL
- Key differences: no extensions, no schemas, no RLS, no PL/pgSQL, no JSONB, no UUID type

### Phase 3: DB generator dialect-awareness
- Update `ddl.rs`: use `SqlDialect` for type mapping, skip PG-only features
- Update `entity.rs`: no `schema_name` for SQLite, adjust `auto_increment`
- Update `codelist.rs`: minor dialect adjustments
- Update `platform_schema.rs`: pass dialect context to template
- Update `basejump_setup.rs` → empty for SQLite
- Update `event_trigger.rs` → simple event table for SQLite

### Phase 4: Scaffold templates
- `cargo_toml.tera`: conditional `sqlx-postgres` vs `sqlx-sqlite`
- `main.tera`: no `set_rls_org()`, different DB connection
- `middleware.tera`: conditional RLS/session variable code

### Phase 5: Workflow crate
- `codegraph-workflow/Cargo.toml`: feature-gate `sqlx-postgres` / `sqlx-sqlite`
- `engine.rs`: dynamic `DbBackend`, no `set_config()` for SQLite
- `approval.rs`, `delegation.rs`, `timer.rs`: dynamic `DbBackend`

### Phase 6: Global DB generators
- `workflow_seed.rs`: SQLite-compatible UPSERT
- `seed.rs`: SQLite seed data
- `report_view.rs`: SQLite-compatible views

### Phase 7: Testing and verification
- Verify default profile (no `database_target` set) produces identical output
- Add SQLite E2E test profile
- Add dialect unit tests
- Test `cargo test -p codegraph` passes

## Usage

```toml
# profiles.toml
[profiles.default.features]
database_target = "sqlite"   # default is "postgres"
```

```bash
# CLI override via feature flag
cargo run -- run --profile default --profiles-config profiles.toml
```

## Key design decisions

1. **Default is PostgreSQL** — `DatabaseTarget::default()` returns `Postgres`.
   Existing profiles without `database_target` set continue unchanged.

2. **Separate template directories** — `templates/db/postgres/` and
   `templates/db/sqlite/`. The generator picks the right template path
   based on the dialect. This avoids `{% if %}` branching in templates.

3. **SqlDialect trait** — All SQL differences are captured in the trait.
   Generators call `dialect.map_pg_type("UUID")` etc. rather than
   hardcoding type names.

4. **Workflow crate dynamic backend** — The 25 `DbBackend::Postgres` hardcodings
   become `self.db.backend()`. RLS session variables are conditionally skipped.
