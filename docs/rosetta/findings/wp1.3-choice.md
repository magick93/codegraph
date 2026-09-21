# WP1.3 — How are choice / one-of / any-of constructs represented today?

## Verdict

The hypothesis holds, and the reality is worse than "flags only": choice is
represented as exactly two root-level boolean flags on `SchemaNode`
(`has_one_of` / `has_any_of`), set only when the **root object** of a schema
carries the keyword; there is no variant, branch, or union node type anywhere
in the graph, and the flags have exactly one semantic consumer —
`is_enum: schema.has_one_of && field_count == 0` — which feeds only the
auto-classifier's `hard:enum` value-object signal. Two failure modes fall out
of this, both pinned by probes:

1. **Property-level oneOf/anyOf is silently dropped from every generated
   layer.** The property ingests fine (classified by the
   `classify_plain_type` catch-all as a JSONB `ValueObject`), but the DDL
   generator treats `ValueObject` columns as "child CompositionNodes, not
   columns" and no child node exists for a `$ref`-less oneOf — so the column
   vanishes from DDL, the SeaORM entity, and all three DTOs. Data loss with
   no warning.
2. **Schema-level (propertyless) oneOf produces a hollow CRUD entity.**
   Despite `is_enum`-shaped classification, the schema enters the generation
   order (which includes *every* top-level schema with a `pg_table_name`,
   not just entities) and receives the full entity stack — table with only
   id/tenant/audit columns, RLS, triggers, entity, DTOs, handlers, gRPC,
   proto, CLI, UI routes, Playwright tests — with zero variant content, no
   Rust enum, and no codelist migration. The classifier says "enum-like";
   the generators say "empty entity".

There is no enabler gap: `GraphQuerier::get_schema` returns the flags and
`get_classification_data` returns `is_enum`, so probes read them directly.
Static grep confirms zero semantic handling: 0 matches in
`codegraph-classifier/src`, 0 semantic matches in `codegraph-generate/src`
(all 12 are `has_one_of: false` test-fixture initializers), and 0 matches
across all 304 `.tera` templates. `Expr::OneOfOperation` /
`Expr::ChoiceOperation` (listed in
`crates/codegraph/tests/rosetta_gap_coverage_tests.rs:70`) therefore have no
graph-side counterpart to attach to — the choice gap is shared by the schema
plane and the expression plane.

## Evidence

- `crates/codegraph-core/src/types/schema.rs:36-37` — `has_one_of` /
  `has_any_of` booleans on `SchemaNode`; no variant types exist in
  `codegraph-core/src/types/`.
- `crates/codegraph/src/ingest/async_ingest.rs:374-375` — flags set from
  `entry.schema.get("oneOf").is_some()` — **root level only**; property-level
  keywords never surface (pinned by probe: `ContactType` root flags stay
  false despite a property-level oneOf).
- `crates/codegraph-grafeo/src/querier.rs:312` — the only semantic consumer:
  `is_enum: schema.has_one_of && field_count == 0`.
- `crates/codegraph/src/classify/scoring.rs:51-61` — `is_enum` →
  `"hard:enum"` → hard ValueObject in the auto-classifier.
- `crates/codegraph-classifier/src/classify.rs:275-301` —
  `classify_plain_type` catch-all: a oneOf/anyOf property (no `type` key)
  falls to `ValueObject` + JSONB (probe: PropertyNode reads
  `prop_type="object"` (defaulted), `pg_column_type="JSONB"`,
  `rust_field_type="serde_json::Value"`, `render_strategy="value_object"`).
- `crates/codegraph-generate/src/db/ddl.rs:831-834` — `ValueObject` columns
  `return None` ("represented as child CompositionNodes, not columns"); a
  `$ref`-less oneOf has no child node → **column silently dropped** from
  DDL, entity, and DTOs (probe 2/3 observed: zero occurrences of `value` /
  `payload` in `shop_contact`/`shop_ticket` DDL, entity, and DTO files).
- `crates/codegraph-generate/src/lib.rs:2199-2224` — generation order
  includes *all* top-level schemas with a `pg_table_name` (VOs included),
  which is why propertyless oneOf schemas become generation targets.
- Observed hollow-table output for the `is_enum`-shaped `PaymentStatus`
  (probe 1): `shop.payment_status` table contains only `id`,
  `platform_organization_id`, `created_at`, `updated_at`, `deleted_at`,
  `deleted_by`, `updated_by`, `is_demo_data` — yet the same run emits
  `src/entity/shop_payment_status.rs`, handlers, proto, UI routes, and
  Playwright CRUD tests for it. No `CREATE TYPE`, no CHECK, no Rust enum,
  no `*_codelist.sql`.
- Grep census (semantic handling): `codegraph-classifier/src` 0;
  `codegraph-generate/src` 0 semantic (12 fixture-only matches);
  `crates/codegraph-generate/templates/**` 0; flag plumbing only in
  `codegraph-grafeo` (`schema_ddl.rs:81-82`, `ingestor.rs:247-248`,
  `conversions.rs:115-116`); `mox_ingest.rs:932-933,969-970` and
  `atproto_projection.rs:258-259` hardcode both flags `false`.
- Probe tests: `crates/codegraph/tests/rosetta_gap_probes/choice_probe.rs::
  propertyless_oneof_is_enum_shaped`, `::entity_with_oneof_branches_loses_branches`,
  `::anyof_flag_only`, `::oneof_flags_persisted` — 4/4 passing
  (`cargo test -p codegraph --test rosetta_gap_probes choice`).

## Disposition recommendation

Per construct:

| Construct | Today | Disposition |
|---|---|---|
| Schema-level oneOf, zero properties | `is_enum` flag; auto-classifies `hard:enum` VO; generation emits a **hollow entity** (full CRUD stack, empty table) | Gap. Worst-of-both-worlds outcome. Either route `is_enum` into the existing codelist/enum pipeline, or implement real variant representation (options below). Must not stay as-is. |
| Schema-level anyOf, zero properties | No flag consumer at all (`is_enum` ignores anyOf — asymmetry pinned by probe 4); same hollow-entity generation | Gap; note the oneOf/anyOf asymmetry in the disposition table. |
| Property-level oneOf/anyOf (scalar + object branches) | Silently dropped from DDL/entity/DTO | Hard gap (silent data loss). Interim mitigation even before full support: emit the JSONB column the classifier already computes, with a WARN — strictly better than dropping user data. |
| Rune `Expr::OneOfOperation` / `Expr::ChoiceOperation` | No graph target; no choice node family to attach semantics to | Ties into the Data-plane `choice` row (WP1.7) — whatever node family represents schema choice should also carry expression-plane choice payloads. |

Representation options for #261, given today's composition machinery:

1. **Tag-column + child tables** (relational TPT-ish): discriminator column
   (`value_kind`) + per-variant child tables via the existing
   `CompositionNode`/`ChildTableDef`/`ExpandsTo` machinery. Pros: reuses
   child-table DDL/RLS/tenant-column paths that already work; queryable.
   Cons: needs discriminator emission in `table.tera` + a CHECK tying the
   tag to NULL-ability of variant columns; DTO/entity codegen must produce a
   serde `#[tagged]` Rust enum; nullable-column CHECKs get complex for
   scalar-vs-object unions.
2. **JSONB union column** (minimal): keep the classifier's JSONB catch-all
   and emit a `serde_json::Value` (or an untagged newtype) column; validation
   deferred to runtime / the expression plane. Pros: smallest change — it is
   literally what `classify_plain_type` already computes before DDL drops it;
   natural home for `OneOfOperation`/`ChoiceOperation` payloads as
   `ConditionNode`-style data. Cons: no column-level typing, no FK/RLS
   granularity inside the union, weaker than Rune's nominal choices.
3. **New node family** (e.g. `VariantNode` + `HasVariant`/`OneOfBranch`
   edges): most faithful; preserves variant identity for the Data plane and
   gives `Expr::OneOfOperation` rows a graph anchor. Cons: per repo
   conventions it means new types in `codegraph-core/src/types/`, DDL in
   `schema_ddl.rs`, ingestor+querier methods ×(Grafeo, MockEngine,
   CachingQuerier), plus generator support — the largest cost, but the only
   option that serves both planes without overloading VO child tables.

Recommendation: flag oneOf/anyOf as **gap — planned** in the disposition
table; take option 2 as the short-term stopgap (stop the silent drop) and
option 3 as the target for #261, with option 1's discriminator column as the
DDL rendering strategy underneath it.
