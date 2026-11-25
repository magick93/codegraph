# TDD — the hurl planner seeds the fresh row from `machine.initial` (issue #752)

Status: **LANDED** on `fix/752-hurl-fresh-row-machine-initial`.

## 0. The defect, verified

Two planners read the same lifecycle but disagreed about where a fresh row
starts:

- `payload_fields` (the contract's CREATE/PUT bodies) seeded a codelist
  field with the enum's **first declared value** (`values[0]`,
  `hurl_contract/mod.rs`).
- `state_aware_legs` (the action simulation) starts from the graph
  machine's **declared initial state** (`ServiceOpState.initial`).

Upstream validates only `initial ∈ enum members`, not first-position, so a
legal model that declares its initial anywhere else made the two disagree.
Both directions are mis-pins:

- initial declared later → the contract creates a non-initial row (the
  handler refuses a non-initial create on a machine-bound field) and every
  action leg is simulated from a state the row is not in;
- initial declared first → today's aligned case (every in-repo fixture and
  every live gate), which is why no gate saw it.

A second, independent exposure lived in the PUT: the CRUD roundtrip seeds
the codelist with `values[1]`. A `.ddd` machine field is **not**
DTO-excluded (only `domains.toml` workflow status/approval fields are,
`ddd/dto.rs`), so the PUT moved the row off the initial BEFORE the action
legs ran — the same 409/403 mismatch the consumer reported (a reverse leg
pinned 409 and answered 403).

## 1. The decision

Seed the machine-bound field from the graph, once:

- `machines::machine_surface_for_entity(db, title, workflow_field)` is the
  ONE resolver (it already picks the machine bound to the workflow field
  when both exist, and skips a stale IR without the DSL mirror).
- `payload_fields` takes `machine: Option<(&str /*status field, snake*/,
  &str /*initial*/)>`; when the property is the machine's field, CREATE
  and UPDATE both seed the DECLARED initial. The PUT seeding the same value
  is deliberate: the action simulation starts there, and a no-op patch
  keeps the contract coherent (the field is still exercised — the echo
  asserts it).
- Non-machine codelists keep `values[0]` / `values[1]` byte-identically.

Rejected: "decide against the actually-seeded value" (the issue's
alternative). It makes the planner honest but leaves the contract creating
a row in a state the model's create arm refuses, and it cannot fix the PUT
desync. Seeding the declared initial fixes both.

## 2. The teeth

- Unit KAT (`api::hurl_contract::tests::
  machine_bound_field_seeds_the_declared_initial_not_the_first_enum_value`):
  a codelist `[Draft, Posted]` with the machine initial `Posted` seeds
  `Posted` in create/echo/update/update-echo and never `Draft`. It FAILS
  on the pre-fix code (which seeds `values[0]`).
- Integration (`hurl_contract_tests::
  machine_initial_seeds_the_create_body_not_the_first_enum_value`): a real
  `.mox` enum `[Active, Retired]` + a `.ddd` `machine status: PartStatus {
  initial Retired; … }` through `driver::run` emits
  `10_inventory_widget_part.hurl` carrying `"status": "Retired"` in the
  CREATE and the echo asserts it; `"status": "Active"` never appears.
  Pre-fix it emits `Active` (the codelist's first value).
- The original byte-golden `hurl_contract_generator_emits_the_full_contract_suite`
  still passes untouched — a machine-less entity emits exactly the old
  bodies.

## 3. Byte identity

No machine-bearing entity exists in the review fixture or any canaried
tree (`ux_rules`, `forms`, `policy_rls`, `handler_matrix`), so nothing
re-blesses: `review_fixture_drift_tests` is green with no regeneration.
The change is confined to the hurl planner.

## 4. Deferred — own-record op-level `when` guards

The issue's fix shape also names the "own-entity first-leg" — satisfied by
this alignment. The brief's extra ask (model **own-record** `when` guards
with the #663 three-verdict contract) is NOT in this PR:

- it needs a field-identity resolver (own-record `Name` operands are
  ambiguous with enum literals; `DddConditionPath` `OwnRecord` entries give
  the field set, but the collector must also decide each literal's type),
- it adds a plan field to `ServiceOpRoute` (14 construction sites),
- and a mis-verdicting guard planner is worse than an omitted leg (the
  #663 round-3 lesson).

Filed as a follow-up on #752 with the same three-verdict contract; the
injected-scope planner already carries the pattern to mirror.
