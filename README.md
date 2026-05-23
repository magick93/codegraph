# codegraph

Graph-driven code generation from JSON Schema.

## Overview

codegraph ingests JSON Schema files, builds a type dependency graph, auto-classifies entities vs. value objects, and generates full-stack boilerplate code. It currently targets the Rust/Axum/SeaORM/SvelteKit stack.

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
4. **Generate** — Dispatch to 54+ generators producing Rust structs, SQL migrations, Axum handlers, SvelteKit UI, etc.

## Configuration

- `domains.toml` — Define bounded contexts, entity roles, workflows
- `classifier.toml` — Type mappings, naming rules, wrapper classification
- `profiles.toml` — Generator selection profiles with variants
- `seed.toml` — Demo seed data (optional)

## License

MIT
