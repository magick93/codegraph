# WP1.1 — Does generation need an inheritance merge, or does the ingest-time flatten cover Rune `extends`?

## Verdict

**No generation-time inheritance merge is needed: the ingest-time allOf
flatten already composes inherited attributes into every generator's output.**
`VehicleType` (allOf → `AssetType`, own `plateNo`) generates a SeaORM entity
containing `id`, `label`, `plate_no` each exactly once, a vehicle DDL table
with both inherited `label` and own `plate_no` columns, and DTOs carrying all
three fields; a two-level chain (A ← B ← C) composes all three levels' fields
into C's entity and DTO. The graph additionally persists `ExtendsSchema` edges
per allOf `$ref`, and **zero generator code reads `ExtendsSchema`** (grep: 0
matches under `crates/codegraph/src/generate` and
`crates/codegraph-generate/src`) — so Rune `extends` can land entirely on the
**existing graph** disposition: ingest-time merge + persisted composition
edges, mirroring how the mox bridge already handles `extends`
(`mox_ingest.rs:755-767` creates the same `ExtendsSchema` edges). Two
characterized caveats, neither of which argues for a generator-side merge:
(1) the composition tree turns each allOf parent into a *child node*
(`push_allof_children`), which the DDL generator materializes as a **shadow
child table** (`gap.vehicle_assettype`) that *re-declares* the already-flattened
inherited columns — inherited `label` exists twice in one migration file,
hanging off the extending table with no FK to the parent entity's own table;
(2) field order in generated output is **flatten-insertion order
(descendant-first, root-ancestor last)** — not ancestor-first and not
name-sorted at the generator level — while a direct `get_properties` query on
the same ingested graph returns name-sorted rows, so any future code touching
inheritance must not rely on graph query ordering.

## Evidence

- `crates/codegraph/src/ingest/async_ingest.rs:559-585` — allOf entries are
  collected during property ingestion: `$ref` parents are resolved and
  `collect_allof_property_blocks` (`:565` call, `:872-913` definition)
  recursively flattens parent (and grandparent) property blocks into the
  extending schema's property list; each block is then ingested as ordinary
  `PropertyNode`s on the extending schema (`:728`).
- `crates/codegraph/src/ingest/async_ingest.rs:825-867` — `ingest_allof_edges`
  persists one `ExtendsSchema` edge per allOf `$ref` with payload
  `composition_type: "allOf"` (`:858`); edge type DDL at
  `crates/codegraph-grafeo/src/schema_ddl.rs:348`.
- `crates/codegraph-grafeo/src/querier.rs:657-674` — `get_allof_targets`
  resolves the allOf parents from the persisted edges;
  `:676-698` — `get_schemas_that_extend` resolves the reverse direction;
  `:175` — `get_properties` returns `ORDER BY p.name`.
- `crates/codegraph-grafeo/src/querier.rs:2189-2191` and `:2483-2531` —
  `push_allof_children` adds each allOf parent as a *child* `CompositionNode`
  of the extending schema (this is graph-side composition, consumed by
  generators — not a generator-side merge).
- `crates/codegraph-generate/src/db/ddl.rs:1072` — DDL consumes
  `get_composition_tree`; `:1132-1145` converts **all** root children to
  `ChildTableDef`s with no entity/VO filter;
  `crates/codegraph-generate/templates/db/table.tera:44-64` renders each child
  as a shadow table (id, parent FK, columns, timestamps) inside the same
  migration file.
- `crates/codegraph-generate/src/db/entity.rs:128` + `:336-344` — entity
  generator flattens `get_properties` with first-occurrence-wins dedup
  ("allOf composition can produce duplicate HasProperty edges"); `:490` skips
  the `id` property (template PK is unconditional), so inherited `id` never
  duplicates.
- `crates/codegraph-generate/src/domain_model.rs:286` —
  `let inherited = all_of_parents.iter().any(|_| false);` — the domain model
  parses `all_of_parents` but the `inherited` field is a literal no-op; no
  generator consumes an inherited-field concept.
- **Grep count: `ExtendsSchema` has 0 matches in
  `crates/codegraph/src/generate` and 0 in `crates/codegraph-generate/src`.**
  Workspace-wide it appears only in the ingest paths
  (`async_ingest.rs`, `mox_ingest.rs`), the graph layer
  (`ingestor.rs`, `schema_ddl.rs`, `querier.rs`, core trait/edge type). The
  only generation-time consumers of the allOf accessors use
  `find_entity_extended_by_vo` (`codegraph-core/src/traits/querier.rs:473`)
  to resolve VO→entity *ownership* — `api/include_path.rs:879`, `:1047`,
  `playwright/ts_entity_gen.rs:157`, `types/field_def.rs:83` — never to merge
  inherited attributes.
- Probe tests: `crates/codegraph/tests/rosetta_gap_probes/inheritance_probe.rs`
  — `allof_flatten_composes_inherited_fields_in_generators`,
  `allof_two_level_chain_ancestor_order`, `extendschema_edges_persisted_in_graph`,
  `no_double_merge_in_output` (all passing; run
  `cargo test -p codegraph --test rosetta_gap_probes inheritance`).

### Surprises / characterization notes

1. **Shadow child table double-materializes inherited columns (pinned).**
   `gap_vehicle.sql` contains exactly two `CREATE TABLE` statements:
   `gap.vehicle` (id, tenant, `"label"`, plate_no) **and**
   `gap.vehicle_assettype` (id, `vehicle_id` FK → gap.vehicle, tenant,
   `"label"` again). The inherited `label` column is declared twice in one
   migration; `plate_no` (own field) once. There is no FK referencing
   `gap.asset` anywhere — the shadow table hangs off the *extending* table.
   This contradicts a naive "flatten only" reading: the graph composition
   path (allOf parent → composition child → child table) runs *in addition
   to* the flatten. Recursion means deeper chains nest further shadow tables
   (querier.rs:2518 builds each parent node with the same recursive builder).
2. **Field order is descendant-first, and it is not the graph's query order.**
   With names chosen to disambiguate (root `zetaField`, mid `betaField`, leaf
   `deltaField`), the leaf's generated entity order is
   `[id, tenant, delta_field, beta_field, zeta_field, …]` — the leaf's own
   fields first, then ancestors nearest-first (root last): exactly the
   allOf-flatten ingestion order, not ancestor-first and not name-sorted. Yet
   a direct `get_properties("LeafType")` on the ingested (and reclassified)
   graph returns name-sorted `[betaField, deltaField, zetaField]`
   deterministically across repeated queries. The generator-visible order and
   the direct-query order therefore disagree; the mechanism sits below the
   querier trait and was not chased further in this WP. Practical
   consequence: nothing downstream may assume either ordering; a Rune merge
   that cares about field order must sort explicitly.
3. Minor observations from pinning: reserved-word columns are emitted
   double-quoted (`"label" TEXT NOT NULL`), which is why naive substring
   counting miscounts columns; `pg_table_name` carries no domain prefix
   (`gap.vehicle`) while file names do (`gap_vehicle.sql`); the app-level
   `dto_response.rs` is a `pub use domain_types::…` re-export — the real DTO
   body lands at `src/gap/leaf/dto_response.rs` in the output tree; the
   vehicle entity output is clean of any child-table artifact (no
   `assettype` mention) — the double-materialization is DDL-only.

## Disposition recommendation

- **Rune `extends`: `existing graph`** (via `ExtendsSchema` edges + the
  ingest-time allOf flatten). Attribute composition into entity/DDL/DTO/API
  output is complete without any generator-side merge; the mox bridge
  (`mox_ingest.rs:755-767`) already targets the same edge type, so both
  authoring surfaces converge on the same graph representation. A
  generation-time inheritance merge would be redundant with the flatten and
  would double-compose fields unless it re-implemented the entity generator's
  dedup rules.
- **Follow-up (non-blocking): decide whether the DDL shadow child table for
  entity-typed allOf parents is intended.** Today an `extends` of a
  table-backed entity produces `gap.vehicle_assettype` re-declaring inherited
  columns. If unintended, gate `push_allof_children` children out of
  child-table emission when the parent schema `is_entity` (or drop inherited
  columns from the shadow table) — a graph/DDL-side change, not a new
  generator merge feature.
- **Follow-up (non-blocking): field-order contract.** Before any work that
  consumes inheritance order (error messages, snapshot stability, Rune
  round-trip), sort explicitly at the consumer; do not rely on
  `get_properties` ordering or on flatten-insertion order.
- Optionally delete or wire up the `domain_model.rs:286` `inherited` no-op so
  the domain model's `all_of_parents` parsing is either honest or useful.

## Enablers

None — probe-only work; no production code, templates, or manifests changed.
