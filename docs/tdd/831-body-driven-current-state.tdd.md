# TDD — the body-driven selector resolves from the CURRENT state (issue #831)

Status: **LANDED** on `fix/831-body-driven-current-state` (PR pending).

## 0. The defect, verified

The consumer (`yestechgroup/crewbase` @ `106a0fb97`, `core/worker`) could not
activate a Pending worker through the published body-driven op:

```
POST /api/v1/core/worker/$WID/actions/change_status  {"status":"Active"}
→ 409 {"error":{"code":"CONFLICT","message":"operation `changeStatus` cannot
  move `status` from `Pending` to `Active`: no unambiguous transition from the
  current state (legal targets: Active, Terminated)"}}
```

`terminate` from the same row worked; only a target whose state has siblings
failed. The emitted kernel IR is `Pending -> Active via Activate`,
`Pending -> Terminated via Terminate` (one edge each), but `Active` also has
`Inactive -> Active via Reactivate` machine-wide.

The rendered arm (pre-#831) baked the machine's target universe and asked
whether the CURRENT state offered exactly one legal target:

```rust
let legal: Vec<&str> = ["Active","Terminated", …]
    .filter(|c| runtime.transition_for(&current_state, c).is_some())
    .collect();
match legal.as_slice() {
    [only] if *only == target => { … }        // ← requires ONE legal target
    [] => 409 "no transition from the current state",
    _  => 409 "no unambiguous transition from the current state (legal targets: …)",
}
```

`Pending` offers two targets (Active, Terminated), so `legal.len() == 2` and
the requested target — which was one of them — hit the ambiguity arm. The
message contradicted itself. Every state with two out-edges had the same
class, so the consumer's `activate_worker` action contract (200 then 403)
could not pass either (its 200 leg never moved the row).

### Why every gate stayed green

The payroll fixture's machine was LINEAR (`Draft -> Calculating -> Approved ->
Reversed`): every state has exactly one out-edge, so `legal.as_slice()` was
always `[only]` and the bug was unreachable. The in-crate KATs pinned the
rendered TEXT, not the decision, and the kernel had no API for "which edge
from HERE reaches THAT?".

## 1. The fix

- `codegraph-workflow`:
  `MachineRuntime::resolve_target(current, target) -> TargetResolution`
  restricts to the current state's out-edges FIRST, then matches the target:
  - `Unique { event, guard }` — one distinct event reaches it (conditional
    guard variants of that event collapse; first-match order decides).
  - `NoTransition { legal }` — the state's own distinct targets, declaration
    order (empty at a terminal state).
  - `Ambiguous { edges }` — ≥2 DISTINCT events to one target from the current
    state; the request named no trigger.
  `edges_from(current)` is now the ONE derivation behind `read_model`'s
  `available_transitions` AND `resolve_target`, pinned equal per state.
- `ddd/service_ops/render/update.rs::push_whitelist_arm` renders a `match
  runtime.resolve_target(..)`; no candidate list is baked (`ServiceOpState.all`
  is a planner-only fact; its content no longer affects the handler).
- Wire messages: `NoTransition` keeps `…: no transition from the current
  state` + ` (legal targets: …)` when non-empty; `Ambiguous` keeps the old
  `no unambiguous transition from the current state` prefix and appends
  `— competing edges: `A`[ (guarded by `g`)], `B``.
- The NAMED-transition arm is untouched (its candidate universe is the op's
  declared targets — correct for an op that names its edge).

## 2. Gates and teeth

| Gate | Kind | Teeth (mutation) |
|---|---|---|
| kernel KATs: fan-in is unique per source; unknown target lists the CURRENT state's legal targets; two distinct events are ambiguous and name both; guard variants of one event collapse; `edges_from` == the read model's edges | node-free | re-widen the filter to machine-wide targets → the fan-in KAT fails |
| `body_driven_selector_resolves_against_the_current_state`: the emitted `archive`/`declines` arms carry `resolve_target` and no `legal` list | node-free | restore the baked probe → fails |
| `handler_matrix.sha256`: exactly ONE permutation drifted (`root_service_ops`, +6 lines, the body-driven arm region) | node-free | any other drift fails |
| live `service_ops_hurl_gate` probe (second row): Draft → `{pay_run_status: Calculating}` **200** (pre-#831: 409) → `{pay_run_status: Draft}` **409** naming `Approved, Cancelled` → `{pay_run_status: Cancelled}` **200** (the fan-in target) → `{pay_run_status: Draft}` **409** naming `Reinstate`/`Reopen` | live | pre-fix the first leg answers 409 |

## 3. Verification log

- `cargo test -p codegraph-workflow --lib` → 12 passed (5 new kernel KATs).
- `cargo test -p codegraph-generate --lib` → green (99 service-ops KATs incl.
  the rewritten whitelist pins).
- `cargo test -p codegraph --test service_ops_tests` → **41 passed** (the new
  selector pin included); `--test handler_matrix_tests` re-blessed with the
  single-permutation diff above.
- `cargo test -p codegraph --test service_ops_hurl_gate -- --ignored` →
  **2 passed**: the emitted contract (11 requests) and the hand-authored
  probe (22 requests) over real HTTP; psql proofs print
  `probe row state = Approved` and `body-driven row state = Cancelled`.
- `cargo test -p codegraph --test service_ops_compile_gate -- --ignored` →
  **8 passed** — the emitted arm compiles in the generated app (the #551
  policy). Its first run caught the missing crate-root re-export of
  `TargetResolution`/`MachineTransitionEdge` (`E0433` in the generated
  app) — the compile pin doing its job.
- **Teeth (live)**: with the pre-fix semantics mutated back into
  `resolve_target` (ambiguity decided over the state's legal targets), the
  same gate fails on the first #831 leg with exactly the consumer's
  message — `409 "… from `Draft` to `Calculating`: no unambiguous transition
  from the current state — competing edges: `Submit`, `Abandon`"`; restored
  and re-green.

## 4. Consumer acceptance

crewbase deletes the `core: terminate sets Terminated; changeStatus is
body-driven` skip entry and restores the realistic Active walk in its events
journey; the `action activate_worker on worker` contract becomes reachable
(its seed was fixed by #836/PR for #835).
