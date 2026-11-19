# WS-H — the honest ops CI lane (issues #743, #708, #709)

One PR, three slices, one root cause: **the `test-ops-integration` lane
proved less than it said.** The job was green while the band it exists to
protect was never applied — and, once the skips were converted to real
coverage, the lane surfaced two deeper layers of the same disease. This
report covers all three slices and the defect the workstream filed.

## Premise verification (baseline, BEFORE any change)

Against a plain `postgres:15` container (the CI service image), the seven
`db_integration` tests report:

```
skipping: target lacks Supabase-compatible auth schema / pgmq / pg_tle / http
test migrations_apply_full_generated_set ... ok
skipping: target lacks Supabase-compatible auth schema / pgmq / pg_tle / http
test phased_and_file_order_application_match ... ok
test result: ok. 7 passed; 0 failed; ...
```

Two of seven — exactly the pair that exercises the full generated platform
band (basejump + pgmq + extensions) — silently skip. The same self-skip
lives in `idempotency_tests.rs`. Green lane, zero band coverage.

## Slice 1 — #743 (pin the Supabase CLI)

- One `SUPABASE_CLI_VERSION` in `codegraph-ops::supabase`;
  `npx_supabase_args` builds `--yes supabase@3.2.0 …` for every spawn;
  `workers_live_gate.rs` imports the shared constant (its existing
  workflow-side lockstep pin now holds the pairing against the ops
  constant — one literal, two consumers, zero drift surface).
- Teeth (node-free, `supabase_pin_tests`): zero bare `&["supabase", …]`
  sequences under `src/suites/`; the two spawn sites reference the helper;
  the gate file must not redeclare the literal. Red-first: all three
  failed on master naming the files.
- Acceptance: a consumer with the stack down ran `testkit api --workers`
  (throwaway manifest, dead health_url) — the log names the pinned spec
  twice: `Starting Supabase (npx --yes supabase@3.2.0 start)...` and the
  streamed `[supabase] $ npx --yes supabase@3.2.0 start`.
- Prose updated to the pinned spec: `doctor.rs` (both hints, built from
  the constant), `e2e.rs` doc comments, `docs/ops-testing.md` stage table.

## Slice 2 — #708 (Supabase-image service + fail-closed skips)

- ci.yml's service is now `public.ecr.aws/supabase/postgres:17.6.1.132`
  (the image ifml-gate.yml / entity-gate.yml / `test_framework/postgres.rs`
  already run). The ubuntu-24.04 pin and its libxml2 rationale are
  untouched. The job exports `CODEGRAPH_REQUIRE_SUPABASE: "1"`.
- ONE shared probe: `codegraph_ops::supabase::compatibility_gate` returns
  `Run | Skip{reason} | Fail{reason}`; under the env gate a missing
  capability FAILS naming the missing list and the image; unset, the
  historical skip. The private `supabase_compatible` predicates in
  `db_integration.rs` and `idempotency_tests.rs` are retired — three
  consumers, one predicate (the third consumer is slice 3's shape B).
- The justfile `ops-db-integration` recipe mirrors the gate
  probe-conditionally: band target ⇒ exports the env (CI-identical);
  plain target ⇒ loud WARN naming the skips and the image.
- Contract pins (`ops_ci_lane_contract`, node-free): no bare
  `postgres:<n>` service; the export present; both band-test files route
  through `codegraph_ops::supabase`; the justfile recipe mirrors the env.
- **AFTER**: the same seven tests against the Supabase image —
  `7 passed; 0 failed`, zero `skipping:` lines; the band actually applies
  (10 migration files, basejump tenancy, app_user DML sweep) and the
  phased-vs-file-order parity holds.
- **Teeth, validated and recorded**: env set + plain postgres ⇒ both band
  tests panic naming `auth schema, pgmq, pg_tle, http` + the image;
  reintroducing `postgres:15` fails the contract pin naming it.
- Byte-identity: no templates touched — the three canaries are unmoved
  (verified by the workspace test leg).
- **Layer-2 find (the skip was hiding a stale path)**: the band tests'
  basejump fixture path still pointed at the pre-`codegraph-generate`
  extraction location (`../codegraph/static/...`); the tests have been
  unrunnable even on a compatible target since that crate moved. Fixed to
  `../codegraph-generate/static/...`. The premise — the lane proves less
  than it says — held one level deeper than the issue claimed.

## Slice 3 — #709 (the real `testkit api` suite in CI), shape B

- **Shape B landed in `run_api`**: probe `config.api_db` with the shared
  gate; `Run` ⇒ log "Supabase-compatible Postgres already answering at
  {host}:{port} — skipping stack startup (shape B)" and proceed — no
  `supabase start` spawn, and **no manifest `[[supabase]]` section
  needed** in that mode (there is no stack to start; the `supabase_dir`
  requirement now guards only the ensure-up fallback). Non-compatible ⇒
  today's path byte-for-byte.
- **The lane was built and driven for real** against a local Supabase
  container (the review fixture, regenerated with CURRENT generators via
  the shared `review_api_suite_regen` regen — the committed fixture tree
  is stale and does not compile, tracked as #747): stage 2 Database fully
  green (132 migration files, 67 domain tables, app_user DML on 6
  schemas, owner + org-B + limited API keys provisioned), then **stage 3
  Server failed: the generated app cannot boot over the band.**
- **Root cause — filed as #756**: the CLI-less boot migrator re-applies
  every migration file (its `seaql_migrations` tracking sees nothing of
  the harness band), and the #700 `set_org_id_from_session` BEFORE INSERT
  trigger rewrites the re-applied zero-tenant seeds to
  `nullif(current_setting('app.organization_id', true), '')::uuid` — NULL
  on a NOT NULL column. Minimal psql repro in the issue. The
  `server.tera` boot comment literally predicts it for the CLI case
  ("…and die on the first seeded codelist"); the CLI-less fallback kept
  the failure mode. Cross-check: **`review_api_suite` — master's own live
  acceptance gate — is red on current generators** with the same boot
  failure. No existing gate boots the app over a banded DB, which is why
  nothing caught it.
- **Disposition (the brief's template STOP condition)**: the fix is a
  generated-output change (tracking-table sharing, boot-time session org,
  ON CONFLICT seeds, or trigger pass-through — the issue lists the
  directions; byte-identity canaries + fixture regen ride it). No
  templates were touched here. The CI wiring steps + the justfile recipe
  are **deferred to the #756 fix** (a lane that cannot go green must not
  ship); the contract pin
  `the_testkit_api_lane_is_wired_into_ci_and_mirrored_in_the_justfile`
  is ignore-marked naming #756; the `review_fixture_regen` helper is
  dropped (the fix PR re-adds what its lane needs). **#709 stays open** —
  this PR closes #743 and #708 and hands #709 its shape-B prerequisite.
- **GoTrue/PostgREST caveat (recorded in `docs/ops-testing.md` §7)**: the
  image does not provide them and the suite's core legs never needed them
  — migrate, provisioning, and the boot attempt all ran API-key-only
  against axum. They are shadowed, not exercised; the persona/auth legs
  remain the consumer `testkit e2e` lane's job. The literal STOP condition
  (core legs requiring GoTrue/PostgREST) did NOT fire; the template STOP
  condition did.

## Gate ledger (whole-PR)

| Gate | Result |
|---|---|
| `cargo test -p codegraph-ops --test supabase_pin_tests` | 3/3 |
| `cargo test -p codegraph-ops --lib supabase::` | 7/7 (pin KATs + probe parser + message builders + env gate) |
| `cargo test -p codegraph --test ops_ci_lane_contract` | 4/4 (+1 ignored naming #756) |
| `cargo test -p codegraph --test ci_local_contract` | 4/4 (drift contract green with the slice-2 env/image changes; slice-3 steps removed with the recipe) |
| `just ops-db-integration` vs plain postgres | loud WARN + historical skips (7 passed) |
| `just ops-db-integration` vs Supabase image | 7 passed, zero skips, gate exported |
| `just ci-local` | see the PR's final gate run |
| canaries | unmoved (no templates touched) |

## En-route finds on the image (the lane earns its keep)

1. **A Postgres backend segfault in the Supabase image (#757)**:
   `GRANT <role> TO CURRENT_USER` — three lines of psql, zero codegraph
   SQL — crashes the backend deterministically; the instance
   crash-recovers for ~30-60s. The hooks-cornucopia runtime test's
   harness DDL was the one lane surface using that spelling (generated
   SQL never does); it now names the role explicitly (#757 has the repro
   and the image-bump re-test note).
2. **Two lint-strictness fixes**: the shared-gate swap orphaned a
   `psql_query` import (`-D unused-imports`), and the workers-live-gate
   const swap left a dangling doc block (`empty_line_after_doc_comments`).
   Both caught by the lane's own `lint` leg within minutes — the
   discipline working as designed.
