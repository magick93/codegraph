# Events producer v1 — the refusal contract (issue #820)

Status: **contract published** (implementation slice pending; the
`events_producer` flag stays **OFF** by default, so output is byte-identical).

This document is the interface the consumer codes against: the exact message
shape for every generation-time refusal the producer planner can raise, so
payload-field renames in a model can be done **before** the flag is turned on.
It is normative for the implementation slice: the emitted strings are pinned by
KATs in the same PR that lands the planner.

## Where the messages surface

1. **Pre-generation validation** (flag ON): every refusal is a HARD error
   raising before any file is written — the `body_refusals` posture
   (`ValidationPass::check_event_producers`), never a half-generated tree and
   never a silently missing publish.
2. **Diagnostic mode** (flag OFF): `codegraph doctor --ddd-files … --evt-files …`
   prints the SAME lines as `WARN events producer — …` (severity WARN, never
   fails). A consumer aligns names here first, then flips the flag.

Both modes print the identical sentence body; only the surface prefix and the
severity differ. One formatter, one test. The doctor line is
`WARN events producer — <body>`; the hard-error line is
`events producer: <body>` (the body starts at the artifact).

## The sentence

Every line starts with the fixed prefix and names the artifact, then the
reason, then the remediation clause:

```
events producer: <artifact> <reason> — <remediation>
```

`<artifact>` is always one of:

- `event `{Event}`` — the declared `.evt` event;
- `event `{Event}` (operation `{Service}.{op}`)` — when the refusal is about a
  particular emitting operation.

Field spellings are printed **verbatim as authored** (never normalized), so a
rename search in the model is a literal string search.

## Refusals (ordered by the planner's evaluation; first blocker wins)

### R1 — unmatchable payload field (the near-miss class)

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` has no matching field on `{Entity}Response`; nearest: `{candidate}` — rename the payload field in the .evt contract or the class feature so the spellings agree
```

`{candidate}` is computed deterministically over the entity's generated
`{Entity}Response` field names: (1) snake_case equality, (2) the `_code`-stripped
equality (`strip_code_suffix_safe`), (3) the closest name by prefix-normalized
edit distance ≤ 2 (ties broken lexicographically). When nothing is within
distance 2 the clause is `nearest: (none within edit distance 2)`.

This is the message the consumer's nine near-miss payload fields will be
aligned against; the candidate is a literal DTO field name.

### R2 — nullable source feeding a required payload field

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` is required but `{Entity}Response.{candidate}` is nullable — declare the payload field optional or make the source feature required (v1 refuses to invent a null policy)
```

`.evt` payload fields carry no multiplicity in the current pin, so every
payload field is required. "Declare the payload field optional" is the forward
path an upstream multiplicity construct would unlock; v1 has no null policy.

### R3 — source lives in a contained child collection

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` is sourced only from the contained collection `{Entity}.{child}` — v1 publishes record-compatible payloads only; lift the value onto the record or emit a child-scoped event
```

### R4 — no runtime site

```
events producer: event `{Event}` is bound to operation `{Service}.{op}` which has no runtime site ({protected|child-bound|abstract|create-shaped|delete|read-only}) — the event cannot fire; bind it to an executing operation
```

`protected` is the #619 vocabulary (a valid visibility, but not a publisher);
`child-bound` is a non-root child op; `abstract` is the 501 seam;
`create-shaped` is the collection-level create op; `delete` is the delete arm;
`read-only` is a read/projection arm. Slice (a) publishes on **transition and
plain patch** arms only — every other executable arm is refused by name rather
than silently skipping the emission.

### R5 — duplicate emission, both conditions can hold

```
events producer: event `{Event}` is bound to operation `{Service}.{op}` more than once and the conditions can both hold — split the operations or make the conditions exclusive
```

### R6 — a `…Changed` fact on a plain patch that does not touch its anchor

```
events producer: event `{Event}` is a change fact bound to plain patch operation `{Service}.{op}` which does not write `{anchor}` — bind the emission to the operation that owns the fact
```

`{anchor}` is the payload field's source (e.g. `status` for
`WorkerStatusChanged`), decided by the longest `{Entity}<Anchor>` suffix match
on the event name.

### R7 — type disagreement (no coercion)

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` is `{payload_type}` but `{Entity}Response.{candidate}` is `{dto_type}` — v1 does not coerce {shape|nullability}; align the model
```

`{shape}` covers list-vs-scalar and record-vs-id (plus a plain scalar type
mismatch, which prints `the value types`); `{nullability}` covers the
`Option` split. The deliberately SUPPORTED conversions are not refusals: an
identical type, `Uuid → String`, `i32 → i64`, and a codelist enum → the same
enum or `String`.

### R8 — cross-contract event

```
events producer: event `{Event}` is declared by the contract of domain `{contract_domain}` but bound to operation `{Service}.{op}` of domain `{domain}` — cross-domain emission is not lowered in v1
```

v1 derives the contract domain from the event's payload: the domain of the
first field whose type resolves to a schema (`EventDefinition.domain`). An
all-primitive event has none and never refuses R8. A future upstream
contract-domain declaration replaces this inference.

### R9 — no model source at all

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` has no model source — v1 publishes record-compatible payloads only; add the feature (or a context binding) to the model
```

R9 fires when the planner has no `{Entity}Response` surface to match against at
all: the operation binds no entity, the bound class resolves to no schema, or a
context fact the payload names is unavailable on this arm (a `previousState` on
a plain patch, or any transition fact on an entity with no machine). It is
reported once per `(operation, event)` pair, naming the pair's first payload
field. A field that *does* have a surface but matches nothing stays R1 (with
the `nearest: (none within edit distance 2)` clause).

## Warnings (never fail; each distinct line printed once per run)

### W1 — surrogate identity binding

```
events producer: `{Entity}Id` binds to the surrogate `id` (the record also carries `{alt}`) — confirm the payload means the surrogate
```

### W2 — field not written by the emitting op

```
events producer: payload field `{field}` binds `{Entity}Response.{candidate}`, which operation `{Service}.{op}` does not write (the value comes from creation, not the transition)
```

This is a correctness smell, not an error: the event still publishes the
record's current value. W2 is emitted per payload field (deduped across events
on the same op). On a TRANSITION arm it counts the op's OWN writes (its
parameters, body assignments, and the machine state — the client patch body is
not the transition); on a plain patch it counts the Update DTO's writable
fields (the client patch body can set those, so no warning).

### W3 — record field shadows a context binding

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` matches both `{Entity}Response.{candidate}` and the context binding `{context}` — bound to the record field; rename one side to disambiguate
```

A record match always wins; the warning makes the shadowing visible so a
payload that meant the context fact can be renamed deliberately.

## Context bindings (class B)

v1 reserved context vocabulary. A payload field whose snake_case spelling
matches one of these names binds to the context fact — but only when no
`{Entity}Response` field matches (a record match wins, with W3).

| Payload names | Fact | Rust type | Available on |
|---|---|---|---|
| `occurredOn`, `occurred_on` | the operation clock, UTC (`chrono::Utc::now()`) | `chrono::DateTime<chrono::Utc>` | every publishing arm |
| `previousState`, `fromState`, `previous_state`, `from_state` | the transition's source state | the machine field's generated DTO type (a codelist enum, or `String`) | transition arms only |
| `nextState`, `toState`, `next_state`, `to_state` | the transition's target state | the machine field's generated DTO type | transition arms only |
| `correlationId`, `correlation_id` | the command's correlation id | `uuid::Uuid` | every publishing arm |
| `organizationId`, `organization_id` | the session organization | `uuid::Uuid` | every publishing arm |

An unavailable fact is R9 (no model source). A context-bound payload field's
type follows the same supported-conversion table as R7 (identical,
`Uuid → String`, `i32 → i64`, codelist enum → the same enum or `String`);
anything else is R10:

### R10 — context type disagreement

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` is `{payload_type}` but the context binding `{context}` supplies `{context_type}` — v1 does not coerce {shape|nullability}; align the model
```

`{shape}` and `{nullability}` are the R7 phrases. The vocabulary is CLOSED in
v1: a spelling outside it is not a context binding (it falls through to R3/R1
as an ordinary field name).

## Documented decisions (no message)

- Op-clock context dates are **UTC** (`{Event}.occurredOn` from
  `chrono::Utc::now()`); the payload type is the declared one.
- `{Entity}Id` → the DTO's `id` when W1 applies.
- Publish placement is **inside the command's transaction**, between the
  repository write and `commit()` (issue #820 item 4) — a rolled-back save
  leaves no `pgmq` row; the live gate proves it.
- Arm rules: a transition op publishes only after its edge (never on the
  409/403 paths); a plain patch publishes only when a write happened (never on
  the idempotent-read arm).

## Sequencing with pherolang (EV2 / EV3)

- **EV2** (arm-scoped emissions) is the grammar that lets a contract declare
  which arm owns a fact; until it lands, R6 is the fail-closed stand-in for
  the "two events on one op cannot both be true" case and the planner uses the
  op's own arm classification.
- **EV3** (transactionality as a declared semantic) gates the publish
  placement: v1 hard-codes the transaction seam described above; when EV3
  lands, the declared semantic selects it and the default stays identical.
- Neither blocks publishing this contract. The v1 implementation slices:
  (a) planner + R1–R9 KATs + doctor diagnostics (flag OFF, byte-identical),
  (b) the transaction seam + typed construction + the rollback live gate
  (flag ON), (c) the catalogue section that marks each event
  `published at runtime` / `not emitted (<refusal>)`.
