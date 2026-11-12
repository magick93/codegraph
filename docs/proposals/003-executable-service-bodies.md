# Proposal: executable service/repository bodies (normative statement IR)

Status: draft (codegraph lifecycle hardening phase 2, Nov 2026)
Target: rexlang (rex-expr body module, rex-driver ddd lowering, rex-ir)

## Problem

Service and repository operations without a matching repository builtin
delegation are documentation-only: codegraph must emit a `501 NOT_IMPLEMENTED`
seam naming the gap. The seam classes today are exactly the ones consumers
need first: `save` payloads (create-vs-update decisioning), service-to-service
delegation, search, and child mutations. rex-expr already carries a Rune
statement IR (`RuneBody`, separate from `ExprKind`) from rexlang #114's work,
but there is no normative, typed statement subset for service/repository
operations — so every consumer invents hand-written Rust behind a trait, and
the model stops being the source of truth for behavior.

## Proposed syntax

```
service PayRunService {
    inject PayRunRepository;
    inject TimecardRepository;

    op submit(payRunId: Uuid) => PayRunRepository {
        let run = self.findById(payRunId) else error NotFound("PayRun", payRunId);
        guard run.status == Draft else error Conflict("already submitted");
        let hours = timecards.sumApproved(run.employee);
        self.save(run { status: Calculating, calculatedHours: hours });
        return run;
    }
}
```

## Proposed IR + semantics

- Normative statement subset over the existing `RuneBody` IR: `let` (typed by
  inference against the op signature), assignment, injected-repository
  navigation (`self.<repo>.<op>(...)`), field-path reads, codelist-typed
  comparisons, `if`/`guard ... else error`, `return`, and a CLOSED set of
  collection reductions (`sum`/`count`/`any`/`all`) — everything else is a
  compile error naming the construct (the codegraph closed-subset philosophy).
- Typed params/returns: signature slots resolve through the existing
  `OperationParam`/`TypeRef` typing — no new type system.
- Typed error semantics: `error <Variant>(args)` lowers to the domain error
  enum; two variants are reserved and always available (`NotFound`,
  `Conflict`) so guards lower to 404/409 uniformly.
- Capability checks stay declarative (grants/`when`); bodies never bypass
  them — the compiler injects the authorize step, not the body.
- Compilation produces the same wire artifact (`type_json`, `return_type`)
  plus the statement IR; codegraph transpiles the closed subset to Rust (and
  refuses, naming the construct, outside it — today's contract, now with
  real bodies to lower).

## Acceptance

- Conformance fixture per construct (let/guard/navigation/reduction/error/
  return) + refusal fixtures naming unsupported constructs.
- rex-driver compile checks: navigation targets exist on injected repos,
  field paths resolve, types align, every path returns or errors.
- codegraph lowers the subset end-to-end (op route returns real data; 501
  only for constructs outside the subset) and its compile pins cover the
  emitted plane.

## Consumer impact

- The 501 seam classes shrink to genuinely unmodeled behavior; consumers
  stop hand-writing command handlers for modeled logic.
- Existing bodiless ops are unaffected (seam stays); adopting bodies is
  incremental per op.
- Hand-written manual-impl traits (codegraph's `functions` plane) remain the
  escape hatch; bodies and manual impls are mutually exclusive per op
  (compile error if both).
