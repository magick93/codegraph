# WP1.7 — Does the draft "existing graph" Data-plane disposition hold row by row?

## Verdict

The draft holds for every JSON-analog row: a representative data model (entity
with primitives in uuid/date/date-time/string/integer/number/boolean formats, a
`refers` to a second entity, an array-of-string, an array-of-codelist, a
codelist status, and a contained value object) round-trips through the graph
exactly as drafted — entity → `SchemaNode`, attribute → `PropertyNode`,
enumeration → codelist `SchemaNode` + `CodeList` + `EnumValue`, `$ref` property
→ `ReferencesSchema` edge, array-of-ref property → `ItemsOf` edge — and the
generator pipeline turns that graph into the expected artifact set (DDL
migrations, SeaORM entities, DTOs, handlers). Three representation caveats were
pinned (not draft breakers, but #256 must know them): **(1)** the
`UsesCodeList` edge that `get_codelist_for_property` walks is created by *no*
production ingest path — the codelist link actually travels on
`PropertyNode.ref_target` + `classification_kind`, so that accessor is dormant
for every ingested model; **(2)** `get_enum_values` has no `ORDER BY` — value
order is only an insertion-order convention; **(3)** enum values are
graph-only for non-`common` domains: the codelist's own migration is an empty
shell (no `code` column) while every FK routes to `common.<codelist>(code)`.
For the non-JSON kinds, the mox bridge already demonstrates the right shape:
declared type aliases (`type X wraps Y { format "…" }`) and primitive builtins
are lowered **onto the property's typed fields** via a bridge-side alias table
— no graph node — while record-like types are plain object `SchemaNode`s,
which the `ValueObject` composition fallback turns into child tables for free.

## Evidence

Probe fixture: domain `sweep`, entities `OrderType`/`CustomerType`; `OrderType`
carries id(uuid), title(string, required), quantity(integer), price(number
double), active(boolean), due_date(date), completed_at(date-time), customer
(`$ref CustomerType.json#`), tags(array of string), history(array of
`$ref codelist/OrderStatus.json#`), status(`$ref codelist/OrderStatus.json#`),
detail(`$ref OrderDetailType.json#`, a VO not listed in `entities`).

- probe test: `crates/codegraph/tests/rosetta_gap_probes/data_plane_probe.rs`
  — 5 tests, all passing:
  `data_entity_round_trips_through_graph`,
  `referenceschema_and_itemsof_edges_persisted`,
  `codelist_round_trips`, `value_object_composes_child_table`,
  `generator_artifacts_complete`.
- `crates/codegraph-core/src/types/schema.rs:11` — `SchemaNode`: `domain`,
  `is_entity`, `is_codelist`, `pg_table_name`, `classification` all on the
  node; probe pins `OrderType` with `domain = "sweep"`, `classification =
  "entity_reference"`, `pg_table_name = "order"`.
- `crates/codegraph/src/ingest/async_ingest.rs:647` — `prop_type` is the RAW
  JSON `type` string (`"string"`/`"integer"`/`"number"`/`"boolean"`/`"array"`;
  `$ref` properties have no `type` key so it falls back to `"object"`); the
  format (`uuid`/`date`/`date-time`) is a separate `format` field
  (async_ingest.rs:656). Probe pins the full mapping: uuid→`UUID`/`Uuid`,
  date→`DATE`/`chrono::NaiveDate`, date-time→`TIMESTAMPTZ`/
  `chrono::DateTime<chrono::Utc>`, double→`DOUBLE PRECISION`/`f64`,
  integer→`BIGINT`/`i64` (source of the mapping:
  `crates/codegraph-classifier/src/classify.rs:244-267`,
  `crates/codegraph-type-contracts/src/pg_type.rs:68`).
- `crates/codegraph/src/ingest/async_ingest.rs:750-754` — scalar `$ref` →
  `EdgeType::ReferencesSchema`, array `$ref` → `EdgeType::ItemsOf`; edge
  targets resolve stem → `schema_id` (async_ingest.rs:736-749). Accessors:
  `get_property_ref_target` (`crates/codegraph-grafeo/src/querier.rs:743`),
  `get_array_item_schema` (querier.rs:818), `get_referenced_schemas`
  (querier.rs:719), `get_referencing_schemas` (querier.rs:700),
  `list_all_schema_references` (querier.rs:915). Probe: customer/status/detail
  all resolve through `ReferencesSchema`; history resolves `ItemsOf` to the
  OrderStatus codelist node; tags (array of primitive) carries no edge.
- `crates/codegraph/src/ingest/async_ingest.rs:919-981` — codelist-dir schemas
  ingest a `CodeList` node (name, pg_table_name, `render_as = "codelist"`) and
  one `EnumValue` per `enum` entry, pairing `enumNames` positionally into
  `display_name` and index into `sort_order`
  (`crates/codegraph-core/src/types/codelist.rs:4,13`).
- **Surprise 1 — dormant UsesCodeList accessor.** `get_codelist_for_property`
  (querier.rs:594-622) matches a `UsesCodeList` edge, but no production ingest
  creates one (`rg 'UsesCodeList'` → only the DDL verb table
  `codegraph-grafeo/src/ingestor.rs:551`, the DDL verb declaration, test
  fixtures `codegraph-core/src/test_fixtures.rs:206`, and readers). The probe
  pins it returning `None` for `OrderType.status`. Consequence: the
  composition builder's `is_codelist_fk` flag (querier.rs:2085-2088) is always
  false on the JSON path; the codelist FK still materializes because
  `resolve_property_fk_target` (querier.rs:2216-2228) keys off
  `classification_kind` + `ref_target`.
- **Surprise 2 — unpinned enum order.** `get_enum_values` (querier.rs:438-455)
  has no `ORDER BY`; the probe must sort by `sort_order` client-side. Order
  survives today as insertion order, nothing more.
- **Surprise 3 — graph-only enum values outside `common`.** The codelist's own
  migration for a non-`common` domain is a shell: `sweep.order_status` gets id
  + tenant + timestamps and NO `code` column (probe 4 asserts this;
  `inject_codelist_properties` gates the synthetic code columns on
  `domain == "common"` — `crates/codegraph-core/src/types/property.rs:11-15`),
  while FKs route to `common.order_status(code)` (probe 4 pins
  `REFERENCES common.order_status(code)` in `sweep_order.sql`). Enum values
  reach DDL/seed data only via the `common`-domain codelist path or inline
  enums (synthetic codelist + CHECK, async_ingest.rs:619-643).
- **Representation caveat — `$ref` properties drop scalar type info.**
  `prop_type` degrades to `"object"`, `pg_column_type`/`sea_orm_type` are
  empty, and `rust_field_type` is the bare target title
  (`derive_type_strings`, async_ingest.rs:1163-1172). FK types resolve at
  generation time from the target (probe 4: `customer_id UUID` in the entity).
  Rune attributes are typed, so #256 should treat this as lossy-by-design.
- `crates/codegraph-classifier/src/classify.rs:212-218` — the entity/VO
  decision behind `contains`: a `$ref` whose stem is not an entity is a
  `ValueObject`; composition (`get_composition_tree` → `push_value_object_child`,
  querier.rs:2119-2134, 2273-2375) emits a child table instead of flattened
  columns. Probe 4: `sweep."order"`'s own column list has no `address_line`,
  the inline child block `sweep.order_detail` carries `address_line` + `order_id
  … REFERENCES sweep."order"(id)`, and the parent SeaORM entity carries no VO
  fields — the contrast with the allOf-extends flatten pinned in WP1.1
  (`docs/rosetta/findings/wp1.1-inheritance.md`).
- Probe 4 also pins two DDL-shape quirks #256 should not mistake for bridge
  requirements: the VO additionally gets a standalone `sweep_order_detail`
  migration (any Schema with a `pg_table_name` is a generation entry) that
  carries NO parent FK — the linkage exists only in the parent's inline child
  block — and the array-of-codelist child table (`sweep.order_history`, code
  column + parent FK) has no own migration at all (synthetic composition node,
  not a Schema).
- Probe 5: the full artifact set exists for the entity —
  `migrations/000*_sweep_order.sql` (+ customer/codelist/VO twins),
  `src/entity/sweep_order.rs` (`DeriveEntityModel`, `table_name = "order"`),
  `src/domain/sweep/order/dto_{create,update,response}.rs`,
  `src/api/sweep/order_handler.rs`.
- mox bridge precedent for the non-JSON kinds:
  `crates/codegraph/src/ingest/mox_ingest.rs:1035-1133` — `feature_property`
  resolves `TypeRef::Datatype` through a bridge-side `datatype_formats` table
  (mox_ingest.rs:1038) and lowers it to a `format` hint + pg/rust/sea fields
  on the PropertyNode; `TypeRef::Primitive` lowers via `primitive_pg_type`
  (mox_ingest.rs:1135-1138). Declared mox aliases/builtins create **no**
  SchemaNode — byte-identity with the JSON path is achieved precisely because
  both surfaces converge on the same PropertyNode shape
  (`crates/codegraph/tests/mox_equivalence_tests.rs`).

## Disposition recommendation

Data-plane draft, row by row:

| Rune construct | Draft | Verdict | Notes |
|----------------|-------|---------|-------|
| `Data` (entity) | existing graph | **HOLDS** | `SchemaNode` (`is_entity`, `classification = "entity_reference"`, `pg_table_name`); declared via `entities` in domains.toml on the JSON path — probe fixture shape. |
| `Enumeration` | existing graph | **HOLDS** | codelist `SchemaNode` (`is_codelist`, path rule `codelist/`) + `CodeList` + `EnumValue` nodes. Caveat 3: values reach DDL only in the `common` domain (shell table + `common.`-routed FKs elsewhere). |
| attribute | existing graph | **HOLDS** | `PropertyNode` with raw `prop_type` + separate `format`; scalar pg/rust/sea + `classification_kind` + `projection`. Caveat: `$ref`-typed attributes degrade to `prop_type = "object"` with empty pg/sea. |
| enum value | existing graph | **HOLDS** | `EnumValue { value, display_name (enumNames), sort_order }`. Caveat 2: accessor has no `ORDER BY`; caveat 1: the `UsesCodeList` accessor is dormant — link lives on `ref_target`/`classification_kind`. |
| extends | existing graph | **HOLDS** | `ExtendsSchema` edges (async_ingest.rs:825-867) + ingest-time flatten; see WP1.1 (`docs/rosetta/findings/wp1.1-inheritance.md`). |
| cardinality (`min>1`) | gap | **GAP** (confirmed) | arrays are boolean `is_array`; no min>1 representation. See WP1.2 (`docs/rosetta/findings/wp1.2-cardinality.md`). |
| choice | open | **OPEN** | `has_one_of`/`has_any_of` flags exist on `SchemaNode` but no composition semantics. See WP1.3 (`docs/rosetta/findings/wp1.3-choice.md`). |
| `[metadata]` | open | **OPEN** | only `x-*` annotations survive (`custom_annotations`, async_ingest.rs:337-344). See WP1.4 (`docs/rosetta/findings/wp1.4-meta-annotations.md`). |

Non-JSON kinds — bridge-mapping recommendations for #256:

- **TypeAlias** — recommend: **bridge-side alias table lowered onto the
  property's typed fields; no graph node** (defer a `SchemaNode` + alias
  annotation payload unless #256 needs alias round-tripping). Rationale: the
  mox bridge already does exactly this (`datatype_formats` side table →
  `format` hint + pg/rust/sea on the PropertyNode, mox_ingest.rs:1038,
  1085-1133) and reaches byte-identity with the JSON path, which has no alias
  concept at all — only classifier.toml `primitive_wrappers` (the config-driven
  equivalent: named wrapper → pg/rust/sea, classify.rs:80-89). A graph node for
  an alias would add a `pg_table_name`-bearing Schema that generation would
  treat as a real table (probe 4 shows *every* Schema with a `pg_table_name`
  becomes a generation entry).
- **BasicType** — recommend: **map the builtin library onto the existing
  primitive lowering; defer any graph representation**. Rationale: builtin
  scalars already have a closed, lossless mapping
  (`PgType`/`RustType` closed enums, codegraph-type-contracts) keyed by the
  JSON `type`+`format` pair (classify.rs:244-267); the mox path lowers
  `TypeRef::Primitive` through `primitive_pg_type` (mox_ingest.rs:1135-1138).
  A BasicType node would be a third spelling of the same mapping with nothing
  to link to.
- **RecordType** — recommend: **`SchemaNode` with properties, VO-shaped**
  (schema_type `"object"`, not in `entities`). Rationale: the composition
  machinery already turns any non-entity `$ref` into a `ValueObject` child
  table (classify.rs:212-218, querier.rs:2119-2134) with the parent FK on the
  child and no parent-column flattening (probe 4) — a record used as a field
  type gets correct DDL/entity/DTO treatment with zero new bridge code. A
  record used as an aggregate root is just the `Data` row above. Record-typed
  *fields* should ride the same `ReferencesSchema` edge as any `$ref` so
  `get_property_ref_target` keeps resolving them.

No enablers were added: the probes use existing public accessor APIs only
(`support.rs` untouched, no production code changed).
