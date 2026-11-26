# TDD — the generated IFML e2e surface must describe the generated app (issue #835; #808 re-triage)

Status: **LANDED** on `fix/ifml-fixture-dto-keys` (PR pending).

## 0. The report, and why it was misdiagnosed

The consumer (`yestechgroup/crewbase` @ `106a0fb97`) skipped 14 seeded IFML
auth tests under `#808` with the reason *"auth-spec seeding still drops
`headers: seedHeaders`"*. That half of #808 **did** land (#811):

```sh
for f in app/generated/ui/tests/ifml/*.auth.spec.ts; do
  grep -q "request.post(apiUrl" "$f" && grep -q seedHeaders "$f" ...
done   # → 14 OK-HEADERS, 0 NO-HEADERS
```

So the header fix held. The tests still failed. Reading the artefacts
(`app/test-results/artifacts-20261125T083110/playwright/…`) gives two
unrelated defects, both instances of ONE class: **the emitted test asserts
against a shape the emitted app does not have.**

### Defect A — the seed body's keys are not the DTO's keys

The create/update DTOs carry `#[serde(deny_unknown_fields)]` (#539), so a
near-miss key is a 422 — and the emitted `beforeAll` parses the response
without asserting it:

```ts
const seeded_resp = await request.post(apiUrl('/api/v1/core/worker'), { headers: seedHeaders, data: { 'worker_type_code': 'Employee', … } });
const seeded = await seeded_resp.json();
seededId = String(seeded.data?.id ?? seeded.id);   // → "undefined"
```

`seededId = "undefined"` → the detail load 400s → the kit error page → the
assertion fails on an unrelated screen (exactly the reported symptom).

Why the key is wrong: `fixture_entries()` takes the component's **DSL**
field names and `wire_key_for()` applies **casing only**, while the
required-entry seeding beside it uses `codegraph_core::types::resolve_field`
— the projection that strips a trailing `_code` and appends `_id` to a
scalar `refers`. One body, two naming rules.

Live instances in one consumer tree:

| DSL feature | emitted key | DTO field |
|---|---|---|
| `workerTypeCode` | `worker_type_code` | `worker_type` |
| `refers Vendor[0..1] vendor` | `vendor` | `vendor_id` |
| `approvalStatusCode` (+ required seeding) | `approval_status_code` **and** `approval_status` | `approval_status` |
| `rewardCode` | `reward_code` | `reward` |

### Defect B — `heading()` points at a page with no heading

`templates/ifml/svelte/page.tera` renders `<h1>{{ label }}</h1>` only in the
mapped / `timeline` / `table` / `list` branches. `details`, `form` and
`chart` render none. `route_generator/context.rs` hands the single
`show_heading` slot to the **first top-level component regardless of type**,
so a `details`+`table` view (the consumer's `FindingsView`) renders no
heading at all — while the POM emits
`heading(): getByRole('heading', { name: 'FindingsView' })` and the render
spec asserts `toBeVisible()`.

The 2026-11-25 artefact shows the page fully rendered (terms, definitions,
table) with `element(s) not found` for the heading — the seed had actually
succeeded for that view; only the assertion was impossible.

## 1. The fixes (RED first)

### 1a. One projection rule for fixture keys

`fixtures.rs::project_fixture_keys(db, entity, entries)`:
`to_snake_case(name)` is matched against the schema property's `name` and
`rust_field_name`; a match projects through `resolve_field` (the DTO's own
rule), else the snake spelling stands. Dedupe keeps the first position and
lets a later duplicate replace the value (the required-entry value wins).
Called at the head of `with_required_entries`, which **every** construction
path already funnels through (`typed_fixture_entries`,
`valid_fixture_entries`, `ux_fixture_entries`, the auth/workflow/journey
seeds, `plan_required_refs`' parent bodies).

### 1b. One heading rule

New `ifml/page_heading.rs`:
`component_renders_heading` (mapped | table | list | timeline),
`page_heading_owner` (the first top-level component that can),
`page_renders_heading`. The route context assigns `show_heading` to the
OWNER only; the e2e `assert_heading` legs (render, persona, journey
arrival, the POM fallback) are gated on the same predicate.

### 1c. A silent seed is a defect

The seeded `beforeAll` reads the body once and asserts `ok()` naming the
URL, the status and the body — a 422 is a named failure, never
`seededId = "undefined"`.

## 2. Gates and teeth

| Gate | Kind | Teeth (mutation) |
|---|---|---|
| projection KATs (`_code` strip, scalar ref → `_id`, array ref keeps the name, required-FK dedupe, unknown field fallback) | node-free | revert the projection → the body key assertions fail |
| heading predicate KATs (details-only → none; details+table → the table; two tables → the first; mapped details → renders; specless table → none; container-nested → none) | node-free | revert to "first top-level" → the details+table case fails |
| route-context pin: the `<h1>` moves to the table for details+table, and a details-only page renders none | node-free | same mutation |
| e2e pins: the findings-shaped guarded view asserts the heading its page renders; a collection the page cannot head asserts `primaryRoot()` only; the POM fallback refuses an invented heading | node-free | revert either gate → the corresponding pin flips |
| seed-shape pins (`auth.rs::seed_assert_tests`, `fixtures::every_seed_shape_asserts_ok_before_parsing`, `ifml_auth_setup_tests`) | node-free | drop an `expect(ok)` → the pin fails |
| nightly `ifml_codegen_gate` over `RefundFindings` (details + `list`, no label, guarded id) and `GuardedRefundDetail` | live | pre-fix the seed posts `channel_code` → 422 → the loud assert reddens the auth category |

## 2a. Verification log

- `cargo test -p codegraph-generate --lib` → **1379 passed** (55 in
  `ifml::e2e_test`).
- `cargo test -p codegraph --test ifml_auth_setup_tests --test
  ifml_template_tests --test ifml_e2e_gen_tests --test ifml_e2e_tests
  --test ux_rules_byte_identity_tests --test forms_byte_identity_tests
  --test machine_read_model_tests --test ifml_dsl_features_tests --test
  ifml_remote_functions_tests --test ifml_tanstack_forms_tests` →
  **106 passed, 0 failed**. Both byte-identity canaries hold with no
  re-bless.
- `cargo clippy -p codegraph-generate --all-targets -- -D warnings` clean;
  `cargo fmt --all -- --check` clean.
- Generated-shape spot checks over the `ifml_gate` fixture tree:
  `tests/ifml/refund-findings.auth.spec.ts` seeds
  `data: { 'title': …, 'submitted_at': …, 'channel': 'Email' }` (the DSL
  field was `channelCode`), carries the read-once/assert/parse triple, and
  `src/routes/refundfindings/+page.svelte` renders `<h1>RefundFindings</h1>`
  immediately above `<table data-testid="history-table">` (the list owns
  the heading; the leading `details` component does not).

## 3. Consumer acceptance

Per-view causes in the consumer's skipped family (each verified against the
generated tree at `106a0fb97`):

| view | cause | fixed by |
|---|---|---|
| `FindingsView` | details + table, no label → page rendered no `<h1>` | heading owner |
| `ReportViewer` | details + two lists, no label → same | heading owner |
| `OrderDialog` | seed body `vendor` (DTO `vendor_id`) → 422 | key projection |
| `Job Requisition` (`RequisitionEditor`) | seed body `approval_status_code` (DTO `approval_status`) → 422 | key projection |
| `WorkerDetail` action contract (#831 acceptance) | seed body `worker_type_code` (DTO `worker_type`) → 422 → 400 page | key projection |
| (not skipped, also broken) `CompensationHistory`, `RewardEditor` | `reward_code` (DTO `reward`) | key projection |

crewbase deletes the `FindingsView|OrderDialog|ReportViewer|Job Requisition`
skip entry (14 UI tests) and the `WorkerDetail` action contract's seed stops
422ing, which unblocks the #831 `activate_worker` 200-then-403 leg.

## 4. Known gaps (recorded, not fixed here)

A view with NO top-level component at all (only `container` blocks) still
routes its permitted-persona assertion to a heading that no page renders
(`build_persona_tests`' root-less arm returns `(true, None)`). No fixture —
and no consumer view — has that shape today; the primary-root/heading
consistency work should cover it when one appears.
