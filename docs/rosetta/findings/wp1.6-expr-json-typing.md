# WP1.6 — `Expr::to_json` round-trip stability + honest expression-typing scope

Questions: (a) is `Expr::to_json` output stable enough to embed as the
condition payload of a future ConditionNode/RuleNode? (b) does sigil
type-check or interpret expressions?

Sigil pin: `yestechgroup/sigil@49a6a27f589fa89cbb20641504923c911700ee67`.

## Verdict

(a) **Embed `Expr::to_json()` Values directly as the ConditionNode/RuleNode
payload.** It is deterministic per node (two calls on the same `Expr` are
`Value`-equal for every node in a 51-condition fixture), byte-stable through
a serde_json string round-trip (`to_string → from_str → to_string` equal for
all collected nodes), and numeric fidelity is preserved for free: integer
and BigDecimal literals keep their verbatim source text (`{"kind":"Int",
"text":"42"}`, `{"kind":"Number","text":"1.50"}`), so embedding never goes
through float parsing. It is also the *same* Value `canonical_json` embeds
under `conditions[].expression` (sigil-resolve `lib.rs:1320`), so graph
payloads and the canonical artifact are identical by construction. Design
around three caveats: (1) JSON `kind` ≠ Rust variant name — seven Rust
variants collapse to one `"Binary"` tag (the `op` field disambiguates every
one, but the JSON alone does not carry the Rust variant identity); (2)
object keys serialize alphabetically (serde_json `BTreeMap`; no
`preserve_order` anywhere in the workspace lock) — stable, but do not
expect source order; (3) `Expr` is **Serialize-only** (no `Deserialize`
anywhere in sigil-model), so the payload is write-once provenance, not an
interchange format we can rehydrate into an `Expr`. (b) **Sigil does NOT
type-check or interpret expressions — it parses and resolves names,
nothing more.** `amount + name` (number + string), `amount + "x"`
(number + string literal) and `name > 5` (string vs int) all parse and
resolve with **zero diagnostics**; the only diagnostic in the probe model is
the deliberate unknown-symbol control (`bogus` → E0101). The #262
transpiler must either trust sigil's spans + resolved names and lean on
downstream compilation, or perform its own type checking — nothing upstream
will have done any.

## Evidence

- `sigil-model src/expr.rs:182` — the derive list is
  `#[derive(Debug, Clone, PartialEq)]`: no serde derives on `Expr` itself.
- `sigil-model src/expr.rs:391` — manual `impl Serialize for Expr`
  delegates to `to_json()` (`self.to_json().serialize(serializer)`), so
  serializing an `Expr` and serializing its `to_json()` Value are
  byte-identical (asserted per condition in probe 2).
- `sigil-model src/expr.rs:538` — `to_json(&self) -> serde_json::Value`,
  hand-written per variant via the `json!` macro; the tag lives in a
  `"kind"` field (plus `"op"` for the seven Binary-shaped variants).
- **No `Deserialize` for `Expr` anywhere in sigil** — repo-wide grep finds
  Deserialize only in sigil-lsp's JSON-RPC plumbing (`sigil-lsp src/lib.rs:37`,
  `src/dispatch.rs`), never on model types. `Expr` cannot be reconstructed
  from its JSON.
- `sigil-model src/lib.rs:180` (`Condition.expression`), `:324`
  (`Function`: conditions/post-conditions/shortcuts/operations), `:436`
  (`Rule.expression`) — where expressions hang off `SemanticElement`.
- `sigil-resolve src/lib.rs:788-789` — the resolver's own comment:
  "`->` feature segments inside expressions are parsed but not
  type-checked". `resolve_expr_heads` (`:790`) acts ONLY on
  `SymbolReference` heads and `ConstructorExpression` type refs; every
  other variant is walked for head symbols only.
- `sigil-resolve src/lib.rs:844-850` — expression-head E0101s are pushed
  with `span: None`; `canonical_json` (`:1565`) therefore renders
  `"span": null, "location": null` for them (pinned in probe 4). Only
  `file` + model `path` (e.g. `type Widget.conditions[UnknownHead]`)
  identify the site.
- `sigil-syntax src/expr.rs:808` (`missing_left()`) — the grammar's
  "without left parameter" shapes synthesize an `ImplicitVariable`
  receiver: the fixture's `then join ", "` lowers to
  `JoinOperation { left: ImplicitVariable, ... }` without any `item`
  appearing in source. The resolver silently skips that synthesized node
  (it only resolves `SymbolReference` heads).
- Observed `to_json` shapes (probe 2 `--nocapture`):

  ```json
  {"kind":"Binary","left":{"args":[],"explicit":false,"kind":"SymbolReference","symbol":"price"},"op":"+","right":{"args":[],"explicit":false,"kind":"SymbolReference","symbol":"quantity"}}
  {"kind":"Int","text":"42"}
  ```

- Variant recovery from JSON: 55 distinct `(kind, op)` keys observed;
  43 of the 45 distinct `kind` tags. Rust variant name is NOT in the JSON.
- AsKey syntax attempt pinned as a failure mode: `condition AsKey: prices
  as-key` fails to parse (E0001 "found '-' expected something else, '/',
  or qualified name" — the parser reads `as` as the `AsOperation` keyword).
- Probe tests: `crates/codegraph/tests/rosetta_gap_probes/expr_json_probe.rs` —
  `parse_and_resolve_exercises_expression_families`,
  `to_json_is_deterministic_and_serialization_stable`,
  `resolve_does_not_typecheck_expressions`,
  `canonical_json_embds_expressions_losslessly` (all passing).

### What resolve() DOES check (all name-level)

| Check | Codes | Where (sigil-resolve lib.rs) |
|---|---|---|
| Global FQN duplicates | E0104 | `:123`, `:908` |
| Attribute/type refs: unknown, not-a-type | E0101, E0106 | `resolve_type_ref :641` |
| Annotation refs: unknown, not-annotation, missing attribute | E0102, E0103 | `:668` |
| Super-type/parent refs + kind filter | E0101, E0106 | `:602` |
| Report/rule named refs | E0101, E0106 | `:714`, `:749` |
| Inheritance cycles | E0105 | `detect_cycles :1173` |
| Function dispatch: attribute ∈ inputs, enum-ness, value ∈ enum | E0106, E0107 | `:304` |
| Operation assign roots (output/alias only); path segments against receiver type chain | E0101, E0107 | `:379` |
| External rule-source attribute existence | E0107 | `:551` |
| Expression HEAD symbols only (inputs/output/aliases/inline params/enclosing attributes, then file scope); annotation-as-symbol | E0101, E0106 | `resolve_expr_heads :790`, `resolve_head :821` |
| Dotted heads as `Type.feature` chains | E0101 (fallback) | `resolve_qualified_head :858` |
| Constructor `type_call` as a type ref | E0101, E0106 | `:801` |

### What resolve() does NOT check

- Any operand/operator typing: arithmetic on strings, string-vs-int
  comparisons, `contains` on scalars — all silent (probe 3).
- `->` / `->>` feature segments inside expressions (the resolver's own
  comment, `:788-789`).
- Whether a condition is boolean-ish; whether a `set` expression matches
  the output type; whether `as`/`to-*` conversions are meaningful
  (`price as number` and `label to-number` both resolve silently).
- `ToEnumOperation`'s enumeration string (`:714` — never looked up).
- Constructor value keys; `WithMeta` entry keys.
- Interpretation/evaluation: there is no evaluator anywhere in the
  toolchain.

### 51-variant inventory (sigil-model src/expr.rs:183) vs fixture coverage

Legend: ✓ = exercised by the probe fixture; ✗ = not exercised.

| Variants (expr.rs line) | Family | JSON `kind` | Fixture |
|---|---|---|---|
| BooleanLiteral :184 | literals | `Boolean` | ✓ |
| StringLiteral :187 | literals | `String` | ✓ |
| NumberLiteral :191 | literals (text verbatim) | `Number` | ✓ |
| IntLiteral :195 | literals (text verbatim, sign incl.) | `Int` | ✓ |
| ListLiteral :199 | literals (`empty` = same node) | `List` | ✓ |
| SymbolReference :202 | references | `SymbolReference` | ✓ |
| ImplicitVariable :208 | references (synthesized by `missing_left`) | `ImplicitVariable` | ✓ |
| FeatureCall :209 | references / feature calls | `FeatureCall` | ✓ |
| DeepFeatureCall :214 | references / feature calls | `DeepFeatureCall` | ✓ |
| ArithmeticOperation :218 | binary (`+ - * /`) | `Binary` | ✓ (+ `*` `-`) |
| LogicalOperation :223 | binary (`and or`) | `Binary` | ✓ |
| EqualityOperation :228 | binary (`= <>` + cardMod) | `Binary` | ✓ |
| ComparisonOperation :234 | binary (`>= <= > <` + cardMod) | `Binary` | ✓ |
| ContainsExpression :240 | binary (word op) | `Binary(contains)` | ✓ |
| DisjointExpression :244 | binary (word op) | `Binary(disjoint)` | ✓ |
| DefaultOperation :248 | binary (word op) | `Binary(default)` | ✓ |
| JoinOperation :254 | join (`explicitSeparator` flag) | `Join` | ✓ |
| ConditionalExpression :262 | conditional (`full` = has else) | `Conditional` | ✓ |
| SwitchOperation :339 | conditional / dispatch | `Switch` | ✓ |
| OnlyExistsExpression :268 | quantifier / presence | `OnlyExists` | ✓ |
| ExistsExpression :272 | quantifier / presence (modifier) | `Exists` | ✓ |
| AbsentExpression :276 | quantifier / presence | `Absent` | ✓ |
| OnlyElement :279 | unary collection | `OnlyElement` | ✓ |
| CountOperation :282 | unary collection | `Count` | ✓ |
| FlattenOperation :285 | unary collection | `Flatten` | ✓ |
| DistinctOperation :288 | unary collection | `Distinct` | ✓ |
| ReverseOperation :291 | unary collection | `Reverse` | ✓ |
| FirstOperation :294 | unary collection | `First` | ✓ |
| LastOperation :297 | unary collection | `Last` | ✓ |
| SumOperation :300 | unary collection | `Sum` | ✓ |
| AsKeyOperation :303 | unary collection | `AsKey` | ✗ (syntax rejected in attempted form) |
| OneOfOperation :306 | unary collection | `OneOf` | ✓ |
| ChoiceOperation :309 | unary collection (choice) | `Choice` | ✗ |
| ToStringOperation :314 | conversions | `ToString` | ✓ |
| ToNumberOperation :317 | conversions | `ToNumber` | ✓ |
| ToIntOperation :320 | conversions | `ToInt` | ✓ |
| ToTimeOperation :323 | conversions | `ToTime` | ✓ |
| ToEnumOperation :326 | conversions (enum string unresolved) | `ToEnum` | ✓ |
| ToDateOperation :330 | conversions | `ToDate` | ✓ |
| ToDateTimeOperation :333 | conversions | `ToDateTime` | ✓ |
| ToZonedDateTimeOperation :336 | conversions | `ToZonedDateTime` | ✓ |
| WithMetaOperation :343 | meta | `WithMeta` | ✓ |
| AsOperation :347 | meta (cast) | `As` | ✓ |
| ThenOperation :353 | higher-order (inline fn) | `Then` | ✓ |
| FilterOperation :357 | higher-order (inline fn) | `Filter` | ✓ |
| MapOperation :362 | higher-order (`extract`) | `Map` | ✓ |
| ReduceOperation :366 | higher-order (inline fn) | `Reduce` | ✓ |
| SortOperation :370 | higher-order (inline fn) | `Sort` | ✓ |
| MinOperation :374 | higher-order (inline fn) | `Min` | ✓ |
| MaxOperation :378 | higher-order (inline fn) | `Max` | ✓ |
| ConstructorExpression :382 | constructor | `Constructor` | ✓ |

49 of 51 variants exercised; 55 distinct `(kind, op)` wire keys observed.

## Disposition recommendation

- **ConditionNode / RuleNode payload: store `Expr::to_json()` (the raw
  `serde_json::Value`) as the condition payload**, alongside the
  `.rosetta` source text (or a span table) for provenance and #262
  transpilation. Rationale: deterministic, byte-stable, numeric-faithful
  (verbatim literal text), and identical to what `canonical_json` embeds —
  one shape everywhere. Do NOT use `Expr::print()` strings as the payload:
  it is a reconstructable projection, not the conformance artifact, and
  re-parsing it buys nothing over storing the Value. Use
  `sigil_resolve::canonical_json` only for whole-model import (it is the
  lossless full-model embedding, diagnostics included), not as a
  per-condition payload.
- **#262 transpiler scope (honest statement): sigil parses and resolves
  names; it does not type-check or interpret expressions.** The transpiler
  must either (a) perform its own type checking over the `to_json` trees
  (it has spans only on the *declaration* level — expression-head
  diagnostics carry no span, so error reporting back into source is
  coarse), or (b) generate code and let the target compiler be the type
  checker. It must not assume any expression well-typedness upstream.
- Unexercised variants are low-risk: `ChoiceOperation` needs choice-type
  declaration syntax, and `AsKeyOperation` is not accepted in the attempted
  postfix position (`prices as-key` → E0001; the parser claims `as` first) —
  revisit only if a ConditionNode payload ever needs those two.

## Enablers

None — probe-only work; no production code, templates, or manifests changed.
