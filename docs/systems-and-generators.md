# Codegraph — Systems, Generators & Configuration

This document describes, in detail, what codegraph produces: the top-level
systems of a generated application, every generator and template, and every
configuration file surface.

Sources of truth: generator capabilities are declared in
`crates/codegraph-generate/src/profile.rs` (`base_capabilities()` +
`ifml_capabilities()`), generator registration and output paths live in
`crates/codegraph-generate/src/lib.rs`, templates live in
`crates/codegraph-generate/templates/`, and configuration types live in
`crates/codegraph-config`, `crates/codegraph-classifier` and
`crates/codegraph-ext-points`.

## 1. What codegraph does

Codegraph is a model-driven application generator. Model sources —
`.mox` packages (primary), Rosetta (Rune DSL), JSON Schema (legacy/import),
IFML interaction models, and `.actor` authorization policies — are ingested
into a **Grafeo property graph** (entities, properties, codelists,
namespaces, views, events, actors). A **profile** (`profiles.toml`) selects
which of the ~90 registered generators run; each generator queries the graph
and renders output (Rust, SQL, TypeScript, Svelte, proto, TOML, CI) into the
project's `generated/` tree. The generated output is a complete, buildable
application: database migrations + ORM + DDD services + axum REST API
(+ optional gRPC / Cloudflare Workers), a SvelteKit web UI, a CLI, E2E test
suites, and an ops/test harness.

```
.mox / .rosetta / JSON Schema / .ifml / .actor      domains.toml, profiles.toml,
        │                                           classifier.toml, extension-points.toml
        ▼
   Grafeo graph (Schema, Property, Codelist, Namespace,
   ViewContainer, Event, Actor, Grant, ...)
        │
        ▼
   BuildPlan (profile → generator list, features, dialect,
   persistence provider, deployment topology)
        │
        ▼
   Entity / Domain / Global generators  →  generated/ (app server, DB, UI,
                                            CLI, tests, ops harness, …)
```

## 2. Top-level systems

| System | Description | Output (under `generated/`) | Key generators |
|--------|-------------|------------------------------|----------------|
| **Database layer** | Postgres (or SQLite) DDL migrations: tables, child tables (VO composition), FKs, RLS policies, triggers, codelist seeds, FTS + pgvector, platform/API-key/Basejump schema, workflow seeds, report views, policy-derived RLS | `migrations/` | `ddl`, `codelist`, `basejump_setup`, `platform_schema`, `platform_grants`, `pgmq_setup`, `workflow_seed`, `report_views`, `rls_from_policy` (`policy_rls`) |
| **Application server (REST)** | Axum monolith: handlers, domain routers, write/read services, repositories, DTOs, domain events, errors, OpenAPI spec, middleware, API-key auth, workflow action endpoints | `src/`, `openapi/` | `handler`, `router`, `command`, `query`, `repository`, `dto`, `event`, `errors`, `openapi`, `scaffold` |
| **gRPC server** | `.proto` messages + service per entity, tonic server impls, per-domain routers, shared proto + conversion helpers; `build.rs` compiles protos (server + client) | `proto/`, `src/api/grpc/` | `grpc_proto`, `grpc_service`, `grpc_router`, `grpc_scaffold` |
| **Cloudflare Workers topology** | One worker crate per domain + gateway (wrangler configs, Hyperdrive bindings, service bindings, cron/queue handlers, wasm observability); replaces the monolith scaffold | `workers/`, `gateway/` | `worker_scaffold`, `webhook_dispatch` (worker variants) |
| **Webhooks** | Endpoint/subscription CRUD API + HMAC-signed dispatch/delivery with retries and auto-deactivation (monolith SeaORM or worker cornucopia variants) | `src/webhook/`, `workers/{domain}/src/` | `webhook_endpoint_api`, `webhook_dispatch` |
| **Web UI** | SvelteKit app: app shell/scaffold, per-entity list/detail/edit/create pages, stores, entity descriptors, codelist data, org charts, settings pages (API keys, integrations, webhooks, team), login/signup, workflow panels | `ui/` | `ui_scaffold`, `ui_page`, `ui_form`, `ui_store`, `ui_descriptor`, `ui_codelist`, `ui_types`, `ui_orgchart`, `ui-shell`, `ui-domain-layout` |
| **IFML UI** | Behavior-wired pages for SvelteKit/React/Vue/Flutter/SwiftUI generated from IFML interaction models: page loads, navigation map/type helpers, app skeleton, Playwright specs; design-system packs (shadcn-svelte) | per-framework target dir | `ifml_skeleton_{fw}`, `ifml_route_{fw}`, `ifml_navigation_{fw}`, `ifml_e2e_test_{fw}` |
| **CLI** | Clap-based client crate: global scaffold, per-entity/per-domain commands, typed HTTP client, output formatting | `cli/` (crate) | `cli_scaffold`, `cli_command`, `cli_domain` |
| **E2E tests** | Rust Playwright crate + TypeScript Playwright suite: entity CRUD specs, fixtures, auth, data factories, docker-compose test env | `tests/`, `ui/tests/` | `playwright-global`, `playwright-entity`, `playwright_ts_global`, `playwright_ts_entity`, `ui_e2e_test` |
| **Ops harness** | `codegraph-ops.toml` manifest + `testkit` crate driving `api`/`e2e`/`ui`/`smoke`/`quality`/`ext` suites (migrate, hurl, curl smoke, RLS, Playwright) | `codegraph-ops.toml`, `ops/testkit/` | `ops` |
| **AT Protocol stack** | Lexicon documents, Rust record/enum types, XRPC query/procedure clients + routers, AppView ingestors, identity (DID web / handle), label storage, service tables | `lexicons/`, `src/atproto/` | `lexicon`, `atproto_types`, `atproto_client`, `atproto_xrpc`, `atproto_appview`, `atproto_identity`, `label_setup`, `service_tables` |
| **Integrations** | Extension-point–driven integration plane: tables + RLS + seed, typed config structs, event dispatcher, catalog/install management handlers | `src/integration/` | `integration_tables`, `integration_config`, `integration_dispatch`, `integration_catalog` |
| **Domain-types crate** | Separate framework-free types crate: entities, DTOs, query services (shared by server/CLI/UI packages) | `{domain_types_base}` | `domain_types_scaffold`, `domain_types_dto`, `domain_types_query_service` |
| **Lifecycle hooks** | Per-entity lifecycle trait + hook registry (pre/post persist adapters) | `src/hooks/` | `lifecycle_trait`, `hook_registry` |
| **Seed / demo data** | Idempotent codelist + workflow seeds; opt-in demo-data seed module + CLI with a write-path-backed sink | `src/seed/`, migrations | `codelist`, `workflow_seed`, `seed_provision` |
| **Fern SDK** | Fern configuration for generated SDKs from the OpenAPI spec | `fern/` | `fern_config` |
| **EmDash plugins** | EmDash admin-plugin packages per domain (TS plugin, settings, list/detail sites, e2e) + family scaffold | `plugins/` | `emdash_plugin`, `emdash_plugin_scaffold` |
| **Project scaffold** | `codegraph init` consumer monorepo: wrapper graph crate, domains/profiles/extension-points/ops configs, mox or rosetta model starters, justfile, CI | project root (not `generated/`) | `codegraph init` (project templates) |

## 3. Generator reference

Every generator has a capability entry: a **kind** (how often it runs —
`Entity` once per entity, `Domain` once per domain, `Global` once per run),
a **target** (which profile section lists it — `Api`, `Ui`, `Cli`, `Common`),
and optional **feature gates** (`[features]` keys that must be true).
Unknown/ungated generators below are registered when their name appears in
a profile section.

### 3.1 Entity generators

| Name | Target | Feature gate | Output | Description |
|------|--------|--------------|--------|-------------|
| `ddl` | Api | — | `migrations/` | Postgres/SQLite DDL per entity: table, VO child tables, FKs, indexes, comments, deferred cross-domain FKs. Dialect-driven (SqlDialect trait). |
| `sea_orm_entity` | Api | — | `src/entity/{module}.rs` (namespace-aware paths when `namespace_layout` on) | SeaORM entity model (`DeriveEntityModel`) per entity. |
| `cornucopia_queries` | Api | — | `queries/{domain}/{entity}.sql` | Annotated SQL CRUD/query files for the Cornucopia persistence provider (build_persistence_entity IR). |
| `cornucopia_repo` | Api | — | repository adapter | Cornucopia-backed repository implementation wrapping generated query functions. |
| `codelist` | Api | — | `src/codelist/mod.rs` + migration SQL | Rust enums for codelists + idempotent seed inserts (PG enum/insert or SQLite `INSERT OR IGNORE`). |
| `dto` | Api | — | `src/domain/{domain}/{module}/dto*.rs` | Create/Update/Response/Summary/Included DTOs with config-driven field exclusions and key casing. |
| `repository` | Api | — | repository trait + SeaORM impl | Repository trait per entity plus SeaORM implementation, policy-aware (soft delete, tenant scope), include-eager-loading fetch helpers. |
| `command` | Api | — | `src/domain/{domain}/{module}/command.rs` | Write-side command service (create/update/delete) with RLS session context bundle + lifecycle hooks. |
| `query` | Api | — | `src/domain/{domain}/{module}/query.rs` | Read-side query service (find/list/search) with filters, pagination, `?include=` eager loading. |
| `event` | Api | — | domain event module | Domain event types + emission for entity changes. |
| `handler` | Api | — | axum handler per entity | REST handlers (CRUD, bulk, filters, search, includes, tree endpoints) wired to command/query services. |
| `workflow_action` | Api | — | workflow endpoints | Transition/approve/reject action endpoints for entities with a `domains.toml` workflow. |
| `media_route` | Api | — | media routes | Multipart upload/download/delete routes for entity media fields. |
| `test` | Api | — | entity unit tests | Unit tests for entity/DTO behavior. |
| `lifecycle_trait` | Common | — | hooks trait | Per-entity lifecycle hook trait (pre/post persist) used by command services. |
| `domain_types_dto` | Api | — | domain-types crate | DTOs emitted into the separate domain-types crate (for sharing across server/CLI/UI). |
| `domain_types_query_service` | Api | — | domain-types crate | Query-service trait emitted into the domain-types crate. |
| `ui_page` | Ui | — | `ui/src/routes/...` | List + detail SvelteKit pages (+page/+page.server.ts) with child sections and workflow badges. |
| `ui_form` | Ui | — | `ui/src/routes/.../form` | Edit/create form pages with validation messages, defaults, workflow states. |
| `cosmos_entity_form` | Ui | — | form component | Cosmos-variant entity form component. |
| `ui_store` | Ui | — | `ui/src/lib/stores/` | Svelte store per entity wrapping the API client. |
| `ui_e2e_test` | Ui | — | `ui/tests/` | Playwright specs per entity: CRUD, validation, isolation, search, workflow, webhooks. |
| `playwright-entity` | Ui | — | Rust Playwright crate | Rust-side E2E harness per entity (page objects, data factory). |
| `playwright_ts_entity` | Ui | — | `ui/tests/e2e/` | TypeScript Playwright specs + fixtures + typed API client per entity. |
| `ui_descriptor` | Ui | — | descriptor module | Entity descriptor (fields, types, UI hints) driving generic form/table rendering; honors ui-overrides/ui-domains config. |
| `ui-shell` | Ui | — | shell CRUD pages | Generic shell CRUD pages (list/create/edit/detail) rendered from the descriptor. |
| `cli_command` | Cli | — | `cli/src/commands/` | Clap subcommands per entity (CRUD against the HTTP API). |
| `grpc_proto` | Api | `grpc_backend` | `proto/{domain}/{module}.proto` | Protobuf messages (entity + CRUD + search + tree + transition) and service definition; codelist-aware enum vs string choice. |
| `grpc_service` | Api | `grpc_backend` | `src/api/grpc/{module}_grpc.rs` | Tonic server implementation + `From` conversions to/from domain types. |
| `lexicon` | Common | `atproto_backend` | `lexicons/` | AT Protocol Lexicon JSON documents (record/object/enum + macros) per schema type. |
| `atproto_types` | Common | `atproto_backend` | `src/atproto/types/` | Rust types generated from lexicons (records, enums, record impls). |
| `atproto_client` | Common | `atproto_backend` | `src/atproto/client*` | Typed XRPC client methods per lexicon record. |
| `atproto_xrpc` | Common | `atproto_backend` | `src/atproto/xrpc/` | XRPC query/procedure implementations per lexicon type. |

### 3.2 Domain generators

| Name | Target | Feature gate | Output | Description |
|------|--------|--------------|--------|-------------|
| `errors` | Api | — | `src/domain/{domain}/errors.rs` | Domain error enum (thiserror) with per-error HTTP mapping; `Forbidden` classification for RLS denials. Always emits (InternalError-only when no definitions). |
| `router` | Api | — | `src/api/{domain}/router.rs` | Axum router per domain nesting all entity routes with permission middleware. |
| `api_contract` | Api | — | `src/api/{domain}/contract.rs` | Machine-readable plugin API contract for the domain (source-of-truth doc for consumers). |
| `links` | Api | — | HAL links module | HAL-style link relations/types for API responses. |
| `ui-domain-layout` | Ui | — | `ui/src/routes/(domain)/` | Per-domain SvelteKit layout + navigation. |
| `cli_domain` | Cli | — | `cli/src/commands/{domain}.rs` | Domain-level clap command grouping entity commands. |
| `grpc_router` | Api | `grpc_backend` | `src/api/grpc/{domain}_router.rs` | Per-domain gRPC service registration router. |
| `condition_validations` | Api | `rosetta_backend` | validations module | Constraint-plane validation code lowered from Rosetta condition expressions. |
| `regulatory_reports` | Api | `rosetta_backend` | report endpoints | Regulatory report scaffolding (report handlers + routers). |
| `functions` | Api | `rosetta_backend` | functions module | Rosetta function codegen (lowered computation helpers). |
| `rules` | Api | `rosetta_backend` | rules module | Rosetta rule codegen (business-rule evaluation). |
| `emdash_plugin` | Ui | `emdash_plugins` | EmDash plugin package | Per-domain EmDash admin plugin (settings schema, list/detail sites, e2e specs). |
| `atproto_appview` | Common | `atproto_backend` | `src/atproto/appview/` | AppView ingestor (graph → index tables) + index-rebuild CLI. |
| `atproto_xrpc_router` | Common | `atproto_backend` | `src/atproto/xrpc/routes` | XRPC route wiring for the domain's queries/procedures. |

### 3.3 Global generators

| Name | Target | Feature gate | Output | Description |
|------|--------|--------------|--------|-------------|
| `basejump_setup` | Common | — | migration | Basejump-compatible platform foundation (accounts, teams, roles, API keys) incl. RBAC role/rank helpers. |
| `pgmq_setup` | Common | — | migration | pgmq extension setup + domain event queues/triggers (simple event-table trigger on SQLite). |
| `label_setup` | Common | `has_labels` | migration | AT Protocol label storage (materialized index of applied labels). |
| `service_tables` | Common | `atproto_backend` | migration | Service tables for hand-written extension features. |
| `platform_schema` | Common | — | migration | `platform` schema: API keys, scopes, webhook tables, usage logs. |
| `platform_grants` | Common | — | migration | Grants for `app_user`/`api_key` roles on platform tables + app-user DML grants. |
| `workflow_seed` | Common | — | migration/seed | Workflow definition seed SQL (states, transitions, timers) for the workflow engine. |
| `cornucopia_config` | Common | — | `cornucopia.toml` | Cornucopia codegen config with type mappings (Cornucopia provider runs). |
| `openapi` | Common | — | `openapi/` | Unified OpenAPI spec (+ per-domain split, catalog, security scheme docs). |
| `api_contract_index` | Common | — | contract index | Index + readme tying per-domain API contracts together. |
| `scaffold` | Common | — (monolith topology) | crate root, `src/`, `migration/` | The axum server scaffold: `main.rs`/`server.rs`/`app_state`/config/error/meta/middleware/doctor, root `Cargo.toml`, `build.rs` (proto compile when gRPC on), migration crate, OpenAPI dump, integrations handler. |
| `worker_scaffold` | Common | — (workers topology) | `workers/`, `gateway/`, workspace Cargo.toml | Per-domain Cloudflare Worker crates (native or wasm32 slice, wrangler configs, Hyperdrive, queues, cron) + gateway worker; replaces `scaffold`. |
| `ui_scaffold` | Ui | — | `ui/` | SvelteKit app scaffold: package.json/vite/svelte configs, app shell + layout, login/signup/callback, dashboard, error page, settings pages (API keys, team, integrations, webhooks), search input, i18n, Playwright config + global setup/teardown, persona fixtures, supabase client, workflow panel. |
| `ui_types` | Ui | — | `ui/src/lib/types.ts` | Shared TypeScript types for the UI. |
| `ui_codelist` | Ui | — | `ui/src/lib/codelist/` | Codelist value modules for UI dropdowns/validation. |
| `ui_orgchart` | Ui | — | `ui/src/routes/(app)/org-chart/` | Org-chart page + load for entities with `has_orgchart`. |
| `hook_registry` | Common | — | `src/hooks/` | Hook registry collecting all lifecycle hook implementations. |
| `domain_types_scaffold` | Common | — | domain-types crate | Cargo.toml + mod scaffold for the domain-types crate. |
| `report_views` | Common | — | migration | SQL report views (incl. HR Open ProcessHistory-compatible views over workflow transitions). |
| `cli_scaffold` | Cli | — | `cli/` crate | CLI crate scaffold: Cargo.toml, main, config, commands mod, typed HTTP client, output formatting, util, build.rs. |
| `playwright-global` | Ui | — | Rust Playwright crate | Global files for the Rust Playwright E2E crate. |
| `playwright_ts_global` | Ui | — | `ui/tests/e2e/` | Global TS Playwright config + search specs. |
| `integration_tables` | Common | — | migration | Integration extension-point tables + RLS + seed. |
| `integration_config` | Common | — | `src/integration/` | Typed config structs for integration extension points. |
| `integration_dispatch` | Common | — | `src/integration/` | Integration event dispatcher (poll → dispatch). |
| `integration_catalog` | Common | — | `src/integration/` | Catalog + installation management handlers/router. |
| `webhook_dispatch` | Common | — | `src/webhook/dispatch.rs` | HMAC-signed webhook dispatcher with retry/backoff + endpoint auto-deactivation (worker variant for workers topology). |
| `webhook_endpoint_api` | Common | — | `src/webhook/` | Webhook endpoint/subscription CRUD API + router (cornucopia variants for workers). |
| `seed_provision` | Common | — (when seed enabled) | `src/seed/` | Demo-data seed module + CLI; sink writes through the generated command layer (validation, hooks, RLS). |
| `ops` | Common | `ops_backend` | `codegraph-ops.toml`, `ops/testkit/` | Ops manifest + testkit crate (api/e2e/ui/smoke/quality/ext suites). |
| `grpc_scaffold` | Api | `grpc_backend` | `proto/shared.proto`, `src/api/grpc/mod.rs` | Shared proto, gRPC module wiring, common conversion helpers. |
| `policy_rls` | Common | `rls_from_policy` | `migrations/020000_policy_rls.sql` | RLS policies/roles/grants derived from the rexlang actor policy graph (permit/forbid/when → SQL closed subset). |
| `fern_config` | Api | `fern_sdk` | `fern/` | Fern SDK generator config pointing at the OpenAPI spec. |
| `emdash_plugin_scaffold` | Ui | `emdash_plugins` | plugins scaffold | EmDash plugin family scaffold (package/tsconfig/index/settings/plugin registration/readme). |
| `lexicon_scaffold` | Common | `atproto_backend` | lexicons scaffold | Lexicon directory scaffold for the atproto app. |
| `atproto_client_scaffold` | Common | `atproto_backend` | client scaffold | XRPC client module wiring. |
| `atproto_identity` | Common | `atproto_backend` | `src/atproto/identity/` | DID-web document, handle resolution, service auth. |
| `atproto_generated_types` | Common | `atproto_backend` | generated types index | Index module for generated atproto types. |
| `atproto_xrpc_merge` | Common | `atproto_backend` | `src/atproto/xrpc/routes` | Merged `/xrpc/*` router assembly aggregating all per-domain XRPC routers. |

### 3.4 IFML generators

IFML capabilities are generated per framework — `fw` ∈ `svelte`, `react`,
`vue`, `flutter`, `swiftui`. All are gated on `ifml_backend` (+ implicit
`framework_{fw}`).

| Name | Kind | Output | Description |
|------|------|--------|-------------|
| `ifml_skeleton` / `ifml_skeleton_{fw}` | Global | app skeleton | Buildable app skeleton (SvelteKit: package.json, vite, svelte/tsconfig, app.html, Playwright deps); runs first so e2e stubs never clobber it. |
| `ifml_route` / `ifml_route_{fw}` | Global | pages/routes | Behavior-wired pages per view: loads, form submit/goto handlers, guards (roles + capabilities), mapped-component rendering, workflow badges. |
| `ifml_navigation` / `ifml_navigation_{fw}` | Global | navigation map | Route map + typed navigation helpers. |
| `ifml_e2e_test` / `ifml_e2e_test_{fw}` | Global | Playwright specs | Render/click-through/validation/CRUD/persona/workflow specs per view. |

## 4. Template reference

Templates are Tera files under `crates/codegraph-generate/templates/`,
embedded at build time and shadowable per-name via `--template-dir`
(later directories win). The database dialect selects `db/` vs `db/sqlite/`
variants.

### 4.1 `api/` — REST API layer

| Template | Purpose |
|----------|---------|
| `handler.tera` | Axum REST handlers per entity (CRUD, bulk, filter, search, includes, tree). |
| `router.tera` | Domain router nesting entity routes with middleware. |
| `workflow_action.tera` | Workflow transition/approve/reject endpoints. |
| `media_route.tera` | Multipart media upload/download/delete routes for a media field. |
| `contract.tera` | Machine-readable plugin API contract for a domain. |
| `contract_index.tera` | Index over all domain API contracts. |
| `contract_readme.tera` | Human readme for the contract directory. |
| `links.tera` | HAL-style link types for API responses. |
| `openapi_all.tera` | Unified OpenAPI document. |
| `openapi_domain.tera` | Per-domain OpenAPI document (split mode). |
| `openapi_catalog.tera` | Catalog of available OpenAPI documents. |
| `openapi_security.tera` | Security-scheme documentation fragment. |
| `report_handler.tera` | Report query endpoints (regulatory reports). |
| `report_router.tera` | Router for report endpoints. |

### 4.2 `atproto/` — AT Protocol stack

| Template | Purpose |
|----------|---------|
| `lexicon_record.tera` | Lexicon JSON for a record type. |
| `lexicon_object.tera` | Lexicon JSON for an object type. |
| `lexicon_enum.tera` | Lexicon JSON for an enum (tokens/strings). |
| `lexicon_macros.tera` | Shared lexicon macro fragments. |
| `scaffold.tera` | Lexicon directory scaffold. |
| `rust_type.tera` | Rust type for a lexicon object. |
| `rust_enum.tera` | Rust enum for a lexicon enum. |
| `rust_record_impl.tera` | Record trait impl (create/put/get/uri helpers). |
| `generated_types.tera` | Index module for generated atproto types. |
| `client.tera` | Typed XRPC client methods per record. |
| `client_scaffold.tera` | XRPC client module wiring. |
| `xrpc_query.tera` | XRPC query (GET) implementation. |
| `xrpc_procedure.tera` | XRPC procedure (POST) implementation. |
| `xrpc_router.tera` | Per-domain XRPC router. |
| `xrpc_routes.tera` | Merged `/xrpc/*` router assembly (all per-domain routers, wired in server.rs). |
| `xrpc_mod.tera` | Merged XRPC module. |
| `appview_ingestor.tera` | AppView ingestor (firehose → index tables). |
| `appview_index.tera` | Index-rebuild CLI command. |
| `did_web.tera` | DID document (did:web). |
| `handle.tera` | Handle resolution support. |
| `auth.tera` | Service auth (JWT/signing) helpers. |

### 4.3 `cli/` — CLI crate

| Template | Purpose |
|----------|---------|
| `cargo_toml.tera` | CLI crate manifest. |
| `main.tera` | CLI entrypoint (clap parser, auth, base URL). |
| `config.tera` | CLI config (profiles, API key storage). |
| `commands_mod.tera` | Commands module registry. |
| `entity_command.tera` | Per-entity CRUD subcommands. |
| `domain_command.tera` | Per-domain command group. |
| `client.tera` | Typed HTTP client (reqwest) with auth headers. |
| `output.tera` | Output formatting (table/json/yaml). |
| `util.tera` | CLI helpers. |
| `build_rs.tera` | build.rs for the CLI crate. |

### 4.4 `codelist/` — codelist enums

| Template | Purpose |
|----------|---------|
| `enum.tera` | Rust enum for a codelist (with display/parse helpers). |
| `empty_enum.tera` | Empty codelist module when the domain has no codelists. |

### 4.5 `db/` — database migrations

| Template | Purpose |
|----------|---------|
| `table.tera` | CREATE TABLE per entity incl. VO child tables, FKs, indexes, comments. |
| `deferred_fks.tera` | Deferred cross-domain foreign keys (separate migration band). |
| `entity.tera` | SeaORM entity model per entity (Postgres variant). |
| `trigger.tera` | Audit/updated-at triggers. |
| `domain_event_trigger.tera` | Domain-event capture into pgmq/event tables. |
| `rls.tera` | Row-level-security policies (org isolation, scope/role enforcement). |
| `policy_rls.tera` | Policy-derived RLS migration (`020000_policy_rls.sql`) from the actor graph. |
| `codelist.tera` | Codelist seed INSERTs (idempotent). |
| `seed.tera` | Generic seed SQL. |
| `workflow_seed.tera` | Workflow definition seed SQL. |
| `fts.tera` | Full-text-search tsvector columns, indexes, triggers. |
| `embedding.tera` | pgvector embedding columns + semantic search infrastructure. |
| `process_history_view.tera` | ProcessHistoryType-compatible view over workflow transitions. |
| `report_view.tera` | Report SQL views. |
| `api_key_migration.tera` | API-key management schema migration. |
| `rbac_roles.tera` | RBAC role enum extension + role-rank enforcement helpers. |
| `label_setup.tera` | AT Protocol label storage tables. |
| `service_tables.tera` | Service tables for hand-written extension features. |
| `platform_schema.tera` | `platform` schema (API keys, webhooks, usage). |
| `platform_grants.tera` | Grants on platform tables. |
| `app_user_grants.tera` | DML grants for the `app_user` role on domain tables. |
| `sqlite/*` | SQLite dialect variants: `table` (STRICT tables), `entity`, `trigger` (inline), `fts` (FTS5), `codelist`, `seed`, `rls` (placeholder), `embedding`, `domain_event_trigger`, `process_history_view`, `workflow_seed`. |

### 4.6 `ddd/` — domain layer

| Template | Purpose |
|----------|---------|
| `command.tera` | Write-side command service (create/update/delete with RLS context + hooks). |
| `query.tera` | Read-side query service (find/list/search/includes). |
| `repository.tera` | Repository trait + SeaORM implementation. |
| `event.tera` | Domain event types/emission. |
| `dto_create.tera` | Create DTO. |
| `dto_update.tera` | Update DTO (immutable fields excluded). |
| `dto_response.tera` | Detail response DTO (expansions). |
| `dto_summary.tera` | List/summary DTO. |
| `dto_included.tera` | Include-eager-loading response types. |
| `errors.tera` | Domain error enum with HTTP mapping. |

### 4.7 `domain_types/` — shared domain-types crate

| Template | Purpose |
|----------|---------|
| `cargo_toml.tera` | Crate manifest for the domain-types crate. |
| `domain_mod.tera` | Root module. |
| `entity_mod.tera` | Per-domain entity module. |
| `dto_create.tera` / `dto_update.tera` / `dto_response.tera` | DTOs in the shared crate. |
| `query_service.tera` | Query-service trait in the shared crate. |

### 4.8 `emdash/` — EmDash admin plugins

| Template | Purpose |
|----------|---------|
| `package_json.tera` | Plugin family package.json. |
| `tsconfig_json.tera` | TypeScript config. |
| `plugin_jsonc.tera` | Plugin descriptor (emdash plugin manifest). |
| `plugin_ts.tera` | Sandboxed plugin implementation (context, event poller). |
| `index_ts.tera` | Plugin entrypoint registering the domain plugin. |
| `settings_ts.tera` | Settings schema for the domain. |
| `site_list.tera` | Admin list site. |
| `site_detail.tera` | Admin detail site. |
| `admin_ts.tera` | Admin registration glue. |
| `e2e_spec.tera` | Plugin e2e spec. |
| `scaffold_readme.tera` | Scaffold readme. |
| `types_node_index.tera` / `types_node_package.tera` | Node types shim for the plugin workspace. |

### 4.9 `fern/` — Fern SDK

| Template | Purpose |
|----------|---------|
| `fern_config.tera` | Fern organization config. |
| `generators.tera` | Fern generators config (OpenAPI source, target languages). |

### 4.10 `grpc/` — gRPC

| Template | Purpose |
|----------|---------|
| `proto_message.tera` | Protobuf messages per entity (field numbering, codelist enums). |
| `proto_service.tera` | Service definition (CRUD/search/tree/transition RPCs). |
| `proto_shared.tera` | Shared proto (common types/pagination). |
| `tonic/server_impl.tera` | Tonic server implementation per entity. |
| `tonic/conversions.tera` | From conversions proto ↔ domain types. |
| `tonic/domain_router.tera` | Per-domain service registration. |

### 4.11 `hooks/` — lifecycle hooks

| Template | Purpose |
|----------|---------|
| `lifecycle_trait.tera` | Per-entity lifecycle hook trait. |
| `registry.tera` | Hook registry collecting implementations. |
| `generated_mod.tera` | Generated hooks module. |
| `domain_mod.tera` | Hooks domain module. |

### 4.12 `ifml/` — IFML UI generators

| Template | Purpose |
|----------|---------|
| `svelte/page.tera`, `page_load.tera` | SvelteKit page + load fn per view (events, guards, mapped components). |
| `svelte/layout.tera` | Nav shell layout emitted from landmark views. |
| `svelte/navigation_map.tera` | Route map + type helpers. |
| `react/page.tera`, `page_load.tera` | React (file-based routing) equivalents. |
| `react/navigation_map.tera` | React route map. |
| `vue/page.tera` / `flutter/page.tera` / `swiftui/page.tera` | Vue / Flutter / SwiftUI page equivalents. |
| `vue/navigation_map.tera` / `flutter/navigation_map.tera` / `swiftui/navigation_map.tera` | Route maps for those frameworks. |
| `packs/shadcn-svelte/ifml/svelte/layout.tera` | shadcn-svelte design-system pack shell layout. |

### 4.13 `integration/` — extension-point integrations

| Template | Purpose |
|----------|---------|
| `tables.tera` | Integration tables for extension points. |
| `rls.tera` | RLS policies for integration tables. |
| `seed.tera` | Integration seed data. |
| `config_struct.tera` | Typed config structs per extension point. |
| `dispatcher.tera` | Integration event dispatcher. |
| `catalog_handler.tera` | Catalog/install management handlers. |
| `catalog_router.tera` | Catalog router. |

### 4.14 `ops/` — ops harness

| Template | Purpose |
|----------|---------|
| `testkit_cargo.tera` | Testkit crate manifest (pins codegraph-ops rev). |
| `testkit_main.tera` | Testkit entrypoint with extension registration hook. |

### 4.15 `playwright/` — Rust + TS E2E harness

| Template | Purpose |
|----------|---------|
| `crate_cargo.tera` | Rust Playwright crate manifest. |
| `crate_lib.tera` | Crate lib root. |
| `entity_page.tera` | Page object per entity. |
| `test_data_factory.tera` | JSON test-data factory per entity. |
| `docker_compose_test.tera` | Docker Compose E2E environment. |
| `ts_playwright_config.tera` | TypeScript Playwright config (env-gated Bearer auth). |
| `ts_spec.tera` | TS CRUD spec per entity. |
| `ts_fixture.tera` | API fixtures per entity. |
| `ts_api_client.tera` | Typed TS API client for tests. |
| `ts_auth.tera` | Auth helpers for tests. |
| `ts_search_spec.tera` | Search e2e spec. |

### 4.16 `project/` — `codegraph init` scaffold

| Template | Purpose |
|----------|---------|
| `workspace_cargo.tera` | Consumer workspace manifest. |
| `wrapper_cargo.tera` / `wrapper_main.tera` | Thin codegen wrapper crate (clap over the driver). |
| `domains.tera` | `domains.toml` (one entry per domain). |
| `profiles.tera` | `profiles.toml` (meta + features + sections). |
| `extension_points.tera` | `extension-points.toml`. |
| `ops_manifest.tera` | `codegraph-ops.toml` seed (mox/rosetta model lists). |
| `model_mox.tera` | Starter `.mox` model per domain (TODO example). |
| `rosetta_model.tera` | Starter `.rosetta` model per domain (rosetta-first). |
| `justfile.tera` | Recipes: generate/classify/doctor/api/e2e/full/clean. |
| `ci_yml.tera` | GitHub Actions CI (mox-first generate job). |
| `readme.tera` | Getting-started readme. |
| `gitignore.tera` | Ignores `generated/`. |
| `hurl_health.tera` | Health-check hurl file. |
| `testkit_cargo.tera` / `testkit_main.tera` | Testkit workspace member scaffold. |

### 4.17 `scaffold/` — server scaffold (monolith + workers)

| Template | Purpose |
|----------|---------|
| `cargo_toml.tera` | Generated app root manifest (feature-conditional deps). |
| `lib.tera` | Crate lib root. |
| `main.tera` | Server entrypoint (migrations, boot, shutdown). |
| `server.tera` | Axum router assembly + middleware stack. |
| `app_state.tera` | AppState (pools, pool mode app_user/legacy, config). |
| `db_client.tera` | DB client/pool helpers. |
| `config.tera` | Environment/file configuration. |
| `error.tera` | Shared error types + HTTP mapping. |
| `meta.tera` | Shared response metadata envelope. |
| `doctor.tera` | `doctor` subcommand (config/toolchain/db checks). |
| `dump_openapi.tera` | OpenAPI dump binary target. |
| `middleware.tera` | Auth/context middleware (API key, JWT). |
| `permission_middleware.tera` | Permission middleware (JWT role checks). |
| `metrics_middleware.tera` | Prometheus metrics middleware (monolith). |
| `worker_middleware.tera` | Metrics/context middleware for workers (console, observability-gated). |
| `qs_query.tera` | serde_qs bracket-notation query extractor (`?filter[field]=`). |
| `integrations_handler.tera` | Integration status endpoint. |
| `migration.tera` / `migration_cargo_toml.tera` / `migration_lib.tera` / `migration_main.tera` / `migration_migrator.tera` | Standalone migration crate (apply migrations without the server). |
| `build_rs.tera` | build.rs (proto compilation when gRPC enabled). |
| `worker.tera` | Per-domain worker entry (wasm/native slices, tracing subscriber). |
| `worker_main.tera` / `worker_lib.tera` / `worker_cargo_toml.tera` | Worker crate entry/lib/manifest. |
| `worker_app_state.tera` | Worker state (Hyperdrive connection). |
| `worker_hooks_mod.tera` | Worker lifecycle hooks module. |
| `worker_workflow_client.tera` | Cross-domain workflow client (service bindings). |
| `worker_wrangler.tera` | Wrangler config per worker (bindings, queues, crons, observability). |
| `gateway_main.tera` / `gateway_lib.tera` / `gateway_cargo_toml.tera` | Gateway worker (routing to domain workers). |
| `gateway_wrangler.tera` | Gateway wrangler config. |
| `workers_workspace_cargo_toml.tera` | Workers workspace manifest. |

### 4.18 `seed/` — demo-data seeding

| Template | Purpose |
|----------|---------|
| `mod_rs.tera` | Seed module root. |
| `sink_rs.tera` | SeedSink impl writing through generated command handlers. |
| `cli_rs.tera` | Seed CLI (dry-run without a database). |

### 4.19 `shared/` — shared macros

| Template | Purpose |
|----------|---------|
| `filter_help.tera` | Macro listing filterable field names (help text). |

### 4.20 `test/` — entity tests

| Template | Purpose |
|----------|---------|
| `entity_test.tera` | Unit tests per entity. |
| `dto_test.tera` | Unit tests per DTO. |

### 4.21 `ui/` — SvelteKit web UI

| Template | Purpose |
|----------|---------|
| `list_page.tera` / `list_load.tera` | List page + server load (filters, pagination, workflow badges). |
| `detail_page.tera` / `detail_load.tera` | Detail page + load (child sections, workflow panel). |
| `edit_page.tera` / `edit_load.tera` | Edit page + load. |
| `form_page.tera` | Create/edit form page. |
| `entity_form.tera` | Generic entity form component (validation, defaults). |
| `cosmos_entity_form.tera` | Cosmos-variant entity form. |
| `entity_store.tera` | Svelte store per entity. |
| `descriptor.tera` | Entity descriptor (fields, hints, overrides). |
| `child_section.tera` | Child-collection accordion section on detail pages. |
| `domain_layout.tera` | Per-domain layout + nav. |
| `codelist_data.tera` | Codelist value modules for the UI. |
| `orgchart_page.tera` / `orgchart_load.tera` | Org-chart page + load. |
| `shell_list.tera` / `shell_create.tera` / `shell_edit.tera` / `shell_detail.tera` / `shell_detail_load.tera` | Generic descriptor-driven shell CRUD pages. |
| `scaffold/*` | App scaffold (see `ui_scaffold`): `package_json`, `vite_config`, `app_html`, `app_layout`, `app_css`, `app_d`, `app_guard_layout`, `dashboard`, `dashboard_page`, `login_page`, `signup_page`, `auth_callback`, `error_page`, `root_redirect`, `version_page`, `version_server`, `api_client`, `supabase_client`, `types`, `utils_ts`, `env_ts`, `hooks_server`, `hooks_reroute`, `entity_navigation_store`, `integrations_store`, `search_input`, `structured_wrapper_field`, `structured_wrapper_field_index`, `workflow_panel`, `messages_en`, `paraglide_settings`, `components_json`, `global_setup`, `global_teardown`, `playwright_config`, `persona_fixtures`, `test_helpers`, `settings_api_keys`, `settings_team`, `settings_integrations*` (list/detail/edit/install + servers), `settings_webhooks`, `settings_webhook_detail`, `settings_webhook_form`. |
| `test/*` | Playwright suites: `crud`, `validation`, `isolation`, `search`, `search_isolation`, `include`, `workflow`, `auth`, `employee_view`, `manager_team`, `owner_crud`, `webhooks_crud`, `webhooks_delivery`, `webhooks_advanced`, `_dep_setup` (shared dependency setup). |
| `webhook/*` | (see §4.22) |

### 4.22 `webhook/` — webhooks

| Template | Purpose |
|----------|---------|
| `api_endpoints.tera` | Endpoint/subscription CRUD handlers (SeaORM; JSONB-safe INSERT workaround). |
| `api_router.tera` | Webhook routes. |
| `api_endpoints_cornucopia.tera` | Cornucopia variant for workers topology. |
| `api_router_cornucopia.tera` | Cornucopia webhook router. |
| `dispatch.tera` | Monolith dispatcher (HMAC POST, retries, deactivation). |
| `dispatch_worker.tera` | Worker dispatcher (queue consumer / native loop). |

## 5. Configuration reference

### 5.1 `domains.toml` — bounded contexts, entities, behavior

Parsed into `DomainConfig` (`crates/codegraph-config/src/config.rs`).

Top-level tables:

| Key | Type | Description |
|-----|------|-------------|
| `[defaults]` | table | Global defaults for all entities (below). |
| `[domains.<name>]` | table | One bounded context / deploy boundary per entry. |
| `[namespaces."<fqn>"]` | table | Namespace declaration (issue #267). Quoted flat keys or nested unquoted tables normalize to the same dotted FQN. |
| `[rbac]` | table | Role hierarchy for DB-level role policies. |

`[defaults]` options:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `operations` | string[] | `["create","read","update","delete","list"]` | Enabled CRUD operations. |
| `auto_discover` | bool | `false` | Auto-discover entities from schema files for all domains. |
| `split_openapi_by_domain` | bool | `false` | Emit per-domain OpenAPI specs in addition to the unified spec. |
| `app_name` | string | `"codegraph-app"` | Application name used in generated scaffolding. |
| `max_bulk_size` | int | `100` | Max items per bulk-create request. |
| `type_suffix` | string | `"Type"` | Suffix stripped from schema titles (HR Open convention). |
| `types_import_prefix` | string | `"codegraph_type_contracts"` | Import prefix for structured wrapper types. |
| `generation_mode` | string | `"full"` | `full` \| `handler_only` \| `ddd_only` \| `none`. |
| `api_version` | string | `"v1"` | URL prefix (`/api/v1/...`). |

`[domains.<name>]` options (`DomainEntry`):

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `label` | string | required | Human label. |
| `schema_dir` | string | required | Schema directory relative to root. |
| `postgres_schema` | string | required | Postgres schema name for this domain's tables. |
| `depends_on` | string[] | `[]` | Domains this one depends on (cross-domain FK validation, service bindings fallback). |
| `entities` | string[] | `[]` | Explicit entity list (mox models are author-declarative — typically unset). |
| `entity_config` | table | `{}` | Per-entity `EntityConfig` (below). |
| `auto_discover` | bool | inherit | Auto-discover entities for this domain. |
| `exclude_entities` | string[] | `[]` | Force-exclude from auto-discovery (treated as value objects). |
| `force_entities` | string[] | `[]` | Override graph classification → entity. |
| `force_value_objects` | string[] | `[]` | Override graph classification → value object. |
| `exclude` | string[] | `[]` | Skip these types entirely. |
| `auditable` | bool | `true` for entity domains | Soft delete + audit columns. |
| `tier` | string | `"extended"` | Progressive disclosure tier: `core` \| `extended`. |
| `custom_routes` | bool | `false` | Entity-less domain delegating to a hand-written `handwritten_routes.rs` (router/worker scaffold only). |
| `worker_name` | string | `{app}-{domain}` | Cloudflare Worker name (workers topology). |
| `custom_domain` | string | gateway default `/{domain}/*` | Custom route pattern for the domain's worker. |
| `service_bindings` | string[] | `depends_on` | Other domain workers callable via Cloudflare service bindings. |
| `hyperdrive_binding` | string | `"HYPERDRIVE"` | Hyperdrive binding name. |
| `cron_triggers` | string[] | none | Cron expressions for the worker's scheduled handlers. |
| `remote_include_mode` | string | `"sql"` | How cross-domain `?include=` resolves: `sql` \| `http` (service bindings). |
| `webhooks` | bool | `false` | Webhook CRUD + dispatch + delivery on this domain's worker. |
| `queue_name` | string | `{app}-{domain}-webhooks` | Cloudflare Queue name for webhook deliveries. |
| `queue_binding` | string | `"WEBHOOK_QUEUE"` | Queue binding name. |
| `queue_max_retries` | int | `5` | Delivery attempts before endpoint auto-deactivation. |
| `queue_max_concurrency` | int | unset | Queues consumer `max_concurrency`. |
| `observability` | bool | `false` | Workers native observability (wrangler block, console panic hook, per-request metrics). |

`[domains.<name>.entity_config.<Entity>]` options (`EntityConfig`):

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `source_schema` | string | — | Schema path relative to the schema root. |
| `operations` | string[] | defaults | Enabled CRUD operations for this entity. |
| `parent_ref` | string | — | FK field defining URL nesting under a parent. |
| `path_segment` | string | auto-plural kebab | URL path segment override. |
| `tag` | string | — | utoipa tag for OpenAPI grouping. |
| `role` | string | — | `root` \| `child` \| `value_object`. |
| `append_only` | bool | `false` | Insert-only table (no UPDATE/DELETE grants or API ops; #284). |
| `parent` | string | — | Parent entity name (nesting). |
| `dto` | table | — | `DtoConfig` (below). |
| `workflow` | table | — | `WorkflowConfig` (below). |
| `workflow_file` | string | — | External workflow config path. |
| `search` | table | — | `SearchConfig` (below). |
| `filter_fields` | string[] | auto | `?filter[field]=` params; `[]` disables. |
| `max_bulk_size` | int | default | Bulk-create cap override. |
| `hierarchy_field` | string | — | Self-FK column → recursive CTE tree endpoints + parent FK index. |
| `tree_include` | table[] | — | `TreeIncludeConfig[]` (`via_entity`, `alias`) resolved into tree responses. |
| `has_orgchart` | bool | `false` | Generate an org-chart SvelteKit page. |
| `allow_include` | string[] | auto | Allowed `?include=` paths; `[]` disables. |
| `generation_mode` | string | default | `full` \| `handler_only` \| `ddd_only` \| `none`. |
| `errors` | table | `{}` | Custom error codes → `{description, http_status}`. |
| `public_operations` | string[] | — | Operations skipping permission checks. |
| `ui_detail_extensions` | string[] | `[]` | Consumer-owned Svelte components mounted on the detail page (#162). |
| `permissions` | table | — | `PermissionConfig` (below). |
| `api_key_scope` | string | module name | Entity part of the `{domain}.{entity}.{action}` API-key scope string. |

`dto` (`DtoConfig`): `immutable_fields`, `list_exclude`, `list_include`,
`expand_in_response` (string[]), `groups` (map of group → fields).

`search` (`SearchConfig`):

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `fts_columns` | string[] | auto (all TEXT) | tsvector columns; `[]` disables FTS. |
| `fts_weights` | map | `{}` | Per-column A/B/C/D weights. |
| `fts_language` | string | `"english"` | Postgres text-search config. |
| `fts_rest_mode` | string | `"query_param"` | `query_param` (`?q=`) \| `dedicated` (GET /search) \| `both`. |
| `embedding_columns` | string[] | `[]` | pgvector columns (opt-in only). |
| `embedding_dimensions` | int | `1536` | Vector dimensions. |

`workflow` (`WorkflowConfig`): `status_field` (required),
`approval_status_field`, `states`, `initial_state` (required),
`terminal_states`, `generate_action_endpoints` (default `false`),
`transitions` (from → [to]), `status_codelist` (CHECK constraint),
`approval_status_codelist`, `dual_status_guards` (status → required approval
status), `data_guards` (`{transition_to, rule, message}`), `timers`
(name → `{trigger_on_enter, type, duration_hours, target_state}`),
`approval_chains` (name → `{from, to, steps[]}`, step =
`{role, required=true, timeout_hours, auto_delegate}`).

`permissions` (`PermissionConfig`): `scope` (prefix, ops appended
`:create|:read|:update|:delete|:list`), `record_scoped` (bool, record-level
DID authorization), `min_roles` (op → minimum role, validated against the
hierarchy), `user_scope_column` (per-row user scoping column, #169).

`[rbac]` (`RbacConfig`): `roles_hierarchy` (string[], earlier = higher;
default `owner > manager > member > employee`).

`[namespaces."<fqn>"]`: `domain` (optional string assigning the namespace to
a bounded context; reserved at every level; unknown scalar keys are parse
errors).

Companion files (same parser module): `ui-overrides.toml`
(`overrides.<Type>.{detail, list-cell, form, inline}` → component paths) and
`ui-domains.toml` (`<domain>.<entity>.{wizard, wizard_config.steps}`).

### 5.2 `profiles.toml` — build plans

Parsed into `ProfilesConfig` (`crates/codegraph-generate/src/profile.rs`).
Each profile has `[meta]`, `[features]`, section tables (`[api]`, `[ui]`,
`[cli]`, …), optional `[ifml]` and `[variants.<name>]`.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `profiles.<p>.meta.name/version/description/tags/authors/since` | — | — | Profile metadata. |
| `profiles.<p>.<section>.generators` | string[] | — | Capability names to run for the section (validated against the registry). |
| `profiles.<p>.<section>.output` | string | `--output` | Output directory for the section. |
| `profiles.<p>.<section>.scripts.post_gen` | string[] | `[]` | Shell commands run after the section; non-zero aborts the run. |
| `profiles.<p>.ifml.frameworks` | table[] | `[]` | `{name, output, target}` IFML framework targets. |
| `profiles.<p>.variants.<v>` | table | — | Inline variant overriding sections/features/template pack (`--variant`). |

`[features]` flags (all optional; unknown enum values behavior noted):

| Feature | Type | Default | Description |
|---------|------|---------|-------------|
| `auth` | bool | — | Auth surface (API key + JWT middleware, login UI). |
| `pagination` | bool | — | Pagination on list endpoints. |
| `validation_level` | string | `"strict"` | Validation strictness (`strict`, `balanced`, …). |
| `offline_mode` | bool | — | Offline-capable generation. |
| `database_target` | string | `"postgres"` | `postgres` \| `sqlite` — selects the SqlDialect + template dir. |
| `persistence_provider` | string | `"sea_orm"` | `sea_orm` \| `cornucopia` (unknown values silently default). |
| `deployment_topology` | string | `"monolith"` | `monolith` \| `workers` (unknown values are a hard error; `workers` requires `cornucopia`). |
| `grpc_backend` | bool | `false` | Gates the 4 gRPC generators. |
| `ops_backend` | bool | `false` | Gates the `ops` generator. |
| `ifml_backend` | bool | `false` | Gates IFML generators (+ per-framework flags). |
| `ifml_design_system` | string | `""` | Built-in component pack (e.g. `"shadcn-svelte"`); unknown packs error strictly. |
| `atproto_backend` | bool | `false` | Gates the AT Protocol generator family. |
| `atproto_tenancy` | string | `"shared_pds"` | AT Protocol tenancy mode. |
| `atproto_float_policy` | string | `"integer_scaled"` | Float handling policy for lexicons. |
| `rosetta_backend` | bool | `false` | Gates rosetta generators (functions/rules/validations/reports). |
| `function_postconditions` | bool | `false` | Rosetta function postcondition codegen. |
| `rls_from_policy` | bool | `false` | Gates `policy_rls` (issue #219). |
| `namespace_layout` | bool | `false` | Namespace-aware module paths (#268); off = flat/byte-identical. |
| `fern_sdk` | bool | `false` | Gates `fern_config`. |
| `fern_sdk_languages` | string[] | `["typescript"]` | Fern SDK target languages. |
| `emdash_plugins` | bool | `false` | Gates EmDash plugin generators (requires plugins.toml). |
| `dto_key_casing` | string | `"snake"` | `snake` \| `camel` wire keys (unknown → snake). |
| `has_admin_cli` | bool | `false` | Admin CLI surface in scaffold. |
| `has_auth_rate_limit` | bool | `false` | Auth rate limiting in scaffold. |
| `has_labels` | bool | `false` | Gates `label_setup`. |
| `migration_strategy` | string | — | Migration strategy flavor for the scaffold. |
| `seed` section | table | — | Enables `seed_provision` demo-data generation. |
| `framework_{fw}` | bool | `false` | Per-IFML-framework flag (set automatically by ifml-only plans). |

### 5.3 `classifier.toml` — type classification

Parsed into `ClassifierConfig` (`crates/codegraph-classifier/src/config.rs`).
Drives JSON-Schema type classification (entity/VO/wrapper/codelist) and type
mapping. mox/rosetta models bypass it (author-declarative).

| Option | Type | Description |
|--------|------|-------------|
| `inline_enum_threshold` | int | Max enum values inlined before becoming a codelist reference. |
| `required_extensions` | string[] | Postgres extensions the model requires. |
| `primitive_wrappers` | map | Property name → `{postgres, rust, sea_orm}` type mapping. |
| `array_wrappers` | map | Array property → element type mapping. |
| `range_wrappers` | map | Range property → `{postgres, rust, open_end}` mapping (PG range types). |
| `composite_wrappers` | table[] | Multi-column wrappers: `{schema, columns[]}` (suffix, types, fk_table, dto type). |
| `composite_ranges` | table[] | Two-field ranges folded into one column: `{schema, start, end, column, types}`. |
| `structured_wrappers` | map | Structured wrapper (JSONB) property mappings. |
| `media_wrappers` | map | Media wrapper: columns + accepted MIME types. |
| `codelist_as_check` | table | Schemas whose codelists become CHECK constraints instead of enums. |
| `naming_rules` | map | Title-pattern scoring rules (`{score, rule_type}`) for classification. |

### 5.4 `extension-points.toml` — extension points

Parsed into `ExtensionPointsConfig` (`crates/codegraph-ext-points`).
Describes consumer extension surfaces the generators integrate with
(integrations plane, UI extension slots).

| Option | Type | Description |
|--------|------|-------------|
| `points.<name>.name` | string | Point identifier. |
| `points.<name>.description` | string | Human description. |
| `points.<name>.cardinality` | enum | How many extensions may attach. |
| `points.<name>.entities` | string[] | Entities the point attaches to. |
| `points.<name>.directions` | enum[] | Data-flow directions (inbound/outbound). |
| `points.<name>.config` | map | Declared config fields: `{field_type, required, label, options, default}`. |

### 5.5 `codegraph-ops.toml` — ops manifest

Parsed into `OpsManifest` (`crates/codegraph-config/src/ops_manifest.rs`),
consumed by the generated testkit (`codegraph-ops` harness). Values support
`{env:VAR}` indirection for passwords/keys.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `app_name` | string | — | Application name. |
| `graph_binary` | string | — | Codegen binary crate name (enables harness-driven regeneration). |
| `schemas_dir` | path | — | JSON schema dir (JSON pipeline runs). |
| `mox_files` | string[] | `[]` | `.mox` model files (mox-first; overrides schemas/classifier for generation). |
| `rosetta_files` | string[] | `[]` | `.rosetta` model files (rosetta-first). |
| `classifier` | path | — | classifier.toml path. |
| `domain_config` | path | — | domains.toml path. |
| `profile` | string | — | Profile name for regeneration. |
| `output_dir` | path | `"generated-app"` | Generation output directory. |
| `ui_dir` | path | `{output_dir}/ui` | UI dir override (monorepo sync). |
| `api_version` | string | `"v1"` | Route prefix. |
| `servers.api_port` / `ui_port` / `bind_addr` | int/int/string | `3000`/`5173`/`0.0.0.0` | Server ports/bind. |
| `database.api` / `.e2e` / `.e2e_app` | table | api required | Postgres targets: `{host, port, user, password, database, reset_sql, seed_sql, grant_role, grant_strict}`. |
| `supabase.dir` / `.health_url` / `.anon_key` / `.service_key` / `.jwt_secret` | — | — | Local Supabase stack (required for `e2e`). |
| `capabilities.has_cli` / `.has_ui` / `.has_admin_cli` / `.has_grpc` / `.database_target` / `.persistence_provider` | — | false…/`postgres`/`sea_orm` | Which suites/steps run (mirrors profile). |
| `hurl.dir` / `.skip` / `.org_id_a` / `.org_id_b` / `.limited_key` | — | `hurl`/`[]`/`…0001`/`…0002`/`false` | hurl contract-test config (+ read-only key for scope-denial tests). |
| `smoke.entity` / `.route` / `.create_body` | string | — | Entity exercised by the api suite's curl CRUD checks. |
| `hooks[]` | table[] | `[]` | `{name, exec, args, on}`; `on` ∈ pre_generate, post_generate, post_migrate, pre_e2e, post_e2e, pre_api, post_api, pre_playwright. Failures abort (except post_e2e). |
| `extensions[]` | table[] | `[]` | `{name, exec, requires_api, args}` — out-of-process test extensions. |

### 5.6 `ifml-components.toml` — IFML component mappings

Parsed into `IfmlComponentMappings`
(`crates/codegraph-config/src/ifml_components.rs`). Maps IFML elements to
handcrafted components; resolution priority **name → type → role → kind**,
optionally scoped by `view`. Project entries shadow design-system pack
entries at the same tier.

`[[component]]` options:

| Option | Type | Description |
|--------|------|-------------|
| `name` | string | IFML view component name (highest priority). |
| `type` | string | IFML component type (`list`, `form`, `details`, `chart`). |
| `kind` | string | Layout kind (`table`, `form`, `details`, `chart`, `list`). |
| `role` | string | Semantic role (closed enum, below). |
| `view` | string | Restrict mapping to one view container. |
| `path` | string (required) | Import path of the component. |
| `export` | string | Exported symbol (defaults to filename). |
| `testids` | map | Stable test-id overrides rendered on the component. |

`SemanticRole` (kebab-case, unknown = parse error): `action-control`,
`navigation-control`, `field`, `selection-field`, `collection`,
`modal-view`, `presentation-container`, `display`, `shell`, `pagination`.

Design-system packs: `--ifml-design-system shadcn-svelte` (or the
`ifml_design_system` feature) loads the built-in pack
(`crates/codegraph-config/src/packs/shadcn-svelte.toml`) covering all ten
roles; project mappings merge before pack entries.
