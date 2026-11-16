# TDD evidence report — DD9 typed repository queries (codegraph#659)

**Branch:** `ddd-typed-queries` (off `master` @ 8921b36f, epic #654 / P5-1)
**Runner:** `cargo test -p <crate>` per crate; workspace gates with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI).
**Source plan:** `docs/pherolang-upgrade.md` §3 / P5-1 (marked LANDED by this
branch); the issue is magick93/codegraph#659.

> Reconstructed at session close from the landed branch: the work was
> committed as a squashed `feat:` plus a docs follow-up, so the RED commits
> are not separately archived. Every row below was **re-executed on the
> branch head**, and the gate's teeth were re-validated by mutation (see
> "Teeth validation").

## Issue coverage

| #659 acceptance criterion | Where it is proven |
|---|---|
| `where … sort … page 20/100` generates a parameterized method on both providers, compiles, returns rows in declared order | `ddd_query_tests::library_query_operations_lower_to_parameterized_repository_methods`; `ddd::design_generation_tests::query_emitted_in_trait_and_seaorm_impl_with_parameters_order_and_page`; `repository_compile_tests::design_finder_emission_compiles` |
| A non-lowerable construct / bad sort field / `limit > max` fails generation with a **named** rejection | `ddd_query_tests::the_refusal_message_names_the_repository_operation_and_construct`; the 31-case predicate matrix in `ddd::repository_query` (one refusal per construct) |
| `readOnly` honoured: no mutation, no machine advance, no emitted event | `readOnly: false` is a refusal (a hand-written artifact); query-carrying ops never enter `op_transitions` or the command plane — `query_refusals` runs pre-generation |
| No SQL ever contains an interpolated parameter value | `ddd_query_tests`: "predicate source never interpolated" assertion over the emitted SQL; every literal binds as `:_lit0` |
| A repo with no queries is byte-identical to pre-DD9 | `ddd_query_tests::a_query_less_design_emits_no_query_surface` + the three standing canaries |

## Task report (RED → GREEN)

| Task | RED evidence | GREEN evidence (branch head) | What the passing tests guarantee |
|---|---|---|---|
| T1 mirror + ingest | `ddd_ingest_tests` shape assertion written against the absent `query_json`/`capabilities_json` | `ddd_ingest_tests` ok (+54 lines) | `query`/`capabilities` ride `DddRepositoryOperation` surfaces as serde-defaulted JSON; old payloads deserialize to `None`/`[]` |
| T2 predicate lowerer | `ddd::repository_query` matrix written first (operator, reversal, negation push-down, null/bool/date literals, determinism) | `cargo test -p codegraph-generate --lib -- ddd::repository_query ddd::design_generation` → **55 passed**, 0 failed | Closed subset lowers to `SeaQuery`/cornucopia condition trees; every value is a **bind** with an explicit cast; every out-of-subset construct is a named refusal |
| T3 pre-generation refusal | `a_non_lowerable_query_fails_the_run_before_any_entity_is_generated` | ok (4-test file) | A run never half-writes a tree on a refused query; the message names repository + operation + construct |
| T4 sort/pagination | `query_emitted_in_trait_and_seaorm_impl_with_parameters_order_and_page`, `query_without_a_page_has_no_runtime_page_parameters` | both ok | `ORDER BY` in declaration order + `, "id" ASC` tiebreaker; `limit` defaults to `page limit` and clamps at `max` |
| T5 both providers | `cornucopia_query_sql_and_adapter_render_the_same_plan`, `cornucopia_query_literals_bind_through_distinct_temporaries`, `finder_mirrored_on_cornucopia_query_and_adapter` | all ok | SeaORM `Condition` builder and annotated cornucopia SQL resolve from one `QueryPlan` — they cannot drift |
| T6 compile pin (#551) | `design_finder_emission_compiles` — first run failed on a missing `QuerySelect` import | `repository_compile_tests` → **3 passed** (51.7s, scratch crate `cargo check`) | The emitted query methods actually compile; a shape the string pins cannot see fails here |
| T7 wasm leg | — (compile-only leg) | `worker_wasm_compile_tests -- --ignored` (CI `test-ops-integration`, green on this head) | The cornucopia query plane compiles on native **and** `wasm32-unknown-unknown` |
| T8 byte identity | canaries run against the query-less fixture tree | `ux_rules_byte_identity_tests`, `forms_byte_identity_tests`, `policy_rls_snapshot_tests` — unchanged (no re-bless in this PR) | Query-less models emit exactly the pre-DD9 tree; the golden re-bless is confined to `library.ddd.json` (+96 lines, enumerated in the PR body) |
| T9 graph format bump | `artifact_kit_tests` v5→v6 fixture rename + `format_version_too_new` 6→7 | `artifact_kit_tests` ok | A warm `.graph-cache/` from pre-DD9 cold-re-ingests instead of silently degrading queries to the equality heuristic |

## Test specification (guarantee table)

| # | What is guaranteed | Test file | Result |
|---|---|---|---|
| 1 | Query ops lower end-to-end from `.ddd` to emitted repository methods | `crates/codegraph/tests/ddd_query_tests.rs` | PASS (4) |
| 2 | Predicate matrix: operators, negation, literals, determinism, SQL + SeaORM rendering, one refusal per construct | `codegraph-generate/src/ddd/repository_query.rs::tests` | PASS (31, in the 55 above) |
| 3 | Trait + impl emission, page window, `0..1`, param collision, refusal | `codegraph-generate/src/ddd/design_generation_tests.rs` | PASS (+7 in the 55 above) |
| 4 | Emitted SeaORM repository compiles (incl. design finders) | `crates/codegraph/tests/repository_compile_tests.rs` | PASS (3) |
| 5 | Graph mirror round-trip for `query`/`capabilities` | `crates/codegraph/tests/ddd_ingest_tests.rs` | PASS |
| 6 | Golden conformance (pherolang `library` fixture) | `crates/codegraph/tests/ddd_golden_tests.rs` | PASS (re-blessed, enumerated) |
| 7 | End-to-end pipeline + config-vs-design byte equality | `crates/codegraph/tests/ddd_pipeline_tests.rs` | PASS |
| 8 | Upstream fixture sweep compiles through the production front doors | `crates/codegraph/tests/pherolang_fixture_sweep.rs` | PASS |
| 9 | Byte-identity canaries (entity / forms / policy-RLS) | the three canary suites | PASS, no re-bless |
| 10 | Artifact format v6 | `crates/codegraph/tests/artifact_kit_tests.rs` | PASS |

## Teeth validation (mutation, at session close)

The refusal gate is the load-bearing half of the issue ("never a silently
broadened query"), so its teeth were re-validated rather than asserted:

1. Neutralised the pre-generation refusal path so a non-lowerable predicate
   would be dropped instead of rejected.
2. `cargo test -p codegraph --test ddd_query_tests` → **FAILED**
   (`a_non_lowerable_query_fails_the_run_before_any_entity_is_generated`,
   `the_refusal_message_names_the_repository_operation_and_construct`),
   confirming a silent broadening cannot pass CI.
3. Reverted; the same suite returns **4 passed**.

## Final gate sweep

On the branch head (and mirrored by CI, all four jobs green):

- `cargo test -p codegraph --test ddd_query_tests` — 4 passed.
- `cargo test -p codegraph-generate --lib -- ddd::repository_query ddd::design_generation` — 55 passed.
- `cargo test -p codegraph --test repository_compile_tests` — 3 passed.
- Full matrix: `lint`, `test`, `audit`, `test-ops-integration` — SUCCESS on
  `7d38b091`.

## Coverage and known gaps (all recorded deferrals)

- **`page cursor`** lowers `LIMIT/OFFSET` with a one-time WARN; keyset
  pagination on the sort keys + identity is the follow-up (noted on the issue).
- **A generated HTTP route** carrying the query's page — the repository plane
  lands first; the HTTP 400-on-out-of-range contract belongs to that route.
- **Runtime `capabilities` enforcement** — needs the route; the
  actor/RLS plane owns enforcement today, and the capability is documented on
  the generated method.
- **Named refusals, not lowering**: class-typed parameter navigation,
  DTO-boundary returns, and a bare boolean *parameter* as the whole predicate.
- **`lower_finder`/EXISTS retained** for query-less declared operations
  (`findByTitle(String title)`), which consumer models still declare. The
  module docs and `FinderKind::Declared` mark that path DEPRECATED.
