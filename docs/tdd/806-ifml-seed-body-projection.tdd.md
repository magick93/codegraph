# TDD — the IFML seed body IS the create DTO's body (issues #806 / #808 residual / #809)

Status: **LANDED** on `fix/ifml-seed-body-projection` (PR pending).

## 0. The report

The crewbase bump to `10a9ebac` (codegraph #806 comment) re-ran its suite
after #836/#837 and reported 5 residual UI failures, all from ONE class —
the emitted `beforeAll` seed body is not the create DTO's body:

```
ifml/order-dialog.auth.spec.ts  → seed POST /api/v1/vendors/vendor-order
                                  answered 422 {"details":[{"field":".",
                                  "message":"missing field `order_id`"}]}
ifml/requisition-editor.auth.spec.ts → seed POST /api/v1/recruiting/position-opening
                                  answered 422 missing field `position_opening_id`
(not skipped, also broken) Timecard `missing timecard_id`,
      Reward `missing reward_id` + `unknown field amount`,
      Compensation History `unknown field amount`
```

The consumer also noted the second half of the same body: *"once supplied,
the random `vendor_id` would FK-fail against `fk_vendor_order_vendor_id`"* —
i.e. the FK value was a literal uuid, not a seeded parent.

#809 (the `PositionDetail` scroll/detach loop) has **no failing artefact at
this pin**: the newest evidence bundle carries only a PASSING
`activate-worker` trace, and the last failing position-detail artefact
records `Expected "Active", Received "Approved"` — the pre-#84 body-driven
arm, not a detach. So #809 is treated as the two mechanisms that can present
as one: an under-seeded record and a click that lands in a DOM the framework
is still replacing.

## 1. Root cause

`fixtures.rs` built the body from the component's DSL `fields` alone. The
create DTO is a different surface:

1. **Required members have no serde default** — identity / natural-key parts
   included (`domain_types/dto_create.tera` emits `pub x: T` for every
   required field; issue #522 pins that a composite `id` key's parts are
   plain required columns). `fixture_entries` dropped every `*_id` name and
   both required passes skipped `prop.is_id`, so `order_id`,
   `position_opening_id`, `timecard_id`, `reward_id` were **never** in the
   body → `missing field`. (The entity-plane `#479` rule, missing on the IFML
   half.)
2. **A DSL-declared reference was posted as a literal.** A camelCase ref
   (`vendorId`) is not `*_id`-suffixed, so it survived the filter, got
   `js_value_for_type(.., Uuid)` — a random uuid — and
   `plan_required_refs` then treated "present in entries" as satisfied, so
   the dep seed was skipped exactly when the placeholder was wrong.
3. **No name-convention FK resolution.** The entity plane resolves a required
   `*_id` column with no `$ref`/edge by naming convention
   (`ui/e2e_test/refs.rs`, with a comment naming VendorOrder); the IFML plane
   only followed graph edges.
4. **Keys the create DTO does not declare were still posted.**
   `project_fixture_keys` fell back to the snake spelling for any unmatched
   key, and `property_fixture_value` returns `None` for composite/media/VO
   kinds → `unknown field amount` for a composite wrapper (`amount_cents` /
   `amount_currency` in the DTO), and 422s for derived/consumed/media fields.
   `deny_unknown_fields` makes every one of these a guaranteed 422.

## 2. The fix

### 2a. One create-body projection — `ifml/e2e_test/body.rs`

`plan_seed(db, config, api_version, entity, dsl_entries, casing)` derives the
body from the DTO plane's OWN create surface — `ddd::dto::build_dto_context`
(never a second copy of the rules; the #835 lesson generalised):

- **The surface** = `DtoContext.fields` minus the surrogate `id` minus
  `workflow_excluded_fields` — exactly what `dto_create.tera` emits. Media
  children are `Option` + `#[serde(default)]`, so an omitted child is valid.
- **DSL values** are projected onto that surface (exact field name, then the
  prop's `resolve_field` projection), composite wrappers expand to their
  collapsed columns with type-valid samples, and any key the DTO does not
  declare is DROPPED.
- **References** are classified once (`classify_ref`): the DTO's own kind,
  then the name-convention shape (`vendor_id` with no `$ref`/edge). A
  reference is never a literal — the required-ref pass seeds the parent
  through its own create endpoint and wires `dep_N_id`; an optional reference
  is left null.
- **Required scalars** the DSL does not carry are sampled with the existing
  typed sampler (codelist first value, `machine.initial`, wrapped-datatype
  samples, uuid literals) — which is what seeds identity / natural-key parts.
- `None` (no schema / no owning domain / a DTO build error) falls back to the
  historical prop-driven pass, so an ifml-only run is unchanged.

The historical passes were also patched where they remain reachable (the
fallback): `with_required_entries` skips only the surrogate `id` (not every
`is_id`), and `plan_required_refs_inner` consults the shared convention
resolver.

### 2b. A gap is named, and its spec is omitted

A required reference whose target has no API surface / no create / a cycle /
no DTO surface, and a required scalar with no type-valid sample, are
`SeedGap { entity, field, reason }` values on the fixture. The generator:

- drops the spec that would open the unseeded record (click-through,
  round-trip, workflow, ux, machine, action; the auth seed returns `None` for
  the existing omission path; a journey with an unseedable seed is dropped),
- prints one `warning: ifml e2e: seed gap — <entity>: <field> (<reason>)` per
  unique gap, and
- writes them into `tests/ifml/omitted-specs.md` (which now also carries the
  pre-existing guarded-detail omissions).

The plane never emits a seed it knows will 4xx — the `#806` complaint about
"skip restored with the link" becomes a generated, self-describing record.

### 2c. `#809`: the click waits out hydration

- `ifml/skeleton.rs` emits an if-absent `src/routes/+layout.svelte` that adds
  `body.hydrated` on mount/afterNavigate — the same contract the entity app
  layout already had, so `BasePage.waitForHydration` can actually resolve in a
  standalone IFML app (it previously had no marker at all).
- The shell layout (`templates/ifml/svelte/layout.tera` + the shadcn pack
  override) carries the same marker, because it OVERWRITES the skeleton's
  root layout whenever a `shell` mapping resolves.
- The POM action method soft-waits for the marker before the click (10 s cap;
  a consumer-owned layout without it keeps working via the click's own
  actionability wait). A control replaced under the cursor is Playwright's
  "element was detached from the DOM, retrying" loop.
- The other half of the #809 picture is 2a: `seededId` is no longer
  `"undefined"`, so the page under test is the real record.

### 2d. The hurl contract gets the same convention FK

`api/hurl_contract::resolve_fk_parents` consulted only `$ref`/graph edges, so
a convention FK was scaffolded as a literal uuid (`fixture_pair`) — the same
FK violation one plane over. It now falls through to the shared
`fk_convention::resolve_convention_ref`, and `payload_fields` wires the parent
capture for any FK column before the kind dispatch (the `_` arm would
otherwise emit the scaffold literal).

## 3. Gates and teeth

| Gate | Kind | Teeth (mutation) |
|---|---|---|
| `ifml::e2e_test::body::tests` (9): identity/natural key, convention FK + parent seed, DSL FK placeholder replaced, unresolvable ref → named gap, optional ref null, composite expansion + unknown-key drop, camel projection, codelist projection, no-domain fallback signal | node-free | re-add the `is_id` skip → the identity pin fails; drop the convention arm → the FK pins fail; keep DSL ref literals → the placeholder pin fails; fall back to the snake spelling for unknown keys → the composite pin fails |
| `ifml_auth_setup_tests::seeded_auth_specs_carry_identity_members_and_convention_fks` (new) over the mox front door: a guarded id view over a composite-natural-key entity with a `String vendorId` FK | node-free end-to-end | pre-fix the body omits `entry_code`/`period` and posts `'Test vendorId'` |
| `tests::action_plan_renders_pom_action_methods` (extended): the action drive waits for `body.hydrated` before the click | node-free | drop the wait → the pin fails |
| `ifml::skeleton::tests::the_root_layout_signals_hydration` | node-free | drop the marker → the pin fails |
| `api::hurl_contract::tests::required_convention_fk_seeds_the_parent_and_carries_the_capture` (new): the capture is wired, no scaffold uuid, the entity's own `*Id` natural key is never its own FK | node-free | restore the edge-only resolver → the FK pin fails |
| `hurl_contract_tests` byte-golden | node-free | unchanged (`3 passed`) |
| nightly `ifml_codegen_gate` (T1/T3 action + auth categories, `GuardedRefundDetail`) | live | unchanged fixtures; the marker + wait are exercised by the guarded action leg |
| `uw_rules_pre_feature_ifml_tree` / `forms_pre_feature_ifml_tree` canaries | byte identity | re-blessed with an ENUMERATED diff: `+svelte/src/routes/+layout.svelte` (one added file each; verified by regenerating with the layout entry removed and diffing the trees) |

## 4. Verification log

- `cargo test -p codegraph-generate --lib` → **1437 passed, 0 failed**.
- `cargo test -p codegraph --test ifml_auth_setup_tests --test
  hurl_contract_tests --test ux_rules_byte_identity_tests
  --test forms_byte_identity_tests --test ifml_remote_functions_tests
  --test ifml_tanstack_forms_tests --test ifml_media_upload_tests
  --test ui_e2e_test_tests --test entity_remote_functions_tests
  --test template_harness --test ux_rules_tests --test
  pherolang_fixture_sweep` → **all green**.
- `cargo clippy -p codegraph-generate -p codegraph --all-targets -- -D
  warnings` and `cargo fmt --check` clean.
- Regeneration spot check: the `GuardedLedgerEntry` auth seed becomes
  `data: { 'entry_code': 'Test entryCode', 'period': 'Test period',
  'amount': 42 }` preceded by a `POST /api/v1/portal/vendor` whose id is
  wired as `'vendor_id': seeded_resp_dep_0_id`.

## 5. Deferred

- Cross-domain required refs are seeded with the parent's OWN domain route
  (already the hurl plane's rule); a cross-domain parent whose domain the
  child's config does not list still seeds correctly, but there is no
  dedicated pin.
- A required **child DTO** (VO/structure) is always `Option` in the create
  DTO, so it stays omitted; synthesizing nested members for a REQUIRED JSONB
  wrapper is the pre-existing `structured_wrapper_value` path only.
- `#820` events-producer payload naming (the nine near-miss renames) is the
  consumer's side and unaffected.
- Deterministic **non-prelude** hurl legs and the worker-topology action plane
  are unchanged.
