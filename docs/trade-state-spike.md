# WP-A spike: can codegraph handle the CDM TradeState pattern?

**Question.** The FINOS COMMON Domain Model's `TradeState` — an append-only
state snapshot with per-primitive lineage (`resetHistory`, `transferHistory`),
1→N event fan-out (`BusinessEvent.after`), and function-expressed state
transitions — is the canonical shape codegraph's generators were never asked
to produce. Does the rosetta pipeline (`sigil parse → lower → resolve` →
bridge → generators) accept a **real** CDM fragment today, and where does it
fight back?

**Method.** Shallow-clone of `finos/common-domain-model` (master) at
`/tmp/opencode/cdm/cdm-master`; target elements extracted; fixture authored
under `crates/codegraph/tests/fixtures/rosetta_trade_state/` and driven
through the EXACT production path (`ingest_rosetta_files`, one resolve call
with builtins, hard-fail on Error diagnostics — same as the rosetta_bridge
fixture). Sigil pinned at the workspace rev `49a6a27`. Every construct that
sigil rejected was captured as a diagnostic and trimmed minimally; every trim
is marked inline in the model text with `TRIM:` and tabulated below. Pinned by
`crates/codegraph/tests/rosetta_trade_state_tests.rs` (6 tests, all green).
Fixture provenance: fragments quote the FINOS CDM `rosetta-source` tree
(Community Specification License 1.0) for interoperability testing; kept
elements are marked `VERBATIM` above their source in this doc's ledger.

**Headline.** Real CDM parses 100% cleanly at the pinned rev. Resolution is
clean once the cross-namespace closure is present and the three sigil-rejected
constructs are trimmed. The full pipeline emits 3,155 files / 0 errors over
the fragment. The data plane (namespaced types, DDL with child tables and
codelist FKs), conditions plane, and namespace plane handle real CDM shapes
with only config-level friction. The functions transpiler handles the simple
real func verbatim-shape but produces **non-compiling** output for composed
real funcs — every degradation is a TODO marker, never a guess. G1–G5 are all
**verified** against emitted code, plus six new gaps.

---

## 1. What the fixture contains

8 `.rosetta` files (one per real source module, mirroring real names) +
`domains.toml`. Bridge stats for one full run (`Pass 1b`):

```
8 files, 56 types (3 choices), 143 properties, 111 edges,
12 enums (41 values, 12 codelist schemas), 6 extends,
9 conditions recorded (9 nodes, 3 one_of),
19 regulatory nodes (3 refs), 15 functions,
0 rules (0 applies-to, 0 rule-source refs, 0 skipped),
10 namespaces (8 imports)
```

### Real-vs-stub ledger

Verbatim = copied from `rosetta-source` unmodified (doc-string trims excluded).
Trimmed = real element, attributes/expressions dropped. Stub = minimal
carrier with real attribute names where cheap.

| File (namespace) | Verbatim | Trimmed | Stub |
|---|---|---|---|
| `base-desc.rosetta` (`cdm.base`) | **whole file verbatim**: 4 `body`s (ISDA/ICMA/ISO/AcadiaSoft), 7 `corpus`es (GMRA, ERCCBestPractice, …), 6 `metaType`s, 2 `segment`s | — | — |
| `base-datetime-type.rosetta` (`cdm.base.datetime`) | `TimeZone`, `AdjustableOrAdjustedDate` (+ its condition) | — | `BusinessDayAdjustments`, `BusinessDayConventionEnum` |
| `base-staticdata-party-type.rosetta` (`cdm.base.staticdata.party`) | `Counterparty`, `AncillaryParty`, `CounterpartyRoleEnum` | `Party` (2/8 attrs), `PartyIdentifier`, `PartyRole`, `PartyRoleEnum` (2/~100 values), `AncillaryRoleEnum` | — |
| `base-staticdata-identifier-type.rosetta` (`cdm.base.staticdata.identifier`) | `AssignedIdentifier`, `Identifier` (incl. multi-line `[docReference ICMA GMRA …]` + `required choice issuerReference, issuer`), `TradeIdentifier` | — | `TradeIdentifierTypeEnumItem` (stands in for a trimmed enum) |
| `product-template-type.rosetta` (`cdm.product.template`) | `TradableProduct.counterparty (2..2)` cardinality kept real | `TradableProduct` (adjustment attr + 3 conditions dropped), `Payout` choice (3/~10 options) | `Price`, `PriceQuantity`, `TradeLot`, `EconomicTerms`, `OptionPayout`, `CreditDefaultPayout`, `SettlementPayout`, `Product` choice, `NonTransferableProduct`, `OptionTypeEnum`, `NotionalAdjustmentEnum` |
| `event-common-enum.rosetta` (`cdm.event.common`) | `ClosedStateEnum` (all 7 values incl. `Novated`), `PositionStatusEnum` | `EventIntentEnum` (8/~20 values, real docs kept for Novation) | `QuantityChangeDirectionEnum`, `CorporateActionTypeEnum`, `TransferStatusEnum` |
| `event-common-type.rosetta` (`cdm.event.common`) | `TradeState`, `State` (+`ClosedStateExists`), `ClosedState`, `TransferState`, `Transfer` choice, `Instruction` (both conditions incl. `NewTrade`), `ResetInstruction`, `SplitInstruction` (incl. the real `//` comment), `TransferInstruction`, `PartyChangeInstruction`, `BusinessEvent`, `Reset` (+`AveragingMethodologyExists`) | `Trade` (extends kept; executionDetails/contractDetails/collateral + 3 giant conditions dropped), `Reset` (doc texts), `Valuation` (2/9), `PrimitiveInstruction` (9/13 attrs), `EventInstruction` (4/7 attrs), `ExerciseInstruction` (docs), `ExecutionInstruction` (attrs verbatim, `ExecutionDetails` stubbed), `ObservationEvent` (**bare one-of dropped — sigil gap**, see §6) | `Observation`, `AveragingCalculation`, `CreditEvent`, `CorporateAction`, `TransferBase`, `Scheduled/Unscheduled/ContingentTransfer`, `BusinessCenterTime`, `TermsChangeInstruction`, `ContractFormationInstruction` |
| `event-common-func.rosetta` (`cdm.event.common`) | `Create_Reset` (whole), `Qualify_Novation` (whole, incl. `[qualification BusinessEvent]` + 5-conjunct `set`), `Create_NonTransferableProduct` (whole body: deep choice-hop set paths), `Create_Exercise` (inputs/output/aliases/condition/both `add` ops; 1 enum literal qualified), `Create_TradeState` (5 alias stages verbatim) | `ExtractCounterpartyByRole`, `FilterOpen/ClosedTradeStates` (`filter item ->` rewrite), `ChangeCounterparty` (2 enum literals qualified; record literals + list literal + `(2..2)` kept) | `Create_Execution`, `Create_QuantityChange`, `Create_TermsChange`, `Create_PartyChange`, `Create_ContractFormation`, `Update_ProductDirection` (interface-only stubs with verbatim signatures) |

Dependency closure: the line was drawn at the product/economic-terms plane
(`Trade -> TradableProduct -> product/economicTerms` is stubbed one hop deep),
which is enough for every function expression in the fragment to resolve.
**Result: 32/56 types and 10/15 functions carry real CDM structure; 4
functions are verbatim including their bodies.**

### Construct census (sigil at rev `49a6a27`)

Accepted with zero fuss (all exercised by the fixture):

- `[metadata key]`, `[metadata id]`, `[metadata reference]`, `[metadata scheme]`, `[rootType]`
- multi-line attribute-level `[docReference ICMA GMRA namingConvention "…"
  provision "…"]`, and type-level `[docReference …]`
- `namespace cdm.event.common : <"doc">` (namespace doc annotation) and
  `version "${project.version}"` (Maven template — resolves as a plain string)
- `(2..2)` exact cardinalities, `(0..*)`, `(1..*)`
- `condition:` with nested if/then/else, `and`, `only exists`, `True`,
  `required choice a, b`, `switch … then …, default …`
- functions: `alias` chains, `if … then … else …`, `as` casts,
  `only-element`, `count`, `filter … then only-element`, multi-arg calls,
  **record literals** `Counterparty { partyReference: …, role: … }`,
  **record literal with `...` spread**, **list literals** `[a, b]`,
  `empty`, `//` comments inside bodies, func-level `condition`,
  `[qualification BusinessEvent]`

Rejected (captured diagnostics — the only three sigil failures on real CDM):

| Real construct | Diagnostic | Fixture trim |
|---|---|---|
| bare one-of condition: `condition:` / `one-of` (on `ObservationEvent`) | `E0101 unknown symbol 'one'` / `'of'` — the body has no left operand so the postfix `one-of` op never forms; sigil's one-of synthesis covers `choice` types only | condition dropped, noted inline |
| implicit filter member: `tradeStates filter state -> closedState exists` (both real `Filter*TradeStates`) | `E0101 unknown symbol 'state'` | rewritten `filter item -> state -> …` |
| bare enum value literal: `= Party1`, `= Put` (real `ChangeCounterparty`, `Create_Exercise`) | `E0101 unknown symbol 'Party1'` | qualified: `= CounterpartyRoleEnum -> Party1` |

(Real CDM overwhelmingly writes qualified enum paths — the three bare
literals are outliers — but they are in master.)

Tooling quirk found on the way: `sigil check` renders **syntax** diagnostics
only; resolution diagnostics silently set the exit code without ever being
printed (CLI rev `49a6a27`, `sigil-cli/src/main.rs` render call passes only
`syntax_diagnostics`). The codegraph bridge formats them itself, so this does
not affect the pipeline — but spike tooling built on the CLI will misreport.

---

## 2. Per-plane results

### 2.1 Types / DDL — works, with real-CDM nesting

`TradeState` bridges as a namespaced SchemaNode (`cdm.event.common`,
`domain = common` via last-segment fallback — see gap G8) and, promoted with
`force_entities = ["TradeState", "Trade"]`, generates the entity surface.
The DDL nests exactly along the real composition:

```sql
CREATE TABLE IF NOT EXISTS common.trade_state (
    id UUID NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    platform_organization_id UUID NOT NULL DEFAULT '00000000-…'::UUID,
    trade_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL, …
);
-- Child table: common.trade_state_state
CREATE TABLE IF NOT EXISTS common.trade_state_state (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trade_state_id UUID NOT NULL, … position_state TEXT, …
);
-- Child table: common.trade_state_state_closed_state
… FOREIGN KEY (state) REFERENCES common.closed_state_enum(code) …
-- Child tables: trade_state_reset_history (+ _observations,
-- _reset_value, _averaging_methodology), trade_state_transfer_history
-- (+ _transfer + one child per Transfer choice option),
-- trade_state_observation_history (+ _credit_event, _corporate_action)
```

Real CDM doc strings land as `COMMENT ON COLUMN` texts (e.g.
`trade_state_state_closed_state.activity_date` carries the full real
definition). `ClosedStateEnum` → `common.closed_state_enum` codelist table
with all 7 real values. `Trade extends TradableProduct` exercises the
ingest-time attribute merge: `common.trade` gains the ancestor's
`counterparty (2..2)` + `ancillary_party` + `trade_lot` columns/children.

### 2.2 Functions / transpiler — the simple real func is right; composed real funcs degrade (loudly)

`Create_Reset` (verbatim real CDM) transpiles to exactly the copy-from-before
+ lineage-append semantics the model expresses:

```rust
/// This function processes a lifecycle reset event … VERBATIM.
pub fn create_reset(instruction: ResetInstruction, trade_state: TradeState) -> TradeState {
    let mut reset = TradeState::default();
    reset = trade_state;
    reset.reset_history.push(instruction.reset);
    reset
}
```

The fan-out signature survives: `pub fn create_exercise(exercise_instruction:
ExerciseInstruction, original_trade: TradeState) -> TradeState` with both
`add exercise` ops recognized (one transpiled as `exercise += execution;`,
the other TODO'd). Real CDM has **no dispatch heads** in the fragment
(no `(attr: Enum->Value)` syntax) — the emitted module contains no `match`,
and that is the correct reading of the source.

Where composition exceeds the transpiler, output is explicit TODO markers,
never guesses:

```rust
pub fn create_trade_state(primitive_instruction: PrimitiveInstruction, before: TradeState) -> TradeState {
    // TODO(#263): transpile alias 'execution' (unsupported: SymbolReference:
    //   function-like reference 'Create_Execution(...)' is not a field access)
    …
    let mut after = TradeState::default();
    after = contract_formation;   // ← undefined local: module would not compile
    after
}
```

Emitted-module defects observed on real funcs (each is gap-ledger G9):

- **No `use` lines at all** — `Update_ProductDirection(… NonTransferableProduct …)`
  references `party`/`template`-domain types bare; even same-domain types
  (`ExerciseInstruction`) are unqualified. The module cannot compile as emitted.
- **Input/output cardinality dropped**: `counterparties Counterparty (1..*)`
  emits `counterparties: Counterparty` (not `Vec<_>`); `Create_Exercise`'s
  `(1..*)` output emits `-> TradeState`. The one place cardinality survives is
  the validations plane, not signatures.
- **Multi-line func definitions break doc comments**: `Create_TradeState`'s
  real definition contains newlines; the emitted `///` block leaves the
  continuation lines unprefixed (syntax error).
- `only-element` lowers to `.first()` without unwrapping (type drift).

### 2.3 Conditions / validations — real conditions in, transpilable ones out, rest TODO'd

All 9 real conditions land as `ConditionNode`s with canonical `Expr::to_json`
payloads (plus 3 one_of nodes for the choices). The `condition_validations`
generator emits, among others:

```rust
/// Item-count constraints for Trade (create path).
pub fn validate_trade_items(dto: &CreateTradeRequest) -> Result<(), String> {
    if dto.counterparty.len() < 2 { return Err(format!("Trade.counterparty allows at least 2 item(s), got {}", …)); }
    if dto.counterparty.len() > 2 { return Err(format!("Trade.counterparty allows at most 2 item(s), got {}", …)); }
    Ok(())
}

/// Condition 'AveragingMethodologyExists' for Reset (create path).
pub fn validate_averaging_methodology_exists(dto: &CreateResetRequest) -> Result<(), String> {
    if !(if (dto.observations.len() > 1) { dto.averaging_methodology.is_some() } else { vec![] }) {
        return Err("AveragingMethodologyExists failed".to_string());
    }
    Ok(())
}

// TODO(#262): transpile condition 'NewTrade' from its Expr::to_json payload
//   (unsupported: FeatureCall: optional receiver requires exists-guard: 'primitive_instruction')
// TODO(#262): transpile condition 'ClosedStateExists' … (optional field 'position_state' accessed bare)
// TODO(#262): transpile one_of 'Transfer_one_of' from its options [ScheduledTransfer, …]
```

The real `(2..2)` counterparty cardinality becomes a real validation pair —
a genuinely CDM-specific rule for free. But note the defect in
`AveragingMethodologyExists`: an `if` **without else** in boolean position
lowers its else branch to `vec![]`, which does not typecheck (gap G10c).
The guarded-optional family (`exists` guards, enum `=` on optionals) is
entirely TODO'd — that family covers most real CDM conditions.

### 2.4 Rules plane — real CDM has nothing to feed it

Grepping all 145 real `.rosetta` files: **zero** `reporting rule`,
`eligibility rule`, `rule source`, or `report` elements exist in
`rosetta-source` (CDM's regulatory reporting is expressed outside the
`.rosetta` tree). The bridge's rule planes (#264/#265 node families) are
therefore unexercisable from real CDM and remain pinned only by synthetic
fixtures. Not a defect — a scope fact for the epic: **do not use real CDM as
the acceptance model for rules.**

### 2.5 Regulatory plane — verbatim bodies make real docReferences resolve

With the real `base-desc.rosetta` copied in (bodies/corpora/segments verbatim,
including the ~90-word ICMA body definition), the `Identifier` type's real
multi-line `[docReference ICMA GMRA namingConvention "Identifier" provision
"…"]` produces `RegulatoryReference` edges to the `ICMA` body / `GMRA` corpus:
19 regulatory nodes, 3 refs. Without `base-desc`, the same docReferences are
silently skipped (documented bridge behavior: unresolvable targets are not
errors). Multi-line docReference metadata parses and ingests cleanly.

### 2.6 Namespaces / paths — the plane works; the domain mapping fights

The namespace plane picks up real CDM structure faithfully: 10 namespaces
(`cdm`, `cdm.base`, `cdm.base.staticdata`, `cdm.base.staticdata.party`,
`cdm.base.staticdata.identifier`, `cdm.base.datetime`, `cdm.product`,
`cdm.product.template`, `cdm.event.common` rosetta-sourced +
`com.rosetta.model` discovered from sigil builtins), 8 `NamespaceImports`
edges from the real `import cdm.base.staticdata.party.*` lines, dotted
`NamespaceParent` chains. Generation order resolves the import DAG.

Two frictions, both real-CDM-shaped:

1. **Cross-domain imports hard-error without `depends_on`** (#267
   `namespace_import_undeclared_dependency`) — real CDM's
   `import cdm.product.template.*` from `cdm.event.common` trips it the
   moment the namespaces land on different domains. The fixture declares the
   5 edges; a whole-CDM import would need ~140. Working as designed, but the
   config burden is per-edge and manual.
2. **Domain assignment ignores `[namespaces."x"].domain`** (gap G8): the
   rosetta bridge resolves domain via the mox last-segment fallback, so
   `cdm.event.common` → domain `common`, `cdm.base.datetime` → `datetime`,
   etc. — the bounded context "cdm" fragments into 5 domains unless you
   declare five. Declaring `[namespaces."cdm.event.common"] domain = "cdm"`
   does not change the bridge's behavior; it only makes validation warn
   (`namespace_domain_conflict`) — which is how the mismatch surfaces. The
   fixture therefore declares the 5 observed domains and leaves the
   `[namespaces]` entries without `domain` keys (2 residual warnings remain
   for the intermediate parents `cdm.base` / `cdm.base.staticdata`, which
   legitimately span domains in real CDM).

schema_id: the rosetta bridge emits `cdm.event.common/TradeState` (mox
`package/Name` convention) while the JSON path's #267 contract is
`cdm.event.common::TradeState` — three producer conventions now coexist
(gap G7).

---

## 3. Gap ledger (G1–G5 + new)

### G1 — no append-only policy: VERIFIED (config mitigation works; DB stays mutable)

With `operations = ["create","read","list"]` on TradeState, the generated
router narrows exactly as configured:

```rust
fn trade_state_routes() -> Router<AppState> {
    … .route("/", axum::routing::get(trade_state_handler::list)…
        .merge(axum::routing::post(trade_state_handler::create)…))
    .route("/{trade_state_id}", axum::routing::get(trade_state_handler::get_by_id)…)
```

and the repository contract is create/find/list only. **But the same run's
DDL remains a mutable table**:

```sql
updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), deleted_at TIMESTAMPTZ NULL, …
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE common.trade_state TO app_user;
CREATE TRIGGER trg_trade_state_updated_at BEFORE UPDATE ON common.trade_state …
```

and sibling entities (`Trade`, …) still emit full `async fn update` +
`ActiveModel::update` machinery. There is no `append_only`/`immutable`
model option: nothing stops `UPDATE common.trade_state SET state = …` at the
DB, which is precisely what the CDM pattern forbids (a state snapshot must
never change after creation). The operations config is a route-level
mitigation only.

### G2 — no lineage materialization: VERIFIED

before→after lives only in (a) function bodies (`set reset: tradeState`,
the alias chains of `Create_TradeState`) and (b) the graph's function
payloads (`Expr::to_json` ops). Nothing in the data plane connects successor
to predecessor: `trade_state` rows link to `trade_id` and nothing else; the
`BusinessEvent.after (0..*)` / `Instruction.before (0..1)` real-Cdm fan-out
generates as two unrelated tables (`business_event`, `instruction`) with no
successor-of edge, no event table that ties `before → {after…}`, and no
materialization of the novation pattern (quantifyChange closes the
stepped-out state with `ClosedStateEnum.Novated` while `contractFormation`
mints the stepped-in state — expressed in real CDM via `Qualify_Novation`'s
5-conjunct predicate, which our conditions plane carries as JSON but emits
only a TODO for).

### G3 — func emission nits: VERIFIED (and worse on real input)

- Output default-init vs copy-from-before: for `Create_Reset`, emission
  (`let mut reset = TradeState::default(); reset = trade_state;`) matches
  CDM's `set reset: tradeState`. For `Create_TradeState`, CDM's output
  **derives from `before` through an alias chain whose base case is
  `before` itself**; our emission default-inits `after` and then assigns an
  untranspiled alias — the copy-from-before seed is lost and the local is
  undefined. There is no "start output from before" fallback rule.
- Non-builtin I/O type paths unqualified: emitted module has zero imports —
  `pub fn update_product_direction(… original_payer: CounterpartyRoleEnum …)
  -> NonTransferableProduct` names `party`/`template`-domain types bare, and
  even same-domain types (`ExerciseInstruction`, whose struct lives in the
  sibling `exercise_instruction` module) are unqualified in
  `create_exercise`'s signature — bare paths do not resolve from a sibling
  module, so nothing in the module typechecks beyond builtins.
- Cardinality: `Create_Exercise`'s real `(1..*)` output emits `-> TradeState`
  (should be `Vec<TradeState>`); the two `add` ops' fan-out shape survives
  only as `+=`.

### G4 — no snapshot-aggregate template: VERIFIED

`TradeStateRepository` = `create` / `find_by_id` / `list`. `create` inserts
the snapshot row **and** every child row (`reset_history`, `state`,
`transfer_history`, …) from one flat `CreateTradeStateRequest`. There is no
notion of "create successor state for trade X", no effective-dating, no
immutability, no aggregate boundary tying `Trade → [TradeState]` as a
timeline; reads rehydrate one snapshot but nothing answers "current state of
trade X" or "state as of date D".

### G5 — no timeline UI: VERIFIED (upstream of generation)

`codegraph ifml-scaffold` hard-requires `--schemas <dir>` + `--classifier`
and has zero rosetta plumbing (CLI help mentions only "from JSON schemas";
0 occurrences of "rosetta" in its args). A rosetta-bridged graph cannot
scaffold a starter `.ifml`, so the IFML plane never sees `TradeState` unless
the model is duplicated as JSON. (`ifml-generate` reads the graph and might
accept rosetta-bridged titles, but there is no scaffold path to start from —
untested beyond this.)

### New gaps found by the spike

- **G6 (sigil, 3 diagnostics)**: bare one-of conditions, implicit filter
  members, and bare enum value literals — all present in CDM master — fail
  resolution (`E0101`, exact diagnostics in §1). Also: `sigil check` never
  prints resolution diagnostics (exit-code only).
- **G7**: schema_id convention divergence (`ns/Name` rosetta+mox vs
  `ns::Name` JSON/#267).
- **G8**: producers ignore `[namespaces."x"].domain`; domain = namespace
  last segment; real CDM fragments into 5 domains; `namespace_domain_conflict`
  warnings fire on the intermediate parents (`cdm.base`,
  `cdm.base.staticdata`) that legitimately span domains.
- **G9 (functions emission on real input, see §2.2)**: no module imports;
  list input/output cardinality dropped; multi-line definitions break doc
  comments; TODO'd aliases leave undefined locals (module won't compile);
  `only-element` → `.first()` type drift.
- **G10 (conditions emission, see §2.3)**: `if` without `else` in boolean
  position lowers the else branch to `vec![]` (AveragingMethodologyExists);
  the guarded-optional family (exists-guards, enum equality on optionals,
  `only exists`, `switch`) — the bulk of real CDM conditions — is
  TODO-marked.
- **G11 (scope fact)**: real CDM contains zero rule/report elements — rules
  plane acceptance must stay on synthetic fixtures.

---

## 4. Drafted follow-up issue bodies (orchestrator files)

### Issue A — `sigil: support bare one-of conditions, implicit filter members, bare enum literals (all in FINOS CDM master)`

```markdown
Title: sigil(resolution): three real-CDM constructs fail E0101 — bare one-of, implicit filter member, bare enum literal

Body:
Spike evidence: docs/trade-state-spike.md §1 (construct census), fixture
crates/codegraph/tests/fixtures/rosetta_trade_state/model (every trim is
annotated inline with `TRIM:`).

All three parse (no syntax diagnostics) and fail resolution with E0101 at
rev 49a6a27:

1. Bare one-of condition on a TYPE (real CDM `ObservationEvent`):
       condition:
           one-of
   Lowers to unknown symbols 'one' / 'of' — the postfix `one-of` operator
   never forms without a left operand. Sigil already synthesizes
   `OneOf(ImplicitVariable)` for choice types with no conditions
   (sigil-resolve element_json); extend the same handling to bare
   type-level conditions.

2. Implicit filter member (real CDM `FilterClosedTradeStates`):
       add closedTradeStates: tradeStates filter state -> closedState exists
   `state` is unbound (E0101 unknown symbol 'state'). Rosetta semantics bind
   the iteration variable implicitly; explicit `filter item -> …` resolves
   and is our current workaround.

3. Bare enum value literal (real CDM `Create_Exercise`, `ChangeCounterparty`):
       if optionPayout -> optionType = Put …
   E0101 unknown symbol 'Put'. Qualified form `OptionTypeEnum -> Put` works
   and is the CDM-majority style, but master carries bare literals.

Ask: resolution-level lowering for 1 (synthesize OneOf), name-binding for 2,
enum-literal scoping for 3. Each unblocks deleting a TRIM note from the
trade_state fixture.

CC tooling: `sigil check` prints only syntax diagnostics; resolution
diagnostics affect the exit code silently. Worth printing them.
```

### Issue B — `rosetta bridge: schema_id should follow the #267 <ns>::<Name> contract`

```markdown
Title: rosetta-bridge: schema_id uses `<ns>/<Name>` while #267 standardized `<ns>::<Name>` (JSON path)

Body:
codegraph-core::types::namespace::qualified_schema_id (#267) defines the
namespaced schema_id contract as `<ns>::<Title>`; the JSON producer follows
it. The rosetta bridge (crates/codegraph/src/ingest/rosetta_ingest.rs,
`fn schema_id`) keeps the mox `package/Name` shape:
`cdm.event.common/TradeState` vs the JSON path's `cdm.event.common::TradeState`
for the same concept. Three producer conventions now coexist (mox
`package/Class`, rosetta `ns/Name`, JSON `ns::Name`), which leaks into
schema_id-keyed surfaces (skip-sets, doctor, tooling that joins on ids).

Decide one canonical form (and whether mox migrates with a back-compat
alias), then route both bridges through `qualified_schema_id`. Pinned by
rosetta_trade_state_tests::trade_state_lands_as_a_namespaced_schema… (the
`/` shape is asserted today; flip with the fix).
```

### Issue C — `producers ignore [namespaces."x"].domain; rosetta domain = namespace last segment`

```markdown
Title: namespaces(#267/#268): [namespaces."x"].domain is validation-only — rosetta/mox bridge still maps domain = last segment

Body:
Evidence: docs/trade-state-spike.md §2.6. Ingesting a real-CDM fragment:

- The bridge resolves each file's domain via mox_ingest::resolve_domain —
  exact domain-key match, then namespace LAST SEGMENT.
- `[namespaces."cdm.event.common"] domain = "cdm"` changes nothing in
  ingestion; it only arms namespace_domain_conflict, so the declared intent
  (all CDM namespaces → one `cdm` domain) surfaces as WARN noise while
  generation fragments the model across 5 domains (common, datetime, party,
  identifier, template).
- Intermediate parents that legitimately span domains in real CDM
  (`cdm.base`, `cdm.base.staticdata`) warn even with the observed layout.

Ask: make producers consult the namespace→domain assignment in
resolve_domain (declared `domain` wins, last-segment fallback remains), and
decide whether parent-namespace spanning is a warning, an info, or a config
shape (`domain` on `cdm.base`?) — today it is un-actionable WARN noise for
any real-CDM-shaped model. Fixture domains.toml documents the current
workaround (declare the 5 observed domains, omit `domain` keys).

Also: the #267 cross-domain import validation (namespace_import_
undeclared_dependency) fires per imported namespace — real CDM needs one
`depends_on` edge per namespace pair (~140 for whole-CDM). Consider
wildcard/transitive depends_on before any whole-CDM import attempt.
```

### Issue D — `functions transpiler: real-CDM gaps (Vec cardinality, module imports, multi-line docs, defined locals)`

```markdown
Title: rosetta functions generator: emitted module does not compile on real CDM funcs — 4 concrete defects

Body:
Evidence (all from one pipeline run over
tests/fixtures/rosetta_trade_state, quotes in docs/trade-state-spike.md §2.2):

1. No imports: the module references cross-domain types bare
   (`update_product_direction(… original_payer: CounterpartyRoleEnum …)
   -> NonTransferableProduct`) and even same-domain types
   (`ExerciseInstruction`). Need `use` synthesis from the graph (I/O type →
   domain module path), mirroring what the type_registry does for handlers.
2. Cardinality: `(1..*)` inputs/outputs emit bare `T` —
   `pub fn create_exercise(…) -> TradeState` must be `Vec<TradeState>`;
   `add`-ops then push. Input `(1..*)` (ExtractCounterpartyByRole) likewise.
3. Multi-line definitions break doc comments: `Create_TradeState`'s real
   definition contains newlines; emitted continuation lines lack `///`
   (syntax error). Sanitize definitions to a single line or prefix every
   line.
4. Defined locals for TODO'd aliases: when an alias is TODO'd, later ops
   still reference the snake_cased name (`after = contract_formation;`) —
   undefined identifier. Emit the TODO as a `todo!()`-seeded let-binding of
   the declared type, or drop dependent statements.

Also (minor): `only-element` lowers to `.first()` (Option) without unwrap —
pick `.remove(0)`/`expect("only-element")` semantics or thread Option.

Test seed: rosetta_trade_state_tests::generated_functions_show_copy_from_
before_and_no_dispatch currently pins the degraded shapes; tighten into a
`cargo check` gate on generated functions.rs once 1–4 land. (Blocked on a
generated-crate compile harness — the ifml gate's NodeProject pattern may
generalize.)
```

### Issue E — `condition transpiler: if-without-else lowers else to vec![] in boolean position`

```markdown
Title: rosetta conditions: `if COND then X` (no else) transpiles `else { vec![] }` — type error in boolean position

Body:
Real CDM Reset condition AveragingMethodologyExists
(`if observations count > 1 then averagingMethodology exists`) emits:

    if !(if (dto.observations.len() > 1) { dto.averaging_methodology.is_some() } else { vec![] }) {

`vec![]` is not `bool` — the generated crate does not compile. The
default-only-else lowering (cf. commit dd8292b8's switch fix) needs a
boolean-position rule: `else false`.

Larger observed family (docs/trade-state-spike.md §2.3): every guarded-
optional construct in the fragment TODOs out — `exists` guards on optionals,
enum equality on optionals (`positionState = PositionStatusEnum -> Closed`),
`only exists`, `switch`. These ARE most real CDM conditions (8 of our 9).
A `Option::is_some_and`-style lowering for exists-guarded navigation would
convert the majority.
```

### Issue F — `snapshot-aggregate template: append-only TradeState-shaped entities (G1+G2+G4)`

```markdown
Title: generators: append-only snapshot entities (CDM TradeState pattern) — DDL still mutable, no successor lineage, no aggregate reads

Body:
Evidence: docs/trade-state-spike.md §3 G1/G2/G4 (real-CDM fixture, all
quotes from emitted code).

Today the only lever is operations=["create","read","list"], which narrows
routes+repository but leaves: (a) mutable DDL — updated_at/deleted_at
columns, `GRANT UPDATE, DELETE … TO app_user`, BEFORE UPDATE trigger; (b) no
lineage — successor snapshots link to the trade but not to each other;
BusinessEvent/Instruction generate as unrelated tables (real CDM:
Instruction.before (0..1) → BusinessEvent.after (0..*) is THE fan-out);
(c) no aggregate reads — nothing answers "current state of trade X" /
"state as of D".

Proposal (opt-in via domains.toml entity_config, e.g. `append_only = true`
on a `role = "snapshot"` entity):
- DDL: drop updated_at/deleted_at + UPDATE grants; keep insert-only + select.
- Repo: `create_successor(pred, cmd) -> id` (+ predecessor FK column) and
  `current_for_trade(trade_id)` / `as_of(trade_id, date)` queries.
- Function plane: seed the transpiler's output state from the `before`
  argument when the func's first op is copy-from-before (Create_Reset/
  Create_TradeState evidence), so generated transition fns thread snapshots.
- Validation: forbid `update`/`delete` in operations for append_only
  entities (config error, gRPC/ops precedent).

Fixture: tests/fixtures/rosetta_trade_state (TradeState + Trade + ClosedState
family) is the acceptance model; the six tests in
rosetta_trade_state_tests.rs pin today's behavior and should flip.
```

### Issue G — `ifml-scaffold: no rosetta path (G5)`

```markdown
Title: ifml-scaffold is JSON-only — rosetta-bridged graphs cannot scaffold a starter .ifml

Body:
`codegraph ifml-scaffold` requires `--schemas` + `--classifier` (JSON) and
has no `--rosetta-files` / graph mode, so a rosetta-only project cannot
start the IFML loop (docs/trade-state-spike.md §3 G5). Two options:
(a) accept a graph-only mode (scaffold from querier titles — the bridge
already lands SchemaNodes with namespaces/domains), or (b) document the JSON
sidecar requirement for rosetta projects. (a) preferred; the TradeState
fixture is the natural test model (list/details views over a snapshot
entity expose the timeline-UI gap in the same stroke).
```

---

## 5. WP-B/C/D/E refinements suggested by the evidence

- **WP-B (types/DDL)**: the composition/child-table machinery already
  reproduces real CDM nesting (5-level child chains, codelist FKs, real
  comments). Spend effort on the append-only DDL variant (Issue F) rather
  than on broader type coverage; `force_entities` is adequate for
  promotion, but rosetta `choices` all score VO — confirm that is desired
  for `Transfer`/`Payout` (they became child tables per option).
- **WP-C (functions)**: prioritize Issue D item 1+4 (compilation) over
  expression coverage — a module that compiles with TODO stubs is
  reviewable; one that doesn't is dead output. The `Create_*` interface-func
  pattern (declaration-only, real signature) is how real CDM actually
  composes — signature-only emission is already correct for them.
- **WP-D (conditions/rules)**: the guarded-optional lowering (Issue E) is
  the highest-value transpiler work — it converts the majority of real CDM
  conditions. Rules/regulatory planes: keep synthetic fixtures (G11);
  regulatory docReference→body resolution already works on real data.
- **WP-E (namespaces/UI)**: Issue C is the blocker for any whole-CDM or
  multi-namespace consumer story — declared-vs-observed domain must agree
  before the namespace plane feels real. UI: after Issue G, a TradeState
  timeline view is the natural first timeline-UI spike target.

## 6. Verification record

- `cargo test -p codegraph --test rosetta_trade_state_tests` — 6 passed
  (`full_pipeline_runs_clean_over_the_real_cdm_fragment`,
  `trade_state_lands_as_a_namespaced_schema_and_closed_state_as_a_codelist`,
  `real_cdm_functions_land_with_structured_io_and_ops`,
  `fragment_conditions_land_as_nodes_and_emit_into_validations`,
  `generated_functions_show_copy_from_before_and_no_dispatch`,
  `trade_state_operations_config_yields_no_update_or_delete_surface`).
- Full rosetta battery green: bridge 16, bridge_suite 6, function 12,
  condition 8, rule 12, regulatory 8, transpiler 13, pipeline 4,
  classification 5, scalar_bounds 2, gap_probes 29, gap_coverage 4 + the 6
  new. `cargo fmt --check` clean; `cargo clippy -p codegraph -p
  codegraph-generate --all-targets -- -D warnings` clean.
- Manual CLI runs: default profile → 4,577 files / 0 errors; spike profile
  (rosetta_backend + functions + condition_validations) → 3,155 files /
  0 errors; `Pass 1b` stats quoted in §1.
