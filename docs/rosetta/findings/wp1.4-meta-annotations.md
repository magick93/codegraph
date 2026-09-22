# WP1.4 — Where do `[metadata]` / `[ruleReference]` / `[docReference]` land?

Question: where do Rune DSL meta fields — `[metadata id/key/scheme]`,
`[ruleReference]`, `[docReference]`, rationale/labels — land in the
codegraph pipeline? The JSON-schema analogs are top-level vs property-level
`x-*` annotations. Hypothesis under test: top-level lands in
`SchemaNode::custom_annotations`; property-level is dropped entirely.

## Verdict

The hypothesis is confirmed on both halves, with one sharpening: top-level
`x-*` annotations persist into `SchemaNode::custom_annotations` **with the
`x-` prefix stripped** (`x-rosetta-metadata` → key `rosetta-metadata`,
nested JSON value intact) and round-trip through the Grafeo store — but
they surface in **no standard generator output** (DDL, COMMENT, entity,
DTO, API, CLI): the only readers of `custom_annotations` are the atproto
generators. Property-level `x-*` annotations are **silently dropped**:
`PropertyNode` has no annotations field and the JSON ingest loop reads only
a fixed key list (`type`, `description`, `format`, `pattern`, `minLength`,
`maxLength`, `minimum`, `maximum`), so annotation payloads on properties
neither error nor persist. This asymmetry matters for Rosetta because in
the Rune DSL most annotation weight sits at the *attribute* (property)
level — `[metadata scheme]` on a field, `[ruleReference]` under an
attribute — which is exactly the level the graph currently discards.
Top-level persistence therefore covers only class-level metadata; the
property level needs an uplift before any Rosetta meta construct can ride
the graph.

## Evidence

Graph model and ingest:

- `crates/codegraph-core/src/types/schema.rs:39-44` — `custom_annotations:
  HashMap<String, serde_json::Value>` exists on `SchemaNode` ONLY
  ("Custom top-level JSON Schema annotations (`x-*` keys) preserved from
  the raw schema file").
- `crates/codegraph-core/src/types/property.rs:79-127` — `PropertyNode`
  has NO annotations/custom_annotations field (all 25 fields accounted
  for; nothing annotation-shaped).
- `crates/codegraph/src/ingest/async_ingest.rs:337-344` — top-level
  capture loop: `key.strip_prefix("x-")` (:340), inserts the *stripped*
  key (:341). Prefix-strip contract pinned by probe 1.
- `crates/codegraph/src/ingest/async_ingest.rs:645-691` — PropertyNode
  construction reads only the fixed key list; no `x-*` key is ever read,
  so property-level annotations vanish without error or warning.

Persistence (values survive the store, not just the in-process node):

- `crates/codegraph-grafeo/src/schema_ddl.rs:84` — `custom_annotations
  STRING NOT NULL` column in the Schema DDL.
- `crates/codegraph-grafeo/src/ingestor.rs:185-255` — serde → JSON string
  on insert; `crates/codegraph-grafeo/src/conversions.rs:118` and
  `querier.rs:25` — deserialized back on read.

Generator surface:

- Grep over `crates/codegraph-generate/src` + `templates/`:
  `custom_annotations` appears only in `src/atproto/lexicon_context.rs:142`
  (`contains_key("selfLabels")` — post-strip key), `atproto/types_context.rs:124`,
  `atproto/xrpc_gen.rs:184`, plus constructors defaulting it. Zero template
  references. Standard DDL/entity/DTO/API/CLI/UI output never sees it.

Precedent for Rosetta payloads riding `custom_annotations`:

- `crates/codegraph/src/ingest/mox_ingest.rs:904-908` (and :945-946) —
  mox provenance already rides `custom_annotations["source"] = "mox"`
  (`MOX_SOURCE` const, schema.rs:8); mirrored onto
  `SchemaClassificationData::source` (schema.rs:68-71); pinned by
  `crates/codegraph/tests/mox_ingest_tests.rs:505-511`. A string payload
  with downstream classifier semantics already uses this channel — the
  same shape works for Rosetta meta payloads.

Probe tests (`crates/codegraph/tests/rosetta_gap_probes/meta_probe.rs`):

- `top_level_x_annotations_persist_to_custom_annotations` — PASS. Entity
  `PermitType` with top-level `x-rosetta-metadata: {id, key, scheme}`;
  fetched via `get_schema`; key stored as `rosetta-metadata` (prefix
  stripped), id/key/scheme values intact as JSON.
- `property_level_x_annotations_dropped` — PASS. `permit_name` carries
  `x-rosetta-rule-ref` in its property schema; ingestion succeeds normally
  (silent drop); the PropertyNode's exhaustive serde view contains no
  annotation-shaped key and no trace of the payload.
- `property_annotations_absent_from_generator_output` — PASS. Full
  `driver::run` tree (DDL/entity/DTO/API/...): neither the property-level
  rule-ref payload nor the top-level metadata id/scheme payloads appear in
  any generated file; even the key spellings `rosetta-metadata` / `x-rosetta`
  are absent. (The bare word "rosetta" legitimately appears as the postgres
  schema name — that is why payload-token assertions were used.)

Sigil annotation surface inventoried (pin
`yestechgroup/sigil@49a6a27f589fa89cbb20641504923c911700ee67`):

- `AnnotationRef` — `crates/sigil-model/src/lib.rs:98` (`annotation`,
  `attribute: Option<String>`, `qualifiers`, resolution slot
  `annotation_resolved`). `[metadata id]`/`[metadata key]`/`[metadata
  scheme]` are NOT special-cased: they parse as generic AnnotationRefs
  with `attribute: Some("id"|"key"|"scheme")` (sigil
  `docs/compatibility.md`: "annotation use `[<ann> scheme]`, qualifiers →
  `AnnotationRef` + `AnnotationQualifier`").
- `AnnotationQualifier` — lib.rs:111; `QualifierValue` (Str | Path)
  lib.rs:119.
- `DocReference` — lib.rs:128 (`body`, `corpora`, `segments`,
  `rationales`, `structured_provision`, `provision`, `reported_field`).
- `Rationale` — lib.rs:139 (`rationale`, `rationale_author`).
- `LabelAnnotation` — lib.rs:145 (`label`).
- `RuleReference` — lib.rs:150 (`rule: Option<String>`, `empty`,
  resolution slot `resolved: Option<usize>` — reference resolution is a
  separate pass from parsing).
- `TransformAnnotation` — lib.rs:368.
- `WithMetaOperation` — `crates/sigil-model/src/expr.rs:343` (Expr
  variant: `argument` + `Vec<WithMetaEntry>`; `WithMetaEntry` expr.rs:168)
  — expression-level meta, rides the expression tree, not the schema graph.

## Disposition recommendation

Per annotation kind — column vs `custom_annotations` payload vs new node:

| Rune construct | Sigil shape | Disposition |
|---|---|---|
| `[metadata id/key/scheme]` (class-level) | `AnnotationRef` | **Payload** in `custom_annotations` (top-level `x-rosetta-metadata` analog already persists). Not a column: id/key/scheme are identity/provenance metadata, JSON-shaped (key+scheme pair), and no generator needs them as a first-class queryable column; a column would hard-code a Rosetta concept into the dialect-agnostic DDL. |
| `[metadata ...]` (attribute-level) | `AnnotationRef` with `attribute` | **Blocked until property-level capture exists.** Today these are silently dropped (probe 2). This is the common Rosetta placement — the property uplift below is the gate. |
| `[ruleReference]` | `RuleReference` lib.rs:150 | **Payload linking to the RuleNode family (#264).** Keep the raw rule FQN as a `custom_annotations` payload (schema- and later property-level); sigil's `resolved: Option<usize>` shows resolution is a separate pass — the codegraph equivalent is a #264 resolution pass that turns payloads into links to RuleNode graph nodes. Do not invent a column; eventually the link wants to be an edge, not a string column. |
| `[docReference]` | `DocReference` lib.rs:128 | **Payload + doc generator (#265/#266).** Rich structured shape (corpora, segments, provision, rationales) — JSON payload in `custom_annotations`; a column cannot represent it and a dedicated node is premature before the doc generator defines its query shape. #265/#266 consume the payload. |
| Rationale / `LabelAnnotation` | lib.rs:139 / lib.rs:145 | **Payload.** Pure descriptive text; `custom_annotations` JSON. No column, no node. |
| `[transform ...]` / `with-meta { ... }` | `TransformAnnotation` lib.rs:368 / `WithMetaOperation` expr.rs:343 | **Out of scope for SchemaNode/PropertyNode** — expression-level meta that rides the expression tree; belongs to the expression plane (WP1.6 / #264), not the schema graph. |

Top-level meta disposition summary: `custom_annotations` is the right and
sufficient channel for schema-level Rosetta meta (mox provenance is the
operating precedent, `mox_ingest.rs:904-908`). No new node types are
warranted at this layer; the gap is entirely at the property level.

## Enablers

None implemented in this WP (tests + findings only). Flagged, NOT
implemented:

- **PropertyNode annotations uplift** (tiny enabler for #256/#258): add
  `#[serde(default)] pub custom_annotations: HashMap<String,
  serde_json::Value>` to `PropertyNode` (`crates/codegraph-core/src/types/
  property.rs:79-127`, mirroring `SchemaNode` schema.rs:43-44) and capture
  `x-*` keys in the property ingest loop (`async_ingest.rs:645-691`, same
  `strip_prefix("x-")` pattern as :337-344). Serde-defaulted, so graph
  payloads and hand-constructed nodes stay backward compatible (the same
  wire-compat pattern as `SchemaClassificationData::source`, schema.rs:70).
  Until this lands, every attribute-level Rune meta construct
  (`[metadata scheme]`, `[ruleReference]`, `[docReference]` on an
  attribute) has nowhere to land.
