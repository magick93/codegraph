# codegraph

Graph-driven code generation from JSON Schema.

## Overview

codegraph ingests JSON Schema files, builds a type dependency graph, auto-classifies entities vs. value objects, and generates full-stack boilerplate code. It targets the Rust/Axum/SeaORM/SvelteKit stack, with optional gRPC support via tonic, optional IFML interaction models, configurable database dialect (PostgreSQL default, SQLite experimental), and a Rust test & deploy harness (`codegraph-ops`) that every generated app can drive.

## Quick start — hello-world TODO app

```bash
# Scaffold a working consumer project (interactive when NAME omitted)
cargo run -- init todo-app
cd todo-app

just generate          # wrapper run -> generated/ (187 files, 0 errors)
cargo build --manifest-path generated/Cargo.toml
just api               # 39/39 PASS: migrate, CRUD smoke, RLS, graceful shutdown
```

The scaffold ships two starter entities (`TodoListType`, `TodoItemType`) and is verified end-to-end against a real Postgres. Use `--codegraph-path ~/git/codegraph` instead of a git rev while developing codegraph itself.

## Project lifecycle

| Command | Purpose |
|---------|---------|
| `codegraph init [NAME]` | Scaffold a consumer monorepo: wrapper binary (`{name}-graph`), `domains.toml`/`classifier.toml`/`profiles.toml`, schemas, ops harness (manifest + testkit member), justfile, CI. Refuses to overwrite without `--force`. |
| `codegraph doctor` | Validate an existing project: configs parse, profiles pass BuildPlan/capability validation, schemas present, ops manifest loads, rev pins match the binary's embedded rev, tools present. |
| `codegraph add domain <name>` | Append a domains.toml entry and create `schemas/<name>/` with the starter schemas. |

The wrapper binary calls the stable `codegraph::driver` library API (`run`/`generate`/`classify`) — consumers no longer need to fork the generate pipeline.

## CLI

```bash
cargo run -- run --schemas <dir> --classifier classifier.toml \
  --config domains.toml --output <dir>          # ingest + classify + generate
cargo run -- classify --schemas <dir> ...        # entity/VO decisions only
cargo run -- generate --config domains.toml --output <dir>
cargo run -- migrate --config domains.toml ...   # ingest API model only
cargo run -- lsp --schemas <dir> ...             # IFML language server
cargo run -- init / doctor / add domain ...      # project lifecycle (see above)
```

Common flags: `--template-dir` (repeatable; shadows built-in templates), `--profile`/`--profiles-config`, `--ifml-files`, `--no-post-gen`.

## Ops harness (`codegraph-ops`)

A Rust re-imagining of the hand-written bash test suite, shared by every consumer. The `ops` generator (enabled by `ops_backend = true` in `profiles.toml`) emits `codegraph-ops.toml` (seeded from your `ProjectConfig`) plus a thin `testkit/` crate into the generated output.

```
cargo run -p testkit -- api        # preflight, migrate, hurl, curl smoke, RLS, shutdown
cargo run -p testkit -- cli        # CLI e2e (starts API first)
cargo run -p testkit -- e2e        # Supabase -> generate -> migrate -> build -> Playwright
cargo run -p testkit -- ui         # Playwright only (API must be running)
cargo run -p testkit -- full       # api then e2e
cargo run -p testkit -- smoke      # remote deployment smoke test
cargo run -p testkit -- quality    # cargo test/clippy/fmt + generate + check
cargo run -p testkit -- clean      # stop services, remove generated output
cargo run -p testkit -- ext <name> # run a test extension
```

Global flags: `--config`, `--keep`, `--skip-build`, `--skip-generate`, `--release`, `--verbose`, `--metrics <tsv>` (`--metrics-format json`), `--retry N`, `--headed`, `--grep`.

Project-specific integrations (Xero, Stripe, IRD, ...) plug in without touching codegraph: implement the `TestExtension` trait and register it in the testkit binary, declare an `[[extensions]]` exec entry in the manifest, or run shell steps as `[[hooks]]` at pipeline points (`pre_generate`, `post_generate`, `post_migrate`, `pre_e2e`, `post_e2e`, `pre_api`, `post_api`, `pre_playwright`).

## Architecture

1. **Ingest** — Load JSON Schema files, resolve `$ref` references, build a typed property graph
2. **Classify** — Auto-classify schemas as entities (own table, CRUD) or value objects (embedded JSONB)
3. **Validate** — Check codelists, ref targets, FK targets, composition depth, circular refs
4. **Generate** — Dispatch to 60+ generators producing Rust structs, SQL migrations, Axum handlers, SvelteKit UI, gRPC proto + tonic services, etc.

### gRPC Code Generation

When `grpc_backend = true` is set in `profiles.toml`, four additional generators produce:

| Generator | Output |
|-----------|--------|
| `grpc_proto` | `proto/{domain}/{entity}.proto` (messages + gRPC service definition) |
| `grpc_service` | `src/api/grpc/{module}_grpc.rs` (tonic server impl + From conversions) |
| `grpc_router` | `src/api/grpc/{domain}_router.rs` (tonic router with all entity services) |
| `grpc_scaffold` | `proto/shared.proto`, `src/api/grpc/mod.rs`, shared conversion helpers |

**Build integration**: The generated `build.rs` compiles all `.proto` files via `tonic_build`, producing both server and client code. Clients are auto-generated (`{Entity}ServiceClient<T>`) with zero additional codegen.

**Prerequisites**: `protoc` (the protobuf compiler) must be in `PATH` for the generated project to build.

### IFML Interaction Models

IFML (Interaction Flow Modeling Language) DSL files describe views, navigation, events, and data bindings as a complement to JSON Schema. See `crates/codegraph-ifml-dsl/` for the Pest grammar and `codegraph lsp` for the language server.

### Database Dialect Support

Generated SQL can target PostgreSQL (default) or SQLite via the `database_target` profile feature:

```toml
# profiles.toml
[profiles.default.features]
database_target = "sqlite"   # or "postgres" (default)
```

The `SqlDialect` trait (`crates/codegraph/src/generate/db/dialect.rs`) abstracts all
dialect differences. Adding a new target requires implementing the trait and creating
templates under `templates/db/<target>/`.

## Configuration

- `domains.toml` — Bounded contexts, entity roles, workflows
- `classifier.toml` — Type mappings, naming rules, wrapper classification
- `profiles.toml` — Generator selection profiles with variants (`ops_backend`, `grpc_backend`, `ifml_backend`, `database_target`, `persistence_provider`, `deployment_topology`)
- `extension-points.toml` — Integration infrastructure extensions
- `codegraph-ops.toml` — Ops harness manifest (test & deploy configuration)
- `seed.toml` — Demo seed data (optional)

## Development

```bash
cargo test --workspace                      # full test suite
cargo test -p codegraph-ops                 # ops harness tests (103)
cargo test -p codegraph --test init_tests   # project lifecycle integration tests
cargo test -p codegraph-ops --test db_integration -- --ignored  # needs a running Postgres
```

## License

MIT
