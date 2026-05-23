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

## Config files

- `classifier.toml` — Type wrapper mappings, naming rules, codelist config
- `domains.toml` — Bounded contexts, entity roles, workflows
- `profiles.toml` — Generator selection profiles with variants
- `seed.toml` — Demo seed data (config-driven, optional)

## Testing

```bash
cargo test --workspace              # all tests
cargo test -p codegraph             # pipeline tests only
```

## Pipeline commands

```bash
# Full pipeline: ingest + classify + generate
cargo run -- run --schemas <dir> --classifier classifier.toml \
  --config domains.toml --output <dir>

# Classify only (show entity/VO decisions)
cargo run -- classify --schemas <dir> --classifier classifier.toml \
  --config domains.toml
```

## Code conventions

- No `unwrap()` in production code. Use `thiserror` + `?` propagation.
- Imports grouped: std → external → internal → current crate, separated by blank lines.
- Templates in `crates/codegraph/templates/` use Tera syntax.
- 54 generators in `crates/codegraph/src/generate/` organized by target (api, db, ddd, ui, cli, etc.).
