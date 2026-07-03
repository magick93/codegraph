# AGENTS.md

## Project structure

Workspace root `Cargo.toml` with 10 crates:

| Crate | Purpose |
|-------|---------|
| `codegraph` | Main binary: CLI, ingest, classify, validate, 58+ generators |
| `codegraph-core` | Graph data model: `GraphQuerier`, `GraphIngestor`, node/edge types |
| `codegraph-grafeo` | Grafeo graph database adapter implementing core traits |
| `codegraph-backend` | Backend factory (currently Grafeo-only) |
| `codegraph-type-contracts` | Type system: PgType, RustType, DddFieldProjection |
| `codegraph-naming` | Identifier naming: snake_case, PascalCase, PG identifier handling |
| `codegraph-classifier` | Config-driven JSON schema type classification |
| `codegraph-config` | Domain config parsing (`domains.toml`, classifier.toml, profiles.toml) |
| `codegraph-ext-points` | Extension points config types |
| `codegraph-workflow` | Generic state machine workflow engine (SeaORM) |

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

# Dialect tests
cargo test -p codegraph --lib -- generate::db::dialect  # 12 dialect unit tests

# gRPC tests (all levels)
cargo test -p codegraph --lib -- grpc     # 34+ unit tests
cargo test -p codegraph --test grpc_snapshot_tests  # Level 2: Insta snapshots
cargo test -p codegraph --test grpc_compile_tests   # Level 3: protoc compilation

# Profile smoke tests (includes gRPC profile validation)
cargo test -p codegraph --test profile_smoke_tests

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

## Code conventions

- No `unwrap()` in production code. Use `thiserror` + `?` propagation.
- Imports grouped: std → external → internal → current crate, separated by blank lines.
- Templates in `crates/codegraph/templates/` use Tera syntax.
- 58+ generators in `crates/codegraph/src/generate/` organized by target (api, db, ddd, ui, cli, etc.).
- IFML-specific generators in `crates/codegraph/src/generate/ifml/`.
- gRPC-specific generators in `crates/codegraph/src/generate/grpc/`.
- New node/edge types go in `crates/codegraph-core/src/types/` + `crates/codegraph-grafeo/src/schema_ddl.rs`.
- New GraphIngestor/GraphQuerier trait methods need implementations in Grafeo engine AND MockEngine AND CachingQuerier.
- New gRPC generators need registration in `generate/mod.rs`, a capability entry in `profile.rs`, and an entry in `profiles.toml`.
- New DB generators (or modifications to existing ones) must use the `SqlDialect` trait (see `crates/codegraph/src/generate/db/dialect.rs`) for type mapping and feature gating instead of hardcoding PostgreSQL types.
- When adding new template files for a dialect, place them in `templates/db/<dialect>/` and the generator selects the right template path based on `database_target`.
- The `project.database_target` variable is available in all Tera templates via `ProjectConfig`.
