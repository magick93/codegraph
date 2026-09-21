# Rosetta (Rune DSL) → codegraph Gap Analysis — Disposition Table

**Issue**: #254 (gate for the sigil/Rosetta integration effort, sub-issues #255–#268)
**Status**: DRAFT — awaiting sign-off
**Sources of truth**: sigil `yestechgroup/sigil` @ `38bf91f` ("Milestones 2-3"); codegraph @ `e6def684` (tag `0.2.0`)
**Verification method**: code reading (file:line refs below) + empirical pipeline runs (E1–E7, appendix A) against the real generator, plus sigil's own conformance corpus.

---

## 1. Purpose and gate

Sigil models the **entire** Rune DSL: 16 `SemanticElement` kinds, 50 `Expr` variants, and an
annotation surface (9 builtin annotations + `[docReference]`/`[ruleReference]` + transform
annotations). Codegraph's graph natively covers only the **data plane**. This document is the
construct-by-construct disposition table for the whole effort. Every construct gets exactly one
disposition:

| Disposition | Meaning |
|---|---|
| **existing graph** | Bridges onto today's node/edge families with no schema change |
| **new node family** | Requires new node/edge types in `codegraph-core` + grafeo DDL + traits (IFML precedent) |
| **annotation payload** | Stored as data on an existing node (`custom_annotations` / JSON payload), no new node |
| **generator feature** | Graph representation exists or is trivial; the work is in a generator |
| **deferred** | Explicitly out of scope for now, with rationale |

**Gate**: this table must be signed off before the bridge implementation (#256) is sized.

Baseline tag: `0.2.0` was cut at `e6def684` **before** any of this work lands, because the
integration introduces new node families and cross-cutting uplifts (namespaces).

---

## 2. Executive summary — the four planes

Draft dispositions from #254, verified against real generator behavior (findings in §3):

| Plane | Constructs | Verified disposition |
|---|---|---|
| **Data** | `type`, `choice`, `enum`, attributes, cardinality, `extends`, builtins, `[metadata]`, `typeAlias`, `recordType` | Mostly **existing graph** (SchemaNode/PropertyNode/CodeList/ExtendsSchema/ReferencesSchema), with two codegraph uplifts: PropertyNode cardinality bounds + PropertyNode annotations (§3.2, §3.4) |
| **Constraint** | `condition` (incl. auto-derived choice one-of), cardinality `min>1`/finite-max, `[ruleReference]` | **new node family** (ConditionNode) + validation generators — #261 |
| **Computation** | `func` (dispatch/aliases/`set`/`add`/post-conditions), reporting rule, eligibility rule | **new node families** (FunctionNode, RuleNode) + expr→Rust transpiler — #262/#263/#264 |
| **Regulatory** | `report`, `body`/`corpus`/`segment`, rule source, `schema`, `metaType`, library function, `[docReference]`, transforms | **metadata nodes + report/doc generators** — #265; library functions → transpiler stdlib registry |

Namespaces are **not** mapped onto domains: they become first-class via #267 (core/config/
validation) and #268 (bridging/generation). This table uses that model (§7.1).

---

## 3. Verified generator-behavior findings

These are the "must verify" items from #254, each now backed by code reading and an
empirical run (appendix A). They drive several dispositions below.

### 3.1 Inheritance: generators never compose from `ExtendsSchema` — flattening is the established pattern

- Ingestion **pre-flattens** allOf properties into the child's own `HasProperty` edges:
  `ingest_properties_from_schema` (`crates/codegraph/src/ingest/async_ingest.rs:541-585`)
  collects top-level + inline-allOf blocks, and `collect_allof_property_blocks`
  (`async_ingest.rs:872-908`) recursively follows `$ref` chains. Pass 4 additionally records
  `ExtendsSchema` edges with `composition_type: "allOf"` (`async_ingest.rs:825-867`).
- The mox bridge does the identical dual write: features flattened in allOf-canonical order
  (ancestors first — `ordered_bridge_features`/`collect_ancestor_features`,
  `crates/codegraph/src/ingest/mox_ingest.rs:976-1008`) plus `extends` → `ExtendsSchema`
  edges (`mox_ingest.rs:755-777`).
- **No generator traverses `ExtendsSchema` for properties.** `get_properties`
  (`crates/codegraph-grafeo/src/querier.rs:166-185`) is own-edges-only; consumers (SeaORM
  entity `crates/codegraph-generate/src/db/entity.rs:128`, DTO
  `crates/codegraph-generate/src/ddd/dto.rs:450`, repository emitter
  `crates/codegraph-generate/src/ddd/repository_emitter/emitter.rs:554`) dedup but never
  chase the edge. The edge is consumed only by `push_allof_children`
  (`querier.rs:2483-2529`), which models the allOf parent as a **separate child table** with a
  synthetic `{parent}_id` FK — deliberately *not* property inheritance.
- **Evidence E1** (appendix A): an allOf-`$ref` child generated `party_id`/`display_name`
  columns inline on the child table **and** a `gap_child_basepartytype` child table with FK.

**Decision (disposition for Rosetta `extends`)**: **flatten at ingest** — copy the super-type's
attributes onto the child (ancestor-first, matching mox), plus an `ExtendsSchema` edge for
lineage. Rationale: zero generator changes (all 60+ generators see the full attribute set via
the existing `get_properties()`), identical to both existing precedents, and it matches the
target SQL semantics (Rosetta materializes inherited fields as inline columns on the child's
table). Edge-only + generator-side traversal was considered and rejected: it changes the
`get_properties()` contract for every consumer including classification signals (`field_count`),
needs per-call-site override/merge policy, and still needs a second edge flavor because the
allOf child-table semantics differ from inline inheritance. Graph duplication is free — the
graph is rebuilt from source every run.

### 3.2 Cardinality: `min>1` / finite-max is lost today

- `PropertyNode` (`crates/codegraph-core/src/types/property.rs:78-127`) carries only
  `is_required`/`is_nullable`/`is_array` plus `min_length`/`max_length` and numeric
  `minimum`/`maximum`. Ingestion never reads `minItems`/`maxItems` (zero occurrences repo-wide).
- sigil's `Cardinality { min: u32, max: CardinalityMax (Finite | Unbounded) }` covers
  `(0..1)`, `(1..1)`, `(0..*)`, `(1..*)`, `(2..10)` …
- **Evidence E2**: `minItems: 2, maxItems: 10` produced a bare `bounded_tags TEXT[] NOT NULL`
  column — no CHECK, no validation artifact anywhere in 220 generated files.

**Disposition**: PropertyNode gains `min_items: Option<u32>` / `max_items: Option<u32>`
(new ingestion + classifier passthrough; no node-family change). Constraint *enforcement*
(validation generators, SQL CHECK) is the constraint plane's job — #261. `[0..1]` →
`is_required=false`, `[1..1]` → `is_required=true`, `*` → `is_array=true` map onto existing
flags; `min>1` and finite-max map onto the new fields.

### 3.3 Choice / one-of: silently dropped today

- No node or edge type represents oneOf. `SchemaNode.has_one_of`/`has_any_of` are plain flags
  (`crates/codegraph-core/src/types/schema.rs:36-37`); oneOf blocks are not collected as
  properties and produce no error. Sole semantic use: `is_enum = schema.has_one_of &&
  field_count == 0` (`querier.rs:312`).
- **Evidence E3**: the oneOf block's variant properties (`cash_amount`, `kind_code`) appear
  nowhere in the generated output.
- sigil side: a `choice` resolves to the same shape as a type (`kind: "Choice"`) with an
  **auto-derived one-of condition** `[{name: "Choice", expression: {kind: "OneOf", …}}]`
  (verified over `tests/conformance/model/choices/case.rosetta`, E7).

**Disposition**: choice shells bridge as SchemaNodes (data plane, existing graph — #256);
the one-of condition flows to the **ConditionNode family** (#261) as a constraint-plane
construct with a canonical `Expr::to_json` payload (§3.6).

### 3.4 Metadata: `custom_annotations` exist on SchemaNode only

- `SchemaNode.custom_annotations: HashMap<String, serde_json::Value>`
  (`crates/codegraph-core/src/types/schema.rs:43-44`), populated from top-level `x-*` keys
  (`async_ingest.rs:335-344`). **PropertyNode has no annotations field.**
- **Evidence E4**: property-level `x-metadata-*` keys surface nowhere in the generated output.
- Key conventions today: `source = "mox"` (const `MOX_SOURCE`, `schema.rs:8`; the
  `is_mox_sourced` check at `async_ingest.rs:1486-1492` skips mox nodes in re-classification)
  and atproto `selfLabels`.

**Decisions**:
1. **PropertyNode gains `custom_annotations`** (mirroring SchemaNode). `[metadata id/key/
   scheme/reference/location/address]` occurrences become structured keys
   (`metadata.id`, `metadata.scheme`, …) on the property — future generators can promote them
   to columns if needed, nothing is lost at bridge time.
2. Rosetta provenance uses a **distinct key** `origin = "rosetta"` — never `source`, so
   `is_mox_sourced` and the mox priority-0 classification bypass are untouched (#255's
   requirement; Rosetta types stay auto-scored per #258).

### 3.5 Namespaces: not domains (per #267/#268)

Rosetta `namespace a.b` + `import a.b.* as x` do **not** become domains. They become
`NamespaceNode { fqn, parent, source }` + `NamespaceParent`/`NamespaceImports` edges; domain
assignment is optional, many-to-one, config-declared. Precedent: an atproto `Namespace` node
type already exists (`crates/codegraph-grafeo/src/schema_ddl.rs:33`) and can be generalized by
#267. See §7.1 for the title-collision rules this interacts with.

### 3.6 Expression typing and `Expr::to_json`

- Sigil parses and resolves but **does not type-check or interpret** expressions. The
  transpiler (#262) owns semantics. Honest scoping precedent: `lower_when_expr`
  (`crates/codegraph-generate/src/db/expr_sql.rs`) lowers a **closed, documented subset** of
  rex-expr to SQL and hard-errors naming the construct on anything else — the same
  "Cedar-backend" philosophy applies to the expr→Rust transpiler.
- `Expr::to_json` round-trip stability is **proptest-verified** in sigil
  (`crates/sigil-model/tests/roundtrip.rs`, printed ↔ reparse ↔ JSON equality, 256 cases,
  E6: passing at `38bf91f`). Canonical `Expr::to_json` payloads are therefore safe to embed
  in ConditionNode/FunctionNode/RuleNode payloads.

---

## 4. Data plane — construct dispositions

| Construct (sigil surface) | Disposition | Graph home | Generator impact | Issue | Notes / evidence |
|---|---|---|---|---|---|
| `type` (`Data`, `is_choice=false`) with attributes | existing graph | SchemaNode + PropertyNode (`HasProperty`) | none beyond bridge | #256 | mox-bridge pattern; provenance `origin=rosetta` |
| `choice` (`kind: "Choice"`) | existing graph (shell) + constraint plane (condition) | SchemaNode (`has_one_of=true`) + ConditionNode payload | none | #256, #261 | auto-derived one-of condition → §5; E7 |
| `enum` (+ value `displayName`) | existing graph | CodeList + EnumValue{value, display_name} | codelist generators already render display names | #256 | mox enum precedent `mox_ingest.rs:945-949` |
| attributes (`Attribute`) | existing graph | PropertyNode | none | #256 | attribute surface: annotations, cardinality, docReferences, labels, ruleReferences, override (E7) |
| cardinality `(0..1)`, `(1..1)`, `0..*`, `1..*` | existing graph | `is_required`/`is_array` flags | none | #256 | `(2..10)`-style bounds → next row |
| cardinality `min>1` / finite max, e.g. `(2..10)` | **generator feature (codegraph uplift)** | PropertyNode.`min_items`/`max_items` (new fields) | validators/SQL CHECK land with #261 | #261 | E2: bounds fully lost today (§3.2) |
| `extends` (single super-type) | existing graph, **flatten at ingest** | child PropertyNodes (ancestor-first) + `ExtendsSchema` edge | none (decisive: §3.1) | #256 | E1; multi-level chains and cycle guard mirror mox `collect_ancestor_features` |
| builtin `int`, `number`, `string`, `date`, `dateTime`, `time`, `boolean` | existing graph | classifier primitive mappings | primitive_wrappers/classifier | #256, #258 | `number(digits, fractionalDigits, min, max)` and `string(minLength, maxLength, pattern)` parameters → existing min/max fields; `pattern` → open decision D3 |
| `time` / `zonedDateTime` | existing graph | classifier primitives (PG `TIME` / `TIMESTAMPTZ`) | classifier mapping additions | #256 | verify classifier primitive set covers both |
| `recordType date/dateTime/zonedDateTime` (builtins) | existing graph | primitive mappings (not user types) | none | #256 | resolved to builtins before bridging |
| user `recordType` | existing graph (rare) | SchemaNode + PropertyNode | none | #256 | same as `type` |
| `typeAlias` | existing graph (transparent) | — (resolved by sigil-resolve pre-bridge) | none | #256 | aliases are expanded during resolution (E7: `int = number(...)`, `calculation`); nothing reaches the graph |
| `BasicType` declarations | existing graph | primitives; parameters → PropertyNode constraint fields | none | #256 | builtin surface fixed by `basictypes.rosetta` |
| `[metadata id/key/scheme/reference/location/address]` (type level) | annotation payload | SchemaNode.custom_annotations (`metadata.*` keys) | future: key/scheme → unique index/reference columns (deferred) | #256, #265 | §3.4 decision 1 |
| `[metadata …]` (attribute level) | **generator feature (codegraph uplift)** | PropertyNode.custom_annotations (new field, `metadata.*` keys) | future promotion to columns deferred | #256 | E4: no home today (§3.4) |

## 5. Constraint plane

| Construct | Disposition | Graph home | Generator impact | Issue | Notes |
|---|---|---|---|---|---|
| `condition` (named boolean expr on a type) | **new node family** | ConditionNode + `HasCondition` edge (payload = canonical `Expr::to_json`) | first validation generator: Rust `validate` fns; SQL CHECK for trivial cases; TS zod later | #261 | IFML-family precedent for core+traits+mock+grafeo plumbing |
| choice one-of (auto-derived `Choice` condition) | **new node family** (same) | ConditionNode | validator: exactly-one-variant check | #261 | §3.3 |
| cardinality `min>1` / finite max | generator feature | PropertyNode.`min_items`/`max_items` | SQL CHECK / validator emission | #261 | resolves §3.2 gap |
| `[ruleReference …]` (attribute) | annotation payload (now) + validation hook (later) | PropertyNode.custom_annotations (`rule_references` array) | validator references rule names once RuleNode lands | #261, #264 | attribute surface carries `ruleReferences` (E7) |

## 6. Computation plane

| Construct | Disposition | Graph home | Generator impact | Issue | Notes |
|---|---|---|---|---|---|
| `func` (`Function`: inputs/output/dispatch/`set`+`add` operations/aliases (`shortcuts`)/post-conditions/superFunction/transform) | **new node family** | FunctionNode + structured child nodes (IFML precedent) | func → generated Rust free functions; dispatch head → `match`; aliases → let-bindings; post-conditions → debug_assert (config-gated) | #263 | verified Function surface, E7; transform annotations ([ingest]/[enrich]/[projection]) carried as metadata (#265 consumes) |
| expr→Rust transpiler (all 50 `Expr` variants, §8) | generator feature | — | typed-emission strategy decision is #262 slice 0 | #262 | closed-subset + hard-error philosophy (§3.6); slices per variant table |
| reporting rule (`rule` with `eligibility=false`) | **new node family** | RuleNode{input, expression payload, kind} | computed-field functions attached to entities | #264 | E7: `Rule.eligibility` flag distinguishes kinds |
| eligibility rule (`rule` with `eligibility=true`) | **new node family** (same) | RuleNode | endpoint guards (ApiOperation precedent from OpenAPI ingest) | #264 | #265 Report.eligibilityRules references these |

## 7. Regulatory plane

| Construct | Disposition | Graph home | Generator impact | Issue | Notes |
|---|---|---|---|---|---|
| `report` (`Report`: inputType, regulatoryBody, reportType, ruleSource, eligibilityRules) | **new node family** | ReportNode + refs to Body/Corpus/Segment/ExternalRuleSource/RuleNode | report-generation scaffolding: endpoint + rule dispatch per corpus | #265 | verified surface, E7 |
| `body` / `corpus` / `segment` | **new node family** | BodyNode/CorpusNode/SegmentNode (regulatory reference metadata) | doc/reference generators | #265 | E7: Body `CDRBody`, Corpora `ESMA`/`CFTC`, Segments with `reference: "1.a"` |
| rule source (`ExternalRuleSource`) | **new node family** | ExternalRuleSourceNode | provenance metadata in report scaffolding | #265 | |
| `schema` (Rosetta `schema` declaration) | annotation payload | SchemaNode.custom_annotations or small metadata node — see open decision D1 | none initially | #265 | name-clash with codegraph `Schema` node type must be avoided in DDL |
| `metaType` (`MetaType`) | **new node family** (metadata) | MetaTypeNode | none initially | #265 | distinct from the builtin `metadata` annotation |
| library function (`LibraryFunction`, 6 builtins) | generator feature | signature registry (transpiler stdlib) — optionally a graph node later | transpiler stdlib mapping (Min/Max/Adjust/Within/IsLeapYear/DateRanges) | #262, #265 | no persistence semantics; registry first |
| `[docReference Body Corpus (Segment "ref") …]` | annotation payload | SchemaNode/PropertyNode.custom_annotations (`doc_references`) | report/doc generators | #256 (capture), #265 (render) | **sigil grammar gap**: bare-string segments (`"S1" "S2"`) fail to parse (E7, conformance `model/doc-refs` expects E0001); codegraph must not treat docReferences as load-bearing until upstream settles (D4) |
| transforms (`[ingest]` / `[enrich]` / `[projection]`, `Function.transform`) | annotation payload | FunctionNode metadata | API-ingest hooks | #265 | verified `transform` field on Function (E7) |

---

## 8. Expr variant disposition table (50 variants)

All variants are handled by the expr→Rust transpiler (#262); slices as scoped there.
Payloads everywhere are canonical `Expr::to_json` (§3.6). "S1/S2/S3" = #262 slice 1/2/3.

### Family A — literals (S1)

| Variant | Disposition | Notes |
|---|---|---|
| `BooleanLiteral` | S1 | direct |
| `StringLiteral` | S1 | direct |
| `NumberLiteral` | S1 | decimal text → `f64`/`Decimal` (emit-time decision) |
| `IntLiteral` | S1 | direct |
| `ListLiteral` | S1 | `vec![…]` |

### Family B — references & calls (S1)

| Variant | Disposition | Notes |
|---|---|---|
| `SymbolReference` | S1 | fn params / aliases / implicit `this` |
| `FeatureCall` | S1 | receiver.field → field access / method call |
| `DeepFeatureCall` | S1 | multi-hop path |

### Family C — scalar operators (S1)

| Variant | Disposition | Notes |
|---|---|---|
| `ArithmeticOperation` | S1 | `+ - * /`; Decimal-aware emission |
| `LogicalOperation` | S1 | `&& \|\| !` |
| `EqualityOperation` | S1 | incl. `null` comparisons (expr_sql precedent) |
| `ComparisonOperation` | S1 | `< <= > >=` |

### Family D — membership (S2)

| Variant | Disposition | Notes |
|---|---|---|
| `ContainsExpression` | S2 | `.contains(&x)` / `.iter().any()` |
| `DisjointExpression` | S2 | `.iter().all(\|i\| !rhs.contains(i))` helper |

### Family E — existence & cardinality (S1)

| Variant | Disposition | Notes |
|---|---|---|
| `ExistsExpression` | S1 | `!x.is_empty()` / `.iter().any(...)` |
| `AbsentExpression` | S1 | negation of exists |
| `OnlyExistsExpression` | S1 | `.iter().all(\|i\| p(i))` |
| `OnlyElement` | S1 | single-or-error accessor (Result vs panic — slice 0 decision) |
| `CountOperation` | S1 | `.len()` |

### Family F — collection algebra (S2)

| Variant | Disposition | Notes |
|---|---|---|
| `FilterOperation` | S2 | `.into_iter().filter(…)` |
| `MapOperation` | S2 | `.map(…)` |
| `ReduceOperation` | S2 | `.fold(…)` |
| `SortOperation` | S2 | `sort_by_key` / `sort_by` (typed comparator) |
| `FlattenOperation` | S2 | `.flatten()` |
| `DistinctOperation` | S2 | needs `PartialEq`/`Hash` discipline per type |
| `ReverseOperation` | S2 | `.rev()` / `.reverse()` |
| `FirstOperation` | S2 | Option semantics |
| `LastOperation` | S2 | Option semantics |
| `SumOperation` | S2 | Decimal-aware |
| `MinOperation` | S2 | Option on empty |
| `MaxOperation` | S2 | Option on empty |

### Family G — control flow (S2)

| Variant | Disposition | Notes |
|---|---|---|
| `ConditionalExpression` | S2 | `if/else` expression |
| `SwitchOperation` | S2 | `match` |
| `ThenOperation` | S2 | sequencing (let-chain / block) |
| `OneOfOperation` | S2 | exactly-one predicate (choice validator kernel) |
| `ChoiceOperation` | S2 | choice-type construction |

### Family H — casts (S2)

| Variant | Disposition | Notes |
|---|---|---|
| `ToStringOperation` | S2 | `.to_string()` |
| `ToNumberOperation` | S2 | parse → Result |
| `ToIntOperation` | S2 | parse/truncation semantics documented |
| `ToTimeOperation` | S2 | chrono `NaiveTime` |
| `ToEnumOperation` | S2 | enum lookup, unknown-value policy documented |
| `ToDateOperation` | S2 | chrono `NaiveDate` (expr_sql `date("…")` precedent) |
| `ToDateTimeOperation` | S2 | chrono `NaiveDateTime` |
| `ToZonedDateTimeOperation` | S2 | `DateTime<Utc>`/`DateTime<FixedOffset>` — generated-stack type policy needed |

### Family I — misc / metadata-adjacent (S3 stubs, documented semantics gaps)

| Variant | Disposition | Notes |
|---|---|---|
| `DefaultOperation` | S2 | `.unwrap_or(default)` |
| `JoinOperation` | S2 | `.join(sep)` |
| `AsKeyOperation` | S3 | stub + documented gap (keyed identity) |
| `WithMetaOperation` | S3 | stub; interacts with `[metadata]` annotations (§4) |
| `AsOperation` | S3 | stub; typed view semantics |
| `ConstructorExpression` | S3 | struct literal init — actually low-risk; may promote to S2 if slice 3 shrinks |

Slice totals: S1 = 17, S2 = 29, S3 = 4. Each slice ships conformance fixtures against sigil's
expression corpus (`tests/conformance/expressions/wave1-4`, `tests/oracle/expressions.rosetta`).

---

## 9. Annotation surface disposition table

| Annotation | Home | Generator relevance | Issue |
|---|---|---|---|
| `metadata` (id/key/reference/scheme/location/address) | SchemaNode / PropertyNode `custom_annotations` (`metadata.*`) | key/scheme → index/reference columns later (deferred) | #256 |
| `rootType` | `custom_annotations["rootType"]=true` | already mirrors `role = "root"` in domains.toml entity_config — bridge may also emit `entities` hints | #256, #258 |
| `qualification` | `custom_annotations["qualification"]` | event/product qualification → guards (#264 adjacency) | #264, #265 |
| `deprecated` | `custom_annotations["deprecated"]=true` | doc notes; optional generator warnings | #256 |
| `calculation` (type alias + annotation) | alias resolves pre-bridge; annotation → `custom_annotations` | none initially | #256 |
| `codeImplementation` | `custom_annotations` | suppresses func body generation expectations | #263 |
| `externalConfig` | `custom_annotations` | pairs with transform annotations | #265 |
| `suppressWarnings` / `suppressUnused` | `custom_annotations` | editor-side only (sigil-lsp); no codegraph behavior | — |
| `[docReference …]` | `custom_annotations` (`doc_references`) | report/doc generators | #265 (§7 grammar-gap note) |
| `[ruleReference …]` | PropertyNode `custom_annotations` (`rule_references`) | validation hooks once RuleNodes land | #261, #264 |
| `[ingest]` / `[enrich]` / `[projection]` | FunctionNode transform metadata | API-ingest hooks | #265 |

Provenance for all bridged nodes: `custom_annotations["origin"] = "rosetta"` — distinct from
`source = "mox"` so `is_mox_sourced` (`async_ingest.rs:1486-1492`) and the classification
priority-0 bypass stay untouched (#255, #258).

---

## 10. Cross-cutting sections

### 10.1 Namespaces and title collisions

- Namespaces follow #267/#268: `namespace a.b` + `import` declarations → NamespaceNode +
  NamespaceImports edges; domain assignment is config-declared, many-to-one. The bridge
  (#256) writes them only after #267 lands (hard dep recorded on #256).
- **Title collisions JSON ↔ Rosetta**: same rule as mox — the Rosetta pass runs before the
  JSON-schema pass and seeds the skip set (`ingest_schemas_with_skips`,
  `async_ingest.rs:83-103`); Rosetta wins by ordering, mirroring `driver.rs:286-344`.
- **Suffix rule**: codegraph strips a configurable `Type` suffix
  (`crates/codegraph-naming/src/lib.rs:16-21`; Tera `upper_camel` hardcodes `"Type"`).
  Rosetta type names conventionally already end in no suffix; bridged titles go through the
  same `strip_suffix` + `rust_type`/`pg_table_name`/`api_path_segment` derivation as JSON and
  mox, so collision behavior is uniform across sources.
- **Generation-order title dedup** lives in `crates/codegraph-generate/src/lib.rs:2228,
  2285-2319` (`seen_titles`/`title_claim_domain`) — first domain to claim a title wins; with
  namespaces (#268) this gains a namespace-qualified variant. (Note: AGENTS.md still points
  at `generate/mod.rs:1035-1078`; stale.)

### 10.2 Classification interplay (#258 checklist)

- `SchemaClassificationData.source` stays **unset** for rosetta nodes → no scoring bypass;
  rosetta types are auto-scored like JSON schemas (decision recorded in #258).
- Scoring signals (`in_degree`, `field_count`, `composes_noun_type`, `has_all_of`) operate
  unchanged on rosetta-derived graphs; flattening (§3.1) means `field_count` includes
  inherited attrs — same as allOf/mox today.
- `force_entities` / `force_value_objects` in domains.toml must be honored for rosetta titles.
- `reclassify_with_entities` must not skip rosetta nodes (only `is_mox_sourced` skips).
- Choices: all-`(0..1)` attributes → VO scoring needs a sanity check post-bridge (#258
  revisit item; E7 corpus case `model/choices` is the fixture).

### 10.3 `Expr::to_json` payload convention

All constraint/computation payloads are stored as canonical `Expr::to_json` JSON (serde
tag = `kind`), regenerated identically from source each run. Round-trip stability is
proptest-pinned upstream (E6). Consumers must treat payloads as opaque except the transpiler
and (later) validation generators.

---

## 11. Gaps requiring codegraph work independent of Rosetta

These are codegraph uplifts surfaced by this analysis (they benefit mox/JSON too and land as
part of the sub-issues shown):

1. **PropertyNode cardinality bounds** `min_items`/`max_items` (E2) — #261.
2. **PropertyNode.custom_annotations** (E4) — #256 (bridge writes it), used by #261/#265.
3. **ConditionNode/FunctionNode/RuleNode/ReportNode(+regulatory) node families** — #261/#263/#264/#265 (core types + EdgeType variants + trait methods with default-Err impls + MockEngine + grafeo DDL/ingestor/querier + CachingQuerier delegation, IFML precedent).
4. **Namespace uplift** — #267/#268 (generalizes the existing atproto `Namespace` node precedent).

---

## 12. Open decisions (to close at sign-off)

| # | Decision | Recommendation |
|---|---|---|
| D1 | Rosetta `schema` element: `custom_annotations` vs small metadata node | annotation payload first; promote only if report generators need to query it (#265) |
| D2 | Transpiler typed-emission strategy: minimal local inference vs untyped + annotations | decide in #262 slice 0, informed by §8 slice contents; expr_sql closed-subset philosophy is the fallback |
| D3 | `string(pattern)` constraint: PropertyNode.`pattern` now, or defer to validators (#261) | carry as `custom_annotations["pattern"]` at bridge; enforce via #261 validators |
| D4 | docReference bare-string segments don't parse in sigil (E0001) | file upstream sigil issue; codegraph stores docReferences as advisory payloads only |
| D5 | `ToZonedDateTimeOperation` target type in generated stack | pick `DateTime<Utc>` + explicit offset policy in #262 slice 2 |
| D6 | Choice bridging: keep `has_one_of` flag in sync AND write ConditionNode? | yes — flag for classification compat, ConditionNode for semantics (#258 + #261) |

---

## Appendix A — Verification evidence (E1–E7)

Run context: codegraph `e6def684` (tag `0.2.0`), debug binary rebuilt at HEAD; sigil `38bf91f`.
Fixtures: `/tmp/opencode/rosetta-gap/fixtures/` (schemas `common/BasePartyType.json`,
`gap/GapChildType.json`, `gap/CollisionType.json`, `model/gap.mox`, minimal
domains/classifier). Output: `/tmp/opencode/rosetta-gap/out/` (220 files, 0 errors).

| # | Experiment | Result |
|---|---|---|
| E1 | allOf-`$ref` inheritance (GapChildType extends BasePartyType) | Child table `gap.gap_child` contains parent columns `party_id UUID NOT NULL`, `display_name TEXT` (flattened) **and** allOf parent materialized as child table `gap.gap_child_basepartytype` with `fk_..._parent FOREIGN KEY (gap_child_id)` — dual-write confirmed (§3.1) |
| E2 | `bounded_tags: array, minItems 2, maxItems 10` | Emitted `bounded_tags TEXT[] NOT NULL`; grep across all 220 outputs finds **zero** `minItems`/`maxItems`/array-length CHECK artifacts — bounds lost (§3.2) |
| E3 | oneOf block with variant props `cash_amount`/`kind_code` | grep across output: **zero** occurrences — oneOf variants silently dropped (§3.3) |
| E4 | top-level `x-meta` + property-level `x-metadata-id`/`x-metadata-scheme` | Property-level keys surface nowhere in output (no column/comment/dto artifact). SchemaNode-level `x-*` → custom_annotations confirmed by code (`async_ingest.rs:335-344`) (§3.4) |
| E5 | mox class `CollisionType` + JSON schema `CollisionType` | Log: `INFO: entity 'CollisionType' covered by .mox, skipping gap/CollisionType.json`; generated table has `mox_field`/`json_twin_note`, **no** `json_only_field` — skip-set/mox-wins confirmed (§10.1) |
| E6 | sigil roundtrip proptest | `cargo test -p sigil-model --test roundtrip` → `printed_expressions_reparse_to_the_same_ir ... ok` (§3.6) |
| E7 | sigil conformance corpus (`sigil model` over `tests/conformance/`) | Full construct inventory: Data/Choice (auto one-of condition), Enumeration, Annotation ×9, TypeAlias (resolved away), BasicType ×5 (param constraints), RecordType ×3, LibraryFunction ×6, Function (inputs/output/dispatch/operations incl. `add`, postConditions, shortcuts, superFunction, transform), Rule (`eligibility` flag), Report (regulatoryBody{body,corpora,segments}, reportType, ruleSource, eligibilityRules), Body/Corpus/Segment/MetaType/ExternalRuleSource. **Found**: doc-refs case parses with captured `E0001` (bare string segments unsupported) — D4 |

## Appendix B — Sub-issue dependency map (post-disposition)

```
#254 (this doc) ─┬─> #255 sigil dep wiring (needs sigil#3; key choice settled: origin=rosetta)
                 ├─> #256 data-plane bridge ──> #257 CLI/driver ──> #260 lifecycle
                 │         └──> #258 classification ──┐
                 │         └──> #261 constraint plane (needs PropertyNode uplifts §11.1-2)
                 │                  └──> #262 transpiler S1-S3 ──> #263 FunctionNode ──┐
                 │                  └────────────────────────────────> #264 RuleNode <-┘
                 │                                            #265 regulatory plane <-┘
                 └─> #267 namespaces core (independent; parallel) ──> #268 bridging/generation
                                       (#256 namespace writes depend on #267)
#259 test suite <- #256 + #257 + #258      #266 docs <- #259
```
