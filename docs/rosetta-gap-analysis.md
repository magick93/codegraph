# Rosetta integration: gap analysis — Rune DSL → codegraph disposition table

- **Issue**: magick93/codegraph#254 (gate for the Rosetta epic #255–#268)
- **Sigil pin**: `yestechgroup/sigil@49a6a27f589fa89cbb20641504923c911700ee67`
- **Status**: REVIEW
- **Evidence**: probe tests in `crates/codegraph/tests/rosetta_gap_probes/`,
  findings notes in `docs/rosetta/findings/`. Every disposition below is
  backed by observable behavior, pinned by a characterization test.
- **Scope**: every Rune DSL construct — 16 `SemanticElement` kinds, 51 `Expr`
  variants, the annotation surface — gets exactly one disposition:

| Disposition | Meaning |
|-------------|---------|
| `existing graph` | A current node/edge type carries it (possibly via bridge-side lowering); bridge maps onto it |
| `new node family` | Needs new graph node/edge types (issue-scoped) |
| `annotation payload` | Persisted as data ON an existing node (custom_annotations / typed fields / expression payload), not a node of its own |
| `generator feature` | Graph carries it; emitting it is generator work |
| `deferred` | Explicitly out of scope for now, with rationale |

---

## Verification findings

These six sections answer the issue's "must verify against real generator
behavior" list. Full evidence trails live in `docs/rosetta/findings/`.

### 1. Inheritance — merge at generation, or ingest-time flatten? (WP1.1)

**Verdict: no generation-time merge needed — `extends` takes the
`existing graph` disposition.** The JSON path flattens allOf `$ref` property
blocks at ingest (`collect_allof_property_blocks`,
crates/codegraph/src/ingest/async_ingest.rs:872); `ExtendsSchema` edges are
persisted per `$ref` (async_ingest.rs:825-867) for graph consumers, and
**zero** generators traverse them (grep: 0 matches under both `generate/`
trees; `crates/codegraph-generate/src/domain_model.rs:286` is a literal
no-op). The mox bridge deliberately mirrors this contract ("ancestors first,
name-sorted, first occurrence wins", pinned by
`tests/mox_equivalence_tests.rs`). Probes confirm inherited fields appear in
entity/DDL/DTO output exactly once each, and two-level chains flatten fully.
The Rosetta bridge (#256) must do the same ingest-time merge for Rune
`extends`.

Caveats pinned by the probes (not blockers, filed below):
- `push_allof_children` (crates/codegraph-grafeo/src/querier.rs:2190) turns
  each allOf parent into a composition child, so DDL **also** materializes a
  shadow child table (`vehicle_assettype`) re-declaring the inherited
  columns — twice in one migration, with an FK to the child and none to the
  parent entity. Decide whether this is intended for entity-typed allOf
  parents.
- Flattened field order is **descendant-first** (leaf's own fields first,
  root ancestor last), while a direct `get_properties` query returns
  name-sorted rows — generator-visible order and query order disagree. The
  bridge should pin an explicit order contract (the mox fix: insertion-order
  parity).

Evidence: `probe:inheritance_probe::allof_flatten_composes_inherited_fields_in_generators`,
`probe:inheritance_probe::allof_two_level_chain_ancestor_order`,
`probe:inheritance_probe::extendschema_edges_persisted_in_graph`,
`probe:inheritance_probe::no_double_merge_in_output`.

### 2. Cardinality `min>1` (WP1.2)

**Verdict: not representable.** `PropertyNode`
(crates/codegraph-core/src/types/property.rs:79-127) carries only
`is_array: bool`; `minItems`/`maxItems` are never read anywhere in the
workspace. Generated output for `(2..10)`-shaped arrays: `TEXT[] NOT NULL`,
zero CHECK, `#[garde(skip)] Vec<String>` — nothing enforces or even
represents the bounds.

Sharpening found by the probe: **scalar bounds are dropped too** —
`minimum`/`maximum`/`min_length`/`max_length` ARE parsed into PropertyNode
(async_ingest.rs:669-676) but the Grafeo `ingest_property` INSERT
(crates/codegraph-grafeo/src/ingestor.rs:271-282) never persists them, so
they round-trip as `None` and `dto_create.tera`'s `garde(range)`/`garde(length)`
branches are dead code end-to-end (`#[garde(skip)] pub seat_count: i64`).
Fixing that persistence gap is a prerequisite for the cardinality work.

Disposition: `annotation payload` — new typed fields `min_items`/`max_items`
on PropertyNode (mirroring `min_length`), plus `generator feature` for
validation emission (SQL `cardinality()` CHECK / garde length on the Vec /
zod `.min().max()` — #261). Upstream: rexlang `FeatureConstraints` needs a
cardinality slot for the mox path (mox `migrate.rs:500-506` collapses all
multiplicities to unbounded).

Evidence: `probe:cardinality_probe::array_cardinality_dropped_from_graph`,
`probe:cardinality_probe::array_cardinality_not_enforced_in_ddl`,
`probe:cardinality_probe::array_cardinality_not_validated_in_rust`,
`probe:cardinality_probe::scalar_bounds_survive_for_contrast`.

### 3. Choice / one-of representation (WP1.3)

**Verdict: choice exists as two root-level booleans with exactly one
semantic consumer, and both consumption paths misfire.**
`SchemaNode.has_one_of`/`has_any_of` (schema.rs:36-37) are set at ingest
(async_ingest.rs:374-375); the only consumer is
`is_enum = has_one_of && field_count == 0` (querier.rs:312). Grep census:
zero semantic oneOf/anyOf handling in the classifier, the generator crate,
and all 304 templates. Probes pin two failure modes:
- **Property-level oneOf is silently dropped**: the property ingests as a
  catch-all `ValueObject` (JSONB), but `ddl.rs:831-834` returns `None` for
  ValueObject columns ("child CompositionNodes, not columns") and a
  `$ref`-less oneOf never produces a child node — zero trace in DDL, entity,
  DTOs.
- **Propertyless oneOf becomes a hollow CRUD entity**: `is_enum` flags it
  and the classifier says `hard:enum` VO, but every schema with a
  `pg_table_name` enters generation order — full stacks (table with only
  id/tenant/audit columns, entity, handlers, proto, UI routes, Playwright)
  instead of a Rust enum or codelist.
- anyOf asymmetry: `is_enum` reads `has_one_of` only.

Disposition: `new node family` — a variant representation (tag-column +
child tables, JSONB union stopgap, or VariantNode family; trade-offs in
`docs/rosetta/findings/wp1.3-choice.md`), anchored by #261. Interim guard
for the bridge: stop the silent property-level drop via the JSONB column the
classifier already computes. The same decision anchors the Data-plane
`choice` row and `Expr::OneOfOperation`/`ChoiceOperation`.

Evidence: `probe:choice_probe::propertyless_oneof_is_enum_shaped`,
`probe:choice_probe::entity_with_oneof_branches_loses_branches`,
`probe:choice_probe::anyof_flag_only`, `probe:choice_probe::oneof_flags_persisted`.

### 4. Meta fields and the annotation surface (WP1.4)

**Verdict: schema-level metadata lands in `SchemaNode.custom_annotations`
(prefix-stripped, JSON intact, Grafeo-round-tripped); property-level
metadata is silently dropped — and Rune places most meta at attribute
level.** `x-rosetta-metadata` persists as key `rosetta-metadata`
(async_ingest.rs:340 strips `x-`); PropertyNode has no annotations field
(property.rs:79) and ingest reads a fixed key list, so unknown property keys
vanish without warning. Neither surface reaches any standard generator
output; the only `custom_annotations` readers today are atproto generators,
and mox provenance (`custom_annotations["source"]="mox"`,
mox_ingest.rs:904) is the operating precedent for Rosetta payloads riding
the channel.

Disposition: `annotation payload` for the whole meta surface —
`[metadata id/key/scheme]` is **not** a column: payload on
custom_annotations; `[ruleReference]` payload resolved against the RuleNode
family (#264); `[docReference]` payload feeding #265/#266; rationale/labels
payload. Enabler: a serde-defaulted `PropertyNode.custom_annotations` field
(recommended for #256/#258, not implemented here).

Evidence: `probe:meta_probe::top_level_x_annotations_persist_to_custom_annotations`,
`probe:meta_probe::property_level_x_annotations_dropped`,
`probe:meta_probe::property_annotations_absent_from_generator_output`.

### 5. Namespaces vs domains (WP1.5)

**Verdict: domain is dir-derived and is the ONLY grouping axis; there is no
namespace visibility axis — not even in embryonic form.**
`extract_domain_from_path` (crates/codegraph/src/ingest/schema_loader.rs:320)
takes the segment before `/json/` (versioned `<v>/<domain>/json` layouts map
to the domain segment, pinned). Same-title cross-domain claims dedup to ONE
owner: alphabetical-descending domain order wins (DomainRegistry sorts names
alphabetically, registry.rs:47-48, and `topological_order` reverses Kahn
output, registry.rs:162-164) — a domain that loses all its titles vanishes
from the generated tree entirely.

Sharpening found by the probe: `fk_target_undeclared_dependency`
(validate.rs:192) is **dead code for JSON-schema refs** — it looks up
`prop.ref_target` in a title-keyed map, but ingest stores the raw `$ref`
string (async_ingest.rs:1019), so cross-domain references generate FKs with
no `depends_on` and zero warnings. The visibility contract #267/#268 assumes
does not exist yet even as a check.

Disposition for Rosetta `namespace a.b` + `import`: `deferred` to the
#267/#268 uplift model (namespace = visibility/scoping, first-class;
domain = ownership/boundary; optional many-to-one assignment in config).
The bridge emits `NamespaceNode`/`NamespaceImports` per #268. Collision to
reconcile in #267: `NamespaceNode` already exists with different semantics
at crates/codegraph-core/src/types/atproto.rs:4-8 (+ `EdgeType::InNamespace`).

Evidence: `probe:namespace_probe::domain_is_dir_derived_from_path`,
`probe:namespace_probe::same_title_in_two_domains_deduped_to_first`,
`probe:namespace_probe::versioned_layout_maps_to_domain_segment`,
`probe:namespace_probe::cross_domain_reference_generates_fk`.

### 6. Expression typing and `Expr::to_json` (WP1.6)

**Verdict: embed `Expr::to_json()` Values as ConditionNode/RuleNode
payloads — deterministic, serialization-stable, numeric-faithful; sigil
does NOT type-check or interpret expressions, so the #262 transpiler must do
its own checking.** Probes over a 51-condition fixture (49/51 variants
exercised): to_json twice is Value-equal, string round-trip is byte-equal,
and `canonical_json` embeds `conditions[].expression` as exactly
`Expr::to_json()` — that is the bridge's natural embedding path. Numbers
keep verbatim source text (no float fidelity loss). `Expr` is Serialize-only
(derive `Debug, Clone, PartialEq` + manual `Serialize` at expr.rs:391; no
`Deserialize` anywhere) — payloads are write-once provenance, to be
re-parsed by the transpiler, not deserialized into AST.

What resolve() DOES check: name resolution only — type/annotation/rule/
report refs (E0101–E0107), FQN duplicates (E0104), inheritance cycles
(E0105), dispatch/assign-root scoping, and expression **head symbols** (its
own comment: feature segments "parsed but not type-checked"). It does NOT
check: operand typing (probes: `amount + "x"` resolves clean), boolean-ness
of conditions, `->` segments, cast meaningfulness; no evaluator exists.

Two sigil-side gaps found: `as-key` is rejected by the parser (E0001 — `as`
claims the keyword; upstream sigil issue), and expression-head E0101s carry
`span: None`.

Evidence: `probe:expr_json_probe::parse_and_resolve_exercises_expression_families`,
`probe:expr_json_probe::to_json_is_deterministic_and_serialization_stable`,
`probe:expr_json_probe::resolve_does_not_typecheck_expressions`,
`probe:expr_json_probe::canonical_json_embds_expressions_losslessly`.

---

## Plane: Data

Draft disposition held row by row (WP1.7): existing graph carries the
surface, with the cardinality and choice gaps called out below.

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| Data | `SemanticElement::Data` (lib.rs:180) | existing graph | entity/VO → `SchemaNode` (+ entity/VO decision); round-trip + full artifact set verified | probe:data_plane_probe | #256 |
| Enumeration | `SemanticElement::Enumeration` (lib.rs:210) | existing graph | codelist `SchemaNode` + `CodeList` + `EnumValue` (+ `ReferencesSchema`/`ItemsOf`); caveat: non-common codelist DDL is a shell, values graph-only — FKs route to `common.<codelist>(code)` | probe:data_plane_probe | #256 |
| Annotation | `SemanticElement::Annotation` (lib.rs:236) | annotation payload | annotation declarations ride schema-level custom_annotations; property-level needs the PropertyNode.custom_annotations uplift (enabler) | probe:meta_probe | #256/#258 |
| TypeAlias | `SemanticElement::TypeAlias` (lib.rs:248) | existing graph | bridge-side alias lowering onto property typed fields; no graph node (mox `datatype_formats` precedent, mox_ingest.rs:1038) | probe:data_plane_probe | #256 |
| BasicType | `SemanticElement::BasicType` (lib.rs:270), `sigil_resolve::builtin_files` | existing graph | builtin library lowers via a bridge-side builtin table onto classifier built-in primitive handling | probe:data_plane_probe | #256 |
| RecordType | `SemanticElement::RecordType` (lib.rs:281) | existing graph | plain object → `SchemaNode`; the `ValueObject` fallback composes child tables for free | probe:data_plane_probe | #256 |
| attribute | `Attribute` (lib.rs:192) | existing graph | → `PropertyNode` (raw `prop_type` + separate `format`; `$ref` attrs degrade to `object` typed-lossy-by-design) | probe:data_plane_probe | #256 |
| enum value | `EnumValue` (lib.rs:223) | existing graph | preserved in graph; DDL order is insertion-order convention (`get_enum_values` has no ORDER BY — pinned) | probe:data_plane_probe | #256 |
| cardinality | `Cardinality { min, max }` (lib.rs:63) | annotation payload | gap pinned: no min/max items anywhere; needs `min_items`/`max_items` on PropertyNode + scalar-bounds persistence fix | probe:cardinality_probe | #261 |
| extends | `Data.extends` / allOf path | existing graph | ingest-time flatten + `ExtendsSchema` edges; NO generator merge needed; caveats: shadow child table, descendant-first order (finding 1) | probe:inheritance_probe | #256 |
| choice | choice constructs | new node family | representation decision (tag+child tables / JSONB stopgap / VariantNode) — anchors #261, OneOf/Choice ops | probe:choice_probe | #261 |
| [metadata] | `AnnotationRef` (lib.rs:98), `AnnotationQualifier` (lib.rs:111), `WithMetaOperation` | annotation payload | schema-level custom_annotations (prefix-stripped); NOT a column; property-level uplift is the enabler (finding 4) | probe:meta_probe | #258 |

---

## Plane: Constraint

Draft disposition held: new ConditionNode family + validation generators
(Rust validators, SQL CHECK, TS zod).

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| condition | `Condition` (lib.rs:180) | new node family | ConditionNode with `Expr::to_json()` Value payload (finding 6 embedding contract) | probe:expr_json_probe | #261 |
| one-of | oneOf constructs / `OneOfOperation` | new node family | variant representation; property-level oneOf silently dropped today (finding 3) | probe:choice_probe | #261 |
| cardinality min>1 (validation) | `Cardinality.min > 1` | generator feature | validation emission (SQL CHECK / garde / zod) on top of the data-plane fields | probe:cardinality_probe | #261 |
| [ruleReference] | `RuleReference` (lib.rs:150) | annotation payload | payload resolved against the RuleNode family | probe:meta_probe | #264 |

## Expression surface (all 51 `Expr` variants)

All variants embed as `Expr::to_json()` Values inside Condition/Rule/Function
payloads (`annotation payload`), with the #262 transpiler lowering per
family. WP1.6 exercised 49/51 against parse+resolve (missing:
`AsKeyOperation` — sigil parser rejects `as-key`, E0001, upstream sigil
issue; `ChoiceOperation` — needs choice-type syntax, ties to the choice
decision). resolve() checks head symbols only; typing is the transpiler's
burden (finding 6).

| Expr variant | Family | Disposition | Rationale | Evidence | Upstream |
|--------------|--------|-------------|-----------|----------|----------|
| BooleanLiteral | literal | annotation payload | embedded; transpiler-direct | probe:expr_json_probe | #262 |
| StringLiteral | literal | annotation payload | embedded; transpiler-direct | probe:expr_json_probe | #262 |
| NumberLiteral | literal | annotation payload | embedded; verbatim source text — no float fidelity loss | probe:expr_json_probe | #262 |
| IntLiteral | literal | annotation payload | embedded; transpiler-direct | probe:expr_json_probe | #262 |
| ListLiteral | literal | annotation payload | embedded; transpiler-direct | probe:expr_json_probe | #262 |
| SymbolReference | reference | annotation payload | head-symbol-resolved by sigil (E0101); transpiler binds from scope | probe:expr_json_probe | #262 |
| ImplicitVariable | reference | annotation payload | synthesized by `then join` as missing left operand (sigil-syntax expr.rs:808) | probe:expr_json_probe | #262 |
| FeatureCall | reference | annotation payload | property navigation; segments parsed-not-typechecked | probe:expr_json_probe | #262 |
| DeepFeatureCall | reference | annotation payload | nested property navigation; same contract | probe:expr_json_probe | #262 |
| ArithmeticOperation | operator | annotation payload | Rust operator lowering; typing unchecked upstream (probe: `amount + "x"` resolves clean) | probe:expr_json_probe | #262 |
| LogicalOperation | operator | annotation payload | `&&`/`\|\|`/`!` lowering | probe:expr_json_probe | #262 |
| EqualityOperation | operator | annotation payload | `==`/`!=` lowering | probe:expr_json_probe | #262 |
| ComparisonOperation | operator | annotation payload | `<`/`<=`/`>`/`>=` lowering | probe:expr_json_probe | #262 |
| ContainsExpression | collection | annotation payload | iterator `any()` lowering | probe:expr_json_probe | #262 |
| DisjointExpression | collection | annotation payload | set-intersection-empty lowering | probe:expr_json_probe | #262 |
| DefaultOperation | collection | annotation payload | `unwrap_or` lowering | probe:expr_json_probe | #262 |
| JoinOperation | collection | annotation payload | string/collection join lowering | probe:expr_json_probe | #262 |
| ConditionalExpression | control | annotation payload | ternary/if-else lowering | probe:expr_json_probe | #262 |
| OnlyExistsExpression | existence | annotation payload | quantifier lowering | probe:expr_json_probe | #262 |
| ExistsExpression | existence | annotation payload | quantifier lowering | probe:expr_json_probe | #262 |
| AbsentExpression | existence | annotation payload | negated existence lowering | probe:expr_json_probe | #262 |
| OnlyElement | algebra | annotation payload | single-element access lowering | probe:expr_json_probe | #262 |
| CountOperation | algebra | annotation payload | `.count()` lowering | probe:expr_json_probe | #262 |
| FlattenOperation | algebra | annotation payload | `.flatten()` lowering | probe:expr_json_probe | #262 |
| DistinctOperation | algebra | annotation payload | dedup lowering | probe:expr_json_probe | #262 |
| ReverseOperation | algebra | annotation payload | `.rev()` lowering | probe:expr_json_probe | #262 |
| FirstOperation | algebra | annotation payload | `.first()` lowering | probe:expr_json_probe | #262 |
| LastOperation | algebra | annotation payload | `.last()` lowering | probe:expr_json_probe | #262 |
| SumOperation | algebra | annotation payload | `.sum()` lowering | probe:expr_json_probe | #262 |
| AsKeyOperation | algebra | annotation payload | map-key lowering; SIGIL GAP: parser rejects `as-key` (E0001) — upstream sigil issue required | probe:expr_json_probe | #262 |
| OneOfOperation | algebra | annotation payload | ties to the choice/variant representation decision (finding 3) | probe:expr_json_probe | #262 |
| ChoiceOperation | algebra | annotation payload | ties to the choice decision; not parse-exercised in fixture (choice-type syntax) | sigil:expr.rs:127 | #262 |
| ToStringOperation | cast | annotation payload | `to_string()` lowering; cast meaningfulness unchecked upstream | probe:expr_json_probe | #262 |
| ToNumberOperation | cast | annotation payload | parse/convert lowering | probe:expr_json_probe | #262 |
| ToIntOperation | cast | annotation payload | parse/convert lowering | probe:expr_json_probe | #262 |
| ToTimeOperation | cast | annotation payload | chrono lowering | probe:expr_json_probe | #262 |
| ToEnumOperation | cast | annotation payload | codelist-variant lowering; enumeration string unchecked upstream | probe:expr_json_probe | #262 |
| ToDateOperation | cast | annotation payload | chrono lowering | probe:expr_json_probe | #262 |
| ToDateTimeOperation | cast | annotation payload | chrono lowering | probe:expr_json_probe | #262 |
| ToZonedDateTimeOperation | cast | annotation payload | chrono-tz lowering | probe:expr_json_probe | #262 |
| SwitchOperation | control | annotation payload | match lowering | probe:expr_json_probe | #262 |
| WithMetaOperation | metadata | annotation payload | ties to the [metadata] payload disposition (finding 4) | probe:expr_json_probe | #262 |
| AsOperation | control | annotation payload | typed-view lowering | probe:expr_json_probe | #262 |
| ThenOperation | control | annotation payload | chaining lowering; synthesizes ImplicitVariable operand | probe:expr_json_probe | #262 |
| FilterOperation | higher-order | annotation payload | `.filter()` lowering | probe:expr_json_probe | #262 |
| MapOperation | higher-order | annotation payload | `.map()` lowering | probe:expr_json_probe | #262 |
| ReduceOperation | higher-order | annotation payload | `.fold()` lowering | probe:expr_json_probe | #262 |
| SortOperation | higher-order | annotation payload | `.sort*()` lowering | probe:expr_json_probe | #262 |
| MinOperation | higher-order | annotation payload | `.min()` lowering | probe:expr_json_probe | #262 |
| MaxOperation | higher-order | annotation payload | `.max()` lowering | probe:expr_json_probe | #262 |
| ConstructorExpression | constructor | annotation payload | constructor keys unchecked upstream; ties to RecordType | probe:expr_json_probe | #262 |

---

## Plane: Computation

Draft disposition held: new FunctionNode/RuleNode families + expr→Rust
transpiler. Payloads embed `Expr::to_json()` Values (finding 6); sigil
resolves names, never types — the transpiler owns checking.

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| Function | `SemanticElement::Function` (lib.rs:324) | new node family | FunctionNode; parse+resolve exercised by probes | probe:expr_json_probe | #263 |
| Rule | `SemanticElement::Rule` (lib.rs:436) | new node family | RuleNode parent kind; reporting/eligibility are roles of it | probe:expr_json_probe | #264 |
| FunctionDispatch | `FunctionDispatch` (lib.rs:352) | new node family | dispatch enumeration on FunctionNode; scoping checked by resolve | sigil:lib.rs:352 | #263 |
| function aliases | `Function.aliases` | annotation payload | payload on FunctionNode | sigil:lib.rs:324 | #263 |
| set/add operations | `Function` bodies (set/add) | generator feature | bodies → generated handlers via the #262 transpiler | sigil:lib.rs:324 | #263 |
| post-conditions | `Function` post-conditions | generator feature | transpiled condition payloads on emitted handlers | sigil:lib.rs:324 | #263 |
| reporting rule | `Rule` (reporting) | new node family | RuleNode + report generator wiring | sigil:lib.rs:436 | #264 |
| eligibility rule | `Rule` (eligibility) | new node family | RuleNode + API guard emission (#264); guards reuse the #261 condition machinery | sigil:lib.rs:436 | #264 |

---

## Plane: Regulatory

Draft disposition held: metadata nodes + report/doc generators. Sized in
#265 after sign-off; dispositions below are the sizing input.

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| Report | `SemanticElement::Report` (lib.rs:502) | new node family | Report node + report generator | sigil:lib.rs:502 | #265 |
| ReportTiming | `ReportTiming` (lib.rs:478) | annotation payload | payload on the Report node | sigil:lib.rs:478 | #265 |
| Body | `SemanticElement::Body` (lib.rs:573) | new node family | metadata node; corpus/segment hierarchy beneath | sigil:lib.rs:573 | #265 |
| Corpus | `SemanticElement::Corpus` (lib.rs:584) | new node family | metadata node | sigil:lib.rs:584 | #265 |
| Segment | `SemanticElement::Segment` (lib.rs:598) | new node family | metadata node (`SegmentReference` links) | sigil:lib.rs:598 | #265 |
| ExternalRuleSource | `SemanticElement::ExternalRuleSource` (lib.rs:520) | new node family | rule provenance node (ExternalClass/ExternalAttribute payloads) | sigil:lib.rs:520 | #265 |
| Schema | `SemanticElement::Schema` (lib.rs:558) | new node family | regulatory metadata bundle — NOT codegraph's `SchemaNode`; distinct node name required (collision table below) | sigil:lib.rs:558 | #265 |
| MetaType | `SemanticElement::MetaType` (lib.rs:607) | new node family | metadata node | sigil:lib.rs:607 | #265 |
| LibraryFunction | `SemanticElement::LibraryFunction` (lib.rs:299) | deferred | sized with the #262/#263 transpiler intrinsics registry; not a data-plane concern | defer:sized with #262/#263 | #263/#265 |
| [docReference] | `DocReference` (lib.rs:128) | annotation payload | payload + doc generator output | sigil:lib.rs:128 | #265/#266 |
| transforms | `TransformAnnotation` (lib.rs:368), `TransformKind` (lib.rs:376) | generator feature | graph carries the annotation; emission is generator work | sigil:lib.rs:368 | #265 |
| Rationale | `Rationale` (lib.rs:139) | annotation payload | payload on the owning element | sigil:lib.rs:139 | #265 |
| LabelAnnotation | `LabelAnnotation` (lib.rs:145) | annotation payload | payload on the owning element | sigil:lib.rs:145 | #265 |

---

## Defects & upstream filings discovered during verification

Not #254 scope (probes + doc only) — filed/triaged separately so the epic
issues can reference them:

| # | Defect | Found by | Suggested home |
|---|--------|----------|----------------|
| 1 | ~~Grafeo `ingest_property` never persists `min_length`/`max_length`/`minimum`/`maximum`~~ **RESOLVED** on the epic branch: Property DDL columns + INSERT params + RETURN cols; garde range/length now fire (pinned by `rosetta_scalar_bounds_tests` + flipped `cardinality_probe`) | WP1.2 | resolved pre-#261 |
| 2 | Property-level oneOf silently dropped from DDL/entity/DTO (ValueObject column → no child node for `$ref`-less oneOf) | WP1.3 | #261 (interim guard: emit the JSONB column) |
| 3 | Propertyless oneOf generates a hollow CRUD entity (full stack incl. UI/proto) instead of enum/codelist; anyOf asymmetry (`is_enum` reads `has_one_of` only) | WP1.3 | #261 |
| 4 | `fk_target_undeclared_dependency` is dead code for JSON refs (`ref_target` holds the raw `$ref`, lookup is title-keyed) → cross-domain FKs generate with no `depends_on`, no warning | WP1.5 | #267 enforcement point |
| 5 | `UsesCodeList` edge is dormant — `get_codelist_for_property` walks an edge no ingest creates (real link: `ref_target` + `classification_kind`). Follow-up investigation found a SECOND dormant consumer: `ifml_scaffold::codelist_values` (crates/codegraph/src/ifml_scaffold.rs:383-403) walks the same edge, so scaffolded dropdown values are always empty. Cleanup requires rewriting that caller first; `is_codelist_fk` additionally has ZERO readers (always-false and unconsumed — deletable). Deliberately left in place on the epic branch (behavior-neutral) | WP1.7 | follow-up issue |
| 6 | allOf parent → shadow child table re-declares inherited columns; flattened field order (descendant-first) disagrees with `get_properties` order (name-sorted) | WP1.1 | decide intent; pin order contract |
| 7 | sigil: parser rejects `as-key` (E0001); expression-head diagnostics carry `span: None` | WP1.6 | upstream yestechgroup/sigil |
| 8 | rexlang: `FeatureConstraints` has no cardinality slot; mox migrate collapses multiplicities to unbounded | WP1.2 | upstream rexlang / mox bridge |
| 10 | Rosetta-only runs show migration-sequence nondeterminism across identical inputs (e.g. 000648 vs 000624) and the "Auto-classified N entities" count can flip 0/1 — a scorer sits on a knife edge (net_score ≥ 4) whose in-degree signals vary run-to-run. Content of individual migrations is stable; ordering is not. Found by the #259 fixture suite | #259 | standalone determinism fix; blocker for any future snapshot-on-migration-names |
| 11 | Codelist routing dangles for rosetta-only projects: enums outside the `common` domain emit bare entity tables without a `code` column (ddl.rs:1257) while enum-typed attribute FKs target `common.<codelist>(code)` — and no seed rows are emitted. Same family as WP1.7's "non-common codelists are graph-only" caveat | #259 fixture suite | #261/#265 plane work or standalone |
| 9 | ~~Title-suffix strip collides for `type Product` + `choice ProductType`~~ **RESOLVED** on the epic branch: choices keep their unstripped code names (`ProductType` → table `product_type`); pinned by the flipped `rosetta_pipeline_tests` assertion + `rosetta_bridge_suite_tests::fixture_choices_get_unstripped_names` | #257 | resolved |

## Cross-cutting collisions

| Collision | Detail | Resolution owner |
|-----------|--------|------------------|
| `NamespaceNode` | `crates/codegraph-core/src/types/atproto.rs:4-8` already defines `NamespaceNode` (+ `EdgeType::InNamespace`) with AT-Protocol repo semantics; #267 proposes a graph-wide namespace concept with the same name | #267 |
| `Schema` | sigil's `Schema` SemanticElement is regulatory metadata (rule source bundle); codegraph's `SchemaNode` is the data-plane type container — the regulatory node must take a distinct name | #265 |

## Upstream issue map

| Issue | Slice |
|-------|-------|
| #255 | dependency wiring (sigil crates, rev-pinned; test-only dev-deps landed in #254) |
| #256 | data-plane bridge (`rosetta_ingest`) |
| #257 | driver + CLI threading (`--rosetta-files`) |
| #258 | classification interplay (auto-score) |
| #259 | bridge test suite + committed fixture model |
| #260 | project lifecycle support (init/doctor/add domain) |
| #261 | constraint plane (ConditionNode family + first validation generator) |
| #262 | expr→Rust transpiler (computation plane, slice a) |
| #263 | FunctionNode family + func codegen (computation plane, slice b) |
| #264 | RuleNode family + rules → computed fields & endpoint guards (slice c) |
| #265 | regulatory plane (report/body/corpus/segment/rule source/schema/metaType) |
| #266 | docs (README + integration guide) |
| #267 | namespaces: core model, graph, config, validation |
| #268 | namespaces: source bridging + namespace-aware generation |

## Gate

This table must be **signed off** before bridge implementation (#255+) is
sized. Status flipped `DRAFT` → `REVIEW` on completion of the probe WPs;
approval closes #254 and unblocks #255 sizing.
