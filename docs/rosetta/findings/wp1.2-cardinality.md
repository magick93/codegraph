# WP1.2 — Is `min>1` collection cardinality (Rune `(2..10)`) representable?

## Verdict

No — `min>1` collection cardinality is not representable anywhere in the
pipeline today, and the gap is deeper than the `PropertyNode`-has-only-
`is_array: bool` hypothesis. There are two independent drops. (1) Parse
time: JSON Schema `minItems`/`maxItems` are never read — the keyword
extraction covers only `pattern`/`minLength`/`maxLength`/`minimum`/
`maximum` (`async_ingest.rs:663-676`), and there are zero
`minItems|maxItems|min_items|max_items` matches workspace-wide; `PropertyNode`
(`property.rs:79-127`) carries no fields that could hold them, so inline
arrays reduce to `Vec<String>` / `TEXT[]` (`async_ingest.rs:1098-1117`).
(2) Even the scalar-bounds contrast probe surprised: the Grafeo adapter's
`ingest_property` INSERT (`grafeo/src/ingestor.rs:271-282`) never persists
`min_length`/`max_length`/`minimum`/`maximum`, so a `seat_count` with
`minimum: 2, maximum: 10` comes back from the graph as `None` — meaning the
`garde(range)`/`garde(length)` branches in `dto_create.tera:23-33` are dead
code end-to-end (`#[garde(skip)] pub seat_count: i64` in real output).
Generated output for a required `(2..10)` member array is
`members TEXT[] NOT NULL` with no CHECK, and `#[garde(skip)] pub members:
Vec<String>` with no length validation of any kind.

## Evidence

- `crates/codegraph-core/src/types/property.rs:79-127` — `PropertyNode`
  fields: `is_array: bool` plus scalar constraints only
  (`min_length`/`max_length`/`minimum`/`maximum`); no min/max-items field
  exists to compile against.
- `crates/codegraph/src/ingest/async_ingest.rs:663-676` — the only JSON
  Schema constraint keywords ever read; `minItems`/`maxItems` absent (no
  workspace-wide match for `minItems|maxItems|min_items|max_items`).
- `crates/codegraph/src/ingest/async_ingest.rs:1098-1117` — inline
  `type: array` classification: `Vec<T>` + `{pg}[]`, no cardinality captured.
- `crates/codegraph-grafeo/src/ingestor.rs:271-282` — `ingest_property`
  INSERT column list omits `min_length`/`max_length`/`minimum`/`maximum`
  (scalar bounds parsed in-process are dropped at the graph boundary).
- `crates/codegraph-grafeo/src/conversions.rs:136-153` — the querier reads
  `p.min_length`/`p.minimum` etc., which are never written; always `None`.
- `crates/codegraph-generate/src/db/ddl.rs:656-680` — array properties emit
  a plain array column; the only CHECK sources are codelist value lists
  (`ddl.rs:777-792`) and media completeness (`ddl.rs:839-854`) — no
  length/cardinality path.
- `crates/codegraph-generate/templates/domain_types/dto_create.tera:23-33`
  — scalar `garde(length/range/pattern/email)` branches; `:35-39` arrays
  emitted as plain `Vec`; `:42-46` the only array-adjacent branch is
  `garde(inner(length(...)))`, which validates each ITEM's length, not
  collection size. All of these are unreachable in practice while the graph
  drops the bounds (see ingestor anchor above).
- `crates/codegraph/src/migrate.rs:500-506` — `codegraph migrate` collapses
  all array multiplicities to `[]` / `[0..*]`; bounded mox multiplicity is
  never emitted from `minItems`/`maxItems`.
- `crates/codegraph/src/ingest/mox_ingest.rs:1225-1228` — the mox bridge
  maps `feature.constraints.{pattern,min_length,max_length,minimum,maximum}`;
  no array-cardinality constraint exists on the rexlang side either.
- probe test: `crates/codegraph/tests/rosetta_gap_probes/cardinality_probe.rs::array_cardinality_dropped_from_graph`
  — `members` ingested with `is_array=true`, `is_required=true`,
  `rust_field_type="Vec<String>"`, `pg_column_type="TEXT[]"`; serialized
  PropertyNode keys (26) contain no cardinality key; scalar-constraint
  fields all `None`.
- probe test: `crates/codegraph/tests/rosetta_gap_probes/cardinality_probe.rs::array_cardinality_not_enforced_in_ddl`
  — `migrations/000500_crew_crew.sql` emits `members TEXT[] NOT NULL` and
  zero CHECK constraints; no `array_length`/`jsonb_array_length` anywhere.
- probe test: `crates/codegraph/tests/rosetta_gap_probes/cardinality_probe.rs::array_cardinality_not_validated_in_rust`
  — `pub members: Vec<String>` in entity + dto_create/dto_update/
  dto_response, each under `#[garde(skip)]`; no `minItems` artifact or
  len-based validation in any of the 185 generated files.
- probe test: `crates/codegraph/tests/rosetta_gap_probes/cardinality_probe.rs::scalar_bounds_survive_for_contrast`
  — `seat_count` `minimum`/`maximum` are `None` after the graph round-trip;
  DDL emits `seat_count BIGINT NOT NULL` with no numeric CHECK; the create
  DTO emits `#[garde(skip)] pub seat_count: i64` (no `garde(range`).

## Disposition recommendation

Construct: Rune collection cardinality `(2..10)` (JSON Schema
`minItems`/`maxItems`) — **not representable today; requires graph uplift +
generator feature** (NOT derivable from existing fields).

1. Annotation-payload-style graph uplift: add `min_items: Option<u64>` /
   `max_items: Option<u64>` to `PropertyNode` (serde `#[serde(default)]`,
   backward-compatible, mirroring the `min_length`/`max_length` precedent at
   `property.rs:88-91`). Populate from JSON Schema keywords in
   `async_ingest.rs` and from the mox bridge — the latter needs upstream
   rexlang support first: `FeatureConstraints` has no array-cardinality
   slot (`mox_ingest.rs:1225-1228`), so Rune `(2..10)` must ride the mox
   type-expression multiplicity (`[2..10]`-style) and be surfaced on
   `feature.constraints` (or a sibling field) upstream.
2. Generator feature for validation emission (#261 territory), gated on the
   new fields being `Some`:
   - SQL: `CHECK (cardinality(members) BETWEEN 2 AND 10)` (or
     `array_length(...)`) in the PG DDL; sqlite equivalent via `json_array_length`.
   - Rust: `#[garde(length(min = 2, max = 10))]` on the `Vec` field (garde's
     `Length` applies to collections — distinct from the existing
     `inner(length(...))` item-level branch at `dto_create.tera:42-46`).
   - TS: zod `.min(2).max(10)` on the array schema.
3. Separate defect to file regardless of cardinality (found by the contrast
   probe): scalar-bound persistence — `grafeo/src/ingestor.rs:271-282`
   omits `min_length`/`max_length`/`minimum`/`maximum` from the INSERT while
   `conversions.rs:136-153` reads them, so the schema-side validation
   templates (`dto_create.tera`/`dto_update.tera` range/length branches)
   can never fire end-to-end. Enabler-scale fix (add the four props to the
   INSERT); it would make the existing scalar-bound machinery live with no
   new generator work.

### Enablers

None — this WP is test + findings only; no production code, templates, or
manifest changes were made.
