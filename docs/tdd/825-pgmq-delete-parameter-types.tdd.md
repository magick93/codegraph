# TDD — pgmq resolves by parameter type: every `pgmq.delete` is typed (issue #825)

Status: **LANDED** on `fix/825-pgmq-delete-casts`.

## 0. The defect, verified

Every generated workers dispatcher runs

```rust
client.execute("SELECT pgmq.delete($1, $2)", &[&queue, &msg_id])
```

at two sites — the dead-letter move (`dead_letter`) and the processed-
message delete (`delete_pgmq_msg`). pgmq ships two overloads,
`delete(text, bigint)` and `delete(text, bigint[])` (pgmq 1.5.1), so the
statement fails whenever the SERVER has to infer the parameter types:

```text
ERROR:  function pgmq.delete(unknown, unknown) is not unique
HINT:  Could not choose a best candidate function. You might need to add explicit type casts.
```

Parse-only reproduction against a real pgmq 1.5.1 (a `PREPARE` executes
nothing):

| Statement | Result |
|---|---|
| `PREPARE d AS SELECT pgmq.delete($1, $2)` | `function pgmq.delete(unknown, unknown) is not unique` |
| `PREPARE d AS SELECT pgmq.delete($1::text, $2::bigint)` | prepared |
| `PREPARE d AS SELECT pgmq.send($1, $2)` | prepared (exactly one two-argument overload) |

The consumer (crewbase, workers + cornucopia) logged 962 refusals in two
days. Nothing was ever removed from a queue: a processed message stayed
and was replayed by every later drain after the 30 s visibility timeout,
and a poison message added a DLQ copy on every replay past the attempt
budget (the send precedes the failing delete).

### The mechanism — and why the monolith is not affected

The workers drain binds through `tokio-postgres`' `client.execute(sql,
&[…])`, which prepares with NO declared parameter types; the server infers
them, and an untyped parameter arrives as `unknown`. The monolith drain
runs the SAME text through SeaORM (`Statement::from_sql_and_values`), and
sqlx declares each parameter's type from the bound value — the call
resolves before the server has to guess.

**Observed, not assumed.** With a stand-in that carries BOTH overloads
(§1), the monolith events runtime gate stays GREEN on the pre-fix
templates. The defect lives on the workers leg alone, exactly as the
issue's title says.

### Six sites, not four

The issue names the four template sites (`dispatch_worker.tera:415,468`;
`dispatch.tera:352,559`). The byte-identity golden
`evt_golden/dispatch_pre_evt.rs` carries the monolith template's text twice
more (lines 319 and 526), and `dispatch_template_flag_off_is_byte_identical_to_pre_evt`
holds the template to it byte for byte — a template-only fix fails that
pin. All six are typed.

### Why every gate stayed green

Two blind spots, one per leg:

- **Monolith leg** (`events_runtime_tests`): the binding is typed, so the
  untyped text resolves.
- **Stand-in**: the gate's `PGMQ_FUNCTIONAL_SHIM` defined ONE
  `pgmq.delete(p_queue text, p_msg bigint)` — faithful in behaviour, not in
  function resolution. A single-overload stand-in cannot see an ambiguity.

## 1. The decision

1. **Type the delete at all six sites** — `pgmq.delete($1::text,
   $2::bigint)`, the casts the server's own hint asks for — on BOTH legs.
   The monolith does not need them at runtime; one spelling on both legs
   means neither can drift back, and the two legs already share bytes
   through the golden.
2. **Leave `send` / `read` / `create` untyped.** Each is unambiguous at its
   arity against pgmq 1.5.1 (the PREPARE table above). Casting them would
   widen the byte-identity movement into `events/emit.tera` for no defect;
   the runtime pin's PREPARE loop (§2) guards them instead.
3. **Make the stand-in faithful and share it**
   (`tests/test_framework/pgmq.rs`): both `delete` overloads; the `pgmq`
   schema and its functions only, so any gate can apply it (the events
   harness keeps its own roles and grants beside it).
4. **Pin the workers leg under its real binding discipline.** A string
   assert cannot tell a typed bind from an untyped one; a PREPARE through
   tokio-postgres can.

Rejected: casting only the workers template (the legs would spell the same
call two ways); a database-side workaround (the overloads belong to the
extension — dropping one breaks pgmq's own contract and every upgrade).

## 2. The teeth (written first, watched fail)

### Tooth 1 — runtime: the workers leg, replayed (`webhook_dispatch_pgmq_tests`, ignored, the ops lane)

The rendered workers dispatcher is scanned for its pgmq statements (a
small extractor that reads Rust string continuations and escapes the way
the compiler does); each is PREPAREd through `tokio-postgres` with no
declared types against the shared stand-in; then the processed-message
delete — the emitted text, bound `&queue, &msg_id` exactly as
`delete_pgmq_msg` binds it — must remove the row.

RED on the pre-fix `dispatch_worker.tera`:

```text
`SELECT pgmq.delete($1, $2)` does not resolve under untyped binding: db error (42725: function pgmq.delete(unknown, unknown) is not unique): ERROR: function pgmq.delete(unknown, unknown) is not unique
HINT: Could not choose a best candidate function. You might need to add explicit type casts.
```

That is the production failure, reproduced upstream for the first time.
The pin prints the SQLSTATE and the server message itself: tokio-postgres'
`Display` renders only the error KIND (`db error`; the `DbError` sits in
`source()`) — the same blindness that made the consumer's #810 read as an
opaque database error.

### Tooth 2 — node-free, PR CI (same binary)

- `every_pgmq_delete_in_the_dispatch_templates_is_typed`: both templates
  rendered with the events plane on (the only shape that renders the
  dead-letter twin), every `pgmq.delete` equal to the typed spelling.
  RED on master naming `webhook/dispatch.tera` and
  `webhook/dispatch_worker.tera`.
- `the_workers_dispatcher_pgmq_surface_is_pinned`: the workers
  dispatcher's pgmq statements equal exactly
  `{read, create, send, typed delete}` — a template edit that renames or
  drops a call cannot leave the runtime leg stale. RED on master:
  `unexpected ["SELECT pgmq.delete($1, $2)"]`.
- `the_extractor_reads_continuations_escapes_and_comments`: the
  extractor's own KAT (a backslash-newline continuation, `\"` inside the
  SQL, a `'"'` char literal, a doc comment quoting a statement).

### The monolith leg — the removal contract, documented

`events_runtime_tests` gains two assertions: the source queue ends EMPTY
and the DLQ holds EXACTLY one copy. They pass before and after the fix —
the monolith binds typed values — and they are the assertions the gate
lacked: it never checked that anything was removed. Its module doc now
says why this leg cannot see the workers defect and which pin does.

## 3. Byte identity — the enumerated movement

The only generated-text change is the one statement. What moved:

| Golden | Movement | How it was enumerated |
|---|---|---|
| ux canary `ux_rules_pre_feature_tree.sha256` | `11180755…` → `dc643f70…` | see below |
| forms canary `forms_pre_feature_tree.sha256` | `5b33d974…` → `5e0ae9a6…` | see below |
| review fixture `crates/review/generated-candidate/` | 1 file, 1 line: `src/webhook_dispatch.rs` | the drift pin names the file; `git diff` after the in-place regen |
| `evt_golden/dispatch_pre_evt.rs` | 2 sites, identical to the template edit | `dispatch_template_flag_off_is_byte_identical_to_pre_evt` stays green (48 `evt_events` pins) |
| `handler_matrix_tests` / `policy_rls_snapshot_tests` (8) / `machine_read_model_tests` (10) / `template_harness` (125) | **none** | tests pass unchanged |
| `ux_rules_pre_feature_ifml_tree.sha256` / `forms_pre_feature_ifml_tree.sha256` | **none** | the IFML snapshots carry no dispatcher; unchanged on disk |

**The canaries were not re-blessed blind.** Each hashes the WHOLE flag-off
tree of an init-scaffolded demo app. The same pipeline was generated twice
— once with the embedded templates, once with the pre-change
`webhook/dispatch.tera` shadowing it through the driver's template-override
directory — and compared file by file: 235 files, exactly ONE changed,
`src/webhook_dispatch.rs`, one line:

```diff
-                            "SELECT pgmq.delete($1, $2)",
+                            "SELECT pgmq.delete($1::text, $2::bigint)",
```

One line, not two: the dead-letter twin renders only with the events plane
on, and the init scaffold has none. (Throwaway comparison test, removed.)

## 4. Gates

Local, through `mbx`, `RUSTFLAGS="-C debuginfo=0"`, `DATABASE_URL` on the
Supabase image; the full matrix rides the PR's CI.

| Gate | Result |
|---|---|
| `webhook_dispatch_pgmq_tests -- --include-ignored` (3 node-free pins + extractor KAT + the runtime leg) | pass — and RED on the pre-fix templates as recorded in §2 |
| `events_runtime_tests -- --ignored` (monolith leg, shared stand-in, the two removal assertions) | pass (61 s) |
| `cargo test -p codegraph-generate --lib -- evt_events` | 48 pass (the flag-off golden is byte-identical to the template) |
| `upstream_overrides_tests` (8) / `template_harness` (125) / `machine_read_model_tests` (10) / `policy_rls_snapshot_tests` (8) / `handler_matrix_tests` / `file_length_contract` | pass |
| `ux_rules_byte_identity_tests` (4) / `forms_byte_identity_tests` (4) | pass after the §3 re-bless |
| `review_fixture_drift_tests` | pass once the regenerated fixture is committed (the pin compares against `git archive HEAD` by design) |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings -D clippy::too_many_lines` | pass |

## 5. Deferred / follow-ups

- **The whole workers drain end to end on the cornucopia leg** stays
  unpinned at runtime (the events epic's recorded deferral): this lane pins
  its pgmq statements under the real binding discipline, not its consumers.
- **`crates/codegraph/target/` is tracked** (2,114 files — the events
  runtime harness's scratch crate AND its `target/` build tree, committed
  by the gate's own runs because `.gitignore` covers only the root
  `/target`). Every run of `events_runtime_tests` or
  `machine_read_model_tests` dirties or adds files there. Untracking it is
  a hygiene change outside this fix; until then, stage by path, never with
  `git add -A`, in a checkout that has run those gates.
