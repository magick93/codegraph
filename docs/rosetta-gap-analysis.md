# Rosetta integration: gap analysis — Rune DSL → codegraph disposition table

- **Issue**: magick93/codegraph#254 (gate for the Rosetta epic #255–#268)
- **Sigil pin**: `yestechgroup/sigil@49a6a27f589fa89cbb20641504923c911700ee67`
- **Status**: DRAFT
- **Scope**: every Rune DSL construct — 16 `SemanticElement` kinds, 51 `Expr`
  variants, the annotation surface — gets exactly one disposition:

| Disposition | Meaning |
|-------------|---------|
| `existing graph` | A current node/edge type carries it; bridge maps onto it |
| `new node family` | Needs new graph node/edge types (issue-scoped) |
| `annotation payload` | Persisted as data ON an existing node (custom_annotations / expression field), not a node of its own |
| `generator feature` | Graph carries it; emitting it is generator work |
| `deferred` | Explicitly out of scope for now, with rationale |

Row evidence references probe tests
(`crates/codegraph/tests/rosetta_gap_probes/`) and upstream issues.

---

## Verification findings

These six sections answer the issue's "must verify against real generator
behavior" list. Each cites its findings file under `docs/rosetta/findings/`.

### 1. Inheritance — merge at generation, or ingest-time flatten? (WP1.1)

{FINDINGS-WP1.1 — docs/rosetta/findings/wp1.1-inheritance.md}

### 2. Cardinality `min>1` (WP1.2)

{FINDINGS-WP1.2 — docs/rosetta/findings/wp1.2-cardinality.md}

### 3. Choice / one-of representation (WP1.3)

{FINDINGS-WP1.3 — docs/rosetta/findings/wp1.3-choice.md}

### 4. Meta fields and the annotation surface (WP1.4)

{FINDINGS-WP1.4 — docs/rosetta/findings/wp1.4-meta-annotations.md}

### 5. Namespaces vs domains (WP1.5)

{FINDINGS-WP1.5 — docs/rosetta/findings/wp1.5-namespaces.md}

The disposition table uses the #267/#268 model: namespace = where a type
lives + what it can see; domain = bounded context / deploy boundary;
namespace → domain assignment is optional, many-to-one, declared in config.

### 6. Expression typing and `Expr::to_json` (WP1.6)

{FINDINGS-WP1.6 — docs/rosetta/findings/wp1.6-expr-json-typing.md}

---

## Plane: Data

Draft disposition (issue #254): existing graph —
`SchemaNode`/`PropertyNode`/`CodeList`/`ExtendsSchema`/`ReferencesSchema`.

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| Data | `SemanticElement::Data` | existing graph | DRAFT: maps to `SchemaNode` (+ entity/VO decision) | pending WP1.7 | #256 |
| Enumeration | `SemanticElement::Enumeration` | existing graph | DRAFT: maps to `CodeList` + `EnumValue` (+ `ReferencesSchema`/`ItemsOf` edges) | pending WP1.7 | #256 |
| TypeAlias | `SemanticElement::TypeAlias` | DRAFT | pending: no JSON/mox alias precedent in graph | pending WP1.7 | #256 |
| BasicType | `SemanticElement::BasicType`, `sigil_resolve::builtin_files` | existing graph | DRAFT: builtin library maps onto classifier built-in primitive handling | pending WP1.7 | #256 |
| RecordType | `SemanticElement::RecordType` | existing graph | DRAFT: structured record → `SchemaNode` with properties (VO-shaped) | pending WP1.7 | #256 |
| attribute | `Attribute` (lib.rs:192) | existing graph | DRAFT: maps to `PropertyNode` | pending WP1.7 | #256 |
| enum value | `EnumValue` (lib.rs:223) | existing graph | DRAFT: maps to `EnumValue` on `CodeList` | pending WP1.7 | #256 |
| cardinality | `Cardinality { min, max }` (lib.rs:63) | DRAFT | pending: `PropertyNode` has no min/max items — gap expected | pending WP1.2 | #261 |
| extends | `Data.extends` / allOf path | existing graph | DRAFT: `ExtendsSchema` edges + ingest-time merge (mox precedent) | pending WP1.1 | #256 |
| choice | choice constructs | DRAFT | pending: representation decision feeds Constraint plane too | pending WP1.3 | #261 |
| [metadata] | `AnnotationRef`/`AnnotationQualifier`, `WithMetaOperation` | DRAFT | pending: column vs custom_annotations | pending WP1.4 | #258 |
| Annotation | `SemanticElement::Annotation` (lib.rs:236) | DRAFT | pending: annotation declarations as a node family vs schema-level payload | pending WP1.4 | #258 |

---

## Plane: Constraint

Draft disposition (issue #254): new ConditionNode family + validation
generators (Rust validators, SQL CHECK, TS zod).

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| condition | `Condition` (lib.rs:180) | new node family | DRAFT per issue | pending WP1.3/1.6 | #261 |
| one-of | oneOf / `OneOfOperation` | DRAFT | pending representation decision | pending WP1.3 | #261 |
| cardinality min>1 (validation) | `Cardinality.min > 1` | generator feature | DRAFT: validation emission on top of the data-plane cardinality fix | pending WP1.2 | #261 |
| [ruleReference] | `RuleReference` (lib.rs:150) | annotation payload | DRAFT: payload linking to the RuleNode family | pending WP1.4 | #264 |

## Expression surface (all 51 `Expr` variants)

Every variant must carry a disposition; the transpiler (#262) and validation
generators (#261) are sized from this table. Family-level drafts below; Phase 2
finalizes per variant from WP1.6.

| Expr variant | Family | Disposition | Rationale | Evidence | Upstream |
|--------------|--------|-------------|-----------|----------|----------|
| BooleanLiteral | literal | annotation payload | DRAFT: embedded in Condition/Rule payload; transpiler-direct | pending WP1.6 | #262 |
| StringLiteral | literal | annotation payload | DRAFT: embedded; transpiler-direct | pending WP1.6 | #262 |
| NumberLiteral | literal | annotation payload | DRAFT: embedded; transpiler-direct | pending WP1.6 | #262 |
| IntLiteral | literal | annotation payload | DRAFT: embedded; transpiler-direct | pending WP1.6 | #262 |
| ListLiteral | literal | annotation payload | DRAFT: embedded; transpiler-direct | pending WP1.6 | #262 |
| SymbolReference | reference | annotation payload | DRAFT: scope-resolved via sigil-resolve | pending WP1.6 | #262 |
| ImplicitVariable | reference | annotation payload | DRAFT: scope-resolved; transpiler binds receiver | pending WP1.6 | #262 |
| FeatureCall | reference | annotation payload | DRAFT: property navigation | pending WP1.6 | #262 |
| DeepFeatureCall | reference | annotation payload | DRAFT: nested property navigation | pending WP1.6 | #262 |
| ArithmeticOperation | operator | annotation payload | DRAFT: Rust operator lowering | pending WP1.6 | #262 |
| LogicalOperation | operator | annotation payload | DRAFT: `&&`/`\|\|`/`!` lowering | pending WP1.6 | #262 |
| EqualityOperation | operator | annotation payload | DRAFT: `==`/`!=` lowering | pending WP1.6 | #262 |
| ComparisonOperation | operator | annotation payload | DRAFT: `<`/`<=`/`>`/`>=` lowering | pending WP1.6 | #262 |
| ContainsExpression | collection | annotation payload | DRAFT: iterator `any()` lowering | pending WP1.6 | #262 |
| DisjointExpression | collection | annotation payload | DRAFT: set-intersection-empty lowering | pending WP1.6 | #262 |
| DefaultOperation | collection | annotation payload | DRAFT: `unwrap_or` lowering | pending WP1.6 | #262 |
| JoinOperation | collection | annotation payload | DRAFT: string/collection join lowering | pending WP1.6 | #262 |
| ConditionalExpression | control | annotation payload | DRAFT: ternary/if-else lowering | pending WP1.6 | #262 |
| OnlyExistsExpression | existence | annotation payload | DRAFT: quantifier lowering | pending WP1.6 | #262 |
| ExistsExpression | existence | annotation payload | DRAFT: quantifier lowering | pending WP1.6 | #262 |
| AbsentExpression | existence | annotation payload | DRAFT: negated existence lowering | pending WP1.6 | #262 |
| OnlyElement | algebra | annotation payload | DRAFT: single-element access lowering | pending WP1.6 | #262 |
| CountOperation | algebra | annotation payload | DRAFT: `.count()` lowering | pending WP1.6 | #262 |
| FlattenOperation | algebra | annotation payload | DRAFT: `.flatten()` lowering | pending WP1.6 | #262 |
| DistinctOperation | algebra | annotation payload | DRAFT: dedup lowering | pending WP1.6 | #262 |
| ReverseOperation | algebra | annotation payload | DRAFT: `.rev()` lowering | pending WP1.6 | #262 |
| FirstOperation | algebra | annotation payload | DRAFT: `.first()` lowering | pending WP1.6 | #262 |
| LastOperation | algebra | annotation payload | DRAFT: `.last()` lowering | pending WP1.6 | #262 |
| SumOperation | algebra | annotation payload | DRAFT: `.sum()` lowering | pending WP1.6 | #262 |
| AsKeyOperation | algebra | annotation payload | DRAFT: map-key lowering | pending WP1.6 | #262 |
| OneOfOperation | algebra | annotation payload | DRAFT: ties to choice representation | pending WP1.3/1.6 | #262 |
| ChoiceOperation | algebra | annotation payload | DRAFT: ties to choice representation | pending WP1.3/1.6 | #262 |
| ToStringOperation | cast | annotation payload | DRAFT: `to_string()` lowering | pending WP1.6 | #262 |
| ToNumberOperation | cast | annotation payload | DRAFT: parse/convert lowering | pending WP1.6 | #262 |
| ToIntOperation | cast | annotation payload | DRAFT: parse/convert lowering | pending WP1.6 | #262 |
| ToTimeOperation | cast | annotation payload | DRAFT: chrono lowering | pending WP1.6 | #262 |
| ToEnumOperation | cast | annotation payload | DRAFT: codelist-variant lowering | pending WP1.6 | #262 |
| ToDateOperation | cast | annotation payload | DRAFT: chrono lowering | pending WP1.6 | #262 |
| ToDateTimeOperation | cast | annotation payload | DRAFT: chrono lowering | pending WP1.6 | #262 |
| ToZonedDateTimeOperation | cast | annotation payload | DRAFT: chrono-tz lowering | pending WP1.6 | #262 |
| SwitchOperation | control | annotation payload | DRAFT: match lowering | pending WP1.6 | #262 |
| WithMetaOperation | metadata | annotation payload | DRAFT: ties to [metadata] disposition | pending WP1.4/1.6 | #262 |
| AsOperation | control | annotation payload | DRAFT: typed-view lowering | pending WP1.6 | #262 |
| ThenOperation | control | annotation payload | DRAFT: chaining lowering | pending WP1.6 | #262 |
| FilterOperation | higher-order | annotation payload | DRAFT: `.filter()` lowering | pending WP1.6 | #262 |
| MapOperation | higher-order | annotation payload | DRAFT: `.map()` lowering | pending WP1.6 | #262 |
| ReduceOperation | higher-order | annotation payload | DRAFT: `.fold()` lowering | pending WP1.6 | #262 |
| SortOperation | higher-order | annotation payload | DRAFT: `.sort*()` lowering | pending WP1.6 | #262 |
| MinOperation | higher-order | annotation payload | DRAFT: `.min()` lowering | pending WP1.6 | #262 |
| MaxOperation | higher-order | annotation payload | DRAFT: `.max()` lowering | pending WP1.6 | #262 |
| ConstructorExpression | constructor | annotation payload | DRAFT: ties to RecordType/metaType disposition | pending WP1.6 | #262 |

---

## Plane: Computation

Draft disposition (issue #254): new FunctionNode/RuleNode families +
expr→Rust transpiler.

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| Function | `SemanticElement::Function` (lib.rs:324) | new node family | DRAFT per issue | pending | #263 |
| Rule | `SemanticElement::Rule` (lib.rs:436) | new node family | DRAFT: RuleNode parent kind; reporting/eligibility are roles of it | pending | #264 |
| FunctionDispatch | `FunctionDispatch` (lib.rs:352) | new node family | DRAFT: dispatch enumeration on FunctionNode | pending | #263 |
| function aliases | `Function.aliases` | new node family | DRAFT | pending | #263 |
| set/add operations | `Function` bodies (set/add) | generator feature | DRAFT: bodies → generated handlers via transpiler | pending WP1.6 | #263 |
| post-conditions | `Function` post-conditions | generator feature | DRAFT | pending | #263 |
| reporting rule | `Rule` (reporting) | new node family | DRAFT: RuleNode family | pending | #264 |
| eligibility rule | `Rule` (eligibility) | new node family | DRAFT: RuleNode + API guard emission | pending | #264 |

---

## Plane: Regulatory

Draft disposition (issue #254): metadata nodes + report/doc generators;
eligibility rules → API guards.

| Construct | Sigil anchor | Disposition | Rationale | Evidence | Upstream |
|-----------|--------------|-------------|-----------|----------|----------|
| Report | `SemanticElement::Report` (lib.rs:502) | new node family | DRAFT: report generator | pending | #265 |
| ReportTiming | `ReportTiming` (lib.rs:478) | annotation payload | DRAFT: payload on Report node | pending | #265 |
| Body | `SemanticElement::Body` (lib.rs:573) | new node family | DRAFT: metadata nodes | pending | #265 |
| Corpus | `SemanticElement::Corpus` (lib.rs:584) | new node family | DRAFT: metadata nodes | pending | #265 |
| Segment | `SemanticElement::Segment` (lib.rs:598) | new node family | DRAFT: metadata nodes | pending | #265 |
| ExternalRuleSource | `SemanticElement::ExternalRuleSource` (lib.rs:520) | new node family | DRAFT: metadata node for rule provenance | pending | #265 |
| Schema | `SemanticElement::Schema` (lib.rs:558) | new node family | DRAFT: regulatory metadata bundle, NOT codegraph's `SchemaNode` — naming collision, see below | pending | #265 |
| MetaType | `SemanticElement::MetaType` (lib.rs:607) | new node family | DRAFT: metadata node | pending | #265 |
| LibraryFunction | `SemanticElement::LibraryFunction` (lib.rs:299) | DRAFT | pending: builtin registry vs FunctionNode-light | pending | #263/#265 |
| [docReference] | `DocReference` (lib.rs:128) | annotation payload | DRAFT: payload + doc generator | pending WP1.4 | #265/#266 |
| transforms | `TransformAnnotation`/`TransformKind` (lib.rs:368–376) | generator feature | DRAFT | pending | #265 |
| Rationale / LabelAnnotation | `Rationale` (lib.rs:139), `LabelAnnotation` (lib.rs:145) | annotation payload | DRAFT | pending WP1.4 | #265 |

---

## Cross-cutting collisions

| Collision | Detail | Resolution owner |
|-----------|--------|------------------|
| `NamespaceNode` | `types/atproto.rs` already defines `NamespaceNode` (AT-Protocol repo semantics); #267 proposes a graph-wide namespace concept with the same name | #267 |
| `Schema` | sigil's `Schema` SemanticElement is regulatory metadata (rule source bundle); codegraph's `SchemaNode` is the data-plane type container | #265 (doc-level distinction; distinct node names) |

## Upstream issue map

| Issue | Slice |
|-------|-------|
| #255 | dependency wiring (sigil crates, rev-pinned; dev-deps land in #254) |
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
sized. Sign-off flips `Status: DRAFT` → `Status: REVIEW` → approved.
