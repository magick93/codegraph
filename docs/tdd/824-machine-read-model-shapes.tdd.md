# TDD — machine read models exist only where they have a loader (issue #824)

Status: **LANDED** on `fix/824-machine-read-model-shapes`.

## 0. The defect, verified

PR #819 (the #812 machine-kernel epic) rendered the #817 machine read
model — `api/machine_read.tera` — for EVERY machine-bearing design. Two
ordinary model shapes did not compile, and a consumer's generated workers
workspace failed `cargo check`. Reproduced here over
`tests/fixtures/machine_read_shapes` (both shapes in one recruiting
module), generated with the pre-fix generator:

```text
error[E0599]: no method named `find_by_id` found for struct `ApplicationQueryHandler` in the current scope
  --> src/api/recruiting/application_machine.rs:61:55
error[E0609]: no field `approval_status_code` on type `PositionOpeningResponse`
  --> src/api/recruiting/position_opening_machine.rs:66:26
```

Same files, same lines as the consumer's report.

1. **A machine on a design without a repository.** A design with no
   repository resolves to an EMPTY op set (`operations_for` → `[]`), so
   `ddd/query.tera` emits no `find_by_id` at all — the method the route
   module loads the row through.
2. **A machine bound to a `…Code`-typed status field.** Ingest strips the
   suffix from the Rust field (`approvalStatusCode` → `approval_status`;
   the COLUMN keeps `approval_status_code` — the dto plane's rule,
   `strip_code_suffix_safe`), and `workflow_action.rs` derived the
   accessor from the model name with a bare `to_snake_case`.

### The issue's framing, corrected

The report says "a non-root entity" and proposes gating on
`role == "child"`. That is NOT the discriminator: the consumer's
`Application` resolves **role = root** in this generator (its emitted
route was the flat `/application/{id}/machine` with a single `Path(id)`),
and a parent-scoped member WITH a `create` op does emit an unscoped
`find_by_id`, so it compiled. The real predicate is the LOADER'S
existence, exactly as `ddd/query.tera` gates it:

```text
find_by_id        emitted iff  has_read AND (has_create OR parent_ref is none)
```

The read model loads through the UNSCOPED `find_by_id`; therefore the
route is servable for a root with a `read` op and no parent reference.
Parent-scoped members need the scoped loader — their own follow-up
(#827).

### Why every gate stayed green

- The route is **monolith-only** (`router.rs`: `!project.is_workers_topology()`),
  so on the workers topology it is unrouted code that merely has to
  compile — and no compile pin generated it: every fixture put its
  machines on root scaffold entities with plain `*_status` fields.
- The only read-model pins (`machine_read_model_tests`) assert STRINGS
  over the `ifml_gate` fixture (`ClaimType`: a root scaffold entity with a
  workflow config) — the shapes that work.
- **A second, quieter defect:** OpenAPI's `has_machine` re-derived the
  fact WITHOUT the router's topology gate, contradicting its own comment
  ("Resolved through the router's own helper, so the documented paths and
  the mounted ones cannot drift"): under workers the document listed a
  `/machine` path the router did not mount.

## 1. The decision

**One loader rule, asked by every consumer.**

```rust
pub const fn machine_read_model_has_loader(has_read: bool, has_parent_ref: bool) -> bool {
    has_read && !has_parent_ref
}
```

- `machines::machine_read_model_surface_for_entity(db, config, domain,
  title, workflow_status_field)` = [`machine_surface_for_entity`] (the
  kernel surface) ∧ the loader, with the facts read the way the query
  handler reads them: the op set comes from `ddd::query::effective_operations`
  (EXTRACTED from `QueryGenerator::generate`, so the loader fact and the
  emitted handler cannot drift), and the parent reference from
  `resolve_parent_fk_column_same_domain` (the same helper the query
  generator calls). `machine_read_model_surface_for_title` resolves the
  domain for callers that only know a title (the IFML planes);
  `machine_read_model_titles` is the bulk set.
- **Consumers, all through that one function:** the route module emission
  AND the `machine` context the `/workflow` merge uses
  (`workflow_action.rs` — the merge only exists where the sibling module
  is emitted), the router (×2 — and the API contract through
  `RouterEntity.has_machine`), OpenAPI (×2 — now the router's OWN
  expression, topology gate included), the IFML route generator
  (`machine_surface_for`), and the IFML e2e spec/POM set
  (`machine_read_model_titles`).
- **The `…Code` accessor is the DTO's own field name**, read from the
  graph property (`PropertyNode.rust_field_name`) — never re-derived —
  with `strip_code_suffix_safe(to_snake_case(..))` as the fallback for
  graphs that carry no property (hand-built MockEngine graphs).
- **Topology is NOT the loader's business.** The `/machine` ROUTE stays
  monolith-only (the #703 posture), but the module is emitted on BOTH
  topologies: `state.tera` merges the sibling derivation into
  `/workflow`, and the workflow plane IS served on workers — skipping the
  module there would silently drop a dual entity's `machine` object from
  the workers payload. (The route's absence on workers, while the UI still
  emits the read-model contract, is the pre-existing #703 posture; the
  decision to mount `/machine` on workers is recorded, not taken here.)
- **The kernel is untouched**: a withheld read model keeps the registry
  embed, the workflow seed and the guards. The withholding is VISIBLE —
  `ValidationPass::check_machine_read_model` (check `machine_read_model`,
  Warning) names the class and the reason, and it decides through the
  SAME predicate the emitters use, so it cannot disagree with the
  emission.

Rejected: gating on `role == "child"` (does not cover the failing shape,
and would withhold compiling members); skipping `machine_read.tera` under
workers (breaks `/workflow`'s merge on the topology that serves it);
inlining the derivation into `state.tera` (two derivations, the #817
drift risk this resolver exists to prevent).

## 2. The teeth (written first, watched fail)

Node-free, over the REAL driver (`tests/machine_read_shape_tests`, the
`phantom_machine_tests` shape) on `tests/fixtures/machine_read_shapes`
(monolith profile — the topology that mounts the route — and workers
profile — the consumer's):

| Pin | What it holds |
|---|---|
| `only_the_machine_with_a_loader_has_a_read_model` | `machine_bearing_titles` = both designs; `machine_read_model_titles` = the root only; the kernel surface still resolves for the withheld one |
| `loader_predicate_is_read_without_a_parent_reference` | the loader truth table (no read op, parent-scoped, root) |
| `loaderless_machine_emits_no_read_model_on_any_surface` | no module, no `pub mod`, no router/OpenAPI/contract reference, nothing mentions it; its query handler has no `find_by_id`; the kernel registry still embeds the machine |
| `code_typed_status_field_is_addressed_by_its_dto_name` | the Response DTO names `approval_status`; the module reads `entity.approval_status.to_string()`, never `entity.approval_status_code`; the column keeps the suffix; routed + registered + loading through `find_by_id` |
| `workers_topology_emits_no_module_for_the_loaderless_machine` | the consumer's exact failure: no `application_machine.rs`, no reference; the `…Code` twin keeps its module, spelled by the DTO field |
| `validation_names_the_withheld_read_model` | one Warning, for `Application`, naming the reason; the loader-ful design is quiet |

**Mutation A** (`machine_read_model_has_loader` → always true — the
pre-#824 emission rule): 5 pins red (truth table, titles, monolith,
workers, the Warning); the `…Code` pin stays green. Caught a real drift
while writing it: the Warning originally re-derived "withheld" from the
raw facts, so mutation A left it green — it now calls the same predicate.

**Mutation B** (`status_field_rust` → the old bare `to_snake_case`):
exactly the two `…Code` pins red, on both topologies.

**The compile pin** (`service_ops_compile_gate::
generated_app_with_machine_read_shapes_compiles`, ignored, the
test-ops-integration lane): the whole emitted app over the same fixture
(workspace default profile, path deps) must `cargo check` clean. On the
pre-fix generator the leg is red at its pre-build shape pin (the module
exists) — and the generated app's own check fails with exactly the
consumer's two errors quoted in §0. On the fix: pass (52 s).

## 3. Byte identity

**None.** The withholding only narrows shapes no committed fixture had —
which is what let the bug ship. Verified by the full workspace suite
(`cargo test --workspace`, 227 test binaries, 0 failures):

| Golden | Movement |
|---|---|
| `ux_rules` / `forms` flag-off tree canaries | none (their init fixtures carry no machines) |
| `policy_rls_tests` per-file hash snapshot + `policy_rls_snapshot_tests` | none |
| `machine_read_model_tests` (10) | none — the `ifml_gate` `ClaimType`/`RefundRequest` machines are scaffold entities with `findById` |
| `machine_kernel_tests` (the `machine_kernel` fixture's `repository PayRunRepository { findById; … }`) | none |
| `phantom_machine_tests`, `template_harness` (125), `handler_matrix_tests` | none |
| review fixture (`review_fixture_drift_tests`) | none — the fixture's machine-bearing entities all load through an unscoped `find_by_id` |

The one deliberate output change no committed fixture reaches: **OpenAPI
under the workers topology** no longer registers a `/machine` path the
router does not mount (the second defect in §0). The `workers_gate`
fixture (a live gate, not byte-canaried) compiles through
`workers_gateway_compile_tests`/`workers_live_gate` unchanged.

## 4. Gates

Local, through `mbx`, `RUSTFLAGS="-C debuginfo=0"`:

| Gate | Result |
|---|---|
| `cargo test --workspace` (CI's `test` job) | 227 binaries, 0 failures |
| `machine_read_shape_tests` (6 node-free pins) | pass; mutations A/B reject as §2 records |
| `service_ops_compile_gate::generated_app_with_machine_read_shapes_compiles` (`--ignored`) | pass (52 s); red on the pre-fix generator with the §0 errors |
| `machine_read_model_tests` (10) / `machine_kernel_tests` / `phantom_machine_tests` / `template_harness` (125) | pass unchanged |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings -D clippy::too_many_lines` | pass |

## 5. Deferred / follow-ups

- **The scoped loader for parent-scoped members — #827.** The nested route
  already carries the parent id and `find_by_id_scoped` exists; until then
  a member's read model is withheld (with a Warning naming it), and a
  member WITH a `create` op no longer emits the unscoped route that ignored
  the parent id.
- **Mounting `/machine` under the workers topology** (recorded, not
  decided): the UI still emits the read-model contract while the route is
  monolith-only — the #703 posture, unchanged here. The module now follows
  the loader instead of the topology, so `/workflow`'s merge keeps working
  on workers.

