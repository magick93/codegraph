# TDD evidence report — aggregate child operations (codegraph#661)

**Branch:** `ws/661-child-ops` (off `master` @ c0139c26 — DD9 #659, media
e2e #633, statement bodies #662, transition binding #660 all merged)
**Runner:** `cargo test -p <crate>` per crate; workspace gates run with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI). Ignored/live gates ran with a
resident Postgres (`127.0.0.1:5432`), `hurl` 8.x and the
`wasm32-unknown-unknown` target installed.
**Issue:** magick93/codegraph#661 — "ddd: aggregate child operations
(Design.child_operations / ChildRef) replace the 501 seams".
**Upstream:** pherolang proposal 004 (shipped at the pinned rev
`298267e`): module-level `children <Root> { … => feature.add|remove|save }`
lowers onto `Design.child_operations`; each op carries
`ServiceOperation.child_scope: ChildRef { feature, class, action }`.

## Scope landed

| Layer | File |
|---|---|
| Graph mirror | `codegraph-core/src/types/ddd.rs` (`DddDesignNode.child_operations`, `DddServiceOperation.child_scope`, `DddChildRef`) |
| Ingest | `codegraph/src/ingest/ddd_ingest.rs` (`service_operation_node` — ONE conversion for service + child ops) |
| Grafeo | `schema_ddl.rs` (`DddDesign.child_operations_json`, `GRAPH_FORMAT_VERSION` 9 → 10), `ingestor/nodes_ddd.rs`, `querier/ddd.rs` |
| Lowering | `codegraph-generate/src/ddd/service_ops.rs` (`child_routes_for_entity` pass inside `routes_for_entity`, `ExecutionKind::ChildOp`, `ChildAction`, `child_execution`, improved cross-aggregate seam) |
| Templates | `templates/api/handler.tera` (child_op execution arms + utoipa paths), `templates/api/router.tera` (collection-level `add`), `templates/hurl/actions.tera` (method/url/capture/asserts legs) |
| hurl planner | `codegraph-generate/src/api/hurl_contract.rs` (`child_action_legs`, `OpLeg` extensions) |
| IFML (minimal) | `codegraph-generate/src/ifml/e2e_test/assembly.rs` (`ChildOp` legs: `remove`/`save` → Ok; `add` stays unbuilt — POM is item-addressed) |
| Validation | `codegraph-generate/src/ddd/service_ops.rs::dead_surface_warnings` (protected / unresolved-class child ops) |
| Artifact kit | `tests/fixtures/artifact_kit/graph_document_v10.md` (was `v9` — #668 renamed `v8` to `v9` first) + `artifact_kit_tests` |
| Fixture | `tests/service_ops_fixture.rs` (`CHILD_OPS_MOX/DDD/ACTOR/DOMAINS_TOML`, `write_child_ops_fixture[_with]`, `ingest_child_ops`) |

## Task report (RED → GREEN)

| Task | RED evidence | GREEN evidence | What the passing tests guarantee |
|---|---|---|---|
| T1 graph mirror + ingest | `aggregate_child_operations_ingest_with_their_child_scope` FAILED (`child_operations` did not exist on the node) | `service_ops_tests`: 1 passed | `children PayRunType` lowers onto the design in declaration order with `childScope { feature: "lines", class: "payroll.PayRunLineType", action: "add" }`; service operations carry NO child scope; the DTO parameter rides the mirror |
| T2 lowering + handler/router/hurl | `child_scoped_operations_lower_to_nested_child_action_routes` FAILED (no child action routes at all — the pre-#661 silent drop) | `service_ops_tests`: 1 passed | The child's routes fn carries `/actions/add_line` (COLLECTION level) + `/{pay_run_line_id}/actions/{save_line,remove_line}`; the parent `.nest(…)` mounts it (WS4 shape reused, no second nesting); `add` → the parent load-or-404 (`state.{domain}_pay_run_queries.find_by_id(parent_id, …)`, issue #692) THEN `commands.create(item, parent_id, …)`, `save` → `commands.update(…)`, `remove` → `commands.delete(…)`; item actions load through `find_by_id_scoped(child, parent)`; no seam for the three actions; openapi registers all three through the child's module |
| T3 the retained seam (acceptance 2) | same test, seam assertions | same | An op WITHOUT `child_scope` whose delegation targets the child's repository stays a 501 whose message names `children PayRunType`, `childScope`, and "Refusing to patch the parent row silently" |
| T4 hurl contract (acceptance 3) | same test, hurl assertions FAILED (`{{item_url_base}}` leaked into the read-back leg as a hurl variable) | same | add leg posts the create body collection-level, captures `added_add_line_id`, read-back `GET …/{added}` pins 200 + `$.data.id exists`; `save`/`remove` ride the item route; failure legs pin 422 (create DTO `deny_unknown_fields`) and 404 (unknown id under the parent); `remove` is deferred LAST (it deletes the captured row) |
| T5 fail-closed nesting | `child_operations_without_a_nested_child_declaration_emit_nothing` written with the impl | `service_ops_tests`: 1 passed | The SAME model without `role = "child"` emits zero child action routes + handlers (WARN naming the config fix) — never a top-level route that drops the parent anchor |
| T6 compiles (acceptance 4) | `service_ops_compile_gate::generated_app_with_aggregate_child_operations_compiles` (ignored) | see gate sweep | The emitted child action handlers compile against the child's real DTOs (`CreatePayRunLineBody`, `dto_update::UpdatePayRunLineRequest`) |
| T7 worker wasm | `worker_wasm_compile_tests` child-ops fixture extension | see gate sweep | The same handlers compile in the cornucopia worker crate natively AND on `wasm32-unknown-unknown` |
| T8 live HTTP (acceptance 1 + 3) | `service_ops_hurl_gate::aggregate_child_actions_serve_over_http` (ignored) | **PASS** | create → add 200 → read-back 200 → save 200 → remove 200 over real HTTP; 422/404 failure legs; a two-parent probe proving parent B cannot read/save/remove parent A's child (all 404) and A's child survives untouched; psql proof: A owns exactly the added LIVE line (`1\|2` — the auditable soft-delete keeps the removed row), B owns none (`0\|0`) |
| T9 format bump | `artifact_kit_tests::kit_cases_pass` + `kit_runner_rejects_a_failing_case` (the latter's inline `formatVersion: 9` became valid after the bump) | 2 passed | `GRAPH_FORMAT_VERSION` 10 canonical document fixed point (after the rebase onto #668's 9); a `formatVersion: 11` document is rejected with the named diagnostic |
| T10 DD3 boundary: a projection DTO must not shadow its source's entity design | `ddd::design::tests::a_projection_dto_never_shadows_the_entity_design_of_its_source` + the child handler's auditable-argument pin | **PASS** | The fixture's `dto PayRunLineDraft from PayRunLineType` projection indexes UNDER the entity design (both orderings): the op set and `auditable` survive, so the child plane keeps its `include_deleted` argument |

## Test specification (guarantee table)

| # | What is guaranteed | Test file | Result |
|---|---|---|---|
| 1 | Child ops ingest with their scope; service ops do not | `tests/service_ops_tests.rs` | PASS (16 total) |
| 2 | Nested child action routes/handlers/OpenAPI | same | PASS |
| 3 | hurl add → read-back → failure legs, remove deferred | same | PASS |
| 4 | Un-nested child emits nothing (WARN) | same | PASS |
| 5 | Retained cross-aggregate seam names the construct | same + `ddd::service_ops::tests` | PASS |
| 6 | Native compile of the child action handlers | `tests/service_ops_compile_gate.rs` (ignored) | see below |
| 7 | wasm32 compile of the child action handlers | `tests/worker_wasm_compile_tests.rs` (ignored) | see below |
| 8 | Live HTTP child actions + parent ownership | `tests/service_ops_hurl_gate.rs` (ignored) | see below |
| 9 | Graph artifact document v10 + format rejection | `tests/artifact_kit_tests.rs` | PASS (2) |
| 10 | Byte identity: no `children` block ⇒ unchanged output | `ux_rules_byte_identity_tests` (4), `forms_byte_identity_tests` (4), `policy_rls` snapshot, `template_harness` (118) | PASS |


## Rebase onto WS-G (#668, master `dedbd8ec`)

`#668` landed while this branch was open and claimed **the same format
version 9**. Both additions are independent and legitimate, so the version
moved above both: **`GRAPH_FORMAT_VERSION` 9 → 10**, artifact kit
`graph_document_v9.md` → `graph_document_v10.md`, and
`format_version_too_new` pins 11 (the rejected case + the inline kit drift
case). Only `schema_ddl.rs` conflicted (the two adjacent doc blocks — both
kept, in order) plus the fixture file, which rebased onto WS-G's rename and
was then re-renamed. Every doc reference (`AGENTS.md`, this file) now names
`v10`; historical mentions (the #660 section) keep their version narrative
but point at the current fixture.

WS-G overlap check: the child-op lowering reads the CHILD's storage identity
only through `SchemaNode.pg_table_name` (the effective name WS-G's
`table "…"` declaration introduced — the same fact that names the entity
module, the domain dir and the migration file), never a title re-derivation.
The fixture's `refers` (not `contains`) linkage keeps the child on the
entity plane under WS-G's unchanged `ClassBridge` rule, and the register /
declared-table suites (`mox_table_name_tests` 2, `module_collision_tests`
3, `mox_ingest_tests` 21) stay green alongside `service_ops_tests` (16).

## Final gate sweep (all green — `just ci-local`, 200 suites / 3388 tests)

- `just ci-local` — **exit 0** (lint + test + audit + ops). Lint = `cargo
  check --workspace`, `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets -- -D warnings` (all clean); audit = `cargo
  audit`, 4 allowed (documented) advisories, no new findings.
- `cargo test --workspace` (`RUSTFLAGS="-C debuginfo=0"`) — 200 test-result
  groups, **3388 passed, 0 failed**; includes `service_ops_tests` (16),
  `ddd_ingest_tests` (7), `artifact_kit_tests` (2), `artifact_tests` (3),
  `template_harness` (118), `ux_rules_byte_identity_tests` (4),
  `forms_byte_identity_tests` (4), `hurl_contract_tests` (2),
  `profile_smoke_tests` (33), `mox_ingest_tests` (21),
  `mox_table_name_tests` (2), `module_collision_tests` (3), the grafeo/core/
  config/ops suites, and the doc-tests.
- `cargo test -p codegraph-generate --lib` — **1073 passed** (includes the
  new `ddd::design::tests::a_projection_dto_never_shadows_the_entity_design_of_its_source`).
- `cargo test -p codegraph --test service_ops_compile_gate -- --ignored
  --nocapture` — **PASS 3/3** (61s): the aggregate-child app compiles natively
  alongside the #590 and #660 fixtures.
- `cargo test -p codegraph --test worker_wasm_compile_tests -- --ignored
  --nocapture` — **PASS 1/1** (56s): the child action handlers compile in the
  cornucopia worker crate natively AND on `wasm32-unknown-unknown`.
- `cargo test -p codegraph --test service_ops_hurl_gate -- --ignored
  --nocapture` — **PASS 2/2** (94s) over a real Postgres: the #590 contract
  (probe row `Approved`, `total = 150`) and the #661 aggregate-child contract
  + ownership probe (per-parent counts `ws661-parent-a|1|2`,
  `ws661-parent-b|0|0`).
- The ops lane's other gates also ran green in the same sweep:
  `hooks_cornucopia_runtime_tests` (1), `db_integration`, `ops_generator_tests`,
  `wrapper_compile_tests`.

## Behaviour notes

- **Route shape.** `add` is a COLLECTION-level action
  (`POST /api/{v}/{domain}/{parent}/{parent_id}/{child}/actions/{op}`)
  because the element does not exist yet; `save`/`remove` ride the issue's
  item-level shape (`…/{child_id}/actions/{op}`). Both live in the CHILD's
  router, which the parent nests — reusing the WS4 machinery (nested path,
  parent-scoped load, two-uuid `Path`, OpenAPI nesting, hurl nesting chain)
  rather than inventing a second shape.
- **Parent ownership.** Item-level actions load through the parent-scoped
  read, so an element of another parent 404s before any write (proven live
  by the two-parent probe). A collection-level `add` writes the child's
  parent back-ref from the PATH (`commands.create(item, parent_id, …)`), so
  the element can never land under a parent the route did not name. Since
  issue #692 the `add` ALSO proves the parent row exists first: the handler
  renders a parent load-or-404 (`state.{domain}_{parent_module}_queries
  .find_by_id(parent_id, …)` → `AppError::not_found`) before the create, so
  a bogus or foreign-org `parent_id` answers 404 through the RLS-scoped
  query plane — never Postgres `23503` as a 500. The plan resolves the
  parent's query slot at generation time (the query.tera
  `has_create or (has_read and not parent_ref)` rule); a parent with no
  plain by-id load, and a cross-domain parent, are NAMED generation errors
  (`child_parent_load_refusals` / the `child_op_parent_load` validation
  check) — never a handler referencing a method that does not exist.
- **Aggregate contract.** `remove` deletes the child row through the child
  plane (the aggregate's ownership pair is the child-side back-ref FK);
  `save` updates the parent-scoped row. A junction-only link (`refers T[]`
  with no back-ref column) is NOT written — recorded deferral.
- **Out of scope by design.** Contained children (mox `contains` ⇒ value
  object) have no standalone repository (upstream Rule 6): such an op emits
  no route and the WARN names the modeling limit. Grandchild generalization
  beyond the already-emitted depth-2 route is untouched.

## Deviations from the issue text (recorded)

1. **`add` is collection-level, not item-level.** The issue's illustrative
   route carries `{child_id}` for every action; `add` has no target element
   (its id is generated by the create), so an item-level segment would be a
   fabricated path parameter. The parent anchor is preserved and the route
   still nests under the parent's aggregate path.
2. **The child must be a CONFIGURED nested child** (`role = "child"` +
   `parent` + `parent_ref`). The parent ownership load, the two-uuid path
   and the scoped read all derive from that declaration; without it the op
   warns and emits nothing (never a top-level route).
3. **Child `service` label.** A child op belongs to no `.ddd` service; the
   generated route/hurl comments name the aggregate ROOT design that
   declared the `children` block.

## Known gaps (deferrals)

- Contained-VO child ops (upstream Rule 6 forbids a standalone repository).
- M:N junction link/unlink for an aggregate whose only child link is a
  junction table.
- ~~A collection-add POM/action-spec leg for the IFML plane~~ — **retired by
  issue #692**: the POM gained the `add{Child}(payload, parentId)`
  collection POST method and the action spec gained sound
  `200 then 404` legs (the parent fixture seeds first; the 404 leg posts
  under the zero-uuid parent).
- Cross-domain child entities (the lowering assumes the child's
  commands/queries live in the parent's domain crate — since #692 a
  cross-domain parent is additionally a NAMED generation error).
- Grandchild generalization beyond the depth-2 route already emitted.

## Issue #692 follow-up (collection-add parent gate + IFML legs)

`ws/692-child-add-404`: the collection-level `add` no longer writes
through a bogus/foreign parent.

- **Handler**: the `add` sub-arm renders a parent load-or-404 BEFORE the
  create (the `record_load` block's whitespace/shape). Pins:
  `child_collection_add_loads_the_parent_before_the_create`.
- **Fail closed**: `child_parent_load` (routes.rs) resolves the parent's
  query slot with the query.tera `uses_find_by_id` rule; the refusals ride
  `child_parent_load_refusals` + the `child_op_parent_load` ValidationPass
  check. Pin: `child_add_without_a_parent_by_id_load_fails_closed_naming_the_parent`
  (a parent design narrowed to `findAll` — no `create`/`read`).
- **hurl**: the add contract gains the bogus-parent leg (the collection
  URL with the zero-uuid PARENT segment, the child's create body, 404)
  via `OpLeg.full_url` + `zero_uuid_parent_base`.
- **IFML**: `ActionActKind::NotFound`, the `ActionCollectionAdd` payload
  (`build_collection_add`), the POM `add{Child}(payload, parentId)` method
  (`PomAction::collection`), and the collection-add spec arm in
  `render_action_spec` — zero raw testids, sound legs only.
- **Live gate**: the `CHILD_OWNERSHIP_PROBE` gains the bogus-parent 404
  leg and `assert_child_emitted_surface` pins the emitted contract leg.
