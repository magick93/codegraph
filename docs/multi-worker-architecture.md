# Multi-worker topology: architecture & configuration guide

Codegraph can generate the application as either:

1. **Monolith** (default) — one axum HTTP server binary that nests every
   bounded-context domain under a single `/api/v1/{domain}/{entity}` router,
   served from one shared Postgres.
2. **Workers** — one Cloudflare Worker per bounded-context domain plus a thin
   **gateway** worker, connected to the same shared Postgres through
   Hyperdrive.

This document is in two parts. **Part A** describes how the topology is
designed below the surface (for architects). **Part B** is a configuration
reference for codegraph users who want to run either scenario.

> Status note: this describes the current state (post "workers runtime gate",
> codegraph rev `28d6141`). The feature set is stable; minor generator/template
> details may evolve.

---

## Part A — Architecture

### A.1 Why split per domain

The monolith generator already treats each domain as a first-class seam:

- per-domain axum routers (`src/api/{domain}/router.rs`),
- per-domain DDD modules (`src/domain/{domain}/{entity}/…`),
- per-domain Postgres schemas, migrations, RLS, OpenAPI specs.

Splitting into one Worker per domain is therefore the least invasive
decomposition: each worker contains exactly the generated code its domain
already owns, plus a small runtime shell. The gateway preserves the monolith's
external URL contract (`/api/v1/{domain}/{path_segment}`), so the frontend and
API consumers are unchanged.

The decisive constraint is the **runtime**: Cloudflare Workers execute
`wasm32-unknown-unknown`. The monolith's SeaORM/sqlx persistence stack does
not compile to wasm32 (tokio `mio`/`net`), so the workers topology mandates the
**Cornucopia** persistence provider (SQL-first, typed queries over
`tokio-postgres`), whose `wasm-async` feature drives the Postgres wire protocol
over a Worker socket.

Feasibility evidence from the runtime gate: all domain workers and the gateway
compile via `worker-build`; typical bundle sizes are 1.7–4.6 MB (gateway
~0.5 MB), comfortably under the 10 MB paid limit.

### A.2 The deployment-topology metamodel

`crates/codegraph/src/profile.rs` defines:

```rust
pub enum DeploymentTopology {
    Monolith,   // default
    Workers,    // one Cloudflare Worker per domain + gateway
}
```

- Parsed from `profiles.toml` `[profiles.<name>.features]` key
  `deployment_topology`. Unknown values are a **hard configuration error**
  (unlike `PersistenceProvider`, which silently defaults to SeaORM).
- Stored on `BuildPlan.deployment_topology` and propagated to
  `ProjectConfig.deployment_topology` (a `String`, available to every Tera
  template as `project.deployment_topology`).
- Helpers: `ProjectConfig::is_workers_topology()` and
  `deployment_topology_enum()`; generators dispatch on the build plan first,
  falling back to the project config when no plan is provided.

**Enforced rule (topology × provider):** workers topology requires
`persistence_provider = "cornucopia"`:

```text
workers topology requires the cornucopia persistence provider
(deployment_topology = "workers" with persistence_provider = "sea_orm" is not supported)
```

This is validated at `BuildPlan` construction time.

### A.3 Generation: routing output per domain

`run_generators_with_opts` decides where every generator writes via
`generator_base(root, worker_base, routed)`.

- **Routed entity generators** (emitted into `workers/{domain}/`):
  `sea_orm_entity`, `cornucopia_repo`, `repository`, `command`, `query`,
  `event`, `dto`, `handler`, `workflow_action`, `media_route`.
- **Routed domain generators**: `errors`, `router`, `links`.
- **Root-anchored** (shared, unchanged): DDL migrations, UI, CLI, gRPC,
  playwright, domain-types, hooks, tests, `cornucopia_queries`
  (`queries/{domain}/{entity}.sql` feed the single shared
  `cornucopia-queries` codegen crate every worker depends on by path).

The dispatch rebuilds the entity/domain generator sets whenever the
generation-order domain changes, so each worker crate is constructed against
its own `workers/{domain}/` base. Global scaffolding switches too:

- Monolith: `ScaffoldGenerator` (root `main.rs`/`server.rs`/`Cargo.toml`).
- Workers: `WorkerScaffoldGenerator` (per-domain crates + gateway + workspace
  manifest; a root-level monolith `Cargo.toml`/`src/` would collide).

Post-passes are topology-aware: `generate_mod_files`/`prune_entity_mod` run per
worker `src/` (plus the root `src/` for root-anchored code), per-worker
codelist re-exports are emitted, and `clean_generated_output` cleans
`workers/{domain}/` instead of the root.

### A.4 The worker runtime

Each generated domain worker crate contains four entry points, feature-gated
`native` vs `cloudflare-worker`:

| Entry | Feature | Purpose |
|---|---|---|
| `#[event(fetch)]` | wasm | build the domain router + `AppState` from env, serve it |
| `#[event(scheduled)]` | wasm | workflow-timer sweep + webhook pgmq drain (cron) |
| `#[event(queue)]` | wasm | webhook delivery (Cloudflare Queue consumer) |
| `main.rs` (`axum::serve`) | native | offline/dev mode on `BIND_ADDR` |

**Client-generic persistence** (`scaffold/db_client.tera`) — the heart of wasm
compatibility. A `DbClient`/`DbTx` pair implements cornucopia's `GenericClient`
for both backends:

- native: `DeadpoolClientSource` (deadpool-postgres).
- wasm: `HyperdriveClientSource` — opens a per-request
  `worker::Socket` (`SecureTransport::StartTls`) + `connect_raw` with
  `worker::postgres_tls::PassthroughTls`, driving the connection future via
  `wasm_bindgen_futures::spawn_local`.

Transactions are explicit `BEGIN`/`COMMIT`/`ROLLBACK`, which keeps RLS session
vars (`set_config('app.organization_id'/'app.user_id'/'app.current_api_key',
..., true)`) scoped per operation.

**Workflow engine** (`codegraph-workflow`) is one engine, two clients:

- native: `SeaOrmWorkflowService` over a SeaORM `DatabaseTransaction`;
- wasm: `GenericWorkflowService<CornucopiaWorkflowClient>` over a per-request
  cornucopia client.

`WorkflowTx`/`WorkflowClient` abstract execute/query/commit/rollback, so the
state machine (definitions, instances, transitions, timers, approvals) is
identical on both sides. Timer sweep runs per domain from the `scheduled`
handler (`process_pending_timers(..., Some(domain), ...)`).

**Auth parity** (`scaffold/worker_middleware.tera`) mirrors the monolith:

- `sk_` API keys via `SELECT public.verify_api_key($1)`;
- HS256 via `SUPABASE_JWT_SECRET`; ES256 via JWKS fetched from
  `{SUPABASE_URL}/auth/v1/.well-known/jwks.json` with a 5-minute in-isolate
  TTL cache (workers-rs fetch, bridged into the Send future);
- org/role via `resolve_user_org($1)` and `basejump.account_user`;
- API-key usage logging best-effort.

**Webhooks** (opt-in per domain, `webhooks = true`): DB triggers keep writing
to pgmq; a `scheduled` handler drains `events_{domain}`, matches
subscriptions, inserts delivery rows, and **enqueues delivery jobs on a
Cloudflare Queue**; the `queue` consumer delivers via `worker::Fetch` with
HMAC signatures and queue retry policy. Native mode keeps the tokio
`WebhookDispatcher` loop, and a `POST /_dispatch` pump gives deterministic
tests.

**Observability** (opt-in, `observability = true`): generated wrangler.toml
carries `[observability] enabled = true`, and a hand-rolled wasm tracing
subscriber writes `tracing::info!/warn!/error!` to `console.log` (visible in
`wrangler tail`/Workers Logs) plus a per-request console metric line. No
hand-rolled OTLP.

### A.5 Gateway

`workers/gateway/` is a Worker that:

- forwards `/api/v1/{domain}/{*path}` to the matching domain worker via a
  service binding (`env.service(binding)`), preserving method/headers/query/
  body;
- exposes `/version`, `/health`, and `/health/all` (fan-out to every bound
  service);
- is generated with one `[[services]]` binding per domain.

workers-rs fetch futures are non-`Send`; the gateway bridges them into axum's
`Send`-future handlers via a oneshot + `spawn_local` pattern.

### A.6 Data plane & cross-domain

All workers share one Postgres through Hyperdrive, so:

- RLS policies, triggers, pgmq, pgvector, FTS, and cross-schema foreign keys
  are unchanged;
- cross-domain `?include=` resolution defaults to same-DB SQL joins
  (`remote_include_mode = "sql"`);
- `remote_include_mode = "http"` is the forward-looking option (service-binding
  client per domain pair) — implemented in the config surface, still pending
  the generated client wiring.

### A.7 Known limits

- **SeaORM-in-worker unsupported** (enforced by the topology×provider rule).
- **Cloudflare queues / real Hyperdrive require an account** for runtime
  testing: `wrangler dev` runs locally but queue delivery needs
  `wrangler dev --remote` or a deployed Worker; the native tokio loop is the
  offline path.
- **Cron granularity**: `scheduled` handlers run at 1-minute granularity
  (workflow timers, webhook drain).
- **Bundle budget**: watch the 10 MB limit per worker (domain-types + axum +
  the DMN/compliance engine are the largest contributors).

---

## Part B — Configuration guide

### B.1 Running the monolith (today)

A minimal `profiles.toml`:

```toml
[profiles.default.meta]
name = "default"
domain_types_base = "crates/<app>-domain-types"
hooks_api_base = "crates/<app>-hooks-api"

[profiles.default.features]
auth = true
persistence_provider = "sea_orm"   # or "cornucopia"
# deployment_topology is ABSENT -> Monolith

[profiles.default.api]
generators = [ "ddl", "sea_orm_entity", "dto", "repository", "command",
  "query", "event", "handler", "workflow_action", "errors", "router",
  "links", "openapi", "scaffold", "basejump_setup", "pgmq_setup",
  "platform_schema", "workflow_seed", "hook_registry", "report_views",
  "webhook_dispatch", "webhook_endpoint_api" ]
```

Generate with the ops harness (`codegraph-ops` testkit) or directly:

```bash
cargo run -p <graph-binary> -- run \
  --schemas schemas/ \
  --classifier classifier.toml \
  --config domains.toml \
  --profile default \
  --output generated-app/
```

### B.2 Switching to workers

Add a workers profile. This is the reference recipe (codegraph's own
`workers-cornucopia` profile):

```toml
[profiles.workers-cornucopia.meta]
name = "workers-cornucopia"
domain_types_base = "crates/<app>-domain-types"
hooks_api_base = "crates/<app>-hooks-api"

[profiles.workers-cornucopia.features]
auth = true
pagination = true
validation_level = "strict"
persistence_provider = "cornucopia"   # REQUIRED by workers topology
deployment_topology = "workers"

[profiles.workers-cornucopia.api]
generators = [
  "ddl", "cornucopia_queries", "cornucopia_repo", "dto", "repository",
  "command", "query", "event", "handler", "workflow_action",
  "lifecycle_trait", "domain_types_dto", "domain_types_query_service",
  "errors", "router", "links",
  "cornucopia_config", "worker_scaffold", "basejump_setup", "pgmq_setup",
  "platform_schema", "workflow_seed", "hook_registry",
  "domain_types_scaffold", "report_views", "openapi",
]
```

Points to notice:

- `deployment_topology = "workers"` + `persistence_provider = "cornucopia"`
  are both required; the generator set swaps the SeaORM generators for the
  cornucopia trio and `worker_scaffold`.
- `domains.toml` needs no changes to start; defaults apply (see B.3).

Generate with `--profile workers-cornucopia`; the output layout is described
in B.4.

### B.3 Per-domain configuration reference

All keys live on `[domains.<name>]` in `domains.toml`. They are optional and
only meaningful under the workers topology.

| Key | Type | Default | Effect |
|---|---|---|---|
| `worker_name` | string | `{app_name}-{domain}` | Deployed Worker name; drives `[lib]` name and wrangler `name` |
| `custom_domain` | string | — | Route pattern (e.g. `payroll.example.com/*`) → `[[routes]]`; otherwise reached only via the gateway `/{domain}/*` |
| `service_bindings` | list[string] | `depends_on` | Other domain workers callable via service bindings (`[[services]]`); binding name is the uppercase domain (e.g. `COMMON`) |
| `hyperdrive_binding` | string | `HYPERDRIVE` | Hyperdrive binding name in wrangler + `Env::hyperdrive(binding)` |
| `cron_triggers` | list[string] | — (+auto `*/1 * * * *`) | Cron expressions → `[triggers] crons`. Auto-added for workflow timers and webhook domains |
| `remote_include_mode` | string | `sql` | `sql` (same-DB joins) or `http` (service-binding clients, forward-looking) |
| `webhooks` | bool | `false` | Emit the webhook API + dispatch + queue consumer for this domain |
| `queue_name` | string | `{app_name}-{domain}-webhooks` | Cloudflare Queue name (producer + consumer) |
| `queue_binding` | string | `WEBHOOK_QUEUE` | Queue binding name |
| `queue_max_retries` | int | `5` | Queue `max_retries` + `MAX_RETRIES` in the dispatcher |
| `queue_max_concurrency` | int | — | Queue consumer `max_concurrency` (omitted when unset) |
| `observability` | bool | `false` | Emit `[observability] enabled = true` + console tracing |

Example:

```toml
[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["common", "compensation"]
worker_name = "hr-payroll"
service_bindings = ["common", "compensation"]
webhooks = true
observability = true
cron_triggers = ["*/5 * * * *"]

[domains.timecard]
label = "Timecard"
schema_dir = "timecard"
postgres_schema = "timecard"
depends_on = ["common", "payroll"]
```

### B.4 Generated artifacts (workers topology)

```
{output}/
├── migrations/                  # root-anchored: shared Postgres, applied once
├── queries/{domain}/{entity}.sql# cornucopia annotated queries (shared codegen)
├── cornucopia-queries/          # generated codegen crate (wasm-async feature)
├── ui/  cli/  src/              # root-anchored frontend / CLI / misc
├── codegraph-ops.toml  testkit/ # ops harness manifest + testkit crate
└── workers/
    ├── Cargo.toml               # workspace; members = domains + gateway
    ├── {domain}/
    │   ├── Cargo.toml           # cdylib+rlib, native/cloudflare-worker features
    │   ├── wrangler.toml
    │   └── src/
    │       ├── main.rs  lib.rs  worker.rs  app_state.rs
    │       ├── error.rs  middleware.rs  qs_query.rs  api/meta.rs
    │       ├── db_client.rs  workflow_client.rs      # cornucopia only
    │       ├── webhook_api.rs webhook_router.rs webhook_dispatch.rs # webhooks
    │       ├── hooks/mod.rs     # if hooks enabled
    │       └── domain/ api/ entity/ codelist/        # routed DDD code
    └── gateway/
        ├── Cargo.toml  wrangler.toml
        └── src/  main.rs  lib.rs  worker.rs
```

The per-domain `wrangler.toml`:

```toml
name = "{{ worker_name }}"
main = "build/worker/shim.mjs"
compatibility_date = "2024-12-01"

[build]
command = "cargo install worker-build && worker-build --release --no-default-features --features cloudflare-worker"

[vars]
SUPABASE_JWT_SECRET = ""
JWT_SECRET = ""
SUPABASE_URL = ""
DATABASE_URL = ""

[[hyperdrive]]
binding = "HYPERDRIVE"
id = "<REPLACE_WITH_HYPERDRIVE_ID>"
# localConnectionString = "postgres://user:pass@localhost:5432/<schema>"

# when webhooks = true:
[[queues.producers]]
binding = "WEBHOOK_QUEUE"
queue = "<app>-<domain>-webhooks"
[[queues.consumers]]
queue = "<app>-<domain>-webhooks"
max_batch_size = 10
max_retries = 5

# per service_bindings entry:
[[services]]
binding = "COMMON"
service = "<app>-common"

# when custom_domain set:
[[routes]]
pattern = "payroll.example.com/*"

# when observability = true:
[observability]
enabled = true
head_sampling_rate = 1

# when cron_triggers / workflow timers / webhooks:
[triggers]
crons = [ "*/1 * * * *" ]
```

### B.5 Deploying

Per domain worker (and the gateway):

```bash
cd workers/<domain>
npx wrangler hyperdrive create <domain>-hyperdrive \
  --connection-string="postgres://user:pass@HOST:5432/postgres"
# paste the returned id into wrangler.toml [[hyperdrive]] id
npx wrangler secret put SUPABASE_JWT_SECRET
npx wrangler secret put SUPABASE_URL
npx wrangler deploy
```

For webhook domains, create the queue first and name it exactly as in
`queue_name`. Custom domains (`custom_domain`) deploy via `[[routes]]`.

### B.6 Local development & testing

**Native mode** (no Cloudflare account): each worker crate's `native` feature
builds a plain binary (`BIND_ADDR`, default `0.0.0.0:3000`) using the
deadpool client — run the gateway + N workers locally against the shared
Postgres for offline tests.

**wrangler dev** with Hyperdrive: set
`CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_<BINDING>` or the generated
`localConnectionString` to the local Postgres, then `npx wrangler dev -c
workers/<domain>/wrangler.toml`. The generated `DATABASE_URL` var fallback also
works locally.

**Ops harness** (`codegraph-ops`): drive everything from a `codegraph-ops.toml`
manifest. Key points:

- All manifest paths resolve relative to the manifest's directory.
- `workspace_root` is auto-discovered as the **outermost** ancestor
  `Cargo.toml` containing `[workspace]` — so `cargo build -p <graph-binary>`
  and `target/` resolve to the consumer repo even when the manifest lives in
  the generated output dir (which is its own nested workspace).
- Subcommands: `api`, `cli`, `e2e`, `ui`, `full`, `clean`, `smoke`,
  `quality`, `ext <name>`.
- For `e2e`, set `[[supabase]]`, `database.e2e`, `database.e2e_app`,
  `ui_dir`, and hooks in the manifest. The SvelteKit production build is
  memory-hungry: export `NODE_OPTIONS="--max-old-space-size=8192"` (the
  harness no longer injects it).

### B.7 Gotchas

- **The `ops` generator overwrites `codegraph-ops.toml`** in the output dir
  with a minimal stub. Keep your hand-authored manifest outside generated
  output (e.g. repo-relative) and point the harness at it; extend the
  generated one via manifest `hooks`/`extensions`.
- **Hand-written pages calling custom `/api/*` routes** must use
  `PUBLIC_API_URL` (not a relative `fetch`, which hits the SvelteKit origin).
  See the compliance checker pages for the pattern:
  `fetch(`${env.PUBLIC_API_URL ?? 'http://localhost:3000'}/api/compliance/checks`)`.
- **Plural path segments**: generated routes nest the plural
  `path_segment` (`/compliance-checks`), not the singular entity name.
- **Cross-domain includes** currently use same-DB SQL; enable
  `remote_include_mode = "http"` only when the generated service-binding
  clients are in place.
- **Media/SeaORM-only subsystems** remain monolith-only; workers require the
  cornucopia provider end to end.
