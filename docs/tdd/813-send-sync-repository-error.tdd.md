# TDD — the repository plane's error type is `Send + Sync` (issue #813)

Status: **LANDED** on `fix/813-send-sync-repository-error`.

## 0. The defect, verified

A consumer's generated cornucopia workers workspace (10 domains) fails
`cargo check` with 11 × `future cannot be sent between threads safely`
("the trait `Send` is not implemented for `dyn StdError`"), all in
`cornucopia_repository_impl.rs::find_by_id`. Found bumping crewbase to
`89e24cbf6` (PR #811); `bee6d6e3642` compiled clean.

**The issue's framing — "#781 include-hydration `find_by_id`" — is wrong.**
The `?include=` plane hydrates in the adapter's inherent `fetch_*` methods
and the query wrappers, not in `find_by_id`; and #781 predates the
regression. The cause is PR #811's #810 fix, which replaced the cornucopia
adapter's `|e| e.to_string()` mapping with a typed `db_err` returning
`Box<dyn std::error::Error>` — no `Send + Sync`.

### The mechanism

`{Entity}Repository<C>` is provider-neutral (one trait, two impls), so its
error type is ONE spelling. Both impls are `#[async_trait]`, which boxes
every method future as `Pin<Box<dyn Future + Send>>`.

A `?` that sits in the scrutinee of an `if let` — or in the head of a `for` —
holds its `Result` temporary until the end of the whole statement. rustc
names the held value exactly:

```
52 |         let response = if let Some(row) = pay_run::get_pay_run().bind(db, &id).opt().await.map_err(db_err)? {
   |                                           ----------------------------------------------------------------- has type `ControlFlow<Result<!, Box<dyn StdError>>, std::option::Option<GetPayRun>>` which is not `Send`
53 |             let annotations_rows = {
54 |                 let rows = pay_run::list_pay_run_pay_run_annotations().bind(db, &row.id).all().await.map_err(db_err)?;
   |                                                                                                ^^^^^^ await occurs here, with `pay_run::get_pay_run().bind(db, &id).opt().await.map_err(db_err)?` maybe used later
```

When the success block (or loop body) goes on to `.await`, the error type is
live across that await. A `String` crossing it was fine (pre-#811); the
non-`Send` box is not.

**The shape.** The awaits inside those scopes are the child-collection
reads (`emit_child_reads_cornucopia` → `list_{parent}_{child}().…all().await`),
which exist only for an entity with a contained value-object collection (a
`contains VO[]` feature whose child table has ≥ 1 column). So the futures go
`!Send` for exactly those entities, in THREE methods, not one:

| Method | Where the `?` temporary is held | Await inside the scope |
|---|---|---|
| `find_by_id` | `if let Some(row) = get_x().…opt().await.map_err(db_err)? {` | the child read of the response expression |
| `list` | `for row in list_x().…all().await.map_err(db_err)? {` | the child read of each row |
| design finders (`find_*`/DD9 queries, many-valued) | `for row in {finder}().…all().await.map_err(db_err)? {` | the child read of each row |

### Why every gate stayed green

`worker_wasm_compile_tests` — the gate whose doc comment promises a `!Send`
regression "surfaces here" — had a fixture root entity (`PayRunType`) with
only a nullable scalar include (`refers CompensationType[0..1]`) and a config
back-ref child (`PayRunLineType`, `role = "child"`). Neither is read inside
the load block, so `find_by_id` never awaited there and the non-`Send`
temporary never crossed an `.await`. No other cornucopia compile pin carries
a contained child collection.

## 1. The decision: sweep the whole plane in BOTH providers

The approved fix (and the only one that fits the code): the trait cannot
change for cornucopia alone — the SeaORM impl would stop implementing it —
so every signature, ascription and mapping in the plane moves to
`Box<dyn std::error::Error + Send + Sync>`.

The issue comment's "14 emission sites … only the cornucopia + native-dev
planes need the sweep" undercounts. It is **70 emission sites in 7 files**:

| File (under `crates/codegraph-generate/`) | Sites | What |
|---|---|---|
| `src/ddd/cornucopia_repo.rs` | 14 | `db_err` definition + 13 impl signatures |
| `src/ddd/cornucopia_repo_include.rs` | 6 | adapter signatures (+ 5 in-file test literals) |
| `templates/ddd/repository.tera` | 10 | trait method signatures |
| `src/ddd/repository_emitter/crud.rs` | 6 | 5 signatures + 1 `ok_or_else` turbofish |
| `src/ddd/repository_emitter/finders.rs` | 4 | signatures |
| `src/ddd/repository_emitter/query_search.rs` | 19 | 3 signatures + 16 `Box::<dyn …>::from(…)` closure ascriptions |
| `src/ddd/repository_emitter/tree.rs` | 11 | 8 signatures + 3 closure return ascriptions |

Plus 14 test literals outside those files (`design_generation_tests.rs` 4,
`cornucopia_include_tests.rs` 4, `ddd_query_tests.rs` 1,
`hooks_cornucopia_runtime_tests.rs` 5 — the hand-written stand-in that must
move in lockstep or it stops implementing the generated trait).

The sweep is a pure spelling change: 89 replaced lines, each identical to its
original modulo ` + Send + Sync` (verified by pairing every removed line
with its replacement).

**Callers need no change.** The query/command wrappers route repository
errors through `.map_err(|e| {Domain}Error::from_repo_err(e))?`
(`impl Display + Debug` — accepts the box unchanged) or `e.to_string()`;
`templates/ddd/errors.tera` keeps BOTH `From<Box<dyn std::error::Error>>`
impls untouched, so `errors.rs` output does not move at all.

### The sibling defect (found by the classification, fixed here)

PR #811's blanket replacement also rewrote the as-json `find_tree` arm:

```rust
let mut val = serde_json::to_value(resp).map_err(db_err)?;
```

`db_err` takes `tokio_postgres::Error`; this is a `serde_json::Error` — it
cannot compile for a cornucopia entity with `hierarchy_field` +
`tree_include`. No in-repo cornucopia fixture reaches that shape, so no gate
saw it. It gets its own Send + Sync mapping (`SERIALIZATION_ERROR_MAP`,
`|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) }` — still
the typed error, the #810 intent), spelled ONCE as a constant the pin reads
back.

**Classification of every `map_err(db_err)` site** (40 in
`cornucopia_repo.rs`, 0 in `cornucopia_repo_include.rs`, whose include
adapters still stringify — see §5): after the fix every one is attached to
a cornucopia query call (`.bind(..).…await`), whose error IS
`tokio_postgres::Error`. The pin below makes that a rule rather than a
finding.

## 2. The teeth (written first, watched fail)

### Tooth 1 — structural, the real `#[async_trait]` impl (`worker_wasm_compile_tests`)

The fixture root entity gains a contained value-object collection:
`PayRunType.annotations` → `AnnotationType` (a class targeted only by
`contains`, one text column). `find_by_id`, `list` and the DD9 design finder
now await the child read inside the scope that holds a `?` temporary.

RED on the unmodified emitters — the native whole-package check fails with
exactly three errors:

```
error: future cannot be sent between threads safely
  --> compensation/src/domain/compensation/pay_run/cornucopia_repository_impl.rs:47:5
   |
47 |     async fn find_by_id(
   |     ^^^^^ future created by async block is not `Send`
   |
   = help: the trait `Send` is not implemented for `dyn StdError`
note: future is not `Send` as this value is used across an await
  --> compensation/src/domain/compensation/pay_run/cornucopia_repository_impl.rs:54:96
   |
52 |         let response = if let Some(row) = pay_run::get_pay_run().bind(db, &id).opt().await.map_err(db_err)? {
   |                                           ----------------------------------------------------------------- has type `ControlFlow<Result<!, Box<dyn StdError>>, std::option::Option<GetPayRun>>` which is not `Send`
53 |             let annotations_rows = {
54 |                 let rows = pay_run::list_pay_run_pay_run_annotations().bind(db, &row.id).all().await.map_err(db_err)?;
   |                                                                                                ^^^^^^ await occurs here, with `pay_run::get_pay_run().bind(db, &id).opt().await.map_err(db_err)?` maybe used later
   = note: required for the cast from `Pin<Box<{async block@cornucopia_repository_impl.rs:47:5}>>` to `Pin<Box<dyn Future<Output = Result<Option<_>, _>> + Send>>`

error: future cannot be sent between threads safely
   --> compensation/src/domain/compensation/pay_run/cornucopia_repository_impl.rs:111:5
    |
111 |     async fn list(
    |     ^^^^^ future created by async block is not `Send`
note: future is not `Send` as this value is used across an await
   --> compensation/src/domain/compensation/pay_run/cornucopia_repository_impl.rs:127:100
    |
122 |         for row in pay_run::list_pay_run().bind(db, &page_size_i, &offset).all().await.map_err(db_err)? {
    |                    ------------------------------------------------------------------------------------ has type `ControlFlow<Result<!, Box<dyn StdError>>, Vec<GetPayRun>>` which is not `Send`
...
127 |                     let rows = pay_run::list_pay_run_pay_run_annotations().bind(db, &row.id).all().await.map_err(db_err)?;
    |                                                                                                    ^^^^^^ await occurs here, with `pay_run::list_pay_run().bind(db, &page_size_i, &offset).all().await.map_err(db_err)?` maybe used later

error: future cannot be sent between threads safely
   --> compensation/src/domain/compensation/pay_run/cornucopia_repository_impl.rs:157:5
    |
157 |     async fn recent_by_name(
    |     ^^^^^ future created by async block is not `Send`
note: future is not `Send` as this value is used across an await
   --> compensation/src/domain/compensation/pay_run/cornucopia_repository_impl.rs:169:96
    |
167 |         for row in pay_run::recent_by_name_pay_run().bind(db, &target_name, &page_size, &page_offset).all().await.map_err(db_err)? {
    |                    --------------------------------------------------------------------------------------------------------------- has type `ControlFlow<Result<!, Box<dyn StdError>>, Vec<GetPayRun>>` which is not `Send`
168 |             let annotations_rows = {
169 |                 let rows = pay_run::list_pay_run_pay_run_annotations().bind(db, &row.id).all().await.map_err(db_err)?;
```

The leg is three-fold:

- **the emitted-shape pin**
  (`assert_contained_child_read_crosses_the_find_by_id_load`): the child
  read sits after the `if let` load and before its `} else {`, and the child
  table's three SQL statements are emitted — a fixture edit cannot silently
  drop the shape and hollow the gate out;
- **the compile phases** (native whole-package + wasm32 lib + gateway);
- **mutation C**: the non-`Send` box re-introduced in the emitted trait AND
  adapter (exactly the pre-#813 emission, so nothing else is ill-typed) —
  the native check must REJECT it with `future cannot be sent between threads
  safely`.

The stub (`QUERIES_RS`) gains the child table's three query constructors;
the tripwire pins them to the emitted `--!` names.

Fixture note: the value object carries an EMPTY `pg_table_name`.
`ordering::compute_generation_order` runs the entity phase for every schema
that has one (the mox bridge gives each class a table name, so real models
also emit a module for the value object) — that module is the value object's
own CRUD, orthogonal to this leg.

### Tooth 2 — fast, node-free (`ddd::repository_error_type_tests`)

`include_str!`s the six Rust emitters and `templates/ddd/repository.tera`
and asserts every non-comment occurrence of `dyn std::error::Error` is
immediately followed by ` + Send + Sync`. RED on master: all 7 files, 75
lines (the 70 emission sites + the 5 in-file test literals). The checker
has its own KATs (rejects the plain box and the non-`Send` turbofish,
accepts the bounded box, checks every occurrence on a line, skips comment
lines), so the pin cannot pass vacuously.

Beside it:

- `db_err_only_maps_cornucopia_query_results` — every emitted line carrying
  `map_err(db_err)` must carry the `.bind(` whose result it maps. RED on
  master naming `cornucopia_repo.rs:1114` (the sibling defect).
- `as_json_find_tree_maps_the_serialization_error_through_its_own_closure`
  — renders the as-json `find_tree` arm and asserts the serialization
  error maps through `SERIALIZATION_ERROR_MAP`, never `db_err`. RED on
  master: `let mut val = serde_json::to_value(resp).map_err(db_err)?;`.
- `no_source_under_the_plane_directories_spells_a_non_send_box` — the
  closed set (added after the sweep): the seven-source pin is a LIST, and
  #751's size budget actively splits files (`cornucopia_repo.rs` is at
  ~1,890 of 2,000 lines), so a split-off module would join the plane
  unlisted. The same rule is walked over `src/ddd/**` and
  `templates/ddd/**` at run time. Two documented exemptions: the pin's own
  module (its KAT samples carry the old spelling as literals) and
  `errors.tera` (both `From` impls by design). Tooth validated: an
  unlisted `src/ddd/zz_813_probe.rs` with the old spelling turns it red
  naming that file.

**Compile proof for that arm (one-off, not a permanent pin).** No in-repo
fixture reaches `hierarchy_field` + `tree_include`, so the arm was
compile-checked once in a scratch crate holding the REAL emitter output
(dumped from `emit_adapter_tree`) plus a stand-in `cornucopia-queries`: the
fixed arm compiles under `#[async_trait]` with the tree read's future
asserted `Send`; swapping the serialization line back to PR #811's
`map_err(db_err)` fails with `error[E0631]: type mismatch in function
arguments` — the sibling defect is a real compile error, and the harness
sees it.

## 3. Byte identity — the enumerated movement

The only generated-text change is the error-type spelling, plus the
`find_tree` serialization mapping (reached by no committed fixture). What
moved, per golden:

| Golden | Movement | How it was enumerated |
|---|---|---|
| `handler_matrix_tests` | **none** — pins `handler.tera` bytes | test passes unchanged |
| `policy_rls_snapshot_tests` (8) | **none** | tests pass unchanged |
| ux canary `ux_rules_pre_feature_tree.sha256` | `d61b9d11…` → `11180755…` | see below |
| forms canary `forms_pre_feature_tree.sha256` | `7a0cfe87…` → `5b33d974…` | see below |
| `generate_tests__repository__*.snap` (5) | spelling only: 4/4/4/4/9 lines | every changed line paired old→new |
| `policy_rls_tests__flag_off_full_output_hashes.snap` | exactly 42 of 1040 keys | per-file hash map, diffed key by key |
| review fixture `crates/review/generated-candidate/` | 42 files, 234 lines each way | `git diff -U0`, every line pair checked |

**The ux/forms canaries DID move — the task brief expected them not to.**
Each canary hashes the WHOLE flag-off tree of an init-scaffolded demo app,
and that tree carries `repository.rs` + `repository_impl.rs` for both demo
entities, so the spelling necessarily reaches them. Not re-blessed blind:
master's emitters and the fixed emitters each generated the real tree
(throwaway dump test, reverted), and `diff -r` shows each canary tree
differs by EXACTLY 4 files — `src/domain/common/common_todo_{item,list}/
{repository,repository_impl}.rs` — 5 lines each (20 changed lines per
tree, the 5th being the `Box::<dyn …>::from` turbofish in the soft-delete
guard). Both canaries pass on master's emitters, so the movement is this
change and nothing else. (A first attempt at an inverse-replacement hash
check reported a mismatch; that check was itself wrong — its plain-`Box<…>`
pattern missed the turbofish site — which is why the enumeration rests on
the real-tree `diff`.)

The `policy_rls` snapshot's 42 changed keys are 21 entities ×
(`repository.rs` + `repository_impl.rs`); none added, none removed, none
outside the repository plane. The review fixture shows the same 42 files,
and all 234 line pairs are the pure spelling change (5 sites on 20 files,
6 on 19, and 4/7/9 on one each).

Three of the five `generate_tests` snapshots also refresh a stale insta
metadata header (`source:` still named the pre-#751 `generate_tests.rs`;
now `generate_tests/repository.rs` + `assertion_line`) — insta rewrites
metadata whenever it re-blesses; not generated output.

`templates/ddd/errors.tera` is untouched, so no `errors.rs` moves.

## 4. Gates

All local, through `mbx` with `RUSTFLAGS="-C debuginfo=0"` and a private
`TMPDIR`; DB/node lanes ride CI.

| Gate | Result |
|---|---|
| `mbx test -p codegraph-generate --lib` | 1345 pass (incl. the 10 pins of the new module, `design_generation_tests`, `cornucopia_repo_include`) |
| `worker_wasm_compile_tests -- --ignored` (native whole-package, wasm32 worker lib, wasm32 gateway; mutations A/B/C all rejected) | pass |
| `cornucopia_include_tests` / `ddd_query_tests` / `generate_tests` / `policy_rls_tests` | pass |
| `cornucopia_include_compile_tests` (13) / `repository_compile_tests` (7) | pass — the SeaORM twin compiles through the real trait |
| `template_harness` (125) / `handler_matrix_tests` / `profile_smoke_tests` (33) / `evt_workers_tests` | pass |
| `policy_rls_snapshot_tests` (8) / `ux_rules_byte_identity_tests` (4) / `forms_byte_identity_tests` (4) / `review_fixture_drift_tests` (9) | pass after the §3 re-bless |
| `grafeo_e2e_tests -- grafeo_generated_code_compiles` | pass — the whole generated app |
| `hooks_cornucopia_runtime_tests --no-run` | pass — the hand-written stand-in implements the generated trait |
| `mbx test --workspace` (CI's `test` job), on the final tree | 205 binaries: 4158 pass, 0 fail, 61 ignored |
| `service_ops_compile_gate -- --ignored` (7) / `wrapper_compile_tests -- --ignored` (1) — the DB-free halves of CI's ops lane | pass |
| `cargo fmt --all -- --check` / `mbx clippy --workspace --all-targets -- -D warnings -D clippy::too_many_lines` / `mbx check --workspace` | pass (exit 0 each) |

## 5. Deferred / follow-ups

- **`workers_live_gate` fixture** — the nightly 10-domain gate's inventory
  entity carries no contained child collection; extending it is out of scope
  here (the compile gate owns the class).
- **No compile pin reaches the cornucopia as-json `find_tree` arm**
  (`hierarchy_field` + `tree_include`). Its serialization mapping is
  string-pinned (§2) and was compile-verified once in a scratch crate; a
  permanent pin needs a fixture with the HR-shaped `tree_include` chain
  (a via entity + parent + the two `tree_*` stub queries).
- **The cornucopia include adapters still stringify DB errors**
  (`cornucopia_repo_include.rs`: `map_err(|e| e.to_string())`) — PR #811's
  #810 typed-error intent never reached the `fetch_*` methods. Harmless for
  `Send` (a `String` is `Send`) and outside this fix's byte-identity budget;
  noted, not changed.
- **`worker_wasm_compile_tests` runs plain `cargo`** inside its own
  `target/worker-wasm-gate` — unchanged, pre-existing.
