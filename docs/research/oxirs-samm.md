# OxiRS SAMM research — can SAMM replace JSON Schema as codegraph's primary input?

Status: research spike (branch `feat/samm-integration`, issue #69)
Date: 2026-08-14
Spike: `crates/samm-spike/` (8 fidelity tests, 6 sample models)

## TL;DR

- SAMM (Eclipse Semantic Aspect Meta Model) the *standard* is genuinely more
  expressive than JSON Schema: units, measurements, enumerations/states,
  operations, events, semantic references, fixed-point constraints.
- `oxirs-samm` 0.4.1 the *crate* is today a lossy front-end: the parser drops
  **all constraints**, **all entity definitions**, units on `Quantifiable`
  characteristics, and crashes on anonymous (blank-node) characteristics — a
  core SAMM authoring pattern.
- Adopting oxirs-samm's AST as codegraph's primary input **today would be a
  downgrade**: codegraph already ingests JSON Schema constraints
  (pattern/minLength/maxLength/minimum/maximum — see
  `codegraph-core/src/types/property.rs`) that oxirs-samm currently throws
  away.
- Recommendation: adopt **SAMM as a complementary input** via a full-fidelity
  reader built on the RDF layer (`oxttl`/`oxrdf` — the same crates oxirs-samm
  uses), not via oxirs-samm's lossy AST. Optionally contribute parser fixes
  upstream (Apache-2.0, active project).

## 1. Context — what JSON Schema currently limits in codegraph

codegraph's pipeline: JSON Schema (+IFML) → classifier → Grafeo graph → 59+
generators. `PropertyNode` already captures: `pattern`, `min_length`,
`max_length`, `minimum`, `maximum`, formats, arrays, required/nullable. So
codegraph already uses JSON Schema's constraint vocabulary.

What JSON Schema genuinely lacks, and what SAMM adds:

| Capability | JSON Schema | SAMM | Why codegraph wants it |
|---|---|---|---|
| Units (currency, km/h, kWh) | no | `Measurement`/`Quantifiable` + unit catalog | `Money`/`Amount` newtypes, NUMERIC(p,s), unit-safe codegen |
| Fixed-point / precision | no | `FixedPointConstraint` (integer digits + scale) | SQL `NUMERIC(p,s)`, `rust_decimal` precision |
| Enumeration + default state | `enum` (no semantics) | `Enumeration`/`State` + default | domain codelists with default values |
| Named relationships | `$ref` only | entity graphs, `extends` | the classifier already synthesizes FKs from allOf; SAMM makes this first-class |
| Operations (domain API) | no | `Operation` (inputs/outputs) | API generator surface beyond CRUD (today only IFML events/actions cover interaction) |
| Events | no | `Event` + parameters | domain events → pgmq triggers, webhooks |
| Semantic references | no | `samm:see` → external ontologies (FIBO, LIXI, W3C geo) | ontology edges, cross-domain dedup, docs |
| Collection semantics | `array` | `List`/`Set`/`SortedSet`/`TimeSeries`/`Collection` | sorted-set → PostgreSQL arrays, time-series tables |
| Constraint composition | keyword soup | Characteristics + Traits (AND-combined) | reusable constraint libraries |

## 2. What oxirs-samm 0.4.1 exposes

Apache-2.0, SAMM 2.3.0 claim, published 2026-07-28. API surface:

- `parser::parse_aspect_model` (Turtle → `Aspect` AST), `parse_aspect_from_string`
- `validator::validate_aspect` (SHACL-style validation of the AST)
- `metamodel`: `Aspect`, `Property`, `Characteristic` (kind enum + `Constraint`
  enum: Range/Length/Regex/Encoding/FixedPoint/Locale/Language), `Operation`,
  `Event`, `Entity` (exists, but unused by the parser — see §3)
- `codegen`: JSON Schema + OpenAPI generators
- `generators`: SQL (PostgreSQL/SQLite), GraphQL, TypeScript, Python, Java, Scala
- Extras: unit catalog + converter, model diff/comparison, versioning/migration,
  entity resolver (standalone from parser), aspect differ, JSON-LD/RDF-XML
  serializers, graph analytics (SciRS2), AAS v3, cloud storage, metrics
- Feature flags: all off by default; `codegen`, `aas`, `graphviz`, `gpu`, `image`

Dependency weight (with only `codegen` enabled): tokio, reqwest, quick-xml,
oxttl, oxrdf, scirs2-core/graph/stats, tera, chrono, serde, … — non-trivial but
comparable to other codegraph optional deps.

## 3. Empirical fidelity results (spike, `crates/samm-spike/`)

Eight tests run against five ESMF SDK test models + one custom constraint model
(`SalesOrder.ttl`) + hand-built ASTs for generator probing. Results:

| SAMM feature | oxirs-samm 0.4.1 behavior | Test |
|---|---|---|
| Aspect + properties + metadata | parsed | `movement_parses…` |
| `Measurement` unit | preserved | `movement_parses…` |
| `Enumeration` values | preserved | `movement_parses…` |
| Operations (inputs/outputs) | parsed | `operations_and_events_parse` |
| Events (parameters) | parsed | `operations_and_events_parse` |
| **`Quantifiable` unit** | **dropped** (degrades to `Trait`) | `quantifiable_units_are_dropped` |
| **Entity definitions** | **dropped** — `Entity` is never constructed by the parser; `SingleEntity` characteristics degrade to `Trait`, only a `data_type` URN string survives | `entity_definitions_are_dropped_by_the_ast` |
| **All constraints** | **dropped** — `ttl_parser.rs` never parses `samm-c:constraint`; `Characteristic.constraints` is always empty (Range, Length, Regex, FixedPoint all lost) | `constraints_are_dropped_by_the_ast` |
| **Anonymous characteristics** (blank node: `samm:characteristic [ a samm-c:SortedSet ; … ]`) | **hard parse error** `Invalid URN '_:a28c…'` | `anonymous_characteristics_crash_the_parser` |
| Boolean datatype | lost (`isMoving` → `data_type=None`, generated JSON Schema emits `"type": "string"`) | observed in Movement |
| `List`/`Set`/`Collection`/`TimeSeries` element characteristics | parsed as `None` | source inspection (`determine_characteristic_kind`) |
| `Code`, `Duration`, `StructuredValue`, `SortedSet` kinds | degrade to `Trait` (with a `tracing::warn!`) | source inspection |

Generated-output quality (oxirs-samm's own generators): Boolean → `TEXT`,
entity-valued property → `TEXT`/`string`, index on every column, no domain
schemas, no constraint reflection. These generators are not competitive with
codegraph's; the interesting part of the crate is the metamodel + RDF tooling,
not its generators.

### Root causes

1. The parser maintains its own triple store and resolves only the elements it
   knows (aspect → properties → characteristics → operations/events). Entities
   (`samm:Entity`, `samm-e:`) and constraints are never visited.
2. Blank-node subjects can't be converted to URNs, and inline characteristics
   (very common SAMM style) crash it.
3. `CharacteristicKind` and `Constraint` enums exist and are rich — the parser
   just doesn't populate them.

## 3.1 SQL transpilation in practice

`generators::generate_sql(aspect, dialect)` supports PostgreSQL, MySQL and
SQLite. Verified output on `SalesOrder.ttl` (length/range/regex constraints,
NZD measurement):

```sql
-- PostgreSQL
CREATE TABLE sales_order (
  id BIGSERIAL PRIMARY KEY,
  order_number TEXT NOT NULL,
  amount_nzd NUMERIC NOT NULL,
  customer_email TEXT NOT NULL,
  quantity INTEGER NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_sales_order_order_number ON sales_order (order_number);
CREATE INDEX idx_sales_order_amount_nzd ON sales_order (amount_nzd);
CREATE INDEX idx_sales_order_customer_email ON sales_order (customer_email);
CREATE INDEX idx_sales_order_quantity ON sales_order (quantity);
```

Quality assessment:

| Aspect | Behavior | Verdict |
|---|---|---|
| Type mapping | XSD suffix matching: decimal→`NUMERIC` (PG), `DECIMAL(10,2)` (MySQL), `REAL` (SQLite); float→`REAL`; boolean→`BOOLEAN` only if `data_type` survives the parser | naive but functional |
| **Constraints** | **None.** No CHECK constraints anywhere. The generator never reads `Characteristic.constraints`; the parser never populates it (see §3). `order_number` length 6..20, `amount` range 0..100M, email regex — all silently absent. | **the key gap** |
| Units / precision | `amount_nzd NUMERIC` — unit ignored, no precision/scale even though SAMM carries fixed-point data | weak |
| Enumerations | `speed_limit_warning TEXT` — no PG enum/CHECK on values | weak |
| Entities / FKs | FK branch exists for `SingleEntity`, but the parser degrades `SingleEntity`→`Trait`, so the branch is unreachable from parsed TTL. Even hand-built: only `FOREIGN KEY (…) REFERENCES customer (id)` is emitted — the referenced entity table is **never created** (dangling FK). | broken via parser path |
| Boolean | `is_moving TEXT` (parser drops Boolean's data type) | wrong |
| Naming / schemas | Singular snake_case table, no schemas, `id BIGSERIAL`, `created_at`/`updated_at` | opinionated, no config |
| Indexes | Index on every required column | bloat, no config |
| SQLite | `INTEGER PRIMARY KEY AUTOINCREMENT`, TEXT timestamps — serviceable but no STRICT, no FTS (codegraph has both) | basic |

Conclusion: oxirs-samm's SQL is a **demo-grade single-table DDL generator**.
It does not reflect SAMM's constraint/unit/entity semantics at all — the
information dies in the parser before the generator sees it. codegraph's DDL
generators (composition trees, child tables, FK dedup, RLS, triggers, FTS,
dialects via `SqlDialect` trait) are substantially ahead. Nothing to copy
except the idea of a SAMM→SQL path in Option A, where codegraph's own
constraint model (`PropertyNode.pattern/min/max`) plus new `unit`/`precision`
fields would feed its existing `ddl` generator.

## 4. Verdict on the core question

> "Can OxiRS SAMM provide a more expressive and reliable metamodel than JSON
> Schema as codegraph's primary input?"

**The standard: yes.** SAMM's characteristics/traits/units/operations/events/
semantic-references are strictly more expressive for domain modeling than JSON
Schema, and its RDF/SHACL foundation gives validation and graph-native
interchange for free.

**The crate, as primary input today: no.** oxirs-samm 0.4.1's AST loses more
information than codegraph's current JSON Schema ingestion keeps. Every feature
that motivates the migration (constraints, entities, units) is exactly what
the parser drops. Reliability is also a concern: the 0.4.1 changelog itself
documents removal of previously fabricated features (SHACL conformance results,
analytics metrics), so its quality claims need independent verification —
which this spike has started to do.

## 5. Integration options for codegraph

### Option A — SAMM as complementary input with a full-fidelity reader (recommended)

Build a `codegraph-samm` crate that reads `.ttl` directly through `oxttl` +
`oxrdf` (dependencies oxirs-samm already proves work well for SAMM), producing
a bridge AST that maps 1:1 onto new codegraph graph nodes:

- `Aspect` → Domain/Schema node; `Property` → PropertyNode with
  constraints mapped into existing `pattern`/`min_length`/`max_length`/
  `minimum`/`maximum` fields (plus new `unit`, `precision`, `fixed_point`
  fields)
- `Entity`/`extends` → Entity + FK composition (mirrors the allOf flattening
  the classifier already does)
- `Measurement`/`Quantifiable` + unit → new `Unit` node + `HasUnit` edge;
  feeds newtype generation and `NUMERIC(p,s)`
- `Operation`/`Event` → graph nodes complementing IFML (IFML = interaction,
  SAMM = domain API/events)
- `samm:see` → `SemanticReference` edges

This is the IFML pattern (parser crate → AST → `GraphIngestor` → Grafeo),
keeps codegraph's own Rust metamodel canonical, and doesn't bet the pipeline
on oxirs-samm's fidelity. oxirs-samm remains optional (SHACL validation,
model diff/migration, TTL/JSON-LD export as an output boundary).

### Option B — Adopt oxirs-samm AST now, upstream the fixes later

Faster to a demo, but requires forking/upstreaming parser work (entities,
constraints, blank nodes) before it's trustworthy. Given the lossiness is in
the core parse path, this is a larger upstream effort than writing a focused
reader.

### Option C — oxirs-samm only as an output/validation boundary

Use its SHACL validator, model diff (`ModelComparison`), versioning, and
TTL/JSON-LD serializers to add a `codegraph diff` command and SAMM export
generator. Zero ingestion changes. Good low-risk first step.

### Option D — Wait

Track the crate (it releases aggressively: 0.3.x→0.4.x in weeks). Re-run the
fidelity spike on each release; if the parser gains entities + constraints +
blank-node support, Option B becomes viable.

## 6. Risks

- **Fidelity risk** (main): parser drops the exact features SAMM is being
  adopted for. Mitigated by Option A's own reader.
- **Credibility risk**: history of fabricated features in the OxiRS platform;
  must pin versions and re-verify behavior on upgrade (the spike tests do
  this).
- **Dependency weight**: gate behind a `samm_backend` profile feature like
  `ifml_backend`/`grpc_backend`.
- **Standard churn**: SAMM spec moves (2.1.0 → 2.2.0 → 2.3.0 namespaces in
  the wild); the reader must be namespace-tolerant (the spike samples span
  2.1.0 and 2.2.0).
- **License/policy**: Apache-2.0 is compatible, but the repo has a
  `CORE_USAGE_POLICY.md` worth reviewing before any code reuse.

## 7. Proposed roadmap

1. (This spike) Document fidelity + decision. ✅
2. Prototype `crates/codegraph-samm`: oxttl/oxrdf reader → bridge AST →
   `GraphIngestor` extension → Grafeo (Option A). Reuse oxirs-samm's unit
   catalog + SHACL validator behind a `samm_backend` feature.
3. `--samm-files` CLI flag + e2e test mirroring `ifml_e2e_tests`.
4. Constraint/unit → generator upgrades: NUMERIC(p,s), CHECK constraints,
   newtype + unit conversion, semantic-reference docs.
5. Option C outputs: `codegraph diff` (model comparison → migration SQL) and
   `--export-samm` TTL generator.
6. Re-run fidelity spike on each oxirs-samm release; revisit Option B if the
   parser reaches parity.

## Appendix — running the spike

```bash
cargo test -p samm-spike                 # 8 fidelity tests
cargo run -p samm-spike -- crates/samm-spike/samples/Movement.ttl /tmp/out
# writes aspect.json, aspect.schema.json, aspect.{postgres,sqlite,mysql}.sql,
# aspect.graphql, aspect.openapi.json
```

Samples: ESMF SDK test models (Movement, AspectWithExtendedEntity,
AspectWithOperation, AspectWithEvent, AspectWithUnit) + custom SalesOrder.ttl
(length/range/regex constraints, NZD measurement).
