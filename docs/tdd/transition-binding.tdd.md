# TDD evidence report — `.ddd` explicit transition binding, WS-C (codegraph#660)

**Branch:** `ws/660-transition-binding` (off `master` @ b795f3b4, DD9 #659 +
media e2e #633 + statement bodies #662 merged; one PR closing #660)
**Runner:** `cargo test -p <crate>` per crate; workspace gates with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI); the live gates run under
`just ci-local ops` (DATABASE_URL → Postgres, hurl present).
**Upstream:** pherolang `298267e` — `phero_ir::ddd::ServiceOperation.transition`
(`TransitionRef { trigger, machine }`, camelCase, serde-defaulted). The
driver validates the binding (E0350 unknown trigger / E0351 not-a-`save`
delegation / E0352 qualifier required in a multi-machine module) against the
machine bound to the delegation target's designed class.

## Issue coverage

| Deliverable | Landed |
|---|---|
| 1. Mirror `transition` + format bump | `DddServiceOperation.transition: Option<DddTransitionRef>` (`codegraph-core/src/types/ddd.rs`, serde-defaulted + `skip_serializing_if`, so a binding-less op serializes exactly as before) + `ddd_ingest` mapping + **`GRAPH_FORMAT_VERSION` 7 → 8** + artifact kit `graph_document_v7.md` → `v8` (a warm cache would otherwise deserialize the binding as `None` and silently serve the heuristics) |
| 2. Primary selector, heuristics as the fallback + one WARN per op | `ddd/service_ops.rs::resolved_transitions` — the binding resolves against the machine bound to the entity (explicit `via` name or a bare arrow's derived lowerCamel; optional enum-type qualifier); `heuristic_transitions` stays as the no-binding fallback only, and `transition_binding_warnings` reports one WARN per op it resolves (run path: `ValidationPass::check_transition_bindings`, `service_op_transition_binding`, Warning) |
| 3. Multi-machine qualifier + hard errors | `resolve_transition_binding` resolves through `TransitionRef.machine`; an unqualified binding in a multi-machine module, a qualifier naming another machine, an unknown trigger (the error lists the declared vocabulary), and a binding on a machine-less entity are HARD errors naming the op and both machines. `transition_binding_errors` → `ValidationPass` **Error** pre-generation (the DD9 `query_refusals` / #662 `check_service_op_bodies` posture) |
| 4. hurl contracts follow the authored binding | `state_aware_legs` already simulates off `ExecutionKind::transition_state()`; the authored pairs flow through unchanged. `templates/hurl/actions.tera` needed NO change — the declaration-order walk, the guaranteed 409 replay and the 501 seams are byte-identical machinery |
| 5. Heuristic deletions | nothing became unreachable: the binding is PER OP, so an unbound machine-touching op in a bound model (the gate fixture's `escalate`) still needs the fallback. All three heuristics survive as the documented no-binding fallback (module doc + AGENTS.md) |
| 6. Fixtures + pins | NEW mox-first fixture (two machine-bearing designs in ONE module, three bound ops whose names resemble no trigger) + mirror/route/handler/hurl pins; PAYROLL_DDD stays unbound (fallback + WARN pinned); `ifml_gate/refunds.ddd`'s `approve` gains `transition Escalate` (primary path in the nightly gate) |
| 7. Docs | this report + the AGENTS.md "Service ops lower to API surface (issue #590)" extension |

## The resolution contract (documented at the site)

| Authored binding | Resolution |
|---|---|
| `transition Submit` (single-machine module) | the machine's transitions carrying the explicit `via Submit` |
| `transition posted` (bare arrow `-> Posted`) | the transitions whose target state's lowerCamel is `posted` |
| `transition PayRunStatus.Reverse` | the same, after the qualifier is checked against the bound machine's **enum type** |
| trigger unknown | HARD error naming the op, the machine (`field: EnumType`) and the declared trigger vocabulary |
| trigger both explicit and derived | HARD error (ambiguous) |
| unqualified in a multi-machine module | HARD error naming every machine-bearing design of the module |
| qualifier naming another machine | HARD error naming the bound machine + the module's machines |
| binding on an entity with no machine | HARD error — never a plain update |

One trigger declared from several states yields one pair per `(from, to)`;
the runtime switch still selects by the record's current state, so a
multi-source trigger cannot silently pick the wrong edge.

## Task report (RED → GREEN)

| Task | RED evidence | GREEN evidence | What the passing tests guarantee |
|---|---|---|---|
| T1 graph mirror + ingest + format bump | the mirror binding was absent (`transition` did not exist on `DddServiceOperation`) | `service_ops_tests` mirror pin + `artifact_kit_tests ok. 2` | trigger + enum-type qualifier survive ingest → graph → readback; a delegation without a binding mirrors `None`; the artifact format is v8 and a v9 document is refused as `format_version_too_new` |
| T2 primary selector | `authored_transition_binding_selects_the_declared_transition` FAILED (the binding was ignored: the op lowered as a plain update, no transition arm, no state patch) | `service_ops_tests ok. 13` | `prepareRelease` → exactly `Held -> SignedOff`, `finalize` → `SignedOff -> Released`, `prepareReview` → `Pending -> Cleared`; no inferred arm leaks |
| T3 multi-machine qualifier | in-crate unit pins written with the impl | `cargo test -p codegraph-generate --lib -- ddd::service_ops ok. 35` | qualified resolution picks the bound machine; unqualified in a two-machine module names BOTH (`PayRun` + `PayRunCheck`, statuses included); a foreign qualifier is refused |
| T4 fallback + WARN | `unbound_machine_ops_warn_once_for_the_missing_binding` FAILED (no such check existed) | `service_ops_tests` + `ValidationPass` | exactly one WARN per heuristic-resolved op (submit/approve/reverse on PAYROLL_DDD) naming the missing binding; ops applying no transition stay quiet; a fully bound model is silent |
| T5 hard errors pre-generation | `unresolvable_authored_bindings_fail_validation_naming_the_machines` FAILED | same | a stale artifact with the qualifier dropped is a `ValidationPass` ERROR naming the op and both machines — before any tree is written |
| T6 hurl legs from the binding | the emitted contract pinned plain 200s with no state movement | `service_ops_tests` hurl asserts | create → `prepare_release` 200 → `finalize` 200 → `inspect` 200 → `summarize` 501 → replay `finalize` 409; the review machine mirrors it; the replay guarantee and the seam stay |
| T7 compile pin (#551) | — | `service_ops_compile_gate` (2 ignored tests) | the binding plane's emitted arms compile against the generated Update DTOs (`"Held" => Some("SignedOff")`, `"Pending" => Some("Cleared")`) |
| T8 gate fixture (nightly) | — | `ifml_codegen_gate` action category | `refunds.ddd`'s `approve` now declares `transition Escalate`; the regenerated tree is BYTE-IDENTICAL to the unbound fixture's (`diff -r` empty — the authored pair equals the inferred one), so the 200-then-403 leg plan holds |
| T9 byte-identity | — | workspace suite | no binding ⇒ the serialized `operations_json` is unchanged (a field-level `skip_serializing_if`), no template changed, and the review fixture/canaries see no diff |

## Final gate sweep

- `cargo test --workspace --no-fail-fast` (RUSTFLAGS `-C debuginfo=0`):
  **3347 passed, 0 failed, 42 ignored** across 192 test targets.
- `cargo test -p codegraph --test service_ops_tests` — 13 passed (3 new).
- `cargo test -p codegraph-generate --lib -- ddd::service_ops` — 35 passed
  (9 new resolver KATs).
- `cargo test -p codegraph --test ddd_ingest_tests` — 7.
- `cargo test -p codegraph --test artifact_kit_tests` — 2.
- `cargo test -p codegraph --test ddd_pipeline_tests --test
  ddd_golden_tests` — 3 + 2.
- `cargo test -p codegraph --test hurl_contract_tests` — 2.
- `cargo test -p codegraph --test pherolang_fixture_sweep` — 1 (the binding
  fixture and `refunds.ddd` compile under the pin).
- The byte-identity canaries (`ux_rules_byte_identity_tests`,
  `forms_byte_identity_tests`, `policy_rls_snapshot_tests`) are inside the
  workspace run: **no diff** — no template changed and the binding is
  serialized only when present.
- `cargo check --workspace`, `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test -p codegraph --test service_ops_compile_gate -- --ignored
  --nocapture` — **ran**, 2 tests (the payroll plane and the new
  authored-binding plane), `cargo check` clean.
- `cargo test -p codegraph --test service_ops_hurl_gate -- --ignored
  --nocapture` — **ran** over real Postgres + hurl: green (the unbound
  payroll fixture — the binding plane's live leg is the recorded deferral).
- `cargo test -p codegraph --test ifml_codegen_gate -- --ignored
  --nocapture` — **ran** to enumerate the fixture diff for the
  `refunds.ddd` binding: `generated_app_typechecks` and
  `generated_app_builds` PASSED; `gate_full_stack_boots` /
  `specs_pass_against_real_api` / `regen_after_view_removal_stays_green`
  are blocked by a **pre-existing** compile error in the media plane's
  emitted handler (`api/refunds/media_refund_request_receipt.rs`:
  `DatabaseConnection::execute`/`query_one` without
  `sea_orm::ConnectionTrait` in scope — `templates/api/media_route.tera`
  imports `{DatabaseBackend, Statement}` only; identical at `b795f3b4`,
  and this branch touches no template). NOT fixed here: the media template
  is out of WS-C's scope; recorded as a finding for the #633 owner.
  Regeneration itself succeeded, so the diff was taken directly: the
  generated tree from the BOUND fixture and from the same fixture with the
  binding removed is **byte-identical** (`diff -r` empty) — the authored
  binding resolves the same `none -> escalated` pair the `when`
  source-state heuristic inferred.
