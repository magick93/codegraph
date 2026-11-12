# Proposal: multi-machine and cross-entity `when` scoping

Status: draft (codegraph lifecycle hardening phase 2, Nov 2026)
Target: rexlang (rex-driver ddd machines.rs + integrity; rex-ir condition)

## Problem

`when` guards are machine-bound and single-entity today:

- An op-level `when` is admitted only on machine-bearing modules, and its
  fields must belong to the op's own record — codegraph must refuse
  cross-entity guards (`when Timecard.status == Approved`) as generation
  errors, so separation-of-duties-style guards ("a PayRun can only submit
  when its Timecard is Approved") cannot be modeled at all.
- Multiple machines per module are tolerated (unique state names), but an op
  advancing one of several machines has no way to say which (see proposal
  001 for the transition half; this proposal covers the guard half).

## Proposed syntax + scoping rules

```
machine payRunMachine: PayRunStatus { Draft -> Calculating via Submit; ... }
machine timecardMachine: TimecardStatus { Pending -> Approved via Approve; ... }

service PayRunService {
    inject PayRunRepository;
    inject TimecardRepository;

    submit => PayRunRepository.save transition Submit
        when (payRunStatus == Draft)
             && timecard.status == Approved;   // qualified: injected-repo scope
}
```

- **Own-record fields** stay unqualified (resolved on the bound entity).
- **Qualified paths** (`timecard.status`) resolve against the module's
  injected repositories: the first path segment must name an injection whose
  design binds the named class; the remainder must be fields of that class.
- Multi-machine modules: unqualified state-typed fields must be UNIQUE across
  the module's machines (existing rule); with several machines carrying the
  same state-typed field name, the field must be qualified by machine
  (`payRunMachine.payRunStatus`).
- Compile checks: path exists, types align, codelist literals are members.

## Proposed IR

`ServiceOperation.condition` already exists (string source). Add the resolved
form so consumers don't re-parse:

```rust
pub struct Condition {
    pub source: String,
    pub paths: Vec<ConditionPath>,   // NEW: resolved field paths
}
pub struct ConditionPath {
    pub scope: ConditionScope,       // OwnRecord | Injected { repository, class }
    pub field: String,
    pub field_type: TypeRef,
}
```

## Acceptance

- Conformance fixtures: own-record guard (today's shape, unchanged), single
  qualified injection guard, unknown injection (error), unknown field (error),
  type mismatch (error), multi-machine unqualified ambiguity (error), the
  qualified-machine form (ok).
- codegraph lowers qualified guards: injected-scope reads via the injected
  repository's read op on the handler's already-loaded context (parent/child
  ids on the route) — closed subset, refusal names the construct; RLS scopes
  still apply at the data plane.

## Consumer impact

- SoD-style cross-entity guards become model-expressible — the top ask behind
  the crewbase PayRun/Timecard flow.
- `condition` string stays (wire back-compat); `paths` is serde-defaulted so
  old artifacts parse.
- codegraph's `lower_when_to_rust` gains the injected-scope arm; today it
  hard-errors on any non-own-record field — that refusal becomes a lowering
  for the resolved subset only.
