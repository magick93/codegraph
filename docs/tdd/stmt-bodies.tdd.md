# TDD evidence report — `.ddd` executable statement bodies, WS-B (codegraph#662)

**Branch:** `ws/662-stmt-bodies` (off `master` @ 8cf84acf, DD9 #659 merged;
one PR closing #662)
**Runner:** `cargo test -p <crate>` per crate; workspace gates with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI); the live gates run under
`just ci-local ops` (DATABASE_URL → Postgres, hurl present).
**Upstream:** pherolang `298267e` — `phero_ir::ddd::ServiceOperation.bodies`
(the canonical `phero_expr::stmt::StmtBody` JSON under the `"stmt"` target),
`body_receiver`, `throws`; the posture is orchestration-only (one aggregate,
feature-path assignment, declared raises).

## Issue coverage

| Deliverable | Landed |
|---|---|
| 1. Ingest `bodies`/`body_receiver`/`throws` into the graph mirror + format bump | `DddServiceOperation` gains the three serde-defaulted fields (`codegraph-core/src/types/ddd.rs`); `ddd_ingest` maps them (bodies opaque); `GRAPH_FORMAT_VERSION` 6 → 7 + `graph_document_v7.md` (a warm cache would otherwise deserialize the bodies as EMPTY and silently degrade every bodied op to the 501 seam) |
| 2. Lower `StmtBody` to Rust in the action handler | `crates/codegraph-generate/src/ddd/body_lower.rs` (the closed subset) + `ExecutionKind::Body` (precedence over `save`→Update) + `templates/api/handler.tera` (`{{ plan.block }}`) |
| 3. Raise vocabulary → the generated error plane | `RaiseTable` (prelude `NotFound`/`Conflict` → `AppError::not_found`/`conflict`; a DECLARED error → its `{Domain}Error` variant's own `http_status()`/`code()`), `errors::declared_error_defs` shared with the emitter (the two planes cannot drift), utoipa 4xx docs from `BodyPlan.responses` |
| 4. Seam only when `bodies` is empty; otherwise refuse or lower | every out-of-subset construct is a named `Refusal`; the run path surfaces it through `ValidationPass::check_service_op_bodies` (a HARD pre-generation error — the DD9 `query_refusals` posture, because the domain-generator phase collects its own errors non-fatally) |
| 5. Gates | `service_ops_tests` 10, `body_lower` unit pins 11, `ddd_ingest_tests` 7, `service_ops_compile_gate` (ignored, LIVE: ran, `cargo check` clean), `service_ops_hurl_gate` (ignored, LIVE: hurl + psql proof), workspace 3325 |
| 6. Docs | this report + the AGENTS.md "Service ops lower to API surface (issue #590)" section extension |

## The closed lowering subset (documented at the site)

| `.ddd` body construct | Generated Rust |
|---|---|
| `let x = self.findById("current") else error NotFound(..)` | `let [mut] x = response.clone();` — the route's address-scoped load already 404'd |
| `let x = <feature path>` / `<literal>` / `<param>` | a typed plain local (own kind decides `.clone()`) |
| `let x = <optional feature> else error V(..)` | `let Some(x) = <read> else { return Err(<V>); };` |
| `guard <bool> else error V(..)` | `if !(<cond>) { return Err(<V>); }` |
| `local.field = <value>` | `local.field = <value>;` + `patch.insert(<DTO wire key>, json!(&local.field));` |
| `let y = self.save(local)` | the entity's update command with the accumulated patch, bound to the RELOADED `{Entity}Response` |
| `if`/`else` | plain Rust blocks (branch locals stay branch-local) |
| `return <value>` | the route's `{ "data": …, "meta": … }` envelope |
| anything else | a `Refusal` naming the op and the construct |

The response of a body that persisted is `latest.unwrap_or(response)` — the
reloaded record when a save (or the machine transition) ran, the loaded one
otherwise.

**Machine boundary (`TODO(#662)`):** a bodied op takes precedence over the
`save`→Update delegation lowering. When the op also binds a machine
transition and the body does NOT assign the machine state field, the
transition patch is applied first (#590 P0 semantics preserved). When the
body itself assigns the state field the body owns it and the inferred
transition is dropped — a comment names the boundary; unifying the two is a
recorded follow-up.

## Task report (RED → GREEN)

| Task | RED evidence | GREEN evidence | What the passing tests guarantee |
|---|---|---|---|
| T1 graph mirror + ingest + format bump | `ddd_ingest_tests::conversion_maps_module_major_order_and_every_payload` FAILED on the new asserts (fields absent) | `ok. 7` + `artifact_kit_tests ok. 2` | `bodies` ride OPAQUE (a body-less op omits the map), `body_receiver`, `throws`; the artifact format is v7 and a v6 document is refused as `format_version_too_new` at 8 |
| T2 ingest binding | the first run emitted NO route for `applyPayRun` (a body op declares no delegation ⇒ `bind_entity` saw nothing) | `service_ops_tests` route pin | `bind_entity` step 3 binds the bodied op's `=> Repository` to the schema title (capability → delegation → body receiver → signature) |
| T3 body lowering | `statement_bodies_lower_into_the_action_handler` FAILED (501 seam / no block) | `service_ops_tests ok. 10` + `body_lower ok. 11` | the whole subset renders (params, load, guard, assignment + DTO wire key, save → command plane → reload) |
| T4 raises | `declared_errors_map_onto_the_generated_error_plane` FAILED (no `{Domain}Error` arm) | `body_lower ok. 11` | prelude → 404/409 via `AppError`; a declared error → its OWN status + code (the model's `422`, `PAYMENT_DECLINED`), total over the declared vocabulary; an undeclared name is refused |
| T5 refusals (hard, named) | `unlowerable_body_constructs_fail_validation_naming_the_op` FAILED (no validation check existed) | `service_ops_tests` + `body_lower ok. 11` | arithmetic / collection algebra / non-builtin calls / nested paths / shadowing locals / cross-aggregate `findById` / non-Option let-else / assignment-without-save / no command plane / no record load are all HARD errors naming the op + construct |
| T6 hurl contract | the emitted actions contract would mis-pin a guard-carrying body (no deterministic status) | `service_ops_tests` + live gate | a body may answer 4xx ⇒ NO leg is emitted (never a wrong pin); a lowerable body leg carries its declared parameters as the action body and pins 200 |
| T7 compile pin (#551) | — | `service_ops_compile_gate ok. 1` (LIVE, `cargo check` clean) | the lowered body compiles against the real command/query planes: `UpdatePayRunRequest` spelling, `i32` parameter, `find_by_id(id, false, …)` arity |
| T8 live acceptance | the first probe run pinned the wrong leg order (the guard reads the LOADED row: `150` passes from 42, then `7` is denied from 150) | `service_ops_hurl_gate ok. 1` (LIVE) | create 201 → bodied op **200** with `$.data.total == 150` → replay **409** with `$.error.code == "CONFLICT"` + the declared message → psql proves `total = 150` persisted |
| T9 byte-identity | — | workspace 3325 passed / 0 failed / 41 ignored | a body-less model emits exactly today's handler (the `{% endif %}{% if %}` fold adds no newline), the emitted actions contract is unchanged for no-parameter ops (`{{ auth_header }}{% if op.body %}…{% endif %}`), and no `.ddd` input keeps the whole plane inert |

## Final gate sweep

- `cargo test --workspace` (RUSTFLAGS `-C debuginfo=0`) — **3325 passed, 0
  failed, 41 ignored**.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean;
  `cargo fmt --all --check` — clean.
- `cargo test -p codegraph --test service_ops_tests` — 10.
- `cargo test -p codegraph-generate --lib -- ddd::body_lower` — 11.
- `cargo test -p codegraph --test ddd_ingest_tests` — 7.
- `cargo test -p codegraph --test artifact_kit_tests` — 2.
- `cargo test -p codegraph --test service_ops_compile_gate -- --ignored
  --nocapture` — **ran** (path deps, 40s): compile gate passed.
- `cargo test -p codegraph --test service_ops_hurl_gate -- --ignored
  --nocapture` — **ran** (Postgres 127.0.0.1:15432 + hurl): green.

## Coverage and known gaps (all recorded deferrals)

- **Domain operation calls in a body** (`self.totalFor(..)`) — refused with a
  named construct; the generated plane lowers only `self.findById`/
  `self.save` (a domain-op call needs the mox op arena/`Resource`
  conventions, `TODO(#662)` at the site).
- **Collections** (assignment of a child collection, `lines.map(..)` algebra,
  `self.delete(..)` in a body) — refused by name; child-collection mutation
  stays a 501 seam on the delegation path.
- **Body-assigned machine state** — persisted through the ordinary update
  DTO; the designed machine's transitions are NOT validated against that
  assignment (`TODO(#662)`), and a workflow-managed/immutable state field
  means no inferred transition is applied at all.
- **`let … else` over `self.findById`** — the absent arm is the route's 404
  (the load precedes the body); only a 404-status raise is accepted there.
- **Class-typed / collection action parameters** — refused (`primitives
  only`); the action body carries primitives.
- **Child/aggregate bodies** — the lowering renders the scoped load
  (`find_by_id_scoped`) when the entity config says `role = "child"`; the
  fixture exercises the root shape end to end (the child plane's body leg is
  covered by the unit pins, not the live gate).
- **Doctor parity** — `codegraph doctor --ddd-files` does not yet run the body
  refusal check (the run path + validation pass do); recorded follow-up.
