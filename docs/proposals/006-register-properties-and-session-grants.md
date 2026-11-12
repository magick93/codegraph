# Proposal: register-class properties + session-kind grant predicates

Status: draft — CONTINGENT on product/security approval (codegraph lifecycle
hardening phase 2, Nov 2026)
Target: rexlang (rex-ir class/actors; rex-driver compiler)

Two small, independent asks bundled because both sit at the same
model-to-enforcement boundary.

## Part A — register-class properties (rexlang #83)

**Problem.** Register classes (the BaseJump-style org/role noun space) accept
no property declarations, so consumers cannot model role attributes
(display name, cost center, delegation window) — they hand-maintain SQL
migrations beside the model.

**Proposed.** Allow property declarations on register classes:

```
register Role {
    String display_name
    CostCenter cost_center optional
}
```

Lowered onto the register table as plain columns (`ClassDef.features`
already carries them; the register backend creates the columns). No
relationship features on register classes in v1 (keep the noun space flat).

**Acceptance.** Compile + IR round-trip + migration emission fixtures;
codegraph DDL/DTO emission for register properties behind the existing
register capability.

**Impact.** Additive; models without register properties unchanged.

## Part B — session-kind grant predicates

**Problem.** Grants cannot distinguish session kinds. codegraph deliberately
bypasses capability/role gates for API-key sessions (automation keys must
work; only RLS scopes + op-level `when` guards apply). That policy is
hard-coded, because the model cannot express "this flow is human-only" — so
consumers who need it enforce nothing (or hand-roll middleware).

**Proposed.** Session-kind predicates on grants:

```
permit human SubmitPayRun when (payRunStatus == Draft);   // human sessions only
permit agent  IngestHours;                                // automation sessions only
```

IR: `GrantEffect`/grant edges gain `session_kind: Option<SessionKind>`
(`Human | Agent | Any`, default `Any` — serde-defaulted, back-compat).

Compiler: `human`/`agent` grant modifiers are validated (no unknown kinds).

Downstream contract (codegraph-side, only if approved): the RLS session
bundle already carries the session provenance — `enforce_api_key_scope`
distinguishes key sessions from JWT sessions — so a `human`-only permit
lowers to an RLS/handler check on that provenance. API keys would STOP
bypassing gates for `human`-only capabilities (key sessions satisfy only
`agent`/`Any` grants).

**Acceptance.** Compile fixtures per kind; codegraph enforcement pins:
agent-only capability denied over a JWT session, human-only capability
denied over an API-key session, `Any` unchanged.

**Impact — needs product sign-off BEFORE any codegraph implementation:**
tightening API-key semantics breaks automation keys that rely on today's
bypass. The test/automation-key policy must be defined first (which keys are
`agent`, key rotation, persona keys in e2e). Until then this proposal is
modeling-only; codegraph's documented v1 semantics (API keys bypass
capability gates) remain the contract.
