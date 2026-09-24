# Rosetta integration guide

Rosetta (the Rune DSL, `.rosetta` files) is a **primary model source** in
codegraph, alongside `.mox` and JSON Schema. This guide covers the
integration surface: CLI usage, what bridges into the graph, namespaces,
classification, code generation, and the supported/deferred construct
boundary. The full construct-by-construct disposition table lives in
[docs/rosetta-gap-analysis.md](rosetta-gap-analysis.md) (issue #254) —
linked, not duplicated, here.

## Overview

codegraph does not parse Rune DSL itself. The parse → lower → resolve
pipeline lives in the upstream **sigil** crates (`sigil-model`,
`sigil-syntax`, `sigil-resolve`, `sigil-diag`), rev-pinned git
dependencies consumed by `crates/codegraph` only — no sigil dependency
leaks into `codegraph-generate` or `codegraph-core`. The bridge
(`crates/codegraph/src/ingest/rosetta_ingest.rs`) resolves ALL user files
in ONE resolve call (builtins automatic), then walks the resolved
`SemanticElement`s into the same graph shapes the JSON Schema path
produces: `SchemaNode`, `PropertyNode`, `CodeList`, plus the Rosetta-specific
families (`ConditionNode`, `FunctionNode`, `RuleNode`, `RegulatoryNode`).

Provenance: every bridged node carries `custom_annotations["origin"] =
"rosetta"` — deliberately distinct from the mox bridge's `source` key, so
the classifier's priority-0 mox bypass never fires on rosetta schemas
(rosetta types stay auto-scored; see
[Classification](#classification-interplay)).

## Quickstart

```bash
# Scaffold a rosetta-first project (one model/<domain>.rosetta per domain)
codegraph init my-app --rosetta
cd my-app

just doctor            # config + sigil verification of the starters
just generate          # run --rosetta-files model/<d>.rosetta ... -> generated/
just classify          # entity/VO decisions for the rosetta model
```

What `init --rosetta` produces, versus the default mox-first scaffold:

| Aspect | `--rosetta` behavior |
|--------|----------------------|
| Model files | `model/<domain>.rosetta` starters (type + status enum), **sigil-verified (parse → lower → resolve) before anything is written** |
| `schemas/` + `classifier.toml` | Not scaffolded — JSON interop is optional |
| `domains.toml` | One entry per domain, no `entities` key (types are auto-scored) |
| `profiles.toml` | `rosetta_backend = true` |
| `codegraph-ops.toml` | `rosetta_files = ["model/<d>.rosetta", ...]` |
| `justfile` | `generate`/`classify`/`doctor` recipes pass `--rosetta-files model/<d>.rosetta` per domain |

Growing the project: `codegraph add domain <name>` auto-detects
rosetta-first projects (any `model/*.rosetta` present; `--rosetta` forces
it) and renders a `model/<name>.rosetta` starter namespaced
`{app_name}.{domain}`, sigil-verified before write.

Running an existing model without scaffolding:

```bash
cargo run -- run --rosetta-files model/store.rosetta \
  --config domains.toml --output generated
cargo run -- classify --rosetta-files model/store.rosetta --config domains.toml
```

`--rosetta-files` is repeatable. `--schemas` is optional when it is
provided (`--classifier` is only required when `--schemas` is present).
A broken model is a hard error: syntax or resolution failures abort the
run with sigil diagnostic codes/spans (`Error::RosettaModel`).

### Doctor

`codegraph doctor --rosetta-files <file>...` verifies each file through
the sigil pipeline and checks namespace/domain wiring:

| Check | Severity |
|-------|----------|
| File unreadable | **Hard failure** |
| Sigil diagnostic with severity Error (syntax/resolution) | **Hard failure** |
| File namespace whose last segment matches no `domains.toml` key | Warning (its schemas would be silently dropped from generation) |
| `import <ns>.*` resolving to neither a `--rosetta-files` entry nor a domains.toml key | Warning |
| Sigil rev pin missing from `Cargo.lock` | Warning |

Doctor prints `INFO sigil — sigil-model rev <sha>` so verification
results are reproducible evidence (see [Rev-pinning](#rev-pinning)).

## Model-source semantics

The bridge maps sigil elements onto graph node families. Per-file facts
(domain assignment, namespaces) come from the `namespace` declaration at
the top of each file.

```rosetta
namespace rosetta.fixture.store
version "1.0.0"

import rosetta.fixture.partners.*

type PriorityOrderType extends OrderType:
	priorityLevel int (0..1)
	override label string (1..1)

enum Urgency extends OrderStatus:
	Rush

choice OrderKind:
	OrderType
	CustomerType

func OrderScore: <"...">
	[enrich]
	inputs:
		order OrderType (1..1)
	output:
		result number (1..1)
	alias base: order -> total
	set result:
		base + order -> total
	post-condition ResultPresent: <"the result is present">
		result exists

reporting rule HighValueTotal from OrderType: <"...">
	total > 100.0

eligibility rule EligibleOrder from OrderType: <"...">
	total > 0.0 and label exists

rule source StoreSource {
	OrderType:
		+ total [ruleReference HighValueTotal]
}
```

| Sigil element | Graph landing |
|---------------|---------------|
| `type` (Data) | `SchemaNode` + `PropertyNode`s |
| `extends` | Ingest-time attribute merge (ancestors first, each group name-sorted, first occurrence wins; `override` attributes REPLACE the inherited slot in place) + `ExtendsSchema` edges |
| attributes | `PropertyNode` with `ReferencesSchema`/`ItemsOf` edges; builtins (`int`, `number`, `string`, `date`, `dateTime`, `time`, `boolean`) map onto the JSON path's primitive mappings |
| `enum` (Enumeration) | `CodeList` + `EnumValue`s + the codelist `SchemaNode` the DDL/FK machinery keys on; `enum extends` merges the parent's values first |
| `choice` | Option titles as `(0..1)` attributes + a `rosetta_choice` annotation, plus ONE bridge-derived `kind: OneOf` condition node whose options are the referenced titles |
| conditions | `ConditionNode` (`kind: Condition`) carrying the canonical `Expr::to_json()` payload, linked to its schema via `HasCondition` |
| `func` (Function) | `FunctionNode`: dispatch head (`(attr: Enum->VALUE)`), typed inputs/output, aliases, `set`/`add` operations, post-conditions, `[transform]` annotations; `extends` resolves within the run (`FunctionExtends` edge; a cycle is a hard error naming the cycle) |
| `rule` (reporting/eligibility) | `RuleNode` (`kind` from the `eligibility` flag, `input_type` from the `from TypeCall` clause, `Expr::to_json` body); a resolvable input type becomes a `RuleAppliesTo` edge |
| `rule source` attributes carrying `[ruleReference R]` | Promoted to `RuleReference` edges (Schema → Rule) when R names a rule of this run; unresolvable references are documented skips, never silent drops |
| `report`/`body`/`corpus`/`segment`/`rule source` (class form)/rule `schema`/`metaType` | `RegulatoryNode` — ONE parameterized, kind-tagged family; associations become edges (`RegulatoryReference` for doc references and report regulatory refs, `HasRuleSource` for `with source S`, `CorpusInBody`, `DerivesFrom` for rule-source `extends`) |
| `[metadata]` / labels / `[docReference]` | `custom_annotations` payloads — schema-level on the `SchemaNode`, attribute-level under a property-keyed `rosetta_attribute_annotations` map on the owning SchemaNode |
| TypeAlias / BasicType / annotation declarations / library functions | Bridge-side lowering or counted as `needs_review` — named in the run stats, never silently dropped |

Every expression (conditions, function operations, rule bodies) is
persisted as the canonical `Expr::to_json()` payload — deterministic,
serialization-stable, write-once provenance. Sigil resolves names but
never types expressions; typing is the transpiler's burden at generation
time (see [Codegen surface](#codegen-surface)).

## Namespaces

Rosetta models are namespace-first-class sources (issues #267/#268):

- `namespace a.b` in a `.rosetta` file creates a `NamespaceNode`
  (provenance `source: "rosetta"`) with the dotted `NamespaceParent`
  chain, and every bridged schema in the file gets `SchemaNode.namespace`
  + an `InNamespace` edge.
- `import a.b.*` / `import a.b as x` become `NamespaceImports` edges
  (wildcard/alias payload recorded).
- Import-only targets — namespaces referenced by an import but declared
  by no provided file, such as the sigil builtins' `com.rosetta.model` —
  land as `discovered` namespace nodes so import edges resolve and
  validation sees them.

**Namespaces are NOT domains.** A file's domain comes from resolving its
namespace against `domains.toml`: an exact domain-key match wins, then
the last dotted segment. A namespace whose last segment matches nothing
generates nothing — doctor warns about exactly this.

### Assignment config

`domains.toml` can declare namespaces and optionally pin them to domains:

```toml
[namespaces."com.example.store"]
domain = "store"

[namespaces."com.example.shared"]   # declared, no domain pin
```

Both TOML spellings (quoted flat key and nested unquoted tables) normalize
to the same FQN; `domain` is reserved at every level; unknown scalar keys
and malformed FQNs are parse errors. Declared-but-undiscovered namespaces
are allowed — the declared set is only the validation baseline
(`namespace_import_undeclared`, `namespace_import_undeclared_dependency`,
`namespace_domain_conflict`).

### The `namespace_layout` gate

`namespace_layout = true` under `[features]` in `profiles.toml` (default
OFF = flat, byte-identical output) moves namespace-carrying schemas under
namespace-derived module paths:

```
namespace cdm.base.datetime, entity module foo:

OFF:  src/entity/foo.rs              src/domain/foo/…
ON:   src/entity/cdm/base/datetime/foo.rs
      src/domain/cdm/base/datetime/foo/…
```

Only schemas that carry a namespace move; namespace-less schemas stay
flat. API URL segments stay title/`api_path_segment`-based — namespaces
are not URLs. When the graph has namespaces, generation order also ranks
titles imported-before-importer (an import cycle is a hard error), and
the title-claim key becomes `(namespace, title)` — the same title in two
namespaces generates as two types.

## Classification interplay

Rosetta types are **auto-scored** by the classifier, exactly like JSON
schemas — provenance is not authoritative. This is the deliberate
contrast with mox, where entity-vs-VO is author-declarative and bypasses
the scorer: rosetta bridged nodes carry `origin` (not mox's `source`
key), so the priority-0 mox bypass never fires.

Pinned behaviors (rosetta_classification_tests):

- Scoring parity: an equivalent model scores identically whether authored
  as `.rosetta` or JSON Schema.
- `force_entities` / `force_value_objects` / legacy `entities` lists in
  `domains.toml` are honored for rosetta titles (observable end-to-end:
  FK columns are only emitted to entity targets).
- Choices (all-`(0..1)` attributes) score as value objects.
- `SchemaClassificationData.source` stays unset for rosetta nodes —
  the `override:source=mox` report line never appears for them.

`classify --rosetta-files ...` shows the decisions; rosetta types appear
in the report normally (not as overrides).

## Codegen surface

`rosetta_backend = true` under `[features]` gates four Domain
generators. Each must ALSO be listed in an `[api]` generators list
(the scaffolded profiles set the flag but leave the generators opt-in):

| Generator | Emits |
|-----------|-------|
| `condition_validations` | `src/domain/<domain>/validations.rs` — one `validate_{entity}_items` per array-bounded entity, one `validate_{condition}` per transpilable `ConditionNode`, a `// TODO(#262)` marker per condition that cannot transpile yet (naming the unsupported kind), plus the bridge-derived one_of option sets |
| `functions` | `src/domain/<domain>/functions.rs` — one Rust free function per `FunctionNode`: `set` → assignment, `add` → `push`/`+=`, aliases → `let`-bindings (declaration order), dispatch heads → `match` over extending children, `[transform]` annotations as doc-comment metadata |
| `rules` | `src/domain/<domain>/rules.rs` — reporting rules as computed-field functions attached to their input entity; eligibility rules as boolean endpoint guards whose doc comments name the `ApiOperation`s they gate (handler-site insertion is a documented follow-up) |
| `regulatory_reports` | `src/domain/<domain>/regulatory_reports.rs` — one module per corpus with an API endpoint stub and a rule-dispatch table seeded from the corpus' reports (rules bound via rule-source `RuleReference` edges), plus rule-source and `[transform]` hook stubs; deliberately inert (match arms return `None`) so it compiles standalone |

With all four enabled the generated crate still compiles: untranspilable
constructs degrade to `// TODO(#262)`/`TODO(#263)`/`TODO(#264)` marker
comments — the module is the record, never a guess.

`function_postconditions = true` (separate flag, default OFF) makes the
`functions` generator emit each post-condition as
`debug_assert!(<expr>, "P");`.

All of these are OFF ⇒ byte-identical output.

## Mixed JSON + rosetta runs

`--rosetta-files` and `--schemas` can be combined; the driver
(`crates/codegraph/src/driver.rs`) orders ingestion:

```
Pass 1   mox files          (if provided)
Pass 1b  rosetta files      (if provided)
Pass 1a  JSON schemas       (skips every title already bridged)
```

Bridged titles from both mox and rosetta join the JSON pass's skip-set,
so a same-titled JSON schema never duplicates a bridged node — **mox
wins over rosetta; rosetta wins over JSON** (each by passing first).
With `--schemas` + `--rosetta-files` the run prints:
`INFO: .rosetta files are a primary model source; --schemas fills gaps
for types not authored in .rosetta`. `--schemas` alone remains a
deprecated primary source (deprecation WARN points at `codegraph
migrate`).

The `classify` command applies the same ordering, so the report shows
the same model the run sees.

## Supported and deferred constructs

Two documents define the boundary; neither is duplicated here:

- **[docs/rosetta-gap-analysis.md](rosetta-gap-analysis.md)** — the
  disposition table covering all 16 `SemanticElement` kinds, all 51
  `Expr` variants, and the annotation surface, each backed by a
  characterization probe; plus the defects ledger and upstream issue map
  (#254–#268).
- **The transpiler's documented unsupported list**
  (`crates/codegraph-generate/src/rosetta_expr.rs` module header) —
  currently: `Reduce`, `Sort`, `Min`/`Max` with a lambda, `ToEnum`, the
  date-time casts (`ToDate`/`ToDateTime`/`ToZonedDateTime`/`ToTime` —
  chrono is deliberately absent from codegraph-generate), `Switch` with
  `Reference` guards, and `As`/`AsKey`/`OneOf`/`Choice`/`Constructor`
  stubs; `OnlyExists` exclusivity is not enforced; parameterized
  then-functions are refused. Known emission caveats (verbatim number
  literals, `exists`/`absent` on required fields, `OnlyElement` →
  `.first()` uniqueness gap, scalar comparisons inside lambda bodies) are
  documented in the same header.

Known model-level gaps with a disposition: Rosetta attribute cardinality
`(min..max)` is not yet representable (`min_items`/`max_items` uplift);
property-level oneOf needs the variant representation decision (#261);
`as-key` is rejected by the sigil parser itself (upstream defect). See
the gap doc's defects ledger for the current status of each.

## Rev-pinning

The sigil crates are pinned in `[workspace.dependencies]` of the root
`Cargo.toml`:

```toml
sigil-model = { git = "https://github.com/yestechgroup/sigil.git", rev = "49a6a27f..." }
```

At build time, `crates/codegraph/build.rs` parses the pinned rev out of
`Cargo.lock` and embeds it as `SIGIL_REV`, exposed by
`codegraph::rev::sigil_rev()`. `doctor --rosetta-files` prints it
(`INFO sigil — sigil-model rev <sha>`), so any parse/lower/resolve
behavior is attributable to an exact upstream revision; a binary built
without the pin WARNs with a hint.

Upgrading sigil:

1. Bump the `rev = "..."` on the four `sigil-*` entries in the root
   `Cargo.toml`.
2. Refresh the lockfile (`cargo update`) and rebuild — `build.rs` re-parses
   `Cargo.lock`, so the embedded `SIGIL_REV` follows the pin.
3. Re-run `cargo test -p codegraph --test rosetta_bridge_tests` (and the
   other `rosetta_*` suites) — the bridge contract tests pin observable
   behavior across the bump.
4. `codegraph doctor --rosetta-files ...` should report the new rev.

## Editor story

Language support for `.rosetta` lives upstream with sigil: the
**sigil-lsp** server and the **sigil VS Code extension**. They coexist
with codegraph cleanly because codegraph never parses `.rosetta` text —
it consumes resolved models through the bridge. There is no codegraph-side
tree-sitter grammar, no bundled LSP, and `codegraph lsp` accepts no
rosetta flags. This is the deliberate contrast with IFML and mox, where
codegraph maintains editor grammars (`crates/tree-sitter-ifml/`,
`crates/tree-sitter-mox/`) and serves diagnostics/completions itself.
