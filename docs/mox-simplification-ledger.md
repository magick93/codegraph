# mox-first Simplification Ledger

Epic #228 ("mox-first"), sub-issue #233.

## Why this ledger exists

We own the mox language. We do not own JSON Schema.

JSON Schema is a loose, descriptive format: it says what a JSON document can
look like, not what a domain means. Everything in the pipeline that *guesses*
— is this type an entity or a value object? is this `$ref` a wrapper, a
codelist, or a foreign key? is this nested object a child table or a JSONB
blob? — exists to recover intent that JSON Schema never stated. `.mox`
replaces those guesses with declarations: `class`/`enum`/`datatype`, `refers`
vs `contains` vs attribute, multiplicity `[1]` vs `[0..*]`, `extends`.

Once `.mox` is the primary (eventually sole) model source, every inference
machine below loses its reason to exist. This ledger is the deletion roadmap:
each entry records what the machinery does today (with file:line anchors),
why it exists (the JSON Schema looseness it papers over), the mox construct
that replaces it, and its simplification status.

Status vocabulary:

- **Deletable** — only exercised by JSON-Schema-sourced models; can be removed
  once mox is the sole input.
- **Shrinkable** — still needed for legacy JSON input and mixed
  `--mox-files`/`--schemas` runs, but its scope shrinks to that bridge role.
- **Keep** — no mox construct replaces it yet; deleting it would lose
  capability (the mox language would need to grow first).

Safety net: `crates/codegraph/tests/mox_equivalence_tests.rs` pins **full-tree
byte-identity** between the same model authored as JSON Schema and as `.mox`
(issue #233). Any deletion below can be verified against that harness — the
mox path must stay byte-identical with the mox bridge's declared-mapping
output, and the JSON path may age without dragging the mox path with it.

## Inventory

### 1. Classifier wrapper tiers — `classify_ref`

- **Today**: `crates/codegraph-classifier/src/classify.rs:73-219` runs a
  six-tier cascade over a `$ref` target: primitive wrappers (line 80),
  structured wrappers (92), array wrappers (117), range wrappers (129),
  composite wrappers (141), media wrappers (170), then codelist path sniffing
  (197), then the entity/VO fallback (213). Which tier applies is decided by
  **name lookup in `classifier.toml`** (`primitive_wrappers`,
  `structured_wrappers`, `array_wrappers`, `range_wrappers`,
  `[[composite_wrappers]]`, `[media_wrappers]`) — per-project configuration
  that maps wrapper-typed schemas to storage shapes.
- **Why**: JSON Schema cannot say "this is a money value" or "this is a photo
  reference". Real-world schemas (HR-Open etc.) express these as *wrapper
  objects*, and the only signal left is the schema's name plus a hand-maintained
  config listing each wrapper type and its columns.
- **Mox replacement**: a stored feature typed with a mox `datatype` declares
  its storage format explicitly (`format "uuid"`, `"date"`, `"date-time"`,
  `"email"`, `"uri"`, `"json"`, `"decimal"`); `contains`/`refers` declare
  composition vs reference; enums declare codelists. The bridge
  (`crates/codegraph/src/ingest/mox_ingest.rs:694`, `feature_property`)
  maps these directly to `RefClassificationKind` with no name-based tier
  cascade and no classifier.toml.
- **Status**: **Shrinkable** → **Deletable** for mox-only projects. The
  wrapper tables stay loadable while any consumer still feeds `--schemas`
  (mixed runs and the `codegraph migrate` transition path depend on them);
  once mox is the only input, `classify_ref` collapses to the two branches
  the bridge actually produces (entity ref vs value object — and even that
  choice is already made by `refers`/`contains` before classification runs).

### 2. `classify_plain_type`'s JSONB-for-objects guess

- **Today**: `crates/codegraph-classifier/src/classify.rs:275-301` — any
  property whose schema is not a recognized scalar and has no `$ref` falls
  into a catch-all: `ValueObject` classification with a **JSONB** column and
  `serde_json::Value` domain type. The caller cannot distinguish "author
  meant an inline object value" from "author left the type unspecified" —
  both get an untyped JSONB column.
- **Why**: JSON Schema's `type: "object"` for an inline property is
  ambiguous between a structured value (→ child table) and free-form JSON.
  The guess (JSONB) is the safe default because nothing else is known.
- **Mox replacement**: inline objects do not exist. A structured value is a
  `contains` feature targeting a declared class (→ child table with typed
  columns, via `feature_property`'s `Mapped::Reference { kind: ValueObject }`).
  Free-form JSON is `datatype` with `format "json"` (→ JSONB, deliberately).
- **Status**: **Deletable** once mox-only. Every JSONB column in a mox-sourced
  model is a declaration, never a guess.

### 3. AutoClassifier structural scoring

- **Today**: `crates/codegraph/src/classify/scoring.rs:23-113`
  (`score_structural`) scores each schema on graph-shaped proxies: in-degree
  (`0 → +2 VO`, `1..2 → +1 entity`, `3+ → +3 entity`), field count
  (`>= 8 → +2 entity`, `<= 3 → +2 VO`), `composes_noun_type` (+2 entity — a
  naming heuristic), `has_all_of` (+2 entity), with hard-VO bypasses for
  primitive wrappers, codelists and enums. Net score `>= 4` wins entityhood.
  The threshold semantics ("boundary score 3 is VO, 4 is entity") encode
  tuned judgment calls about a *specific* schema corpus (HR-Open), not a
  general truth.
- **Why**: JSON Schema files do not say which types are entities. A
  `PositionType` with 12 fields and 5 inbound references is *probably* an
  entity; a `NameType` with 3 fields and none is *probably* a value object.
  Structural scoring approximates the missing declaration from usage
  statistics of the graph.
- **Mox replacement**: authorship. A class targeted only by `contains` is a
  value object; anything `refers`-targeted (or standing alone) is an entity —
  decided at bridge time in `mox_ingest.rs:213-233` (`contains_targets` /
  `refers_targets` analysis, class bridge pass 1), recorded as
  `source=mox` provenance, and never re-derived (`reclassify_with_entities`
  skips mox schemas; the AutoClassifier honors the override with reason
  `override:source=mox`, `crates/codegraph/src/classify/mod.rs:78-89`).
- **Status**: **Shrinkable** → **Deletable**. Keep while `--schemas` is
  accepted. Note the equivalence harness needed `entities = [...]` in
  domains.toml *only for the JSON side* — the mox side reached the same
  decision from the model alone. That asymmetry is the whole argument.

### 4. Naming rules + path exclusions

- **Today**: `crates/codegraph/src/classify/naming_rules.rs:6-25`
  (`apply_naming_rules`) substring-matches titles against per-project rules
  from classifier.toml (`"Inclusion"` → hard VO, `"Report"` → soft VO, ...)
  and mutates the VO score. `should_exclude_by_path` (35-41) drops anything
  under `meta/`, `search/`, `samples/`; `SchemaLoader` additionally skips
  those directories at load time (`crates/codegraph/src/ingest/schema_loader.rs:56-63`).
- **Why**: vendor schema trees carry non-model artifacts (sample payloads,
  search projections, metadata envelopes) that *look* like types. Path and
  name sniffing are heuristics to keep junk out of the domain model.
- **Mox replacement**: nothing needs replacing — a mox package contains only
  what the author declared. There are no sample files, no search projections,
  no `*Inclusion` naming conventions to defend against. Junk cannot enter a
  model you compile.
- **Status**: **Deletable** for mox-only input (the load-time skip already
  no-ops — mox files bypass `SchemaLoader` entirely). The rules tables remain
  part of classifier.toml parsing while JSON input is supported.

### 5. allOf flattening + `dedup_fields()`

- **Today**: the JSON path *flattens inheritance at ingest*:
  `ingest_properties_from_schema` collects parent `$ref` property blocks and
  merges them into the child schema's own property list
  (`crates/codegraph/src/ingest/async_ingest.rs:469-494`), recursively via
  `collect_allof_property_blocks` (781-822). Because the same columns then
  arrive twice (merged props **and** the `ExtendsSchema` composition edge),
  `CompositionNode::dedup_fields`
  (`crates/codegraph-core/src/types/composition.rs:126-145`) removes
  duplicates by name at query time — with the carefully documented
  independent-HashSets-per-category subtlety (columns vs children) that a
  naive implementation gets wrong (regression documented in AGENTS.md,
  commits `3305f8b`→`3e82ec6`).
- **Why**: JSON Schema's `allOf` is set-union semantics with no identity:
  the pipeline must both keep the composition edge (for DDL trees) and
  flatten the properties (for SeaORM/DTO field lists, which read
  `get_properties` without traversal), then repair the duplication it just
  created.
- **Mox replacement**: the bridge now performs the same allOf-canonical
  merge exactly once — `ordered_bridge_features` +
  `collect_ancestor_features` (`mox_ingest.rs:643-687`) ingest inherited
  features before own features, first-occurrence-wins — so child schemas
  carry their inherited properties the way the generators expect, without
  duplicate graph nodes. The mox declaration is simply
  `class Vehicle extends Asset`.
- **Status**: **Shrinkable**. The JSON path keeps its flatten+dedup pair
  (its input genuinely has the duplication); the mox path never creates it.
  Once JSON input is gone, `dedup_fields()` loses its raison d'être and the
  flattening logic reduces to the bridge's single ordered walk.

### 6. `force_entities` / `force_value_objects` / `entities` override lists

- **Today**: `crates/codegraph-config/src/config.rs:138,141` define
  per-domain `force_entities`/`force_value_objects`; the AutoClassifier
  applies them as priorities 4/5
  (`crates/codegraph/src/classify/mod.rs:109-127`), and the legacy
  `entities = [...]` list is merged unconditionally into the entity set
  (`crates/codegraph/src/driver.rs:412-417`). The equivalence harness uses
  exactly these to make the JSON side agree with what mox declares for free.
- **Why**: when the structural scorer (item 3) guesses wrong, the only
  recourse is a manual override list in domains.toml — an admission that the
  inference layer needs human patching per project.
- **Mox replacement**: the declaration IS the model (`refers` wins,
  containment-only ⇒ VO). No overrides were needed or consulted for the mox
  side of the equivalence model.
- **Status**: **Shrinkable** → **Deletable** (JSON-input era only).

### 7. Codelist path sniffing (`codelist/`)

- **Today**: `crates/codegraph-classifier/src/classify.rs:197-210` — a `$ref`
  whose *path string* contains `codelist/` becomes a codelist reference (or a
  codelist CHECK via `codelist_as_check.schemas`). The `is_codelist` schema
  flag this sets drives: codelist seed migrations
  (`crates/codegraph-generate/src/db/codelist.rs:74`), FK targets routed to
  the `common` schema (`resolve_fk_target`,
  `crates/codegraph-grafeo/src/querier.rs:2562-2564`), codelist column
  injection for entity models, and Rust enum emission.
- **Why**: directory layout is the only marker distinguishing "enum-valued
  lookup table" from "another object". Move the file out of `codelist/` and
  the type silently degrades to a value object.
- **Mox replacement**: `enum WorkOrderStatus { ... }` — and since #233, the
  bridge produces the identical graph shape (codelist `SchemaNode` with
  `classification=codelist` + `ReferencesSchema`/`ItemsOf` edges from the
  referencing features, `mox_ingest.rs:371-393, 457-478`), byte-identically
  (pinned by the equivalence harness). No path, no sniffing.
- **Status**: **Deletable** for mox-only input once the `codelist_as_check`
  vocabulary has a mox story (a rendering hint on the enum; until then the
  CHECK-variant stays config-driven).

### 8. Inline-enum threshold

- **Today**: a property carrying an inline `"enum": [...]` array is turned
  into a synthetic codelist named `{Domain}{Entity}{Prop}`
  (`async_ingest.rs:933-958`), classified via
  `classify_inline_enum` (`classify.rs:221-227`): `InlineEnum` (CHECK
  constraint) if `values.len() <= inline_enum_threshold` (classifier.toml,
  default 20), else `CodelistReference` (lookup table). The size of the value
  list silently changes the storage strategy and generated artifacts.
- **Why**: JSON Schema has no named enumeration; whether the values deserve
  their own table has to be *inferred from cardinality*.
- **Mox replacement**: every enumeration is a named `enum` declaration —
  always a `CodeList`/`EnumValue` set with explicit labels and ordering
  (`mox_ingest.rs:196-226`, enum ingestion loop). The author decides; no
  threshold.
- **Status**: **Deletable** (JSON-input era only). The threshold config key
  survives as long as `--schemas` does.

### 9. Enum display names by positional pairing

- **Today**: `ingest_codelist_values` (`async_ingest.rs:868-887`) pairs
  `enum` values with `enumNames` display strings **by array index** — a
  silent-corruption hazard if the two arrays drift out of sync.
- **Mox replacement**: `Open as "Open" = 0` binds code, label and sort order
  in one declaration; the bridge ingests the triple directly.
- **Status**: **Deletable** (JSON-input era only).

### 10. Domain-from-path rule, `$ref` stem extraction, ref-string FK fallback

- **Today**: three string-archaeology helpers exist only because JSON files
  live in directories and reference each other by relative path:
  - `extract_domain_from_path` (`schema_loader.rs:257-267`) — the domain is
    the path segment before `/json/` (or the first segment);
  - `extract_ref_stem` (`async_ingest.rs:1090-1100`) — strips `.json#`,
    `.schema.json`, path prefixes from `$ref` strings to find target titles;
  - `resolve_fk_target`'s last-resort branch
    (`crates/codegraph-grafeo/src/querier.rs:2579-2604`) — when graph edges
    are missing, it *parses the ref string* to guess schema/table names for
    an FK, then verifies the guess against the graph.
- **Mox replacement**: packages and classes. `resolve_domain`
  (`mox_ingest.rs:545`) maps package → domain by exact/last-segment match on
  domains.toml; targets are resolved through real `ReferencesSchema`/
  `ItemsOf`/`ExtendsSchema` edges the bridge always writes (no fallback path
  needed — the mox path never relies on the string parser).
- **Status**: **Shrinkable** → **Deletable** (JSON-input era only). The edge
  fallback in the querier becomes dead once no un-resolvable refs can occur.

### 11. Property re-derivation after classification (`reclassify_with_entities`)

- **Today**: `async_ingest.rs:1104-1201` re-walks every schema after the
  AutoClassifier has chosen entities and flips both entity flags and
  property classifications (`entity_reference` ↔ `value_object`) because the
  initial ingest ran with an *empty* entity set (`driver.rs:303` passes
  `&HashSet::new()`). Two-phase classification exists purely because entity
  identity isn't knowable at first pass — the scorer needs the whole graph,
  but properties are ingested per schema.
- **Mox replacement**: none needed. `refers`/`contains` classify each feature
  at bridge time (`feature_property`), and mox schemas are skipped by the
  re-derivation pass (`is_mox_sourced` guard, `async_ingest.rs:1131`).
- **Status**: **Shrinkable** (mixed runs still re-derive JSON-side
  properties) → **Deletable** with the JSON path.

### 12. Composite ranges, composite/media wrappers — *no mox construct yet*

- **Today**: `[[composite_ranges]]` collapses `validFrom`/`validTo` pairs
  into a range column (`classify.rs:304-313`, `async_ingest.rs:1209-1262`);
  composite/media wrappers expand one `$ref` into multiple typed columns
  (`classify.rs:141-192`). Both are classifier.toml-driven column algebra
  over wrapper schemas.
- **Why**: HR-Open-style schemas express money as `{value, currency}` objects
  and periods as `{validFrom, validTo}` pairs; the config tells the pipeline
  how to store them relationally.
- **Mox replacement**: **none yet.** `codegraph migrate` maps these to
  `needs_review` rather than inventing constructs. If these shapes matter
  for mox-first models, the language needs a declaration (e.g. a `money`
  / `period` datatype with column expansion, or feature-level column
  algebra); until then the machinery must stay for migrated models.
- **Status**: **Keep** — pending a mox language decision in a follow-up
  issue. Do not delete; do not grow either.

### 13. Path-based schema skipping in `SchemaLoader` (samples/meta/search)

Covered by item 4 — listed separately because it lives in the loader
(`schema_loader.rs:56-63`) rather than the classifier. **Deletable** with
JSON input; mox bypasses the loader.

### 14. `x-*` custom annotation passthrough

- **Today**: `ingest_schema_node` copies top-level `x-*` keys into
  `custom_annotations` (`async_ingest.rs:246-253`) so protocol layers
  (atproto Lexicons) can hang per-schema behavior off JSON extensions.
- **Mox replacement**: none — mox has no extension-key concept (provenance
  uses the `source=mox` annotation internally). Irrelevant once schemas are
  mox-authored, except for atproto projections which are JSON-Lexicon-driven
  by nature.
- **Status**: **Keep** (atproto era) / **Deletable** if atproto input moves
  to native mox constructs.

## Reading order for the eventual deletion PRs

1. Ship the mox language gaps this ledger exposed (item 12's range/wrapper
   constructs, `codelist_as_check` rendering hint) — *before* removing the
   JSON machinery they substitute for.
2. Deprecate `--schemas` as input (already warned, issue #231), then gate
   items 1, 3, 4, 6, 8, 9, 11, 13 behind an input-format check and delete
   with their tests.
3. Items 2, 5, 7, 10 shrink with the same PR that removes the JSON ingest
   pass; the equivalence harness (mox-only) is the regression net.
4. Items 12 and 14 are the only **Keep** entries and need their own design
   issues on the mox language itself.

Nothing here is a promise to delete today. The ledger exists so that when
mox-first becomes the only supported input, the deletions are a checklist
rather than an archaeology project.
