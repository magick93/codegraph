# TDD evidence report — mox explicit table names, register classes, compiler word tables, WS-G (codegraph#668)

**Branch:** `ws/668-mox-tables` (off `master` @ c0139c26; one PR closing #668)
**Runner:** `cargo test -p <crate>`; workspace gates with
`RUSTFLAGS="-C debuginfo=0"` (mirrors CI).
**Upstream:** pherolang `298267e` — `phero_ir::ClassDef.table_name`
(`table "…"`, #177), `phero_ir::ClassKind::Register` (`register`, #181) and
`phero_syntax::words` (the public word tables). All three landed in 0.3 and
were unclaimed by this repo.

## Issue coverage

| Deliverable | Landed |
|---|---|
| (a) **Table name** — ingest `table_name`, authoritative on every plane | `crates/codegraph/src/ingest/mox_ingest.rs::class_table_name` writes the declared spelling into `SchemaNode.pg_table_name` — the ONE field the DDL (`db/ddl/query.rs`), the SeaORM entity (`db/entity.rs`), the repository emitter (`ddd/repository_emitter/**`), the filters (`filter_fields.rs`), the hurl/openapi naming (`api/hurl_contract.rs`, `api/openapi.rs`) and the composition child-table derivation (`child_table_name(parent, …)` call sites) all read. Nothing re-derives the name, so the planes cannot drift. `SchemaNode::declares_table_name`/`derived_table_name` give the graph a typed way to tell a declaration from a derived default. |
| (a) **Collision is a hard error naming both classes** | `consistency::check_entity_module_collisions` gains a DECLARED-table message: the table key now reports `…classes `Ledger`, `PayRunType` all resolve to pg table `ledger` — `PayRunType` declares it with `table "ledger"`…`. Upstream E0301 covers the same-PACKAGE pair; this gate covers what the compiler cannot see (cross-package, one domain) and names the declaration that must change. |
| (b) **Register classes off the entity plane** | `register X { … }` bridges onto the reference-data (codelist/seed-table) plane: a `CodeList` node + a `codelist`-classified `SchemaNode` carrying `mox_class_kind = "register"` (`MOX_CLASS_KIND`). `ordering::compute_generation_order` skips register schemas, which is what structurally removes the entity module / table / repository / handlers / routes / UI pages. The `common.{table}` seed migration and the (zero-variant, `empty_enum.tera`) codelist Rust enum are emitted by the existing codelist generators. Declared attributes still ingest as properties — the model graph stays non-lossy. |
| (c) **migrate keyword tables** | `crates/codegraph/src/migrate.rs` deletes its hand-copied `KEYWORDS` and escapes from `phero_syntax::words::KEYWORDS`; `MOX_CONTEXTUAL` words stay unescaped (`register` the canonical case). `phero-syntax` is a workspace dependency at the same rev as the other `phero-*` crates. |
| (d) **Descriptions/constraints audit** | Fixture `descriptions_and_value_constraints_ingest_without_loss` pins class + feature descriptions, the enum (CodeList + schema) description, literal labels and all five value constraints (pattern/minLength/maxLength/minimum/maximum) as non-lossy. `unique` and literal doc-comment descriptions are an explicitly recorded GAP — their mirrors (`PropertyNode::unique`, `EnumValue::description`) do not exist; see "Deferrals". |
| Format bump | `GRAPH_FORMAT_VERSION` 8 → 9 + artifact kit `graph_document_v8.md` → `v9` (`newer_format_version_is_rejected` now pins 10). A pre-#668 binary ingested the DERIVED table and treated a register as an entity; a warm cache would keep serving exactly those wrong facts with no error. |

## The register decision (and why the alternative is not reachable in scope)

Options weighed:

1. **Codelist/seed-table plane (chosen).** codegraph already owns a
   reference-data plane: `CodeList` nodes → one `common.{table}` seed
   migration + a codelist Rust enum, with `empty_enum.tera` covering the
   zero-variant shape. A register is exactly that shape in the model (flat
   noun space, no relations), so no new plane is invented.
2. **Relation-free entity with no CRUD surface.** This is what upstream's
   own prose suggests ("an org/role noun space the generated app can list,
   join, and reference"), and it is what we would build if the API plane were
   in scope — but "no CRUD" is decided in
   `api/api_model.rs::resolve_entity_operations` and consumed by
   `api/{router,handler}.rs`, `ddd/{command,query,dto}.rs` and the UI
   templates: all out of WS-G's scope. Shipping half of it (a table with
   handlers pointing at nothing) is the #449 gating-invariant failure mode.
   Recorded as the follow-up that would let registers keep their attributes
   as columns.
3. **Register stays an entity (status quo).** Rejected: the issue's whole
   point is that a register is silently an entity with a CRUD surface.

Consequences, documented in `mox_ingest.rs` and AGENTS.md: a register has a
`common.role` seed table (consumer-seeded rows, the `code`/`display_name`/
`sort_order` reference shape), no entity module/table/API, and references TO
a register still lower as ordinary class references (`role_id UUID` columns /
junction child tables) so referring entities keep working unchanged.

## Task report (RED → GREEN)

| Task | RED evidence | GREEN evidence | What the passing tests guarantee |
|---|---|---|---|
| T1 table name in the graph | `class_table_override_becomes_the_effective_table_everywhere` FAILED (`pg_table_name` was `pay_run`, the derived name, with the declaration swallowed) | `mox_ingest_tests` 21 passed | the declared spelling becomes `pg_table_name`; `api_path_segment` still derives from the TITLE; a declaration-free class keeps the derived name (`declares_table_name` false) |
| T2 table name on every generated plane | `declared_table_wins_on_every_generated_plane` FAILED (DDL/existing module used `pay_run`) | `mox_table_name_tests` 2 passed | `payroll.pay_run_rows` in DDL, `pay_run_rows_lines` child table (derived from the DECLARED parent), `src/entity/payroll_pay_run_rows.rs` with `table_name = "pay_run_rows"`, repository SQL on the declared table, hurl file `*_pay_run_rows.hurl`, and `/api/v1/payroll/pay-run` URLs unchanged |
| T3 declared-table collision | `declared_table_collision_hard_errors_naming_both_classes` FAILED (generation succeeded; two classes emitted one table) | `module_collision_tests` 3 passed | cross-package/one-domain collision is a hard `Error::Config` naming BOTH classes + the declaration; the pre-existing derived-collision and entity+VO-legal cases still pass |
| T4 register mapping | `register_class_bridges_to_the_reference_data_plane` FAILED (no register concept: `stats.registers` did not exist, the class bridged as an entity) | `mox_ingest_tests` + `mox_table_name_tests::register_class_gets_a_seed_table_and_no_entity_surface` | `is_codelist`, `classification = "codelist"`, `is_register_class()`, a `CodeList` node, `stats.registers == 1`; and in a full run: `common.role` seed migration present, `payroll.role` / `src/entity/payroll_role.rs` / `src/domain/payroll/role/**` absent, no `/role` route, referring entity keeps `role_id` |
| T5 register refs | `register_refs_lower_like_any_class_reference` FAILED (`pg_column_name` read `role`) | same suite | `refers Role[0..1] role` stays `EntityReference` → `role_id` column, `ref_target = Role` |
| T6 migrate keywords | `keyword_heavy_schema_round_trips_through_the_compiler` FAILED against the old local list (`compile_errors` non-empty for `event`/`channel`/… ) | `migrate_tests` 5 passed | a schema spelling EVERY documented word (titles, properties, enum literals) emits a `.mox` package the compiler accepts; real keywords escaped, contextual words (`register` included) unescaped |
| T7 descriptions/constraints | — (audit; the five constraints and descriptions already ingested) | `descriptions_and_value_constraints_ingest_without_loss` | class/feature/enum descriptions, literal labels and the five value constraints survive bridge → graph → readback |
| T8 format bump | — | `artifact_kit_tests` 2 passed | the artifact document is v9; a v10 document is refused as `format_version_too_new` |

## Byte-identity

A model with **no `table` and no `register` declarations is byte-identical**:
the bridge writes the same `SchemaNode` fields as before (only a declared
table changes `pg_table_name`, no new annotation is written, no new counter
prints — `MoxIngestStats::registers` is gated on non-zero), and
`ordering::compute_generation_order` skips only schemas carrying the register
marker. Verified by the standing canaries plus the mox/JSON equivalence gate:

- `policy_rls_snapshot_tests` (flag-off full-tree hash) — 7 passed, no diff.
- `ux_rules_byte_identity_tests` / `forms_byte_identity_tests` — 4 + 4, no diff.
- `mox_equivalence_tests::full_trees_are_byte_identical_between_json_and_mox_models`
  — 8 passed (the JSON↔mox equivalence fixture declares no table/register).

## Final gate sweep

See the PR body for the executed command list and counts (workspace suite,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
`pherolang_fixture_sweep`, and the `grafeo_e2e_tests` compile gate
`grafeo_generated_code_compiles` if protoc was available).

## Deferrals

- `PropertyNode::unique` and `EnumValue::description` mirrors: `unique`
  (the value-less many-valued constraint) and enum-literal doc comments are
  still dropped at the bridge. Adding the fields touches ~250 `PropertyNode`
  and ~42 `EnumValue` struct literals across the workspace (ui/ifml/grpc test
  fixtures included) — deliberately a separate slice, not smuggled into this
  one.
- Register attributes projected as seed-table columns (needs the reference
  table to grow beyond `code`/`display_name`/`sort_order`).
- Register CRUD-free entity (option 2 above) — needs the `api`/`ddd` op-plane
  in scope.
- `codegraph doctor --mox-files` does not run the generation-side collision
  sweep (doctor compiles models, it does not build a graph); the run path and
  `mox_ingest` warn on declarations the JSON plane shadows.
- Register refs carry the `role_id` column but no FK constraint (codelist
  tables are not part of `generated_table_set`).
