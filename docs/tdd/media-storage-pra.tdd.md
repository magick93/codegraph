# TDD evidence report — Media storage plane, PR-A (codegraph#625)

**Branch:** `media-storage-plane` (off `feat/lifecycle-followups` — the memo and the
issue were written against this line; `master` predates the charts/capability-audit
precedents the design relies on)
**Runner:** `cargo test -p <crate>` per crate; workspace gates run with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI).
**Source plan:** `/home/anton/.opencode/plan/media-plane-pra.plan.md`; the design is
magick93/codegraph#625 (the reviewed memo distilled).

## User journeys (from the plan)

| # | Journey |
|---|---------|
| J1 | `[domains.X.media]` config validated strictly at generation time — invalid provider/mode combos fail before any file is written |
| J2 | No media config ⇒ zero emitted media files, byte-identical output |
| J3 | Media configured ⇒ `MediaStorage` trait + `NoopStorage` emitted; uploads fail closed until a provider is constructed |
| J4 | The emitted plane ships a compile pin so it can never regress to dormant-`media_route` state |
| J5 | Adding a provider costs one enum arm + one adapter + one matrix row |

## Task report (plan task → test target → RED → GREEN)

| Task | RED evidence | GREEN evidence | What the passing tests guarantee |
|---|---|---|---|
| M1/M2 core matrix + KeyTemplate | `cargo test -p codegraph-core --lib -- media` → `FAILED. 1 passed; 10 failed` (todo!() bodies; commit 08109d12) | `ok. 11 passed` (commit 9828c506) | `provider_capabilities` is total per provider (Supabase full set incl. `ClientDirectUpload`+`StorageSideRls`; R2/S3 lack both); `UploadMode::as_capability` mapping; `KeyTemplate` parse rejects unknown placeholders/unclosed braces/missing `{id}`+`{field}`; render rejects `/` in values |
| M3 config plane | `cargo test -p codegraph-config --lib -- media` → `FAILED. 7 passed; 10 failed` (todo validate(); commit 5bc02cee) | `ok. 17 passed`; full config suite `ok. 140 passed` (commit 00b3c213) | `deny_unknown_fields` strictness; provider/`upload_mode` closed enums (KAT-pinned string sets vs the core mirror — the ChartKind precedent); stray provider subsections are errors naming the table; r2 requires its binding; bucket/max_bytes/accept/mime-shape checks; no media ⇒ `None`, parse path byte-unchanged |
| M4 plan builder | `cargo test -p codegraph --test media_plan_tests` → `FAILED. 0 passed; 10 failed` (commit c5946337) | `ok. 10 passed` (commit 2aa001a0) | `Person.avatar` resolves to `person_avatar`/`person_id` + `url`/`mime_type` columns (via `ReferencesSchema` edges); `MediaReference` resolves through the `Type` suffix; refers-wins targets, array media fields, unresolved classes, and missing columns are `Error::Config`s naming the offender; r2+client-direct refused by the matrix at build; deterministic across runs; no media ⇒ empty map |
| M5 generator | `cargo test -p codegraph --test media_pipeline_tests` → `FAILED. 0 passed; 4 failed` (commit 0c6d4cac) | `ok. 4 passed` (commit af6c35df) | Exact file set `src/media/{mod,noop}.rs` when media is configured; trait contract pinned (formatting-agnostic): 4 operations with exact signatures; PR-A emits no adapter modules; `NoopStorage` fails closed on all four ops; determinism byte-check; **no `[media]` ⇒ zero files** (J2); matrix error ⇒ `Error::Config`, never half-emission |
| M6 compile pin | n/a — pin landed green; **teeth validated in-test** (`the_pin_rejects_a_drifted_trait_signature` assembles a mutated trait and asserts `cargo check` FAILS) | `cargo test -p codegraph --test media_compile_tests` → `ok. 3 passed` | The emitted plane compiles standalone with **only** `async-trait` + `thiserror` (provider purity — no db/runtime deps); the pin rejects a renamed trait method (drift tripwire); the string-contract line fails fast before the scratch-crate run |
| M7 doctor parity | test written after wiring (deviation noted below) | `cargo test -p codegraph --test init_tests doctor_validates_media` → ok; full init_tests `ok. 48 passed` | Valid media config ⇒ 0 hard doctor failures; r2+client-direct ⇒ 1 hard; unknown key placeholder ⇒ 1 hard — the SAME shared validator (`validate_media_matrix_and_template`) runs in doctor and the generation path, so they cannot drift |

## Test specification (guarantee table)

| # | What is guaranteed | Test file | Type | Result |
|---|---|---|---|---|
| 1 | Capability matrix is exhaustive and total per provider | `codegraph-core/src/types/media.rs::tests` | unit KAT | PASS |
| 2 | Key templates cannot collide (id+field placeholders enforced; `/` rejected) | same | unit | PASS |
| 3 | `[media]` parse strictness + provider-subsection agreement | `codegraph-config/src/config/media.rs::tests` | unit | PASS |
| 4 | Config vocabulary KAT vs core mirror (never drifts) | same | KAT | PASS |
| 5 | Graph resolution → child tables + columns via #460 naming helpers | `tests/media_plan_tests.rs` | integration (in-memory Grafeo, node-free) | PASS |
| 6 | Strict resolution refusals name the domain/entity/field | same | integration | PASS |
| 7 | Emission presence gate + trait contract + fail-closed noop | `tests/media_pipeline_tests.rs` | integration | PASS |
| 8 | #551 compile pin + teeth | `tests/media_compile_tests.rs` | compile pin | PASS |
| 9 | Doctor parity via the shared validator | `tests/init_tests.rs::doctor_validates_media_matrix_and_template` | integration | PASS |
| 10 | `media_storage` capability has a registry constructor | existing `charts_capability_audit` (maximal-context audit) | in-crate audit | PASS |
| 11 | Byte-identity: no `[media]` anywhere ⇒ unchanged trees | existing `ux_rules_byte_identity_tests` (4), `forms_byte_identity_tests` (4), `policy_rls_snapshot_tests` (7) | canaries | PASS |

## Coverage and known gaps

The repo gates on named pin suites + byte-identity canaries rather than % coverage;
every new branch/validation arm maps to a named test above (validation arms: matrix
refusal, template refusal, subsection mismatch, bounds, resolution refusals ×4 — all
covered). Intentional gaps, all recorded deferrals in #625:

- **wasm32 leg of the compile pin** — lands with the PR-C worker-binding adapter (the
  wasm claim belongs to the adapter that runs on wasm; recorded in the pin's doc
  comment per the #551 documentation rule).
- **Driver slot exercised indirectly** — `validate_media_configs` in `driver.rs` is
  compile-verified and its body is the same tested builder; a full driver-run test
  was judged heavy for PR-CI (the plan's M4 note). The doctor gate covers the same
  entry condition without a graph.
- **Adapter modules** — deliberately not emitted in PR-A (empty `adapters` context);
  they arrive with PR-B (Supabase) / PR-C (R2) and their own compile pins.
- **INFO line content** in doctor output is not asserted (the harness asserts hard
  failure counts; cmd_doctor prints to stdout, which the test harness cannot
  capture — consistent with the existing function-bindings doctor test).

## Deviations from the plan (recorded)

1. **Matrix/template enforcement lives in `codegraph-generate`, not
   `codegraph-config`** — `codegraph-core` is deliberately dev-only in the config
   crate (the charts/KAT convention), so the capability matrix and `KeyTemplate`
   checks run in `generate::media` (driver pre-generation slot + doctor) instead of
   the config parse. User-visible behavior (J1) is unchanged; the split is documented
   in `config/media.rs`'s module docs.
2. **`media_classes` added to `MediaConfig`** (default `["MediaReference"]`) — the
   config-declared marker for which contained-value classes are media (open question
   #5 resolved config-side for PR-A, keeping the DSL clean).
3. **No `ProjectConfig.media_plans` plumbing** — `GlobalGenerator::generate`
   already receives `db` + `config`, so the generator builds plans itself; the
   pre-built-plans threading from the plan is unnecessary.
4. **M7's doctor test written after the wiring** (same commit pair) — the validator
   itself was TDD'd through the plan tests; the doctor integration test landed with
   its implementation.
5. **Branch base** — `feat/lifecycle-followups` rather than `master` (master predates
   the design's precedents by 82 commits; the PR merges to `master` per trunk policy).

## Merge evidence

Checkpoint sequence on `media-storage-plane` (all reachable from HEAD):

```
08109d12 test: media provider capability matrix + key template KATs (RED)
9828c506 feat: core media matrix types + key template (GREEN)
5bc02cee test: [media] config parse + structural validation (RED)
00b3c213 feat: [media] config plane structural validation (GREEN)
c5946337 test: media plan resolution over in-memory grafeo (RED)
2aa001a0 feat: MediaPlan IR + graph builder + driver validation slot (GREEN)
0c6d4cac test: media_storage generator emission pins (RED)
af6c35df feat: media_storage global generator + trait/noop templates (GREEN)
        test: media plane compile pin (#551 policy)          [M6, green + teeth-in-test]
        feat: doctor parity + init_tests                     [M7]
        fix: media: None in exhaustive DomainEntry constructions
        style: cargo fmt
```

Final gate sweep (all green): `media_plan_tests` (10), `media_pipeline_tests` (4),
`media_compile_tests` (3), `init_tests` (48), `charts_pipeline_tests` (11),
`policy_rls_snapshot_tests` (7), `ux_rules_byte_identity_tests` (4),
`forms_byte_identity_tests` (4), `codegraph-generate --lib` (982, incl.
`charts_capability_audit`), `codegraph-config` (140), `codegraph-core --lib` (117),
`cargo clippy --workspace --all-targets -- -D warnings` clean,
`cargo fmt --check` clean.
