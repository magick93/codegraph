# TDD — the events producer publishing plane (issue #820 slice b)

Status: **LANDED** on `feat/820b-events-producer-publishing` (PR pending).

## 0. The defect, verified

Slice (a) made `emits` bindings *plannable* (exact payload field bindings,
R1–R12 refusals, doctor parity) with the `events_producer` flag OFF. Nothing
published: the emitted service-op handler answered the normal 200, the row
moved, and the declared event never reached `pgmq`. The catalogue's "Emitted
by" table was the only trace of the declaration.

Slice (b) turns a planned `(operation, event)` pair into a real publish that
is **part of the command's transaction**:

- a publish failure rolls the business write back (no orphan event for a
  rolled-back row, and no half-written row for an event that could not be
  sent);
- a rolled-back save leaves no queue row (a rolled-back publish cannot
  survive the write it describes).

The work was split into three commits:

1. `73782c408` — contract + planner facts + pure render (no output change);
2. `e4d39b8b9` — generation wiring (SeaORM/cornucopia `_in_tx`, the command
   seam, the handler argument, the per-run plan index);
3. this commit — the live gate (publish + rollback proofs) and the docs.

## 1. Design

- **Flag** `events_producer` (strict bool, default false). Flag OFF ⇒ the
  planner contributes zero diagnostics and the command/handler/emit
  templates emit exactly the pre-#820 bytes (handler matrix, review-fixture
  drift, evt pipeline/workers and ux/policy canaries all hold). Plan-less
  runs validate with the flag off.
- **Pure render** (`events/producer/render.rs`): the planner's `ProducerPlan`
  lowers to the generated Rust text — the outgoing enum
  (`{Entity}OutgoingEvent`), the pre-write re-read (only when a binding uses
  `previousState`), the publish block, and the handler's outgoing-variant
  argument. The command generator pre-renders these blocks; the template
  carries no binding logic (the #338/#590 pre-rendered-block precedent).
  Typed construction adapts each source to the declared `.evt` type through
  the contract's supported conversions (R7/R10: same type, Uuid→String,
  codelist→String, i32→i64); payload field names are projected through
  `to_snake_case` (the #835 lesson — the contracts struct's own projection).
- **Transaction seam** (`templates/ddd/command_update.tera`, included by
  `command.tera`): `update_with_events(…, outgoing: &[OutgoingEvent],
  traceparent: Option<String>)` and a shared `update_impl`. The plain
  `update()` keeps its signature and passes `&[], None`, so every existing
  caller is unchanged.
  Placement: repository write → `after_update_tx` hook → **publish** →
  `commit()`. A publish error propagates (`?`) and rolls the transaction
  back. The post-write re-read (`repo.find_by_id(&tx, id)`) builds the
  payload from the committed-in-transaction row; the optional pre-write
  re-read (only when `previousState` is bound) runs before the write and is
  read under the same RLS session bundle.
- **Provider-neutral publish** (`templates/events/emit.tera`): both legs
  gain `publish_{channel}_in_tx` — SeaORM `&impl ConnectionTrait`,
  cornucopia `&impl GenericClient` — and the public `publish_{channel}`
  becomes a thin wrapper over it. `EventContext` gains
  `entity_table: Option<&'static str>` / `entity_id: Option<Uuid>` under the
  flag; both envelopes (native and CloudEvents' `entitytable`/`entityid`
  extensions) populate them. Trigger/DSL publishers keep the pre-#820
  envelope shape (`Null`).
- **Fanout**: one publish per bound DSL channel that declares the event, in
  the event's declaration order (`declared_channels` shares
  `collect_channels` with the events generator, so the two planes cannot
  disagree). Any channel failing rolls back the write and the earlier
  publishes.
- **Handler plane** (`api/handler/support.tera`, `service_op.tera`): an
  Update route with plans calls `update_with_events` with the typed
  variants and `extract_traceparent(&headers)` (a support helper emitted
  only when `has_producer_events`). The inbound W3C `traceparent` rides the
  envelope; absent when the request supplies none.
- **R11 / R12** (contract-first, planner): R12 — the event publishes on no
  persisted (pgmq-bound) channel, so the emission cannot fire; R11 — the
  bound entity exposes no `read`, so the in-transaction re-read cannot
  build the payload. A **statement-body op refuses R4**
  (`NonPublishing("statement-body")`): a body's saves are multiple
  transactions, so "the command's transaction" is not a single seam.
- **One plan per run** (`ProducerPlanIndex` on `GeneratorContext`): the
  planner result is indexed by `(domain, title)` once and injected into the
  command and handler generators, so neither re-runs the analysis per
  entity.
- **No graph shape change**: no new node/edge/property;
  `GRAPH_FORMAT_VERSION` untouched.

## 2. Gates

| Gate | Command | Pins |
|---|---|---|
| Render/planner KATs | `cargo test -p codegraph-generate --lib -- events::producer` | payload expressions + adaptations, outgoing enum, pre-read gate, publish block ordering, handler argument, R11/R12, statement-body refusal |
| Flag-off emit goldens | `cargo test -p codegraph-generate --lib -- evt_events` | `emit.tera` flag-off bytes == the pre-workers/first-slice golden; the `_in_tx` pair only under the flag |
| End-to-end generation | `cargo test -p codegraph --test events_producer_tests` | flag-on: outgoing enum, `update_with_events`, `publish_*_in_tx`, `entity_table`, pre/post re-read, typed field projection, handler variants + `extract_traceparent`, `&impl ConnectionTrait`; flag-off: no publishing surface |
| Compile pin (#551) | `cargo test -p codegraph --test events_producer_compile_tests -- --ignored` | the generated app (`cargo check`) with the flag on — the seam and handler compile |
| **Live gate** | `cargo test -p codegraph --test events_producer_live_gate -- --ignored --nocapture` | real HTTP against a booted generated app: typed envelope in `pgmq.q_events_payroll`; a failing second publish rolls the write + first message back; the replay publishes both |
| Byte identity | `handler_matrix_tests`, `review_fixture_drift_tests`, `evt_pipeline_tests`, `evt_workers_tests`, `ux_rules_byte_identity_tests`, `policy_rls_tests` | flag-off output unchanged |
| Regression | `service_ops_tests` | the unchanged `update()` signature and the service-op plane |

## 3. The live gate defect (and its root cause)

The gate's first runs failed at the action POST:

```
HTTP 403 {"error":{"code":"FORBIDDEN","message":"permission denied for schema pgmq"}}
```

Diagnosis on the scratch database:

```
pgmq schema privilege = false
pgmq nspacl = {supabase_admin=UC/supabase_admin}
```

Root cause, in order:

1. The gate's `GateDb::apply_prelude` (the Supabase-image bootstrap) creates
   the `pgmq` schema as the image's superuser — the documented #727 M6
   constraint ("an app-plane writer of pgmq tables needs an owner/GRANT
   decision").
2. The Supabase image ships pgmq as an **available** extension, so the gate
   (and the migration band) chose the real migrations. But `CREATE
   EXTENSION` requires ownership of the schema its control file declares
   (`schema = 'pgmq'`), and the migration role (`postgres`, NOSUPERUSER)
   does not own the pre-created schema: `CREATE EXTENSION pgmq` failed with
   the same `permission denied for schema pgmq`.
3. `0003_pgmq_setup.sql` is classified as a phase-1 ("tables") file and the
   migration runner **warns and continues** on phase-1 failures — so the
   swallowed failure left no extension, no queue tables and no grants. The
   late-binding `GRANT USAGE ON SCHEMA pgmq TO app_user` in
   `0005_platform_schema.sql` / `9000_app_user_grants.sql` failed the same
   way (non-owner), also swallowed.
4. The producer publishes under `SET LOCAL ROLE app_user`, so `pgmq.send`
   resolved `pgmq` and checked schema `USAGE` → 42501 → the #828 SQLSTATE
   classifier maps that to 403.

**Not a product defect on the standard path**: a real project's own
`0003_pgmq_setup.sql` creates the extension as its migration role (pgmq's
control file sets `superuser = false`), so that role owns the schema and
every generated grant applies. Verified on the same image:
`postgres` can `CREATE EXTENSION pgmq` once the placeholder schema is gone,
then owns the schema/functions and the grants succeed.

**The fix is gate-only**, and mirrors that ownership exactly:

- `test_framework::postgres::GateDb::exec_as_admin` — run SQL as the base
  target's resolved admin role against the scratch database (the
  `apply_prelude` pattern).
- The gate's `prepare_pgmq` drops the prelude's placeholder schema as the
  admin role (only when the extension is not installed and is available),
  then installs the extension as the migration role — the schema/functions
  are owned by the role that runs the band, exactly as in production.
- Plain Postgres has no pgmq: the gate applies the shared functional
  stand-in (`PGMQ_FUNCTIONAL_SHIM`) before the band and stages
  `0003_pgmq_setup.sql` with only its `CREATE EXTENSION` line neutralised —
  the queue creation, the grants and the trigger function still run for
  real (the previous staging replaced the whole file, so its queues never
  existed).
- The gate now asserts `has_schema_privilege('app_user','pgmq','USAGE')`
  after migrations, so this class cannot regress silently.

Two more gate corrections found by running it:

- **pgmq 1.5.1 queue tables are extension members** (`ALTER EXTENSION pgmq
  ADD TABLE` in `pgmq.create`), so the rollback leg cannot `DROP TABLE
  q_events_audit`. It renames the table instead — `pgmq.send` then fails
  with `relation ... does not exist` (42P01 → 500), where a revoked
  privilege would map to 403 through the SQLSTATE classifier.
- **the envelope lives in pgmq's `message` column** (1.5.x renamed it from
  `payload`), so the assertions read `message->'payload'->>…`.
- the first approve already publishes BOTH events, so the replay leg's
  audit count is 2 (one per approve), and the rollback leg asserts the
  audit queue stayed at 1.

No generated migration output moved.

## 4. Mutation teeth (run, red, restore)

**M1 — ignore the flag in `emit.tera`.** Replaced every
`{% if project.events_producer %}` with `{% if true %}` and ran the
flag-off golden:

```
test evt_events::tests::consumers_and_emit_templates_flag_off_are_byte_identical_to_pre_workers ... FAILED
assertion `left == right` failed: flag-off events/emit.tera rendering changed
  left:  … "entity_table": ctx.entity_table, … pub entity_table: Option<&'static str>, …
 right:  … "entity_table": serde_json::Value::Null, …
```

Restored; `evt_events` green (49 tests).

**M2 — let the publish escape the transaction.** Changed the rendered
publish call from `&tx` to `&self.db` in
`events/producer/render.rs::render_publish_block` and ran the live gate:

```
assertion `left == right` failed: the failed second publish must roll the first message back
  left: "2"
 right: "1"
```

Restored; the live gate is green.

(The slice-a teeth — dropping the `_code` projection fails the R1 nearest
KAT, ignoring the flag fails the flag-off pins — remain covered by
`docs/tdd/820a-events-producer-planner.tdd.md`.)

## 5. Verification log

```
cargo fmt --check                                            # clean
mbx clippy --workspace --all-targets -- -D warnings -D clippy::too_many_lines  # exit 0
mbx test -p codegraph-generate --lib -- events::producer evt_events  # 91 passed
mbx test -p codegraph --test events_producer_tests           # 7 passed
mbx test -p codegraph --test handler_matrix_tests --test evt_pipeline_tests \
  --test evt_workers_tests --test review_fixture_drift_tests \
  --test ux_rules_byte_identity_tests --test policy_rls_tests \
  --test service_ops_tests                                   # 1/5/2/9(+1 ignored)/4/17/41 passed
mbx test -p codegraph --test events_producer_compile_tests -- --ignored  # 1 passed
mbx test -p codegraph --test events_producer_live_gate -- --ignored --nocapture
```

Live gate output (scratch Supabase image, generated app over real HTTP):

```
gate: server booted on 127.0.0.1:34997
gate: publish proof — typed envelope in pgmq.q_events_payroll
gate: rollback proof — no message, row still Draft
gate: replay proof — both channels published, row Posted
test result: ok. 1 passed; 0 failed; 0 ignored; …
```

The publish proof asserts the full envelope: `event = PayRunPosted`,
`payload.pay_run_id = row id`, `pay_run_status/posted`, `previous_state =
Draft`, `next_state = Posted`, `total = 42`, the correlation id, the
organization id (both payload and `tenant_id`), an ISO `occurred_on`,
`entity_table = pay_run`, `entity_id = row id`, and the inbound
`traceparent`.

## 6. Byte movement

- **Flag OFF: none** — the handler matrix, review-fixture drift pin, evt
  pipeline/workers canaries and ux/policy snapshots hold with no re-bless.
- **Flag ON** (enumerated): per emitting entity, `{Entity}OutgoingEvent` +
  `update_with_events` + `update_impl` in `command.rs`; per bound channel,
  `publish_{channel}_in_tx` (and `EventContext`'s identity fields) in
  `src/events/emit.rs`; on the action route, the `update_with_events`
  argument + `extract_traceparent` helper; the producer plan index is
  in-process only.
- **Migrations: none** (`prepare_pgmq` is a gate fixture).

## 7. Deferred

- the catalogue's `published at runtime` marks and refused/published status
  (slice c);
- the workers live gate for the publishing plane (the action handlers are
  monolith-only today — the #338 posture; the cornucopia `_in_tx` leg is
  template-pinned and compiled by the workers wasm gate);
- statement-body ops as publishing arms (R4 names the reason — multiple
  saves per body need their own transaction seam);
- a preinstalled-but-admin-owned `pgmq` (an operator enabling pgmq via the
  Supabase dashboard before the migration band) still needs the operator to
  grant `USAGE`/`EXECUTE` to the app role; the generated template cannot
  grant as a non-owner. The standard path (the project's own migration
  creates the extension) is what the gate proves.
