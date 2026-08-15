# AGENTS.md

## Project structure

Workspace root `Cargo.toml` with 13 crates:

| Crate | Purpose |
|-------|---------|
| `codegraph` | Main binary: CLI, ingest, classify, validate, 59+ generators + project init/doctor/add-domain lifecycle |
| `codegraph-core` | Graph data model: `GraphQuerier`, `GraphIngestor`, node/edge types |
| `codegraph-grafeo` | Grafeo graph database adapter implementing core traits |
| `codegraph-backend` | Backend factory (currently Grafeo-only) |
| `codegraph-type-contracts` | Type system: PgType, RustType, DddFieldProjection |
| `codegraph-naming` | Identifier naming: snake_case, PascalCase, PG identifier handling |
| `codegraph-classifier` | Config-driven JSON schema type classification |
| `codegraph-config` | Domain config parsing (`domains.toml`, classifier.toml, profiles.toml) + `OpsManifest` |
| `codegraph-ext-points` | Extension points config types |
| `codegraph-workflow` | Generic state machine workflow engine (SeaORM) |
| `codegraph-ifml-dsl` | Pest-based IFML DSL parser + AST (see "IFML Integration" section) |
| `ast-ifml` | auto-lsp AST definitions for IFML |
| `codegraph-ops` | Rust test & deploy harness (see "Ops Harness" section) |

## IFML Integration (feat/ifml-integration branch)

### Overview

IFML (Interaction Flow Modeling Language) DSL integrated alongside JSON Schema as a
**complementary primary input**. JSON Schema defines the data model (entities/fields),
the IFML DSL defines the interaction model (views/navigation/events). Both feed into
the same Grafeo graph, linked by data binding edges.

### Architecture layers

| Layer | Location | Technology |
|-------|----------|------------|
| **DSL Parser** | `crates/codegraph-ifml-dsl/` | Pest (Rust PEG parser) |
| **AST types** | `crates/codegraph-ifml-dsl/src/ast.rs` | Serde-serializable AST |
| **Grammar** | `crates/codegraph-ifml-dsl/src/grammar/ifml.pest` | PEG grammar (13 rule categories) |
| **Graph model** | `crates/codegraph-core/src/types/ifml.rs` | 7 node types, 16 edge types |
| **Grafeo DDL** | `crates/codegraph-grafeo/src/schema_ddl.rs` | GQL CREATE statements |
| **Grafeo ingestor** | `crates/codegraph-grafeo/src/ingestor.rs` | GQL INSERT for IFML nodes |
| **Grafeo querier** | `crates/codegraph-grafeo/src/querier.rs` | GQL MATCH queries for IFML |
| **GraphIngestor trait** | `crates/codegraph-core/src/traits/ingestor.rs` | 6 IFML ingest methods |
| **GraphQuerier trait** | `crates/codegraph-core/src/traits/querier.rs` | 7 IFML query methods |
| **CachingQuerier** | `crates/codegraph-core/src/caching_querier.rs` | Delegates IFML queries |
| **Ingestion bridge** | `crates/codegraph/src/ingest/ifml_ingest.rs` | AST → GraphIngestor |
| **IfmlQuerier** | `crates/codegraph/src/generate/ifml/querier.rs` | High-level trait + impl |
| **Dependency sort** | `crates/codegraph/src/generate/ifml/dependency_graph.rs` | Kahn's algorithm |
| **Route generator** | `crates/codegraph/src/generate/ifml/route_generator.rs` | SvelteKit pages |
| **Nav generator** | `crates/codegraph/src/generate/ifml/navigation_generator.rs` | Route map |
| **Templates** | `crates/codegraph/templates/ifml/` | 6 Tera templates |
| **Profile caps** | `crates/codegraph/src/generate/ifml/profiles.rs` | ifml_backend feature |
| **LSP server** | `crates/codegraph/src/lsp/` | lsp-server crate, 5 tests |
| **CLI** | `crates/codegraph/src/cli.rs`, `main.rs` | `--ifml-files` flag, `lsp` cmd |

### IFML DSL syntax (C-like)

```ifml
domain "sales" { schema "sales"; }

view "CustomerList" {
    label "Customer Management";
    landmark: true;

    component "grid" {
        type: list;
        data: Customer;
        fields: [name, email, phone, status];

        on select(row) -> navigate("CustomerDetail", { customerId: row.id });
    }
}
```

### IFML node types (Grafeo graph)

| Node | Purpose |
|------|---------|
| `ViewContainer` | A screen/page with views, params, components |
| `ViewComponent` | A UI element (list, form, details) with data binding |
| `Event` | User or system event triggering navigation/actions |
| `Action` | Business logic invocation |
| `ParameterDefinition` | In/out/inout params on views |
| `DataBinding` | Connection to JSON Schema entities |
| `ModuleDefinition` | Reusable interaction pattern |

### IFML edge types

`ContainsViewContainer`, `ContainsViewComponent`, `HasEvent`, `NavigationFlow`,
`DataFlow`, `HasParameter`, `HasDataBinding`, `BindsToEntity`, `BindsToProperty`,
`TriggersAction`, `ActionEvent`, `HasModuleDefinition`, `HasConditionalExpr`

### IFML DSL to graph flow

```
.ifml file → Pest parser → AST → GraphIngestor (GQL INSERT) → Grafeo graph
                                                                    ↓
JSON Schema → SchemaLoader → GraphIngestor (GQL INSERT) → Grafeo graph
                                                                    ↓
                                              Generators read via GraphQuerier
                                              (IfmlGraphQuerier wraps it)
```

## gRPC Code Generation

### Overview

Four gRPC generators produce `.proto` files and tonic-based Rust server code alongside the existing REST API. JSON Schema drives the data model; gRPC generators read the same Grafeo graph as the REST generators.

### Generators

| Generator | Kind | Output |
|-----------|------|--------|
| `grpc_proto` | Entity | `proto/{domain}/{module}.proto` — messages + service definition |
| `grpc_service` | Entity | `src/api/grpc/{module}_grpc.rs` — tonic server impl + `From` conversions |
| `grpc_router` | Domain | `src/api/grpc/{domain}_router.rs` — service registration |
| `grpc_scaffold` | Global | `proto/shared.proto`, `src/api/grpc/mod.rs`, shared conversion helpers |

### Architecture layers

| Layer | Location | Notes |
|-------|----------|-------|
| **Type mapping** | `crates/codegraph/src/generate/grpc/proto_type.rs` | Maps `RefClassificationKind` → proto/tonic types. 34 unit tests |
| **Proto context** | `crates/codegraph/src/generate/grpc/proto_context.rs` | Queries graph, builds messages (entity + CRUD + search + tree + transition) |
| **Proto generator** | `crates/codegraph/src/generate/grpc/proto.rs` | `GrpcProtoGenerator` — renders `proto_message.tera` + `proto_service.tera` |
| **Service generator** | `crates/codegraph/src/generate/grpc/service.rs` | `GrpcServiceGenerator` — renders `server_impl.tera` + `conversions.tera` |
| **Router generator** | `crates/codegraph/src/generate/grpc/router.rs` | `GrpcRouterGenerator` — renders `domain_router.tera` |
| **Scaffold generator** | `crates/codegraph/src/generate/grpc/scaffold.rs` | `GrpcScaffoldGenerator` — shared proto + `mod.rs` + conversion helpers |
| **Templates** | `crates/codegraph/templates/grpc/` | 6 Tera templates (proto, service, shared, conversions, server impl, router) |
| **Build integration** | `crates/codegraph/templates/scaffold/build_rs.tera` | Conditional proto compilation via `tonic_build`. Generates both server AND client code |
| **Profile control** | `profiles.toml` | `grpc_backend = true` feature gates the 4 generators |

### Field numbering strategy

- `id` = field number 1
- Entity properties = sequential field numbers starting at 2
- `created_at` = 998, `updated_at` = 999 (synthetic timestamps)

### Codelist enum threshold

- `InlineEnum` → proto `enum`
- `CodelistReference` with ≤20 values → proto `enum`
- `CodelistReference` with >20 values → proto `string`

### Proto compilation

The generated `build.rs` walks the `proto/` directory tree and compiles all `.proto` files via `tonic_build`:

```rust
tonic_build::configure()
    .build_server(true)
    .build_client(true)
    .compile(&protos, &["proto"])
```

Setting `build_client(true)` causes tonic to auto-generate typed client structs (`{Entity}ServiceClient<T>`) — zero additional codegen needed.

### Dependency graph

```
ProtoContext (context builder)
    │
    ▼
proto_type_from_field() (type mapping)
    │
    ▼
GrpcProtoGenerator → .proto files (messages + service)
    │
    ▼
GrpcServiceGenerator → .rs files (server impl + conversions)
    │
    ▼
GrpcRouterGenerator → domain router (service registration)
    │
    ▼
GrpcScaffoldGenerator → shared.proto + mod.rs + convert.rs
    │
    ▼
ScaffoldGenerator integration → build.rs + Cargo.toml (has_grpc flag)
```

## Test Framework

A composable, output-type-agnostic test harness lives at `crates/codegraph/tests/test_framework/`.

### OutputValidator trait

```rust
pub trait OutputValidator: Send + Sync {
    fn name(&self) -> &str;
    fn validate(&self, files: &[GeneratedFile], work_dir: &Path) -> Result<(), Vec<String>>;
}
```

### Built-in validators

| Validator | Checks | Reusable for |
|-----------|--------|-------------|
| `SnapshotCollector` | Collects files into a map for manual assertion | All generators |
| `FilePresenceValidator` | Required files exist | All generators |
| `StringPatternValidator` | Content contains/avoids patterns | All generators |
| `ProtoCompileValidator` | `protoc` compilation (skipped if absent) | Proto output |

### Usage

```rust
#[path = "test_framework/mod.rs"]
mod test_framework;

let test = GeneratorTest {
    db: &engine,
    config: &config,
    tera: &tera,
    output_dir: temp_dir.path(),
    validators: vec![
        Box::new(FilePresenceValidator::new("proto_check", vec!["proto/recruiting/candidate.proto".into()])),
    ],
};
let files = test.run().expect("generation failed");
```

## VS Code Extension

### Location & Structure

```
codegraph-vscode/
├── package.json                    # Extension manifest
├── src/
│   ├── extension.ts                # Activation entry point
│   ├── commands/register.ts        # 4 commands
│   ├── lsp/client.ts               # LSP client (dynamic import)
│   ├── server-manager.ts           # Process lifecycle
│   ├── completion/providers.ts     # Completion provider
│   ├── status-bar.ts               # LSP status indicator
│   └── webview/
│       ├── panel.ts                # WebView panel manager
│       ├── parser.ts               # Lightweight JS IFML parser
│       └── sync.ts                 # Model types + sync protocol
├── webview/                        # SvelteFlow diagram app
│   ├── package.json                # Svelte, @xyflow/svelte, Vite
│   ├── vite.config.ts              # IIFE build → dist/webview/
│   └── src/
│       ├── App.svelte              # Main SvelteFlow canvas
│       ├── main.ts                 # mount(App, #root)
│       ├── types.ts                # IFML model types
│       ├── sync.ts                 # SyncClient (acquireVsCodeApi)
│       ├── nodes/                  # Custom node components
│       │   ├── ViewContainerNode.svelte
│       │   ├── ViewComponentNode.svelte
│       │   ├── EventNode.svelte
│       │   └── ActionNode.svelte
│       ├── edges/
│       │   ├── NavigationFlowEdge.svelte
│       │   └── DataFlowEdge.svelte
│       ├── palette/Palette.svelte  # Element toolbox
│       └── property-sheet/PropertySheet.svelte
├── grammar/                        # Tree-sitter grammar for IFML
│   ├── grammar.js                  # 54 grammar rules
│   └── queries/                    # SCSS queries
├── syntaxes/                       # TextMate grammar fallback
├── test/                           # VS Code extension tests
└── dist/webview/                   # Built SvelteFlow bundle
```

### Key VS Code extension facts

- **Import caveat**: `vscode-languageclient` uses dynamic `import()` to avoid
  `require()` failure in the packaged VSIX (which excludes `node_modules/`).
  `LspClient` is imported via `await import('./lsp/client')` in `extension.ts`.
- **CSP**: The WebView HTML uses `default-src 'none'; style-src <cspSource> 'unsafe-inline';
  script-src 'nonce-<nonce>' 'unsafe-eval'; img-src <cspSource> data:;`
- **Mount target**: `main.ts` mounts to `document.getElementById('root')!`.
- **Message flow**: WebView sends `sync/ready` on load → extension sends
  `sync/modelUpdate` with parsed IFML model.
- **SvelteFlow**: v1.5 uses named exports (`{ SvelteFlow }` not default).
  Requires `bind:nodes` / `bind:edges` for Svelte 5 two-way binding.
- **Vite build**: Uses `define: { 'process.env': {} }` to fix `process is not defined`
  error from `@xyflow/svelte` dependencies.
- **@xyflow/svelte**: ^1.5.2, Svelte 5.56.0, Vite 6

### Build & install

```bash
cd codegraph-vscode
npm run build:webview      # builds SvelteFlow → dist/webview/
npm run compile             # compiles TypeScript → out/
npx vsce package            # creates .vsix
code --install-extension codegraph-ifml-0.1.0.vsix --force
# Reload VS Code completely
```

### Testing

```bash
npm run test:compile        # compiles test files → out/test/
npx tsx test/run.ts         # runs VS Code extension tests
# Or from development path:
npx tsx test/run-vsix.ts    # tests against installed VSIX
```

Tests verify: extension activation, command registration, ifml language ID,
`.ifml` file recognition. 4 tests, all passing.

## VS Code Commands

| Keybinding | Command | When |
|------------|---------|------|
| `Ctrl+Shift+I` | `ifml.openDiagram` | Any editor (shows error if not .ifml) |

Commands: `ifml.openDiagram`, `ifml.validate`, `ifml.generate`, `ifml.refreshLsp`

## LSP Server

### Location

`crates/codegraph/src/lsp/` — Rust binary `codegraph lsp`

```bash
cargo run -- lsp --schemas schemas/ --classifier classifier.toml --config domains.toml
```

### Test coverage

- 5 LSP server tests (initialize, diagnostics, completions, notification)
- Tests use `lsp_server::Connection::memory()` + `tokio::spawn`

## Testing

```bash
# Rust tests
cargo test --workspace                    # all tests (635+)
cargo test -p codegraph-ifml-dsl          # 20 DSL parser tests
cargo test -p codegraph -- lsp            # 5 LSP server tests
cargo test -p codegraph --test ifml_e2e_tests  # 5 E2E tests
cargo test -p codegraph --lib -- ifml     # 6 dependency graph tests
cargo test -p codegraph --test init_tests # project lifecycle integration tests

# Dialect tests
cargo test -p codegraph --lib -- generate::db::dialect  # 12 dialect unit tests

# gRPC tests (all levels)
cargo test -p codegraph --lib -- grpc     # 34+ unit tests
cargo test -p codegraph --test grpc_snapshot_tests  # Level 2: Insta snapshots
cargo test -p codegraph --test grpc_compile_tests   # Level 3: protoc compilation

# Profile smoke tests (includes gRPC profile validation)
cargo test -p codegraph --test profile_smoke_tests

# Ops harness tests (codegraph-ops + ops generator)
cargo test -p codegraph-ops            # 93 harness tests (suites, proc, db, migrate, ext, metrics)
cargo test -p codegraph --test ops_generator_tests  # 7 tests + 1 ignored compile test (manifest + testkit emission, OpsConfig::load contract)
cargo clippy -p codegraph-ops --all-targets         # must be warning-free

# Ignored integration tests (run in CI's test-ops-integration job with a postgres:15 service)
cargo test -p codegraph-ops --test db_integration -- --ignored --nocapture   # needs DATABASE_URL (default postgres://postgres:postgres@localhost:5432/postgres)
cargo test -p codegraph --test ops_generator_tests -- --ignored --nocapture  # slow: compiles the emitted testkit crate

# Full pipeline integration (requires protoc)
cargo test -p codegraph --test grafeo_e2e_tests -- grafeo_all_entity_generators_produce_output_for_candidate

# VS Code extension tests
cd codegraph-vscode
npm run test:compile
npx tsx test/run.ts

# E2E pipeline
cargo run -- run --schemas /tmp/ifml-e2e/schemas \
  --classifier /tmp/ifml-e2e/classifier.toml \
  --config /tmp/ifml-e2e/domains.toml \
  --ifml-files /tmp/ifml-e2e/app.ifml \
  --output /tmp/ifml-e2e/output
```

## Pipeline commands

```bash
# Full pipeline: ingest + classify + generate
cargo run -- run --schemas <dir> --classifier classifier.toml \
  --config domains.toml --output <dir>

# With IFML DSL files
cargo run -- run --schemas <dir> --classifier classifier.toml \
  --config domains.toml --ifml-files app.ifml --output <dir>

# Classify only (show entity/VO decisions)
cargo run -- classify --schemas <dir> --classifier classifier.toml \
  --config domains.toml
```

## Project Initialization (init / doctor / add domain)

### Overview

`codegraph init [NAME]` scaffolds a consumer monorepo. `codegraph doctor`
validates the result, and `codegraph add domain <name>` grows it. The
scaffold is generated from 16 Tera templates in
`crates/codegraph/templates/project/` (see "Templates & context" below).

### Scaffolded file tree

| File | Purpose |
|------|---------|
| `Cargo.toml` | Workspace: members `{name}-graph` + `ops/testkit`; codegraph crates as `git+rev` deps (or `path` deps with `--codegraph-path`); `exclude = ["generated"]` |
| `{name}-graph/Cargo.toml`, `{name}-graph/src/main.rs` | Wrapper binary: clap `Run`/`Classify`/`Generate`/`Doctor` calling `codegraph::driver` |
| `domains.toml` | Example domain entry (`entities = ["ItemType"]` on the first domain) |
| `classifier.toml` | Classifier config seed |
| `profiles.toml` | Profile meta (`domain_types_base`) + feature flags (`ops_backend`, `grpc_backend`, `ifml_backend`, `database_target`, `persistence_provider`, `deployment_topology`) |
| `extension-points.toml` | Extension points config |
| `schemas/{domain}/example.json` | Example entity schema (`ItemType`) |
| `codegraph-ops.toml` | Seeded ops manifest (see "Ops Harness" section) |
| `ops/testkit/Cargo.toml`, `ops/testkit/src/main.rs` | Testkit workspace member |
| `hurl/health.hurl` | Health-check hurl file |
| `justfile` | Recipes: `generate`, `classify`, `doctor`, `api`, `e2e`, `full`, `clean` |
| `.gitignore` | Ignores `generated/` |
| `README.md` | Getting-started readme |
| `.github/workflows/ci.yml` | CI workflow |

### Layout decisions

- **Rev pinning**: `crates/codegraph/build.rs` embeds the checkout's git rev
  at build time (`cargo:rustc-env=CODEGRAPH_GIT_REV`, exposed via
  `codegraph::rev::codegraph_rev()`). `init` uses it as the default `rev` for
  the workspace's codegraph deps, so the scaffold pins the exact codegraph
  revision that generated it. `--rev <sha>` overrides.
- **`--codegraph-path <dir>`**: switches all codegraph deps to local path
  deps (`{dir}/crates/...`) instead of `git+rev`, for local development.
- **`generated/`**: all generator output lands there; it is excluded from the
  workspace and gitignored. The wrapper binary and config stay in the repo.
- **Safety**: init refuses to overwrite existing files unless `--force`, and
  a path containment guard keeps writes inside the project dir.

### Subcommand reference

#### `codegraph init [NAME]`

`NAME` prompts interactively when omitted.

| Flag | Default | Meaning |
|------|---------|---------|
| `--output <dir>` | `./{name}` | Parent dir to create the project in |
| `--domains a,b` | `common` | Comma-separated domain names |
| `--database-target` | `postgres` | DB dialect (`postgres`/`sqlite`) |
| `--persistence-provider` | `sea_orm` | `sea_orm`/`cornucopia` |
| `--deployment-topology` | `monolith` | `monolith`/`workers` |
| `--grpc`, `--ifml` | off | Enable gRPC / IFML features in `profiles.toml` |
| `--no-ops` | off | Disable the ops testkit |
| `--rev <sha>` | embedded rev | Codegraph git rev to pin |
| `--codegraph-path <dir>` | none | Path deps to a local codegraph checkout |
| `--force` | off | Overwrite existing files |
| `--template-dir <dir>` | repeatable | Additional template dirs (later take precedence) |

#### `codegraph doctor`

| Flag | Default | Checks |
|------|---------|--------|
| `--config` | `domains.toml` | domains.toml parses |
| `--schemas` | `schemas` | schemas dir contains JSON schema(s) |
| `--classifier` | `classifier.toml` | classifier.toml parses |
| `--profiles-config` | optional | profiles.toml parses + BuildPlan capability validation |

Hard failures (non-zero exit): domains.toml, classifier.toml, profiles.toml,
schemas dir, codegraph-ops.toml (`OpsConfig::load`). Warnings only: missing
profiles.toml / codegraph-ops.toml, Cargo.toml rev pins vs the binary's
embedded rev (mismatch WARN), missing `psql`/`npx`/`hurl` tools.

#### `codegraph add domain <name>`

Appends a `[domains.<name>]` entry (label, schema_dir, postgres_schema) to
`domains.toml` and creates `schemas/<name>/example.json`. Rejects duplicate
domain names.

### Lifecycle walkthrough

```bash
codegraph init my-app                       # scaffold (add --codegraph-path ~/git/codegraph for local dev)
cd my-app
just doctor                                 # validates config + toolchain
just generate                               # wrapper run -> generated/ (excluded from workspace)
just api                                    # ops testkit api suite (preflight, migrate, hurl, curl, RLS)
just e2e                                    # ops testkit e2e (Supabase -> generate -> migrate -> build -> Playwright)
just full                                   # api then e2e
```

Note: a freshly scaffolded minimal project's generated output does not yet
compile out of the box — pre-existing generator assumptions about
hooks/codelists/error modules (tracked in issue #71). Consumers with richer
schemas (hr-specs style) are unaffected.

### Templates & context

Project templates live in `crates/codegraph/templates/project/` (16
templates, shadowable via `--template-dir`). The render context is
`ProjectTemplateContext` in `crates/codegraph/src/init/context.rs`, and the
canonical (template, output-path) list is `PROJECT_TEMPLATES` in the same
file; output paths support `{graph}` and `{domain}` placeholders.

Adding a new template:

1. Add `project/<name>.tera` under `crates/codegraph/templates/project/`.
2. Append its `(template, output)` pair to `PROJECT_TEMPLATES` in
   `crates/codegraph/src/init/context.rs`.
3. Add any new fields to `ProjectTemplateContext` (it serializes into the
   Tera context).
4. Update the `init` tests / `file_tree()` expectations if the layout changed.

## Template Overrides

### The `--template-dir` flag

Available on both `generate` and `run` commands. May be specified multiple times; later directories take precedence.

```
Paths to additional template directories. Templates in these directories
shadow codegraph's built-in templates by name. May be specified multiple
times; later directories take precedence.
```

### How template shadowing works

Implemented in `crates/codegraph/src/generate/template_engine.rs`:

1. **`create_tera_with_overrides()`** at line 30 loads all built-in templates from `crates/codegraph/templates/` first
2. It then iterates override directories in order, calling `merge_tera_dir()` for each
3. **`merge_tera_dir()`** at line 45 walks each directory, reading `.tera` files and registering them by their relative path name
4. A template with the same relative path from a later directory **shadows** the earlier one — no merging, full replacement

### Available Tera custom filters

| Filter | Description |
|--------|-------------|
| `snake_case` | Converts a string to `snake_case` |
| `upper_camel` | Converts to UpperCamelCase (strips trailing `Type` suffix first) |
| `pascal_case` | Converts to PascalCase |
| `kebab_case` | Converts to `kebab-case` |
| `pluralize` | Pluralizes a word (simple rules: `s`/`es`/`ies`) |
| `truncate_pg` | Truncates to PostgreSQL max identifier length (63 chars) |
| `dollar_quote` | Wraps a string in single quotes with proper escaping |
| `strip_pg_quotes` | Removes double-quote characters from PostgreSQL identifiers |
| `quote_pg` | Double-quotes a PostgreSQL identifier if it is a reserved word |

### Example: overriding SQLite templates

```bash
# Override the SQLite table template with a custom version
cargo run -- run --schemas schemas/ --classifier classifier.toml \
  --config domains.toml --output out/ \
  --template-dir ./my-overrides/

# Multiple override directories; later ones win
cargo run -- generate --config domains.toml --output out/ \
  --template-dir ./team-templates/ --template-dir ./local-tweaks/
```

Place a `.tera` file at the matching relative path to shadow it. For example, `my-overrides/db/sqlite/table.tera` shadows `crates/codegraph/templates/db/sqlite/table.tera`.

## Database Dialect Support (feat/sqlite-support)

### Overview

Codegraph supports configurable database target dialects via the `SqlDialect`
trait. Currently two dialects are implemented:

| Dialect | `database_target` value | Key features |
|---------|------------------------|--------------|
| PostgreSQL | `"postgres"` (default) | UUID, JSONB, TIMESTAMPTZ, RLS, extensions, PL/pgSQL, schemas |
| SQLite | `"sqlite"` | TEXT, INTEGER, no RLS, inline triggers, FTS5, STRICT tables |

### Architecture

```
profiles.toml                           database_target from features
    │                                           │
    ▼                                           ▼
BuildPlan                             ───►   ProjectConfig.database_target
    │                                           │
    ▼                                           ▼
DB Generators (ddl, entity, etc.)     ───►   SqlDialect trait
    │                                           │
    ▼                                           ▼
Tera templates                              {{ project.database_target }}
templates/db/sqlite/*.tera               (available in all template contexts)
```

### SqlDialect trait

Defined at `crates/codegraph/src/generate/db/dialect.rs`:

- **30 methods** covering: type mapping, default expressions, feature flags,
  identifier handling, trigger syntax, FTS engine selection
- `DatabaseTarget` enum: `Postgres`, `Sqlite` (default: `Postgres`)
- Factory: `dialect_for_target(DatabaseTarget)` returns `Box<dyn SqlDialect>`
- 12 unit tests

### Profile configuration

```toml
[profiles.default.features]
database_target = "sqlite"     # default is "postgres"
```

The `database_target` value is parsed from the `[features]` table in
`profiles.toml` and stored in `BuildPlan.database_target`. It's propagated
to all templates via `ProjectConfig.database_target`.

### SQLite templates

Located at `crates/codegraph/templates/db/sqlite/`:

| Template | Purpose |
|----------|---------|
| `table.tera` | CREATE TABLE with STRICT mode, TEXT types |
| `entity.tera` | SeaORM entity without `schema_name` attribute |
| `trigger.tera` | Inline CREATE TRIGGER (no PL/pgSQL) |
| `fts.tera` | FTS5 virtual table with sync triggers |
| `codelist.tera` | INSERT OR IGNORE for idempotent seed |
| `rls.tera` | Placeholder (SQLite has no RLS) |
| `domain_event_trigger.tera` | Simple event table insert (replaces pgmq) |

Generators select the template directory based on the dialect. The existing
`templates/db/` templates remain the PostgreSQL originals and are untouched.

### Adding a new dialect

1. Add a variant to `DatabaseTarget` in `dialect.rs`
2. Implement `SqlDialect` for the new target
3. Add templates under `templates/db/<target>/`
4. Register the dialect in `dialect_for_target()`
5. Unit tests in `dialect.rs` `#[cfg(test)]` block

## Ops Harness (codegraph-ops + `ops` generator)

### Overview

`crates/codegraph-ops` is a Rust test & deploy harness for codegraph-generated
apps — a re-imagining of the hand-written bash suite (`test.sh`,
`lib/common.sh`, `lib/migrate.sh`, `deploy/smoke-test.sh`,
`scripts/quality-check.sh`) that hr-specs used to maintain. It is
configuration-driven and extension-pluggable so every codegraph consumer
shares the same harness while keeping their project-specifics (Xero/Stripe/IRD
integrations, UI-sync rsync steps, integration migrations) as manifest hooks
and extensions.

### Architecture layers

| Layer | Location | Notes |
|-------|----------|-------|
| **Manifest types** | `crates/codegraph-config/src/ops_manifest.rs` | `OpsManifest` (serde TOML): app name, servers/ports, db targets, supabase, capabilities, hurl, hooks, extensions, smoke entity |
| **Harness crate** | `crates/codegraph-ops/` | Runtime: `cli.rs` (clap), `config.rs` (`OpsConfig` resolution), `proc.rs` (SIGTERM→SIGKILL supervision, `Supervisor`), `db.rs` (psql wrapper, extension validation), `migrate.rs` (phased migrations, supabase symlinks), `suites/*` (api, cli, ui, e2e, smoke, quality), `ext.rs` (extension protocol + hooks), `metrics.rs` (stage TSV export), `wait.rs`, `env.rs`, `pg.rs` (`PgTarget`) |
| **Generator** | `crates/codegraph/src/generate/ops.rs` | Global generator `ops` — emits `codegraph-ops.toml` + `testkit/` crate into generated output |
| **Templates** | `crates/codegraph/templates/ops/` | `testkit_cargo.tera`, `testkit_main.tera` (shadowable via `--template-dir`) |
| **Profile gating** | `profiles.toml` + `profile.rs` | `ops_backend` feature; `cap("ops", Global, Common, &["ops_backend"], &[])` |
| **Contract test** | `crates/codegraph/tests/ops_generator_tests.rs` | Emitted manifest must parse via `OpsConfig::load` (cross-crate) |

### Subcommands (run via the generated testkit binary)

```
cargo run -p testkit -- api        # preflight, migrate, hurl, curl smoke, RLS, shutdown
cargo run -p testkit -- cli        # CLI e2e (starts API first)
cargo run -p testkit -- e2e        # Supabase → generate → migrate → build → Playwright
cargo run -p testkit -- ui         # Playwright only (API must be running)
cargo run -p testkit -- full       # api then e2e
cargo run -p testkit -- smoke      # remote deployment smoke test
cargo run -p testkit -- quality    # cargo test/clippy/fmt + generate + check
cargo run -p testkit -- clean      # stop services, remove generated output
cargo run -p testkit -- ext <name> # run a test extension
cargo run -p testkit -- ext --list # list registered extensions
```

Global flags: `--config FILE` (manifest path), `--keep`, `--skip-build`,
`--skip-generate`, `--release`, `--verbose`, `--metrics FILE` (stage timings;
TSV or JSON via `--metrics-format tsv|json`, default tsv), `--retry N`
(retry failed hurl files in the api suite up to N times, default 0),
`--headed`, `--grep PATTERN` (repeatable).

When `--config` is absent the manifest is auto-discovered: walk UP from the
cwd looking for `codegraph-ops.toml`, then walk UP from the testkit
executable's directory (cli.rs `find_manifest()`).

Failures print a `hint:` line when the error type has one (missing tools,
config mistakes, port conflicts — `error::hint()` in `cli.rs`).

Manifest values support `{env:VAR}` indirection for `database.*.password`
and `supabase.anon_key`/`service_key`/`jwt_secret` (`config.rs`
`resolve_env()`): unset variables expand to empty; plain strings pass
through unchanged.

`smoke` flags: `--api-url`, `--web-url`, `--expected-commit`,
`--auth-health-url`, `--worker URL` (repeatable for worker pings).

`quality` accepts extra cargo gate names (e.g. `doc`) as trailing args.

### Extension protocol

- `ext::TestExtension` trait: `name()`, `requires_api_running()`,
  `run(&OpsContext)` — consumers register via `register_extension()` in the
  generated testkit `main.rs` (see `templates/ops/testkit_main.tera`).
- Manifest `[[extensions]]` entries with `exec` run out-of-process via `sh -c`
  (language-agnostic; how hr-specs' Xero/Stripe/IRD-style integrations plug in).
- Manifest `[[hooks]]` entries run at pipeline points: `pre_generate`,
  `post_generate`, `post_migrate`, `pre_e2e`, `post_e2e`, `pre_api`,
  `post_api`, `pre_playwright`.

### Manifest (`codegraph-ops.toml`)

Seeded by the generator from `ProjectConfig`/`BuildPlan` (app name, ports,
db targets, capabilities). Consumers extend: `database.*.reset_sql`/`seed_sql`,
`supabase` dir + keys, `hurl.dir`/`skip`/org ids, `smoke.entity` (entity used
for the api suite's curl CRUD checks), `ui_dir` override (for monorepo sync
setups), hooks, extensions.

### Consumer integration guide

1. Add `codegraph-ops` to the consumer workspace deps:
   ```toml
   codegraph-ops = { git = "https://github.com/magick93/codegraph.git", rev = "<pinned>" }
   ```
   Pin the same rev as the other codegraph crates. During development use
   `branch = "<branch>"` plus a `[patch."https://github.com/magick93/codegraph.git"]`
   entry pointing `codegraph-ops` at a local path.
2. Enable the generator: `ops_backend = true` under `[features]` in the
   consumer's `profiles.toml` plus `"ops"` in the profile's generator list;
   regenerate. This emits `codegraph-ops.toml` + a `testkit/` crate.
   `smoke.entity` is auto-seeded from the first entity in generation order
   (`generate/ops.rs`); the codegraph binary stamps `project.codegraph_rev`
   from its own git rev (`main.rs`) so the testkit `Cargo.toml` pins the same
   rev (see `templates/scaffold/cargo_toml.tera`, `templates/ops/testkit_cargo.tera`).
3. Edit the manifest: `database.api`/`database.e2e`/`database.e2e_app` targets
   (+ `reset_sql`/`seed_sql`), `supabase` dir + keys, `hurl.dir`/`skip`/org
   ids, `ui_dir` override (monorepo sync setups), and — for e2e generation —
   `graph_binary` + `schemas_dir` + `classifier` + `domain_config`.
4. Run: `cargo run -p testkit -- api` / `e2e` / `full` / `smoke` / `quality` /
   `ext <name>`.
5. Add project-specifics as `[[hooks]]` and `[[extensions]]` (exec-based
   out-of-process entries) or trait-based extensions (register in the
   `testkit_main.tera` registration hook — `register_extension()` before the
   tokio runtime starts). hr-specs dogfooding: pgmq patch as `pre_e2e`,
   crewbase rsync as `pre_playwright`, hr-reports views as `post_migrate`; an
   extensions crate implements `TestExtension` for ird/stripe/xero and the
   testkit workspace member registers them; a justfile delegates to the
   harness; the bash suite is deleted.

   Hook points and when they fire:

   | Hook | Suite | Fires |
   |------|-------|-------|
   | `pre_generate` | e2e | Before the graph binary build/generation |
   | `post_generate` | e2e | After generation |
   | `pre_e2e` | e2e | After Supabase start, BEFORE migration symlink + `supabase db reset` |
   | `post_migrate` | api + e2e | After DB reset/migration (api only when migrate=true) |
   | `pre_playwright` | e2e | After the API is up, BEFORE the SvelteKit production build |
   | `post_e2e` | e2e | Every e2e path (success and failure), best-effort |
   | `pre_api` / `post_api` | api | Around the api suite |

   Each hook is `sh -c "{exec} {args...}"` in the repo root; failures abort
   the suite (except `post_e2e`, which warns).
6. CI wiring: run `cargo run -p testkit -- api --metrics ci.tsv` in a job with
   a Postgres service. The codegraph repo's own `test-ops-integration` job
   (postgres:15 service + the `--ignored` integration tests in `.github/workflows/ci.yml`,
   CI triggers on push to `develop` and `master`) is the reference pattern.

### Platform support & prerequisites

Unix-first (Linux/macOS): process supervision is SIGTERM → grace → SIGKILL
(`proc.rs`), `clean` kills ports via `fuser -k`, and consumer hooks commonly
use rsync. Windows is not supported by the process-supervision/port-kill
paths.

Tool prerequisites (validated per-suite; missing tools error or skip):

- `psql` (db access + extension checks), `curl` (health probes, api suite)
- `hurl` (api contract tests — skipped if absent)
- `npx` + Supabase CLI + Docker (e2e Supabase stack)
- `pnpm` + Playwright chromium (ui/e2e; installed via `playwright install chromium`)

### Adding a feature to the harness

1. Module in `crates/codegraph-ops/src/` (or a new suite in `suites/`).
2. Wire the subcommand in `cli.rs` + flag plumbing.
3. Unit tests alongside; `cargo test -p codegraph-ops`.
4. If the generated manifest needs new seed values, extend
   `OpsManifest` in `codegraph-config` + the generator in `generate/ops.rs`.

## Persistence Provider System

### Overview

Codegraph supports swappable persistence backends via the `PersistenceProvider`
enum. The DDL generator is always provider-agnostic (it generates SQL, not
ORM code). The entity model and repository implementation are provider-specific
and selected by the `persistence_provider` feature flag.

### Three-layer architecture

```
                         JSON Schema + Policies
                                 │
                                 ▼
                     GraphQuerier (Grafeo graph)
                                 │
                                 ▼
                     build_persistence_entity()
                                 │
                                 ▼
                     PersistenceEntity (IR)        ← ORM-agnostic
                                 │
                    ┌────────────┼────────────┐
                    ▼            ▼            ▼
              SeaOrmBackend  Cornucopia    (future:
              (entity.tera,  Backend       Diesel,
               repo emitter) (.sql files,  SQLx, ...)
                              cornucopia.toml)
```

### PersistenceProvider enum

Defined at `crates/codegraph/src/profile.rs:15`:

| Variant | Config value | Entity model | Repository | Query layer |
|---------|-------------|--------------|------------|-------------|
| `SeaOrm` | `"sea_orm"` (default) | `sea_orm_entity` generator → `#[derive(DeriveEntityModel)]` structs | `repository` + `repository_emitter` → SeaORM ActiveModel/QueryBuilder | `query` generator → SeaORM `EntityTrait::find()` |
| `Cornucopia` | `"cornucopia"` | `cornucopia_queries` generator → `queries/{domain}/{entity}.sql` annotated SQL files | `cornucopia_repo` generator → wrapper around Cornucopia query functions | Cornucopia-generated typed query structs via `bind()/all()/one()/opt()` |

### Profile configuration

```toml
[profiles.default.features]
persistence_provider = "sea_orm"   # "sea_orm" (default) | "cornucopia"

# SeaORM profile — existing generators
[profiles.default.api]
generators = ["ddl", "sea_orm_entity", "dto", "repository", "command", "query", ...]

# Cornucopia profile — alternative generators
[profiles.cornucopia.api]
generators = ["ddl", "cornucopia_queries", "cornucopia_repo", "cornucopia_config", "dto", ...]
```

The `persistence_provider` value is parsed from `[features]` into
`BuildPlan.persistence_provider` and propagated to generators via
`ProjectConfig.persistence_provider` (available in all Tera templates).

### PersistenceEntity IR

Defined at `crates/codegraph-core/src/types/persistence.rs`:

| Type | Purpose |
|------|---------|
| `PersistenceEntity` | Top-level ORM-agnostic model: title, table_name, schema_name, rust_type_name, columns, child_tables, relations, policies |
| `PersistenceColumn` | Column descriptor: field_name, column_name, rust_type, pg_type, is_primary_key, is_nullable, is_jsonb, is_range, pg_cast, role |
| `PersistenceColumnRole` | Semantic role: Data, PrimaryKey, TenantScope, SoftDeleteMarker, AuditTimestamp, AuditUser, AuditFlag, ForeignKey, HierarchyParent |
| `PersistenceChildTable` | Child table from a ValueObject property: table_name, struct_name, parent_fk, columns |
| `PersistenceEntityRelation` | Relationship: name, relation_type, related_entity, from/to_column, is_self_ref |
| `PersistencePolicies` | Policy effects translated to ORM-agnostic form: SoftDeleteEffect, TenantIsolationEffect, RowSecurityEffect, AuditEffect, RetentionEffect |

### build_persistence_entity() builder

Defined at `crates/codegraph/src/generate/persistence.rs` — the single source
of truth for extracting entity structure + policies from the graph. Both
`SeaOrmEntityGenerator` and `CornucopiaQueryGenerator` call it.

### Key files

| File | Role |
|------|------|
| `crates/codegraph-core/src/types/persistence.rs` | IR types: PersistenceEntity, PersistenceColumn, policy effects |
| `crates/codegraph/src/profile.rs` | `PersistenceProvider` enum + `from_config()` + `BuildPlan` field |
| `crates/codegraph/src/generate/persistence.rs` | `build_persistence_entity()` — graph → IR builder |
| `crates/codegraph/src/generate/db/entity.rs` | `SeaOrmEntityGenerator` — SeaORM model emission (existing, unchanged) |
| `crates/codegraph/src/generate/ddd/repository_emitter.rs` | SeaORM repository impl emitter (existing, unchanged) |
| `crates/codegraph/src/generate/db/cornucopia_queries.rs` | `CornucopiaQueryGenerator` — annotated SQL file generation |
| `crates/codegraph/src/generate/db/cornucopia_config.rs` | `CornucopiaConfigGenerator` — `cornucopia.toml` with type mappings |
| `crates/codegraph/src/generate/ddd/cornucopia_repo.rs` | `CornucopiaRepoGenerator` — repository adapter wrapper |
| `crates/codegraph/src/generate/mod.rs` | `ProjectConfig.persistence_provider` + generator dispatch |
| `profiles.toml` | `persistence_provider` feature flag |

### Policy-aware query generation

Both SeaORM and Cornucopia backends consume the same `PersistencePolicies`
struct built from `PolicyNode` graph data. Policy effects drive:

| Policy | SeaORM effect | Cornucopia SQL effect |
|--------|--------------|----------------------|
| `SoftDelete` | `Entity::active()` / `including_deleted()` scopes | `WHERE deleted_at IS NULL` in SELECT, `UPDATE SET deleted_at = NOW()` for delete |
| `TenantIsolation` | `platform_organization_id` FK column + RLS session vars | `WHERE tenant_column = :tenant_id` on every query |
| `RowSecurity` | RLS template with `RowSecurityPolicy` | Inline `USING`/`CHECK` expressions (future) |
| `Audit` | `created_at`, `updated_at`, `updated_by`, `deleted_by` columns | Same columns in RETURNING clauses + trigger queries |
| `Retention` | Column + archive strategy | Time-partitioned WHERE clauses (future) |

### Adding a new persistence provider

1. Add a variant to `PersistenceProvider` in `profile.rs`
2. Create generators that consume `build_persistence_entity()` and emit
   provider-specific output (e.g. `diesel_entity.rs`, `sqlx_repo.rs`)
3. Register generators in `generate/mod.rs` entity/generator vecs
4. Add capability entries in `profile.rs` `base_capabilities()`
5. Add `persistence_provider` entry in `profiles.toml` features
6. Unit tests + snapshot tests for the new output format

## Deployment Topology System

### Overview

`DeploymentTopology` selects the shape of the generated backend: today's
single-crate axum server (`Monolith`, default) or one Cloudflare Worker per
bounded-context domain behind a gateway (`Workers`). Configuration plumbing
only for now — generator output behavior is unchanged.

### DeploymentTopology enum

Defined at `crates/codegraph/src/profile.rs` (next to `PersistenceProvider`):

| Variant | Config value | Backend shape |
|---------|-------------|---------------|
| `Monolith` | `"monolith"` (default) | Single-crate axum server (today's behavior) |
| `Workers` | `"workers"` | One Cloudflare Worker per domain + gateway |

### Profile configuration

```toml
[profiles.default.features]
deployment_topology = "monolith"   # "monolith" (default) | "workers"
```

Unlike `persistence_provider` (which silently defaults on unknown values),
unknown `deployment_topology` values are a hard configuration error in
`BuildPlan::from_profile()`. The value is stored on `BuildPlan` and propagated
to generators via `ProjectConfig.deployment_topology` (available in Tera
templates as `project.deployment_topology`).

### Per-domain worker config (domains.toml)

All keys on `DomainEntry` in `crates/codegraph-config/src/config.rs` are
optional (`#[serde(default)]`), so existing domains.toml files parse unchanged:

| Key | Type | Default | Semantics |
|-----|------|---------|-----------|
| `worker_name` | `Option<String>` | `{app_name}-{domain}` (via `worker_name_or()`) | Cloudflare Worker name for this domain |
| `custom_domain` | `Option<String>` | None (gateway default route `/{domain}/*`) | Custom domain / route pattern |
| `service_bindings` | `Option<Vec<String>>` | `depends_on` (via `service_bindings_or_depends()`) | Other domain workers this worker can call |
| `hyperdrive_binding` | `Option<String>` | `"HYPERDRIVE"` (via `hyperdrive_binding_or()`) | Hyperdrive binding name |
| `cron_triggers` | `Option<Vec<String>>` | None | Cron expressions for scheduled handlers |
| `remote_include_mode` | `Option<String>` | `"sql"` (via `remote_include_mode_or()`) | `"sql"` or `"http"` — how cross-domain `include` queries are satisfied |

Convenience accessors on `DomainEntry`: `worker_name_or(default)`,
`service_bindings_or_depends()`, `hyperdrive_binding_or(default)`,
`remote_include_mode_or(default)`.

## Code conventions

- No `unwrap()` in production code. Use `thiserror` + `?` propagation.
- Imports grouped: std → external → internal → current crate, separated by blank lines.
- Templates in `crates/codegraph/templates/` use Tera syntax.
- 59+ generators in `crates/codegraph/src/generate/` organized by target (api, db, ddd, ui, cli, etc.).
- IFML-specific generators in `crates/codegraph/src/generate/ifml/`.
- gRPC-specific generators in `crates/codegraph/src/generate/grpc/`.
- Cornucopia-specific generators in `crates/codegraph/src/generate/db/cornucopia_*.rs` and `crates/codegraph/src/generate/ddd/cornucopia_repo.rs`.
- New node/edge types go in `crates/codegraph-core/src/types/` + `crates/codegraph-grafeo/src/schema_ddl.rs`.
- New GraphIngestor/GraphQuerier trait methods need implementations in Grafeo engine AND MockEngine AND CachingQuerier.
- New gRPC generators need registration in `generate/mod.rs`, a capability entry in `profile.rs`, and an entry in `profiles.toml`.
- New persistence provider generators need a `PersistenceProvider` variant, generator capability entries, and registration in `generate/mod.rs`.
- New DB generators (or modifications to existing ones) must use the `SqlDialect` trait (see `crates/codegraph/src/generate/db/dialect.rs`) for type mapping and feature gating instead of hardcoding PostgreSQL types.
- When adding new template files for a dialect, place them in `templates/db/<dialect>/` and the generator selects the right template path based on `database_target`.
- The `project.database_target` and `project.persistence_provider` variables are available in all Tera templates via `ProjectConfig`.

## Cross-Domain Schema Deduplication

### Problem

When a JSON schema type exists in multiple domains via `allOf` extension
(e.g., `PositionType` in `common/` and `screening/`), the ingestion step
flattens allOf properties onto the extension schema. Both Schema nodes
then share copies of the same properties. The graph querier's
`get_properties(title)` returns properties from ALL Schema nodes with that
title, producing duplicates.

This causes:
- Duplicate FK constraints and COMMENT blocks in DDL migrations (3×+)
- Duplicate migration files (one per domain for the same entity)
- Missing child tables when the allOf chain involves entity-like VO types

### Fixed in commits `3305f8b` → `3e82ec6`

Three fixes work together:

1. **`querier.rs:640`** — Call `root.dedup_fields()` on the composition tree
   root in `get_composition_tree()`. Removes duplicate columns and children
   at the source.

2. **`ddl.rs:1210-1220`** — ForeignKey and ColumnComment deduplication in
   `query_ddl_context()`. FKs deduped by `column_name`, comments by `column`.

3. **`mod.rs:1035-1078`** — `seen_titles` HashSet in `compute_generation_order()`
   tracks entity titles across domains. A title assigned to a higher-priority
   domain is skipped in subsequent domains.

### `dedup_fields()` — Critical regression risk

**File**: `crates/codegraph-core/src/types/composition.rs:126-135`

The `dedup_fields()` method MUST use **independent HashSets** per category
(columns, jsonb_columns, children). A shared set would silently remove child
`CompositionNode`s when a column and child share the same `field_name`.

This happens with VO→entity allOf patterns (commit `33240aa`), where
`build_composition_node()` pushes both an FK column and a child node for
the same property. With a shared HashSet, the column (processed first)
blocks the child from being retained.

Fixed in commit `3e82ec6` (independent HashSets per category).

## SeaORM JSONB INSERT Workaround

### Problem

`Statement::from_sql_and_values()` with parameterized binding silently drops
JSONB column values on INSERT. Returns `Ok(rows_affected=1)` but the row
either isn't persisted or has NULL/empty JSONB columns. UUID and TEXT
columns are unaffected.

### Workaround

Use `Statement::from_string()` with inline formatted SQL. The pattern:

```rust
let json_val = serde_json::to_string(&value)
    .unwrap_or_default()
    .replace('\'', "''");
let sql = format!(
    "INSERT INTO platform.webhook_endpoint (..., headers) VALUES (..., '{}'::jsonb, ...)",
    json_val
);
db.execute(Statement::from_string(DatabaseBackend::Postgres, sql)).await?;
```

### Affected templates

| Template | INSERTs needing workaround |
|----------|---------------------------|
| `templates/webhook/api_endpoints.tera` | `create_endpoint`, `create_subscription` |
| `templates/webhook/dispatch.tera` | Delivery creation (already has workaround) |

### Known issue

`issue-01.md` in the hr-specs repo documents this as a general sea_orm bug.
The workaround is SQL-injection-prone (string values must manually escape
single quotes). A proper fix would replace sea_orm's `DatabaseConnection`
with a direct `sqlx::PgPool` for dispatch workers.

## Webhook E2E Test Patterns

### Playwright selector best practices

Generated E2E tests use Playwright locators. Several patterns cause flaky
failures due to substring matching:

| Problem | Bad | Good |
|---------|-----|------|
| Text matches URLs and nav | `getByText('Webhooks')` | `getByRole('heading', { name: 'Webhooks' })` |
| Text matches description text | `click('text=Edit')` | `click('text="Edit"')` (exact match) |
| Multi-element strict mode | `getByText('Failed')` | `getByText('Failed').first()` |

`getByText()` and `text=` are **substring** matchers. They match nav links,
endpoint URLs containing the search string, and description text. Use
`getByRole()` for unique elements, exact text quotes for buttons, and
`.first()` for delivery logs that accumulate entries from parallel tests.

### Webhook delivery test flakiness

The "retries failed delivery" test was flaky for two reasons:

1. **Broad subscription** (`event_entity: null, event_type: null`) matched
   all timecard events from ALL parallel tests, accumulating 42+ delivery
   records. Fixed by narrowing to `event_entity: 'timecard', event_type: 'created'`.

2. **Multi-element strict mode** — 42 delivery log badges all matched
   `getByText('Failed')`. Fixed with `.first()`.

### Template files

| Template | Purpose |
|----------|---------|
| `templates/ui/test/webhooks_crud.test.tera` | CRUD E2E tests |
| `templates/ui/test/webhooks_delivery.test.tera` | Delivery E2E tests |
| `templates/ui/test/webhooks_advanced.test.tera` | Advanced E2E tests (not wired into generator) |
| `templates/ui/scaffold/settings_webhooks.tera` | SvelteKit list page |
| `templates/ui/scaffold/settings_webhook_detail.tera` | SvelteKit detail page |
| `templates/ui/scaffold/settings_webhook_form.tera` | SvelteKit create/edit form |

### Svelte page `data-testid` requirement

The webhook form template MUST include `data-testid="webhook-endpoint-submit-btn"`
on the submit button. Tests wait for hydration of this selector before
interacting with the form. If the attribute is missing (from an outdated
template), all form-submit tests fail with `waitForURL` timeouts.

## DDL Code Generation Architecture

### Composition tree flow

```
JSON Schema → Classifier → Graph Nodes → CompositionTree → DDL Context → Templates → SQL
```

Key files:

| File | Role |
|------|------|
| `crates/codegraph-classifier/src/classify.rs` | Classification: ValueObject vs StructuredWrapper vs Entity vs Codelist |
| `crates/codegraph-core/src/types/composition.rs` | `CompositionNode`, `CompositionTree`, `dedup_fields()` |
| `crates/codegraph-grafeo/src/querier.rs` | `build_composition_node()`, `get_composition_tree()`, `get_properties()` |
| `crates/codegraph/src/generate/db/ddl.rs` | `query_ddl_context()`, `column_info_to_ddl()`, `composition_node_to_child_table()`, FK/Comment dedup |
| `crates/codegraph/src/generate/mod.rs` | `compute_generation_order()`, domain-level entity dedup |
| `crates/codegraph/templates/db/table.tera` | PostgreSQL DDL template with child table rendering |

### How structured fields become child tables

1. Classifier assigns `ValueObject` classification (or explicit `force_value_objects`)
2. `build_composition_node()` creates a `CompositionNode` child for the VO property
3. `composition_node_to_child_table()` converts child → `ChildTableDef`
   with table name `{parent_table}_{field_name}`
4. `flatten_child_tables()` recursively flattens tree, skipping duplicates by name
5. `table.tera` renders `{% for child in child_tables %}` creating separate DDL tables

### AllOf → VO→entity extender pattern

When a ValueObject type extends an entity via allOf (e.g., `RemoteWorkType` allOf
→ `RemoteWork` entity), `build_composition_node()` pushes BOTH an FK column
(`remote_work_id UUID`) AND a child CompositionNode. The entity generator
correctly produces both outputs. The DDL generator must also produce both
(FK column + child table). `dedup_fields()` with independent HashSets
preserves both.
