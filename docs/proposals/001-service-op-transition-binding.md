# Proposal: explicit service-op transition binding

Status: draft (codegraph lifecycle hardening phase 2, Nov 2026)
Target: rexlang (rex-driver ddd compiler + rex-ir)

## Problem

Transition semantics on service operations are inferred, not declared. Today a
`save` delegation whose entity carries a state machine is lowered by codegraph
as a machine-bound transition using per-state heuristics (patch the state
field from the current state to the transition target). Two failure classes
motivate an explicit binding:

1. **Capability-sharing heuristics.** Real consumer models express
   "preparePayRun advances Draft → Calculating" as a capability grant plus a
   save delegation; which trigger fires is guessed from state names. When the
   guess is wrong, the wrong transition (or none) lowers — silently.
2. **Multi-machine modules.** A module may carry several machines (state names
   must merely be unique per module). Inference cannot say which machine an op
   advances; the compiler currently restricts op-level `when` to
   machine-bearing modules, and transitions have no module-level disambiguation
   at all.

## Proposed syntax

```
entity PayRun scaffold repository PayRunRepository { findById; findAll; save; delete; }

service PayRunService {
    inject PayRunRepository;
    // explicit trigger binding — the trigger name comes from the machine
    // declared on the bound entity's design (or the module's single machine)
    submit  => PayRunRepository.save transition Submit;
    approve => PayRunRepository.save transition Approve;
    // multi-machine modules name the machine explicitly
    reverse => PayRunRepository.save transition PayRunStatus.Reverse;
}
```

## Proposed IR

```rust
// rex-ir ddd.rs (serde-defaulted — wire back-compat)
pub struct ServiceOperation {
    // ... existing fields ...
    pub transition: Option<TransitionRef>,   // NEW
}

#[serde(rename_all = "camelCase")]
pub struct TransitionRef {
    pub trigger: String,
    pub machine: Option<String>,   // required when the module has >1 machine
}
```

Compiler checks (rex-driver ddd graph/integrity):

- the trigger must exist on the machine bound to the op's entity (else compile
  error naming trigger + machine);
- `transition` on an op whose delegation is not a repository `save`/`update`
  is a compile error;
- multi-machine modules MUST name `machine` (compile error otherwise) — this
  also relaxes today's single-machine restriction on op-level `when` for the
  transition path.

## Acceptance

- Conformance fixture: explicit binding, unknown trigger (error), non-save
  delegation (error), multi-machine without `machine` (error), multi-machine
  with `machine` (ok).
- codegraph lowers the declared trigger verbatim (state patch = trigger's
  target state; wrong-state = 409 via the existing machine CONFLICT guard) and
  its hurl/openapi contracts pin the declared transition; the inference
  heuristic becomes a transitional WARN ("op advances machine via inference;
  declare `transition <Trigger>` to pin it").

## Consumer impact

- Wire format: `#[serde(default)]` — existing artifacts parse unchanged.
- Existing models keep today's inferred behavior plus a WARN; migration is
  mechanical (add `transition <Trigger>` per op). hr-specs/crewbase-style
  `preparePayRun` flows become one-line changes.
- codegraph-side: retires the preparePayRun capability heuristic; the
  `machine_json` graph mirror gains the declared trigger (one more
  `#[serde(default)]` field, no format bump beyond the usual cold-re-ingest).
