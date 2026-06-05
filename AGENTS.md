# AGENTS.md

## Project structure

Workspace root `Cargo.toml` with 10 crates:

| Crate | Purpose |
|-------|---------|
| `codegraph` | Main binary: CLI, ingest, classify, validate, 54+ generators |
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
cargo test --workspace                    # all tests (164+)
cargo test -p codegraph-ifml-dsl          # 20 DSL parser tests
cargo test -p codegraph -- lsp            # 5 LSP server tests
cargo test -p codegraph --test ifml_e2e_tests  # 5 E2E tests
cargo test -p codegraph --lib -- ifml     # 6 dependency graph tests

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

## Code conventions

- No `unwrap()` in production code. Use `thiserror` + `?` propagation.
- Imports grouped: std → external → internal → current crate, separated by blank lines.
- Templates in `crates/codegraph/templates/` use Tera syntax.
- 54+ generators in `crates/codegraph/src/generate/` organized by target (api, db, ddd, ui, cli, etc.).
- IFML-specific generators in `crates/codegraph/src/generate/ifml/`.
- New node/edge types go in `crates/codegraph-core/src/types/` + `crates/codegraph-grafeo/src/schema_ddl.rs`.
- New GraphIngestor/GraphQuerier trait methods need implementations in Grafeo engine AND MockEngine AND CachingQuerier.
