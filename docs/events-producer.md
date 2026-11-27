# Events producer v1 — the refusal contract (issue #820)

Status: **slices (a) and (b) landed; slice (c) adds the catalogue marks**
(issue #820). The `events_producer` flag stays **OFF** by default, so every
shipped profile's output is byte-identical — the marks below render only when a
run enables the flag.

This document is the interface the consumer codes against: the exact message
shape for every generation-time refusal the producer planner can raise, so
payload-field renames in a model can be done **before** the flag is turned on.
It is normative: the emitted strings are pinned by KATs in the same PR that
lands each slice.

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
A nullable machine state field feeding `previousState`/`nextState` is R2 too:
the state source is nullable, so the payload's required state field has no
invented default.

### R3 — source lives in a contained child collection

```
events producer: event `{Event}` (operation `{Service}.{op}`) payload field `{field}` is sourced only from the contained collection `{Entity}.{child}` — v1 publishes record-compatible payloads only; lift the value onto the record or emit a child-scoped event
```

### R4 — no runtime site

```
events producer: event `{Event}` is bound to operation `{Service}.{op}` which has no runtime site ({protected|child-bound|abstract|create-shaped|delete|read-only|statement-body}) — the event cannot fire; bind it to an executing operation
```

`protected` is the #619 vocabulary (a valid visibility, but not a publisher);
`child-bound` is a non-root child op; `abstract` is the 501 seam;
`create-shaped` is the collection-level create op; `delete` is the delete arm;
`read-only` is a read/projection arm; `statement-body` is an executable
`ExecutionKind::Body` op. Slice (a) publishes on **transition and plain patch**
arms only — every other executable arm is refused by name rather than silently
skipping the emission. A statement body's saves are multiple
`commands.update` calls, so threading one publish through them is a recorded
follow-up; v1 refuses it instead of publishing per save.

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

### R11 — no post-write re-read

```
events producer: event `{Event}` (operation `{Service}.{op}`) publishes from `{Entity}`, which exposes no `read` operation — v1 builds the payload from an in-transaction re-read; add `read` to the class design or bind the emission to a record with one
```

The publish reads the updated row **inside the command's transaction** (before
and after the write). An entity whose design exposes no `read` has no
`find_by_id`, so there is nothing to construct the payload from. `read` is the
same operation the route's record load already requires.

### R12 — no persisted publishing channel

```
events producer: event `{Event}` is published on no persisted channel — the emission cannot fire; declare a channel that publishes it on a queue-backed target
```

An event is published on every DSL channel whose `publishes` list declares it
and whose transport binds to pgmq. No such channel (no channel at all, an
unbound/sqlite target, or trigger-only publications) means there is no publish
function to call — v1 refuses rather than silently dropping the emission.

R11 and R12 are checked AFTER the payload bindings (R1–R10): a payload
name/shape error is the more actionable first blocker, so the channel/read
wiring is fixed once the payload binds.

## Publishing seam (slice b)

With `events_producer` ON, every planned (operation, event) pair publishes
from inside the command's transaction:

- **Placement.** Between the repository write (and the `after_*_tx` hooks)
  and `commit()`. A publish failure rolls the business write back; a
  rolled-back save leaves no `pgmq` row. An `after_*_tx` hook abort still
  happens before the publish.
- **Re-read.** The command reads the row before the write (only when a
  payload field uses `previousState`) and after it; the payload's record
  fields, `nextState`, and `{Entity}Id` come from the post-write response,
  `previousState` from the pre-write response. The reads run under the same
  RLS session bundle and see the uncommitted write.
- **Fanout.** One message per bound DSL channel that publishes the event, in
  the event's declaration order. Any channel's send failing rolls back the
  whole operation (all channels + the write).
- **Envelope.** Producer events populate `entity_table`/`entity_id` (native;
  CloudEvents extensions `entitytable`/`entityid`) and carry the inbound
  `traceparent` when the request supplies one. The op clock (`occurredOn`) is
  UTC at construction; the envelope `occurred_at` is UTC at publish.
- **Arm rules.** A transition publishes after its edge (never on the
  409/403 paths); a plain patch publishes only when a write happened (never
  on the idempotent-read arm). Create/delete/read/statement-body arms do not
  publish in v1 (R4).

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

## Catalogue marks (slice c)

With `events_producer` ON, the generated `docs/events.md` marks every declared
event's runtime publication with ONE line, its own paragraph, directly after
the event's metadata table (before `### Payload`):

```
Runtime publication: published at runtime by {Service}.{op} (arm: {arm}; queue: {queue})
```

- `{arm}` is `transition` or `patch` (the publishing arms); `{queue}` is the
  plan's own bound queue — `ProducerChannel.queue` from the same planner run,
  never a re-derived channel name.
- Several emitting operations and/or channels stay ONE line: the operations
  join with `", "` in declaration order, and the keys pluralize with their
  deduped values in the plan's order — `arms: transition, patch` /
  `queues: events_audit, events_orders`.
- A declared event that no modeled service operation binds carries:

```
Runtime publication: not emitted — no modeled service operation declares this event
```

**Refusals and warnings are deliberately NOT rendered in the catalogue.** A
flag-ON run hard-fails on every R*/W* finding before any file is written
(`ValidationPass::check_event_producers`), so a written tree cannot contain a
refused binding — the mark above would be dead text. Those sentences stay on
the validation and `codegraph doctor` surfaces (see above).

The marks are gated on the flag: flag OFF ⇒ no planner report reaches the
catalogue ⇒ `docs/events.md` is byte-identical to the pre-slice-(c) document.
The gate is the flag alone — a flag-ON run that plans NO publishes still marks
every declared event `not emitted`, so "nothing publishes" is distinguishable
from "the feature is off". (The publishing seam additionally skips an empty
plan index; that gate is the command plane's, not the catalogue's.) The
projector takes the report as `Option`, so the projection is pure and
unit-testable without an engine.

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
  (flag ON), (c) the catalogue marks — `published at runtime` for a planned
  event, `not emitted` for a declared event no operation binds (refusal marks
  are unreachable once (a)'s fail-closed validation runs first).
