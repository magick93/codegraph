# 820c — events producer catalogue marks

Issue #820, slice (c). Slices (a) (the planner + refusal contract) and (b) (the
in-transaction publishing plane) are on master; this slice closes the loop by
making the generated catalogue say, per declared event, whether anything
publishes it at runtime — and on which queue.

## The locked design

1. **Gated on the `events_producer` flag.** The shipped default stays OFF, so
   `docs/events.md` is byte-identical to master (no re-bless, zero movement).
   The marks render only when the run carries a producer report.
2. **One line per event**, its own paragraph, directly after the per-event
   metadata table and before `### Payload`:

   ```
   Runtime publication: published at runtime by {Service}.{op} (arm: {arm}; queue: {queue})
   ```

   Multiple emitting operations and/or channels stay ONE line — ops joined
   `", "` in declaration order, and `arm:`/`queue:` pluralize to
   `arms:`/`queues:` with their deduped values in the plan's order. Queues are
   the plan's own `publish` list (`ProducerChannel.queue`), never re-derived.
   A declared event no operation binds:

   ```
   Runtime publication: not emitted — no modeled service operation declares this event
   ```

3. **Refusal marks are not rendered — structurally.** A flag-ON run hard-fails
   every R*/W* finding in `ValidationPass::check_event_producers` before any
   file is written, so a refused binding cannot exist in a written tree. No
   path writes a tree with refusals under the flag (checked; the whole-corpus
   generation leg in `events_producer_corpus_tests` fails closed, which is
   exactly the evidence). Refusals/warnings stay on the validation + doctor
   surfaces, where `docs/events-producer.md` is normative for both.
4. **One planner run, two consumers.** `build_generator_context` builds a
   `ProducerReport` (`producer::report_for_run`) and derives the slice-(b)
   `ProducerPlanIndex` from it. Both ride `GeneratorContext`; the events
   generator takes the report through `with_producer_report` (registry-wired).
   No second run, no graph nodes/edges/properties, no `GRAPH_FORMAT_VERSION`
   bump.

## Implementation map

| Piece | Where |
|---|---|
| `report_for_run` (replaces `plan_index_for_run`) | `codegraph-generate/src/events/producer/mod.rs` |
| `GeneratorContext.producer_report` + accessor | `codegraph-generate/src/context.rs` |
| Wiring | `codegraph-generate/src/registry.rs` (`EvtEventsGenerator::with_producer_report`) |
| The projector | `codegraph-generate/src/events/docs.rs::runtime_publication_mark` |
| Threading | `catalogue_markdown(…, Option<&ProducerReport>)`, `document_artifacts(…, report)` |
| Corpus fixture | `crates/codegraph/tests/fixtures/events_producer_corpus/{corpus.mox,corpus.ddd,corpus.evt}` |
| Corpus gate + shipped-default pin | `crates/codegraph/tests/events_producer_corpus_tests.rs` |
| Live gate assertion | `crates/codegraph/tests/events_producer_live_gate.rs` (step 1b) |
| Pure projection KATs | `events::docs::producer_mark_tests` |

## Corpus coverage

| Shape | Event | Operation | First blocker / result |
|---|---|---|---|
| A clean record payload | `ShipmentBooked` | `ShipmentService.book` (+ `amend` in the clean subset) | plans → published mark |
| B clock + states + context | `JobStarted` | `JobService.start` (transition) | plans on a transition arm |
| R1 near miss (`_code`) | `ShipmentStatusRecorded` | `recordStatus` | `nearest: \`status\`` |
| R1 no candidate | `ShipmentMystery` | `recordMystery` | `nearest: (none within edit distance 2)` |
| R2 nullable source | `ShipmentNoted` | `recordNote` | `\`ShipmentResponse.note\` is nullable` |
| R3 contained-child-only | `ParcelWeighed` | `recordWeight` | `contained collection \`Shipment.parcels\`` |
| R7 type mismatch | `JobMislabelled` | `mislabel` | `String` vs `i32` |
| R9 no machine | `ShipmentStateGuessed` | `guessState` (patch) | `previousState` has no model source |
| R4 no runtime site | `ShipmentArchived` | `archive` (`protected`) | `(protected)` |
| R4 no runtime site | `ShipmentBrowsed` | `browse` (`findAll`) | `(abstract)` seam |
| pluralization | `JobTouched` | `touch` + `retouch` | one line, ops joined |

## Gates (all green)

```
cargo test -p codegraph-generate --lib -- events::producer evt_events      91 passed
cargo test -p codegraph --test events_producer_tests                        7 passed
cargo test -p codegraph --test events_producer_corpus_tests                 5 passed
cargo test -p codegraph --test evt_pipeline_tests                           5 passed
cargo test -p codegraph --test evt_workers_tests                            2 passed
cargo test -p codegraph --test evt_golden_tests                             2 passed
cargo test -p codegraph --test evt_cloudevents_tests                        2 passed
cargo test -p codegraph --test evt_idempotency_tests                        2 passed
cargo test -p codegraph --test ux_rules_byte_identity_tests                 4 passed
cargo test -p codegraph --test forms_byte_identity_tests                    4 passed
cargo test -p codegraph --test policy_rls_snapshot_tests                    8 passed
cargo test -p codegraph --test policy_rls_tests                            17 passed
cargo test -p codegraph --test file_length_contract                         2 passed
cargo test -p codegraph --test handler_matrix_tests                         1 passed
cargo test -p codegraph --test review_fixture_drift_tests                   9 passed (1 ignored by design)
cargo test -p codegraph --test events_runtime_tests -- --ignored            1 passed
cargo test -p codegraph --test events_producer_live_gate -- --ignored       1 passed
```

`events_runtime_tests` ran against `DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54340/postgres`
(the workspace's Supabase postgres; nothing listens on `:5432`). The live
producer gate ran against the same target (docker for the pgmq path is on the
host; the scratch DB is created and dropped by the gate) and printed
`gate: catalogue marks — both produced events name their queue` before its
publish/rollback/replay legs.

## Mutation proofs (run, red recorded, restored, green re-run)

### 1. Drop the mark emission

Patch `events/docs.rs::catalogue_markdown` so the mark is never written (the
`if let Some(mark) = runtime_publication_mark(…)` block removed), leaving the
`events_producer_corpus_tests` clean-subset leg as the detector:

```
thread 'clean_subset_publishes_and_the_catalogue_carries_the_exact_marks'
panicked at events_producer_corpus_tests.rs:446:
catalogue missing the mark `Runtime publication: published at runtime by
ShipmentService.book, ShipmentService.amend (arm: patch; queues: events_audit,
events_shipments)`:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 3 filtered out
```

Restored → `test result: ok. 4 passed`. The corpus leg is the drift pin between
the planner plane and the catalogue plane; a mark that stops rendering (or a
queue name that stops coming from the plan) fails it by name.

### 2. Flip the shipped flag on

`profiles.toml` line 45 `events_producer = false` → `true` (the product default
flip this slice must never do).

**Finding, recorded honestly:** the in-test flag-off pins
(`events_producer_tests`, the evt suites) do NOT read the shipped
`profiles.toml` — they build their own profile files, so flipping the shipped
default leaves them green (measured: 7 passed with the flag flipped). The repo
had no pin on the shipped default itself. This slice adds one:
`events_producer_corpus_tests::shipped_profiles_keep_the_producer_flag_off`
reads the workspace `profiles.toml` and asserts every `events_producer` line is
`false` — the byte-identity premise of the whole slice (a shipped flip would
put the publishing seam and the marks into every consumer's tree).

With the flag flipped that pin is the detector:

```
thread 'shipped_profiles_keep_the_producer_flag_off' panicked at
events_producer_corpus_tests.rs:602:
assertion `left == right` failed: the shipped default must keep events_producer OFF
  left: "events_producer = true"
 right: "events_producer = false"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 4 filtered out
```

Restored (`git diff profiles.toml` empty) → `test result: ok. 5 passed`.

## Byte movement

**None for flag-OFF output.** Every canary/golden outside a flag-ON fixture is
untouched (`ux_rules_byte_identity_tests`, `forms_byte_identity_tests`,
`policy_rls_*`, `evt_*`, `events_producer_tests`). The projection is
`Option`-gated: no report ⇒ no line, no writer call, no template change.

Inside a flag-ON tree the delta is enumerated by the corpus leg: stripping the
`Runtime publication:` lines (and the blank line each introduced) from the
flag-ON `docs/events.md` reproduces the flag-OFF document byte-for-byte.
