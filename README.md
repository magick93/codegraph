# codegraph

Graph-driven code generation from JSON Schema.

## Overview

codegraph ingests JSON Schema files, builds a type dependency graph, auto-classifies entities vs. value objects, and generates full-stack boilerplate code. It targets the Rust/Axum/SeaORM/SvelteKit stack, with optional gRPC support via tonic, and configurable database dialect (PostgreSQL default, SQLite experimental).

## Getting Started

```bash
# Generate code from JSON schemas
cargo run -- run \
  --schemas ./my-schemas \
  --classifier classifier.toml \
  --config domains.toml \
  --output ./generated-app
```

## Architecture

1. **Ingest** — Load JSON Schema files, resolve `$ref` references, build a typed property graph
2. **Classify** — Auto-classify schemas as entities (own table, CRUD) or value objects (embedded JSONB)
3. **Validate** — Check codelists, ref targets, FK targets, composition depth, circular refs
4. **Generate** — Dispatch to 58+ generators producing Rust structs, SQL migrations, Axum handlers, SvelteKit UI, gRPC proto + tonic services, etc.

### gRPC Code Generation

When `grpc_backend = true` is set in `profiles.toml` (default in all API profiles), four additional generators produce:

| Generator | Output |
|-----------|--------|
| `grpc_proto` | `proto/{domain}/{entity}.proto` (messages + gRPC service definition) |
| `grpc_service` | `src/api/grpc/{module}_grpc.rs` (tonic server impl + From conversions) |
| `grpc_router` | `src/api/grpc/{domain}_router.rs` (tonic router with all entity services) |
| `grpc_scaffold` | `proto/shared.proto`, `src/api/grpc/mod.rs`, shared conversion helpers |

**Build integration**: The generated `build.rs` compiles all `.proto` files via `tonic_build`, producing both server and client code. Clients are auto-generated (`{Entity}ServiceClient<T>`) with zero additional codegen.

**Prerequisites**: `protoc` (the protobuf compiler) must be in `PATH` for the generated project to build.

### Database Dialect Support

Generated SQL can target PostgreSQL (default) or SQLite via the `database_target` profile feature:

```toml
# profiles.toml
[profiles.default.features]
database_target = "sqlite"   # or "postgres" (default)
```

When set to `"sqlite"`, generated DDL uses SQLite-compatible types (`TEXT` for UUID/JSON,
`INTEGER` for booleans/timestamps, `BLOB` for geometry/vectors), omits PostgreSQL-only
features (schemas, RLS, extensions, PL/pgSQL), and uses inline trigger syntax.

The `SqlDialect` trait (`crates/codegraph/src/generate/db/dialect.rs`) abstracts all
dialect differences. Adding a new target requires implementing the trait and creating
templates under `templates/db/<target>/`.

## Configuration

- `domains.toml` — Define bounded contexts, entity roles, workflows
- `classifier.toml` — Type mappings, naming rules, wrapper classification
- `profiles.toml` — Generator selection profiles with variants (includes `grpc_backend` feature)
- `seed.toml` — Demo seed data (optional)

## License

MIT
