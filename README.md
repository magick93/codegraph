# codegraph

Graph-driven code generation from JSON Schema.

codegraph ingests JSON Schema files, builds a type dependency graph, classifies schemas as entities vs. value objects, and generates full-stack boilerplate code. It targets the **Rust/Axum/SeaORM/SvelteKit** stack, with optional **gRPC** (tonic) and **IFML** (interaction flow modeling) support.

## Quick start

```bash
# Full pipeline: ingest schemas → classify → generate code
cargo run -- run \
  --schemas ./my-schemas \
  --classifier classifier.toml \
  --config domains.toml \
  --output ./generated-app

# Or just classify to see entity/VO decisions
cargo run -- classify \
  --schemas ./my-schemas \
  --classifier classifier.toml \
  --config domains.toml

# Generate from a pre-populated graph (skip ingestion)
cargo run -- generate \
  --config domains.toml \
  --output ./generated-app
```

## Table of contents

- [Pipeline](#pipeline)
- [CLI commands](#cli-commands)
- [Configuration](#configuration)
- [Example](#example)
- [IFML integration](#ifml-integration)
- [gRPC code generation](#grpc-code-generation)
- [Output structure](#output-structure)
- [VS Code extension](#vs-code-extension)
- [LSP server](#lsp-server)
- [Testing](#testing)
- [Project structure](#project-structure)

## Pipeline

```
┌─────────┐    ┌──────────┐    ┌──────────┐    ┌───────────┐
│  JSON   │───▶│  Ingest  │───▶│ Classify │───▶│ Generate  │───▶ Rust, SQL,
│ Schemas │    │  (graph) │    │ (entity  │    │  (58+     │     TS, proto
└─────────┘    └──────────┘    │  vs VO)  │    │  gens)    │
                               └──────────┘    └───────────┘
                                      │
                                      ▼
                               Validate: codelists,
                               refs, FK targets, cycles
```

1. **Ingest** — Load JSON Schema files, resolve `$ref` references, build a typed property graph in Grafeo (or mock engine)
2. **Classify** — Auto-classify schemas as **entities** (own table, CRUD) or **value objects** (embedded JSONB) based on `classifier.toml` rules
3. **Validate** — Check codelist references, cross-schema ref targets, FK consistency, composition depth, circular dependencies
4. **Generate** — Dispatch to 58+ generators producing Rust structs, SQL migrations, Axum handlers, OpenAPI specs, SvelteKit pages, gRPC proto + tonic services, CLI scaffolding, IFML routes, and more

## CLI commands

### `run` — Full pipeline (ingest + classify + generate)

```bash
cargo run -- run \
  --schemas <dir>          # Directory of JSON Schema files
  --classifier <path>      # Path to classifier.toml
  --config <path>          # Path to domains.toml
  --output <dir>           # Output directory for generated code
  [--profile <name>]       # Profile from profiles.toml (default: "default")
  [--variant <name>]       # Profile variant (e.g. "lite", "enterprise")
  [--profiles-config <p>]  # Path to profiles.toml (default: ./profiles.toml)
  [--template-dir <d>]     # Additional template dir (shadows built-ins)
  [--ifml-files <f>]       # Paths to .ifml DSL files
  [--ifml-framework <f>]   # IFML framework target (e.g. svelte, react)
  [--extension-points <p>] # Path to extension-points.toml
  [--no-post-gen]          # Skip post-generation scripts
```

### `generate` — Generate from pre-populated graph (skip ingestion)

```bash
cargo run -- generate \
  --config <path>          # Path to domains.toml
  --output <dir>           # Output directory for generated code
  [--ifml-framework <f>]   # IFML framework target
  [--template-dir <d>]     # Additional template dir
  [--extension-points <p>] # Path to extension-points.toml
```

### `classify` — Dry-run classification

```bash
cargo run -- classify \
  --schemas <dir>          # Directory of JSON Schema files
  --classifier <path>      # Path to classifier.toml
  --config <path>          # Path to domains.toml
  [--domain <name>]        # Filter to a single domain
  [--format <table|json>]  # Output format (default: table)
```

### `lsp` — Start the IFML Language Server

```bash
cargo run -- lsp \
  --schemas <dir>          # JSON Schema directories
  [--classifier <path>]    # Path to classifier.toml
  [--config <path>]        # Path to domains.toml
```

## Configuration

### `domains.toml`

Defines bounded contexts (domains), entity roles, and workflows.

```toml
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
depends_on = ["common"]

[domains.recruiting.entity_config.CandidateType]
operations = ["create", "read", "update", "list"]
role = "root"  # "root" or "child"

[domains.recruiting.entity_config.CandidateType.workflow]
status_field = "candidate_status_code"
states = ["new", "screening", "interviewing", "offer", "hired", "rejected", "withdrawn"]
initial_state = "new"
terminal_states = ["hired", "rejected", "withdrawn"]

[domains.recruiting.entity_config.CandidateType.dto]
immutable_fields = ["ssn"]
list_include = ["candidate_id", "uri", "status"]

[domains.compensation.entity_config.RewardType]
role = "child"
parent = "CompensationType"
parent_ref = "compensation_type_id"
```

### `classifier.toml`

Controls type classification, wrapper rules, and naming conventions.

```toml
inline_enum_threshold = 20

[primitive_wrappers.CodeType]
postgres = "TEXT"
rust = "String"
sea_orm = "Text"

[array_wrappers.StringTypeArray]
postgres = "TEXT[]"
rust = "Vec<String>"
sea_orm = "Array(RcColumnType::Text)"

[[composite_wrappers]]
schema = "AmountType"
columns = [
  { suffix = "", postgres = "NUMERIC(19,4)", rust = "rust_decimal::Decimal", sea_orm = "Decimal" },
  { suffix = "_currency", postgres = "TEXT", rust = "String", sea_orm = "Text", dto_rust_type = "CurrencyCodeList" },
]

[[composite_ranges]]
schema = "EffectiveDateType"
start = "validFrom"
end = "validTo"
column = "effective_period"
postgres = "DATERANGE"
rust = "DateRange"
```

### `profiles.toml`

Selects which generators to run per output section (api, ui, cli). Supports variants and features.

```toml
[profiles.default.meta]
name = "default"
version = "1.0.0"
description = "All artifacts"

[profiles.default.features]
auth = true
grpc_backend = true
ifml_backend = true

[profiles.default.api]
generators = ["ddl", "dto", "handler", "router", "scaffold", "grpc_proto", ...]
output = "review/generated-candidate/"

[profiles.default.ui]
generators = ["ui_page", "ui_form", "ui_store", "ifml_route", ...]

[profiles.default.cli]
generators = ["cli_command", "cli_domain", "cli_scaffold"]

# Variants override sections partially
[profiles.fullstack.variants.lite]
[profiles.fullstack.variants.lite.api]
generators = ["ddl", "dto", "handler", "router", "scaffold", "openapi"]
```

Built-in profiles: `default`, `api`, `ui`, `cli`, `fullstack`, `ci`. Use with `--profile` and `--variant`.

## Example

### Input: JSON Schema

```json
{
  "$id": "CandidateType.json",
  "title": "CandidateType",
  "type": "object",
  "properties": {
    "candidateId": { "type": "string" },
    "status": { "type": "string", "enum": ["active", "inactive"] },
    "personName": { "$ref": "../common/NameType.json" },
    "gender": { "$ref": "../common/codelist/GenderCodeList.json" }
  }
}
```

### Input: IFML DSL (optional)

```ifml
view "CandidateList" {
    label "Candidates";
    component "grid" {
        type: list;
        data: Candidate;
        fields: [name, email, status];
        on select(row) -> navigate("CandidateDetail", { id: row.id });
    }
}
```

### Configuration layout

```
my-project/
├── schemas/
│   ├── recruiting/json/CandidateType.json
│   ├── common/json/NameType.json
│   └── common/json/codelist/GenderCodeList.json
├── domains.toml
├── classifier.toml
├── profiles.toml          # optional
└── app.ifml               # optional
```

### Run

```bash
cargo run -- run \
  --schemas ./my-project/schemas \
  --classifier ./my-project/classifier.toml \
  --config ./my-project/domains.toml \
  --ifml-files ./my-project/app.ifml \
  --profile fullstack \
  --variant lite \
  --output ./generated-app
```

## IFML integration

codegraph supports IFML (Interaction Flow Modeling Language) as a complementary input alongside JSON Schema. JSON Schema defines the data model; IFML defines the interaction model (views, navigation, events).

### DSL syntax

```ifml
domain "sales" { schema "sales"; }

view "CustomerList" {
    label "Customer Management";
    component "grid" {
        type: list;
        data: Customer;
        fields: [name, email, phone, status];
        on select(row) -> navigate("CustomerDetail", { customerId: row.id });
    }
}

view "CustomerDetail" {
    params: [customerId: String];
    on init -> load(customerId);
    component "info" {
        type: details;
        data: Customer;
    }
    component "orders" {
        type: list;
        data: Order;
        where: customer_id == customerId;
    }
}
```

### Generated from IFML

- **SvelteKit routes** — Pages for each view with parameter handling, data loading, and event wiring
- **Navigation map** — Route-to-route relationships derived from navigation flows
- **Data bindings** — View components linked to JSON Schema entities via `data:` declarations

For details, see the [ifml-dsl-pest.md](docs/ifml-dsl-pest.md) design doc.

## gRPC code generation

When `grpc_backend = true` is set in `profiles.toml`, four additional generators produce:

| Generator | Output | Description |
|-----------|--------|-------------|
| `grpc_proto` | `proto/{domain}/{entity}.proto` | Protobuf messages + gRPC service definition per entity |
| `grpc_service` | `src/api/grpc/{module}_grpc.rs` | Tonic server implementation + `From` conversions |
| `grpc_router` | `src/api/grpc/{domain}_router.rs` | Service registration for all entity services in a domain |
| `grpc_scaffold` | `proto/shared.proto`, `src/api/grpc/mod.rs` | Shared types and module wiring |

**Build integration**: The generated `build.rs` compiles all `.proto` files via `tonic_build`, producing both server and client code. Clients are auto-generated (`{Entity}ServiceClient<T>`) with zero additional codegen.

**Prerequisite**: `protoc` (protobuf compiler) must be in `PATH` for the generated project to build.

## Output structure

```
generated-app/
├── Cargo.toml                  # Main crate manifest
├── build.rs                    # tonic_build + shadow-rs
├── src/
│   ├── main.rs                 # Axum server entry point
│   ├── lib.rs                  # Module tree
│   ├── app_state.rs            # Shared application state
│   ├── error.rs                # Unified error type
│   ├── middleware.rs            # Auth, CORS, rate limiting
│   ├── router.rs               # Top-level router
│   ├── api/                    # REST handlers, routes, OpenAPI
│   │   ├── grpc/               # gRPC service implementations
│   │   └── openapi.rs
│   ├── db/                     # DDL migrations, entities, seed
│   ├── domain/                 # DTOs, commands, queries, events
│   └── cli/                    # CLI commands (if enabled)
├── proto/                      # .proto files for gRPC
├── migrations/                 # SQL migration files
├── crates/
│   ├── hr-domain-types/        # Shared domain type definitions
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── dto/            # Data transfer objects
│   │       ├── codelist/       # Codelist enums
│   │       └── query_service.rs
│   └── hr-hooks-api/           # Lifecycle hooks trait + registry
│       └── src/
│           ├── lib.rs
│           └── generated/      # Auto-generated hook impls
├── webview/                    # SvelteKit UI (if enabled)
│   └── src/
│       ├── routes/             # Generated pages + forms
│       ├── lib/stores/         # Svelte stores per entity
│       └── lib/types/          # TypeScript type definitions
└── e2e/                        # Playwright E2E tests (if enabled)
```

## VS Code extension

codegraph ships with a VS Code extension for IFML editing and diagramming:

- **Language support**: Syntax highlighting, autocompletion, validation for `.ifml` files
- **LSP client**: Connects to the `codegraph lsp` server for real-time diagnostics and completions
- **Diagram view**: SvelteFlow-powered visual editor showing view containers, components, navigation flows, and data bindings
- **Commands**: `Ctrl+Shift+I` to open diagram, plus validate, generate, and refresh LSP commands

```bash
cd codegraph-vscode
npm run build:webview        # Build the SvelteFlow diagram app
npm run compile               # Compile TypeScript
npx vsce package              # Package VSIX
code --install-extension codegraph-ifml-0.1.0.vsix --force
```

## LSP server

A standalone Language Server Protocol server for IFML files:

```bash
cargo run -- lsp --schemas schemas/ --classifier classifier.toml --config domains.toml
```

Provides diagnostics, completions, and notification handling for IFML DSL files. Integrates with the VS Code extension's LSP client.

## Testing

```bash
# All tests
cargo test --workspace

# IFML DSL parser tests
cargo test -p codegraph-ifml-dsl

# gRPC tests (unit + snapshot + compile)
cargo test -p codegraph --lib -- grpc
cargo test -p codegraph --test grpc_snapshot_tests
cargo test -p codegraph --test grpc_compile_tests   # requires protoc

# Profile smoke tests
cargo test -p codegraph --test profile_smoke_tests

# E2E pipeline tests
cargo test -p codegraph --test grafeo_e2e_tests

# VS Code extension tests
cd codegraph-vscode && npx tsx test/run.ts
```

## Project structure

The workspace contains 12 crates:

| Crate | Purpose |
|-------|---------|
| `codegraph` | Main binary: CLI, ingest, classify, validate, 58+ generators |
| `codegraph-core` | Graph data model: `GraphQuerier`, `GraphIngestor`, node/edge types |
| `codegraph-grafeo` | Grafeo graph database adapter implementing core traits |
| `codegraph-backend` | Backend factory (currently Grafeo-only) |
| `codegraph-type-contracts` | Type system: PgType, RustType, DddFieldProjection |
| `codegraph-naming` | Identifier naming: snake_case, PascalCase, PG identifier handling |
| `codegraph-classifier` | Config-driven JSON schema type classification |
| `codegraph-config` | Domain config parsing (`domains.toml`, `classifier.toml`, `profiles.toml`) |
| `codegraph-ext-points` | Extension points config types |
| `codegraph-workflow` | Generic state machine workflow engine (SeaORM) |
| `codegraph-ifml-dsl` | IFML DSL parser (Pest PEG grammar) |
| `ast-ifml` | IFML AST types |

## License

MIT
