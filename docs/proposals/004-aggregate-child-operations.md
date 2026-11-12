# Proposal: aggregate/child operation semantics

Status: draft (codegraph lifecycle hardening phase 2, Nov 2026)
Target: rexlang (rex-driver ddd designs + Rule 6; rex-ir ServiceOperation)

## Problem

Aggregate children have no first-class operations. Today:

- A **contained** child (Rule 6) may not declare a repository, so it has no
  save/delete builtins and no service op can bind it — mutation code lives
  outside the model entirely.
- A **refers-linked** child with its own repository CAN bind service ops, and
  codegraph now routes them nested (`POST .../{parent}/{parent_id}/{child}/
  {child_id}/actions/{op}`, scoped loads), but the model still expresses them
  as plain repository delegations — the aggregate relationship is implicit in
  entity config, not in the design.
- The consumer shapes that motivated this (Timecard `punch`/`ingestHours`,
  assignment/deployment operations, `addItem`/`removeItem`) are exactly the
  ones with no lowering.

## Proposed syntax

Child-scoped operations on the ROOT design, with an explicit child ref and
typed params:

```
entity PayRun scaffold repository PayRunRepository { findById; findAll; save; delete; }

// aggregate-internal mutation is declared ON THE ROOT, naming the child ref
extend PayRun {
    op addLine(line: PayRunLineDraft)  => lines.add;
    op removeLine(lineId: Uuid)        => lines.remove;
    op recalculateLine(lineId: Uuid)   => lines.save when (line.status == Draft);
}
```

(Alternatively — and equivalently for lowering — the same operations arrive
via proposal 003's body IR with `lines.add(...)` navigation; this proposal is
the declarative shortcut.)

## Proposed IR

```rust
pub struct ServiceOperation {
    // ...
    pub child_scope: Option<ChildRef>,   // NEW
}
#[serde(rename_all = "camelCase")]
pub struct ChildRef {
    pub feature: String,        // the root's child feature name ("lines")
    pub class: String,          // resolved child class
}
```

Compiler checks:

- `feature` must be a containment/refers feature of the op's bound root;
- `add`/`remove` require multiplicity `[]` (collection); `save` on a scalar
  child is allowed (replace);
- Rule 6 stays: contained children get NO standalone repository — their ops
  ride the root's repository/commands (this proposal is the sanctioned shape
  upstream already points to: "behavior confined to one aggregate belongs on
  the class itself");
- the child's design (if any) supplies param/field typing.

## Acceptance

- Conformance fixtures: add/remove/save child ops, `when` on child fields,
  Rule 6 refusal for a standalone contained-child repository, cross-aggregate
  refusal (Rule 13 unchanged).
- codegraph: nested child routes (already shipped for refers-children in the
  lifecycle-hardening work) gain the add/remove command lowerings; contained
  children stop being 501-only — their mutation lowers through the root
  command plane.

## Consumer impact

- Wire: nested child action routes as codegraph already emits them — no new
  URL vocabulary (`.../{parent}/{parent_id}/{child}/{child_id}/actions/
  {op}`; `add`/`remove` lower onto the child collection endpoints).
- Timecard punch/ingestHours, assignment/deployment flows become declarative;
  the current 501 seams for "aggregate-level orchestration over children"
  retire.
- Serde-defaulted IR field — existing artifacts parse unchanged.
