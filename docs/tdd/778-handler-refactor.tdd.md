# TDD — `handler.tera` refactor (issue #778) + `?include=` fail-closed (#781 P0)

Status: **landed on `ws/778-handler-refactor`** (branch off `master` @ `34b93834d`).
Driving issues: #778 (refactor), #781 (cornucopia `?include=` silently empty).

This is the engineering record for the refactor. The plan/ordering rationale lives in
`~/.opencode/plan/778-handler-refactor-plan.md` (session-local); this document is the
repo-facing account of what the architecture is and how it is held in place.

## 1. What the issue asked for, and what the codebase already had

Issue #778 reviews `api/handler.tera` (then 1,624 lines at the pinned rev; 1,657 at the
time of the refactor) and proposes an `ApiPlan` IR, template splitting, service-op
lowering, persistence decoupling, and capability-driven generation.

Most of the *decision* layer already existed:

- `HandlerContext` (`api/handler/mod.rs`) — resolved path segments/params, ancestor depth,
  per-op flags, include paths, resolved imports, guard blocks, mox/service routes.
- Pre-rendered-block pattern — eligibility guard blocks (#338), `authorize_fn` (#590/#663),
  body plans (#662), media field plans (#713), invariant blocks (#666 slice 4d).
- `ExecutionKind` in `ddd/service_ops/` — the exhaustive execution model the issue proposes
  as `OperationExecutionPlan`.

What was missing: the *execution bodies* were still template-owned, the template performed
`set_global` programming and role/depth branching, persistence was branched on inside the
template (includes), and the file had no size budget.

## 2. Target architecture (achieved)

```
Graph + DomainConfig
        │
        ▼
Rust planner + lowerers            ← every decision
  HandlerContext                   ← resolved plan IR
  IncludePlan                      ← provider verdict + pre-rendered hydration
  service_ops::render              ← pre-rendered execution per ExecutionKind
        │
        ▼
Tera templates                     ← rendering only
  api/handler.tera                 ← 19-line assembler
  api/handler/{support,create,read,update,delete,list,search,
               semantic_search,tree,extensibility,mox,service_op}.tera
        │
        ▼
Generated Rust
```

Invariants now enforced by construction:

- No `set_global` in the handler templates.
- No `project.persistence_provider` read in the handler templates (includes go through
  `IncludePlan`; tree still legitimately uses the shared query layer on both providers).
- Every `ExecutionKind` renders from `ddd/service_ops/render/`; the dispatch is exhaustive
  (a new variant is a compile error, not a silent empty body).
- `handler.tera` is 19 lines; every fragment is under the 500-line template budget.
- Path extractors are pre-rendered (five shapes + the child-op-add variant).

## 3. Slices landed

| Slice | Change | Byte identity |
|---|---|---|
| T0.1 | `handler_matrix_tests` — 20 permutations sha256-pinned; bless gated | new gate |
| T1.1 | guard presence flags derived in Rust (`GuardPresence`) | identical (matrix) |
| T1.2 | include default-field dedup in Rust (`included_default_fields`) | identical |
| T1.3 | path extractors pre-rendered (5 shapes + child-op add) | identical |
| T2.1 | `api/include_plan.rs` — `IncludePlan` + 11 KATs | new module |
| T2.2 | include hydration rendered from `IncludePlan`; LIST validation hoisted | scheduled whitespace/motion diff (enumerated) |
| T2.3 | cornucopia → `IncludeSupport::Unsupported`: guarded 501 naming #781; conditional utoipa doc | cornucopia only (1 matrix hash) |
| T3.1 | renderer: seam / read / delete | identical |
| T3.2 | renderer: update (named transition / whitelist / no machine / child) | identical |
| T3.3 | renderer: child_op (add + parent load + boundary / save / remove) | identical |
| T3.4 | render module split (pure move) + body arm | identical |
| T3.6 | renderer: create + projection_read; dispatch exhaustive | identical |
| wiring | template arm chain (298 lines) → `{{ op.execution_block }}`; post-pass fills create guard/invariant slots and re-renders once | identical |
| T6.1 | template size budget (500 lines, shrink-only allow-list) | test only |
| Wave 4 | 12 fragments; `handler.tera` 1,657 → 19 lines; size exemption removed | identical |
| T7.3 | hurl `?include=` legs (provider- and provability-gated) | golden/fixture diff, enumerated |

## 4. The byte-identity oracle

`crates/codegraph/tests/handler_matrix_tests.rs` renders `api/handler.tera` across 20
context permutations and compares sha256 hashes from `tests/fixtures/handler_matrix.sha256`:

- root read/list, full CRUD, ux sort, fts (query-param + dedicated), semantic, tree,
  workflow, include (scalar/array, dot siblings), cornucopia, eligibility guards, mox ops,
  12 service ops covering every execution kind, media cleanup;
- child + grandchild (auditable, guards, service ops incl. every child action).

Rules: `HANDLER_MATRIX_BLESS=1` is the only way to re-bless; a failure names the
permutation and prints a real line diff from the expected snapshots. Every slice above ran
this gate; the only re-bless in the whole refactor is the enumerated include-wiring diff
(T2.2) and the cornucopia 501 (T2.3).

Slices that change *emitted app* bytes beyond the template also have compile/generator
gates: `service_ops_compile_gate` (7 emitted apps compile), `review_fixture_drift_tests`
(the committed fixture equals the generator output), `rest_snapshot_tests`,
`template_harness`, `policy_rls_tests`.

## 5. Service-op execution plane

`ddd/service_ops/render/` (directory module):

```
mod.rs        dispatch (exhaustive), shared validate block, domain_error/audit_arity
basic.rs      seam / read / delete / body
update.rs     update: named transition / body-driven whitelist / no machine / nested child
child_op.rs   add (parent load, boundary payload or decoded body) / save / remove
create.rs     collection-level modeled-boundary create
projection.rs projection_read: item (from_row) + collection (scoped list)
test_support.rs  shared route/facts fixtures
```

Contract:

- `render_execution_block(&ServiceOpRoute) -> String` is pure (no graph/config).
- Blocks are the exact bytes the template used to emit (verified against the blessed
  matrix snapshots byte-for-byte). No trailing newline except the body arm, whose
  `plan.block` already ends with one (documented exception).
- The handler generator re-renders every route ONCE after its post-pass attaches
  `invariant_block` / `create_guard_block` / `create_invariant_block`.
- `ExecutionRenderFacts` carries the generation-time names the arms bake in (serialized
  context keeps `#[serde(skip)]`).

## 6. `?include=` and issue #781

- Resolution stays provider-agnostic (`api/include_path.rs`); execution now goes through
  `IncludePlan`, and the template reads no provider flag.
- **P0 (landed):** a non-SeaORM provider renders a *guarded* 501 — a request without
  `?include=` proceeds normally; a request that asks for includes answers
  `501 NOT_IMPLEMENTED` naming the provider and #781. Never a 200 with silently empty data.
  The utoipa include doc line is pre-rendered and honest per provider.
- The historical cornucopia branch (empty data + `TODO(worker-cornucopia-includes)`) is
  retained only behind `build_include_plan_with_support` for callers that inject a verdict.
- **P2 (T7.3):** generated hurl contracts gain deterministic include legs (array/junction
  includes assert non-null; scalar includes only when the create body provably wires the FK;
  otherwise an omission comment). Cornucopia contracts emit no legs until P1 lands.
- **P1 (deferred, issue #781):** implement cornucopia include fetch queries in
  `cornucopia_queries.rs` + `cornucopia_repo.rs` (mirror `find_tree`), add the include
  methods to the provider-generic repository trait, and flip `IncludeSupport`.

## 7. Assumptions and deviations from the issue

- The issue's `ApiPlan` largely existed as `HandlerContext`; the work deepened it rather
  than introducing a parallel IR.
- The proposed 20-template tree was realized as 12 responsibility fragments plus an
  assembler — not a mechanical line split.
- The service-op **header** (doc comment, utoipa path, signature, load/authorize) stays a
  template fragment. Its branches are presentation + a data-driven record-load/authorize
  call; the execution region — the real logic — is fully Rust. This was a deliberate scope
  reduction (T3.5 downgraded) to avoid a risky 120-line pre-render with no behavioural gain.
- `codegraph-api-runtime` (the issue's separate runtime crate) is deliberately NOT part of
  this refactor: it is an emitted-contract change (generated `Cargo.toml`, dependency
  policy #424/#425) and deserves its own issue.
- Per-entity `ApiCapability` enums were not introduced; generator-level capabilities already
  exist (`cap()` + the #577 audit), and the per-entity surface is `HandlerContext`'s flags.

## 8. Gates added / relied on

Added: `handler_matrix_tests`; `file_length_contract` template half (500-line budget).
Relied on: `service_ops_tests`, `service_ops_compile_gate`, `rest_snapshot_tests`,
`template_harness`, `include_junction_tests`, `include_snapshot_tests`, `policy_rls_tests`,
`review_fixture_drift_tests`, `hurl_contract_tests`, clippy + fmt.

Nightly/live: `review_api_suite` (sea_orm include legs now exercised over HTTP),
`service_ops_hurl_gate`, `workers_live_gate`, `ifml_codegen_gate`.

## 9. Deferred / follow-ups

- #781 P1 (cornucopia include queries + repository-trait include surface) and P2's
  cornucopia live lane (T7.4).
- `codegraph-api-runtime` (runtime helpers for correlation id/validation/error mapping).
- Further template fragments beyond the 500-line budget for other large templates
  (`webhook/api_endpoints.tera`, `ifml/svelte/page_tanstack.tera`, …) — tracked by #751.
- A collection-add POM surface (recorded #661 follow-up), unrelated to this refactor.
