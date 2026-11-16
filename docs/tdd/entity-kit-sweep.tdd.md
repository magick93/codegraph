# TDD evidence report — Entity plane kit-3 sweep + a gate that can fail (WS-E, codegraph#634)

**Branch:** `ws/634-kit-sweep` (off `3e8a7241`, master HEAD after the #633 media epic)
**Runner:** `RUSTFLAGS="-C debuginfo=0" cargo test -p <crate>`; `just ci-local` for the
matrix. Node gates (`entity_codegen_gate`) run with `--test-threads=1` — both tests
regenerate the same review fixture in place.
**Scope:** issue #634 — port the ws6 (IFML plane) kit-2.x defect class to the entity
plane (kit `3.0.0-next.24`), and repair the entity gate that was supposed to catch it.

## The headline

The entity gate could not fail. svelte-check prints `path:line:col` on the line
**before** `Error:`, and the gate filtered lines containing `"Error:"` against a path
prefix — so the `generated` set was always empty, the check's exit status was discarded
(`let _ = check_ok;`), the allowlist omitted the entire `src/lib/forms/**` TanStack
surface, and the gate was wired into no workflow at all ("nightly lane" was
documentation).

With the gate repaired and asserting for real, the **unfixed** tree reports:

```text
svelte-check reported 190 error(s) on GENERATED files:   ← pre-fix, unlisted
gate: 9 NON-GENERATED (consumer-authored/gate-owned) error(s) — recorded, not asserted
```

After this sweep: **1** documented pre-existing generated-plane defect (the
out-of-scope shared server helper, gap G1 below), **0** undocumented. Diagnostic counts
are reconciled against svelte-check's own summary line and exit status on every run, so
the assertion cannot silently go vacuous again.

## RED evidence (test commit `4054342d`)

`entity_remote_functions_tests` over a fixture that now carries a read-only entity
(`ReadOnlyRequest`: operations without `delete`), fixes reverted:

```text
test result: FAILED. 1 passed; 5 failed
  entity_remote_functions_flag_emits_the_server_side_plane                     FAILED
  every_remote_import_and_refresh_target_resolves_against_its_module          FAILED
  remote_fail_legs_read_the_error_message_not_the_object                      FAILED
  generated_pages_declare_the_props_they_read                                 FAILED
  generated_forms_do_not_depend_on_the_consumer_owned_form_registration       FAILED

src/routes/(app)/portal/read-only-request/+page.svelte imports `deleteItem` — not
exported by src/routes/(app)/portal/read-only-request/data.remote.ts
(exports: ["getList"])
```

That last line is the issue's class 1 reproduced exactly (an entity with
list/read/update and no delete): the page imported an export the colocated module never
emits, which is a Rollup hard-fail (`MISSING_EXPORT`).

## Fixes (GREEN)

| # | Class (issue #634) | Defect | Fix | Verified by |
|---|---|---|---|---|
| 1 | imported-but-unemitted remote export | `list_page.tera` imported `deleteItem` whenever the ux plane was on; `ui/remote.rs` emits it only for `ux_delete && has_delete` | import gate follows the emission rule (`ux_actions and has_delete`); `invalidateAll`, `AlertDialog`, `DropdownMenu` follow their real use sites; `detail_page_remote.tera` gates `AlertDialog` on `has_delete` | parity pin + unit pin + gate |
| 2 | kit-3 remote arity | `query.live(async function* (id) …)` — kit's overloads are `live(fn: (arg: void) => …)` / `live("unchecked", fn(arg: Input) => …)` → TS2345. Same class, wider: zero-arg queries used `query('unchecked', fn)` (callable requires an argument → `getEndpoints()` = TS2554) and multi-arg commands (`toggleActive(id, active)`, `deleteSubscription(a, b)`, `updateEndpoint(id, body)`, `delete{X}Child(childId, parentId)`) hit "No overload matches this call" — kit 3 remote functions take ONE input | `query.live('unchecked', …)`; zero-arg `query(fn)`; multi-arg commands take an object input; `updateEndpoint` strips `id` from the JSON body (the update DTO is `deny_unknown_fields`) | string pins updated + gate |
| 3 | refresh of a non-emitted query | the detail module's `transitionItem` refreshes `getItem`, which was gated on `has_read` alone | emit `getItem` whenever something refreshes it (`has_read \|\| has_workflow`) | parity pin (`.refresh()` targets ⊆ module exports) |
| 4 | undeclared prop | `settings_webhook_form_remote.tera` (emitted to `new/` and `[id]/edit/`) read `data` with no `$props()` | `interface Props { data?: { endpoint?: WebhookEndpoint } }` + destructure | pin + gate (a runes-mode contract break, runtime `ReferenceError`) |
| 5 | fail legs read the error OBJECT | `COMMAND_FAIL_LEG` and the three webhook remote modules read `body?.error ?? …`; the API body is `{ error: { code, message, details? } }` → `[object Object]` in toasts | all six sites read `error?.message` | pin (`remote_fail_legs_read_the_error_message_not_the_object`) + unit pin |
| 6 | built-but-never-registered component | `lib/forms/form.ts` is emitted IF-ABSENT (consumer extension point): a tree scaffolding before #601 keeps a creator without `ResetButton`, so `<form.ResetButton>` was an undefined component | the generated form renders the reset affordance INLINE against its own form instance (same markup/testid, `form.reset()`); the registration stays for consumer code. Separately, `child_section_component_remote.tera` existed but was NEVER SELECTED (`page.rs` always emitted the non-remote component) while the remote page passes `onDelete` → every remote details page with child sections was a type error; the template is now selected under the flag | pin (no `form.ResetButton`; `ChildSection` declares `onDelete`; page wires it) + gate |

### Defects the repaired gate surfaced and this sweep fixed

Fixing the gate immediately exposed 190 generated-path errors the vacuous gate had
hidden. Beyond the six classes above, these were mechanically in the same template
family and are fixed here (each enumerated in the PR):

| Defect | Count | Fix |
|---|---|---|
| `required=false` / `hasChildren=false` / `openEnd=false` / `isArray=false` — unquoted attribute interpolations are STRINGS in Svelte markup, not boolean expressions (every boolean form prop was truthy, so **every field rendered required**) | 86 + 2 | `required={…}` braced emission in `entity_form_tanstack.tera` / `detail_page_remote.tera` |
| TanStack structured field wired `onvaluechange` while `StructuredWrapperField` declares `onchange` (plus the array updater mismatch) | 5 | `onchange` + union-typed handler |
| Details pages called `fmt.format{SchemaTitle}(…)` while `format.ts` exported `format{CandidateScopedName}` (runtime `undefined is not a function` on every details page with a structured field), and imported the doubled `…ResponseResponse` type | 6 | one identity — the SCHEMA TITLE — for both the export and the call (`ui/types.rs` + `format.ts.tera`); structural parameter type, no scoped alias import |
| `$app/environment` under kit 3 (moved to `$app/env`) | 2 | `$app/env` in the entity live-plane templates |
| `import type { Reroute } from '@sveltejs/kit'` (kit 3 exports it from `@sveltejs/kit/hooks`); `event.request = …` (readonly in kit 3) | 2 | `@sveltejs/kit/hooks`; `resolve({ …event, request: localizedRequest })` |
| `m.common_actions()` used unconditionally by the child-section menu while the message key was emitted only under `has_ux_rules` | 1 | the two generic keys are always inventoried |
| The array-of-VO cast named a scoped type the page never imports (`QualificationTypeResponse`) | 1 | structural cast (`Array<Record<string, unknown>>`) |

### Gate-side fidelity (the gate's own defects, fixed not skipped)

| Gate defect | Effect | Fix |
|---|---|---|
| `@crewbase/entities` / `@crewbase/ui` stubs exported components as plain functions | 55+5 false diagnostics ("not assignable to Component", "not constructable") | `Component`-typed runtime values (a cast keeps both the type and the runtime export) |
| `stub_api_types` scanned only `src/` with one specifier | every emitted Playwright suite's request-DTO import unresolved (20 diagnostics) | walk the whole ui tree, both `#lib/api/types.ts` and `./api/types.ts` specifiers |
| The report parser only looked at `Error:` lines | the assertion could never fail | `path:line:col` + severity block parsing, reconciled against the summary line and the exit status; unattributed diagnostics are a hard failure |

## Test specification

| # | What is guaranteed | Test | Result |
|---|---|---|---|
| 1 | Page/load `./data.remote` named imports ⊆ the sibling module's exports | `entity_remote_functions_tests::every_remote_import_and_refresh_target_resolves_against_its_module` | PASS |
| 2 | Every `.refresh()` target is a module export | same | PASS |
| 3 | No emitted fail leg reads the error object | `entity_remote_functions_tests::remote_fail_legs_read_the_error_message_not_the_object` + `ui::remote` unit pin | PASS |
| 4 | The webhook form declares the `data` prop it reads | `entity_remote_functions_tests::generated_pages_declare_the_props_they_read` + gate | PASS |
| 5 | The generated form does not depend on the consumer-owned creator registration | `entity_remote_functions_tests::generated_forms_do_not_depend_on_the_consumer_owned_form_registration` | PASS |
| 6 | The remote child-section component declares + receives `onDelete` | `ui::page::…::remote_child_section_component_declares_the_ondelete_callback` | PASS |
| 7 | Kit-3 arity: `query.live('unchecked', …)`, zero-arg queries, single-object commands | `ui::remote` unit pins + `entity_remote_functions_tests` string pins | PASS |
| 8 | The gate's parser attributes the real print shape and refuses unattributed diagnostics | `entity_codegen_gate::{parser_…,generated_paths_are_fail_closed_…}` (node-free, PR CI) | PASS (4) |
| 9 | Generated-path attribution is fail-closed (new directories are asserted) | same | PASS |
| 10 | Zero undocumented generated-path diagnostics, reconciled with svelte-check | `entity_codegen_gate::entity_app_builds_and_typechecks` (nightly) | PASS (1 documented) |
| 11 | The emitted playwright config covers every emitted test file, and the sessionless `auth` project runs green against the booted app | `entity_codegen_gate::entity_specs_run_green` (nightly) | PASS — coverage `154 files, 8 projects`; `auth` project 3 passed / 0 failed (162s) |

## Gate commands + counts

```bash
# node-free PR-CI pins
cargo test -p codegraph --test entity_remote_functions_tests        # 6
cargo test -p codegraph --test entity_codegen_gate                  # 4 (parser + attribution)
cargo test -p codegraph --test entity_form_tests                    # 3
cargo test -p codegraph-generate --lib -- ui::remote ui::page       # 7 + 18
cargo test -p codegraph --test ux_rules_byte_identity_tests         # flag-off canary
cargo test -p codegraph --test forms_byte_identity_tests            # flag-off canary

# nightly (node + postgres + chromium)
cargo test -p codegraph --test entity_codegen_gate -- \
  --ignored --nocapture --test-threads=1
```

Gate summary on this tree:

```text
entity_app_builds_and_typechecks: PASS (36s)
  svelte-check: 1 documented pre-existing generated-plane defect (G1)
                0 undocumented generated-path errors
                9 consumer-authored/gate-owned errors recorded, not asserted
entity_specs_run_green: PASS (163s)
  coverage: 154 emitted test file(s), all matched by a project (8 projects)
  auth project: passed 3, failed 0
```

(Measured with `DATABASE_URL` on the gate's supabase-postgres image; the
default `postgres:15` service in CI would fail the basejump migration's
`auth.users` reference — that is why the workflow uses the supabase image.)

Wired as `.github/workflows/entity-gate.yml` (workflow_dispatch + `0 4 * * *`,
supabase postgres service, chromium install, `--test-threads=1`), mirroring
`ifml-gate.yml`'s no-secret shape.

## Byte identity

Default-profile (flags ON) output changes — the enumerated delta classes are in the
commit message (list-page import gates, detail `AlertDialog` gate, fail legs,
`query.live` arity, `getItem` emission, inline reset button, webhook form `$props()`,
fail-leg messages, `$app/env`, hooks types, `format.ts` identity, child-section
variant selection, i18n keys, braced booleans).

Flag-OFF output changes in exactly FOUR files (measured empirically: the canary
pipeline was run with the fixes stashed and then applied, and the two trees diffed):

| File | Delta | Why it cannot stay off |
|---|---|---|
| `ui/messages/en.json` | `+ common_open`, `+ common_actions` | the child-section menu used these keys under `has_ux_rules = false` — a missing-key build break |
| `ui/src/hooks.server.ts` | `resolve({ …event, request })` instead of mutating | kit 3 makes `RequestEvent.request` readonly |
| `ui/src/hooks.ts` | `Reroute` imported from `@sveltejs/kit/hooks` | kit 3 no longer exports it from the package root |
| `ui/src/lib/format.ts` | formatter identity = the schema title; no scoped-alias import | the details page's `fmt.format{Title}(…)` call |

The three canaries (`ux_rules_byte_identity_tests`, `forms_byte_identity_tests`,
`policy_rls_tests::flag_off_full_output_matches_master_baseline`) are therefore
re-blessed in this PR with exactly that enumerated diff (the `policy_rls` snapshot's
insta diff shows precisely those four rows; `api/types.ts` is unchanged — the nested
interface keeps its scoped spelling via `UiNestedType.ts_name`).

The checked-in review fixture (`crates/review/generated-candidate/`) is a gate artifact:
`entity_codegen_gate`, `review_api_suite` and `grafeo_candidate_inspect_output` all
regenerate it in place, and it currently lags master by several PRs. This PR does not
commit the catch-up regen (~1,278 paths of unrelated churn); the nightly gate
regenerates it in its own checkout.

## Gap table (documented, NOT fixed here)

| # | Gap | Diagnostics | Why out of scope | Where tracked |
|---|---|---|---|---|
| G1 | `src/lib/server/api.ts` imports `env` from `$env/dynamic/private` — kit 3 renamed the env modules to `$app/env/{public,private}`. The source lives in the SHARED `crates/codegraph-generate/src/server_api.rs` (the IFML plane, still on kit 2, emits the identical helper — a version-aware emit is a two-plane decision) | 1 | file outside this workstream's scope (`src/server_api.rs`) | gate `KNOWN_DEFECTS` entry (stale-entries fail) + this table |
| G2 | The review fixture's ux plane is OFF although the shipped default profile sets `ux_rules = true`: `review_api_suite_regen` threads the codegen flags but not the ux plan, so the entity gate never exercises ux-ON output (the ux canaries and the IFML gate cover it) | — (coverage gap, not a diagnostic) | regenerating the fixture with ux ON would rewrite every page (unrelated 1,000+ file churn) | this table; follow-up: thread `ux_rules` into the regen recipe |
| G3 | Twin defects in the FLAG-OFF templates that the gate does not exercise: `detail_page.tera` carries the same `hasChildren=false` string-attribute bug and the same scoped-name array cast as its remote twin; `child_section_component.tera`'s `onRefresh: () => Promise<void>` vs the flag-off page's void `loadChildren` | — (not gate-covered) | fixing them changes flag-OFF bytes and requires re-blessing the ux canary; the flag-off variants are outside this issue's (kit-3 remote plane) scope | this table; a follow-up can fix + re-bless in one enumerated diff |
| G4 | `entity_specs_run_green`'s UI-driven families (`generated/**`, `webhooks`, journeys) need the Supabase persona stack and stay the consumer `testkit e2e` lane (pre-existing contract, unchanged) | — | pre-existing | AGENTS.md |

Nothing above is silently skipped: G1 is a hard ledger entry (the gate fails if it is
ever fixed and the entry not deleted), G2–G4 are coverage gaps recorded here.
