# TDD — the events producer binding planner (issue #820 slice a)

Status: **LANDED** on `feat/820a-events-producer-planner` (PR pending).

## 0. The defect, verified

`.ddd` service operations declare `emits <Event>` and the events plane has no
producer arm: on master `op.emits` is read in exactly two places
(`events/docs.rs`'s catalogue "Emitted by" table and the ingest mirror). A
consumer's 10-domain survey (issue #820) found 40 events / 55 emitting ops:
only 4 payloads are fillable by field name, 5 more with simple context, and
31 need a model change. Every one of the 31 failed silently — the contract
compiled and nothing reported the unfillable binding.

The refusal contract is normative and already on master
(`docs/events-producer.md`, `598e90400`). This slice implements the planner
it describes, with the flag OFF: zero diagnostics and byte-identical output,
while `codegraph doctor --ddd-files … --evt-files …` prints the same lines as
WARN so a consumer can align names first.

## 1. Design

- **Flag** `events_producer` (strict bool, default false): `BuildPlan` →
  `ProjectConfig.codegen` (threaded) → validation. Plan-less runs validate
  with the flag off (the #663 posture).
- **Pure planner** (`events/producer/planner.rs`): `ProducerInputs` in,
  `ProducerReport` out (publishable `ProducerPlan`s, refusal lines, warning
  lines). One formatter per sentence (`events/producer/messages.rs`) — the doc
  is the source; the run path prints `events producer: {body}`, doctor prints
  `WARN events producer — {body}`.
- **Source abstraction** (`ProducerSource`): `GraphProducerSource` (run path
  + doctor) assembles the facts from the graph; `StaticProducerSource` is the
  KAT/future-compiled-source seam. The planner never touches a graph, and the
  source is injectable/configurable.
- **One naming rule**: the `{Entity}Response` surface is projected from
  `build_dto_context` (the DTO plane's own `resolve_dto_fields`), never
  re-derived.
- **Matching**: exact snake equality binds (camel payload against snake DTO
  works); a `_code`-stripped or edit-distance-only spelling is an R1 NEAR MISS
  with a deterministic candidate (snake equality → `strip_code_suffix_safe`
  equality → prefix-normalised Levenshtein ≤ 2, ties lexicographic), never a
  silent bind.
- **R9** = no `{Entity}Response` surface at all (op unbound / schema missing)
  or an unavailable context fact; reported once per pair naming its first
  payload field. Otherwise an unmatched field is R1 (incl. the none clause).
- **R8** uses the event's inferred contract domain (`EventDefinition.domain`:
  the first payload field whose type resolves to a schema) against the op's
  module — the only contract-domain signal in the current pin; an
  all-primitive event never refuses.
- **Context facts** (`§Context bindings`): `occurredOn`, `previousState` /
  `fromState`, `nextState` / `toState` (transition arms only),
  `correlationId`, `organizationId`. A record match always wins, with W3; a
  context type disagreement is R10.
- **Arms**: transition and plain patch publish; create-shaped / delete /
  read-only / seam / protected / child-bound are R4 with a named reason.
- **Write sets**: W2 uses the op's OWN writes (params, body assignments, the
  machine state) on a transition and the Update-DTO writable set on a plain
  patch (the free-form client patch body never counts). R6 refuses a
  `…Changed` fact whose anchor the plain patch cannot write (excluded from the
  Update DTO).
- **Doctor**: compiles the same sources into a scratch Grafeo in-memory graph
  on a scoped worker thread with its own current-thread runtime (doctor is
  sync; main already runs a runtime), then runs the same planner. Presence
  gated on a real `emits` → declared-event join; an analysis failure is one
  INFO line and never fails.
- **No graph shape change**: no new node/edge/property; `GRAPH_FORMAT_VERSION`
  untouched.
- **BodyPlan** gains additive `assigned_fields` (the W2/R6 write set for
  bodied ops); `EventArchitecture::declared_events` is extracted so the
  planner and the catalogue share one event read.

## 2. Gates

| Gate | Command | Pins |
|---|---|---|
| Sentence KATs | `cargo test -p codegraph-generate --lib -- events::producer` | one exact assertion per R1–R10 + W1–W3, nearest variants (snake / `_code` / edit-distance / none), supported conversions, first-blocker ordering |
| End-to-end | `cargo test -p codegraph --test events_producer_tests` | the fixture's exact refusal sentences through mox+ddd+evt ingest; flag-on hard errors; flag-off zero diagnostics + a successful plan-less run; the doctor WARN lines + exit 0; the flag-inert generation pin |
| Profile parse | `cargo test -p codegraph-generate --lib -- profile::tests` | strict bool, default off |
| Byte identity | `evt_pipeline_tests`, `evt_workers_tests`, `ux_rules_byte_identity_tests`, `policy_rls_tests` | no output movement |
| Doctor regression | `init_tests`, `graph_analytics_tests` | other doctor checks unaffected |
| Fixtures compile | `pherolang_fixture_sweep` | the new `.mox`/`.ddd`/`.evt` fixtures stay compiler-legal |
| Budgets | `file_length_contract` | the planner split stays under 2,000 lines |

## 3. Mutation teeth (run, red, restore)

**M1 — drop the `_code` projection.** Removed the `_code`-stripped candidate
step from `nearest_candidate` and ran the R1 KAT:

```
assertion `left == right` failed
  left: "… payload field `workerTypeCode` … nearest: (none within edit distance 2) …"
 right: "… payload field `workerTypeCode` … nearest: `worker_type` — …"
test events::producer::tests::r1_code_stripped_nearest_candidate ... FAILED
```

Restored from the pre-mutation copy; the KAT is green.

**M2 — make the validator ignore the flag.** Removed the
`if !events_producer { return Vec::new(); }` gate (keeping the parameter
unused) and ran the flag-off pins:

```
ERR (events producer) [events_producer]: events producer: event `OrderStatusCoded` …
thread 'flag_off_plan_less_run_succeeds_over_a_model_with_refusals' panicked:
  flag-off run must not fail on producer refusals: Validation("9 validation errors found")
failures:
  flag_off_contributes_zero_events_producer_diagnostics
  flag_off_plan_less_run_succeeds_over_a_model_with_refusals
```

Restored; both pins green.

## 4. Verification log

```
cargo test -p codegraph-generate --lib -- events::producer   # 31 passed
cargo test -p codegraph-generate --lib -- profile:: project_config  # 52 passed
cargo test -p codegraph --test events_producer_tests          # 6 passed
cargo test -p codegraph --test evt_pipeline_tests --test evt_workers_tests \
  --test profile_smoke_tests --test file_length_contract \
  --test wrapper_drift_tests --test ux_rules_byte_identity_tests   # all passed
cargo test -p codegraph --test policy_rls_tests --test pherolang_fixture_sweep \
  --test graph_analytics_tests                               # 27 + 1 + all passed
cargo test -p codegraph --test init_tests                    # 54 passed
cargo clippy -p codegraph-generate -p codegraph --all-targets -- -D warnings  # clean
cargo fmt --check                                            # clean
```

Byte movement: **none** in generated output. Doctor stdout gains
`WARN events producer — …` lines only when `--ddd-files` + `--evt-files`
carry an `emits` → declared-event join (the new fixture exercises it); every
existing doctor fixture has no such join and is unchanged.

## 5. Deferred (slice b/c)

Publishing (the transaction seam, typed construction, the rollback live
gate), the catalogue's `published at runtime` / `not emitted (<refusal>)`
marks, and any `doctor` CLI source knob (the `ProducerSource` trait is the
seam). Context-bound publish construction is slice (b)'s.
