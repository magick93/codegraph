# TDD — `?include_deleted=` on GET/LIST/SEARCH: auth + manager-or-better role gate (issue #792)

Status: **LANDED** on `ws/784d-include-deleted-toggle` (stacked on the open
`ws/784b-cornucopia-crud-soft-delete` PR #791 — the cornucopia
`include_deleted` repository parameter and the
`get_/list_/count_/search_..._including_deleted` SQL variants are REAL only
since that PR; before it this toggle would have been dead on cornucopia).

## 0. The gap

The soft-delete read toggle existed end-to-end BELOW http but not AT http:

- Both providers' query/repository planes take `include_deleted: bool` when
  the entity is auditable (`EntityTree.is_auditable` gates the parameter on
  BOTH providers — SeaORM branches in `ddd/repository_emitter/crud.rs`,
  cornucopia switches between `{entity}` and `{entity}_including_deleted`
  statements in `ddd/cornucopia_repo.rs`).
- The generated handlers HARDCODED `false`:
  `handler/read.tera` (`find_by_id` / `find_by_id_scoped`),
  `handler/list.tera` (`list_filtered` + the `?q=` FTS delegation to
  `search`), `handler/search.tera` (the dedicated `/search` endpoint).
- The HTTP query params struct is `ListParams` (defined in `list.tera`;
  `include` + pagination + filter). `SearchParams` (dedicated FTS) carried
  `q` + pagination.

## 1. Decisions (recorded, not relitigated)

- **Non-auditable entities: accept-and-ignore.** The query plane has no flag
  to forward, so the handler template's non-auditable arms don't reference
  the param: `ListParams`/`SearchParams` carry no field, the forwarding args
  keep their gated absence, and serde_qs drops the unknown key. Documented
  through `ListParams` only where the field exists (auditable entities).
- **Includes do NOT honor the toggle.** The #784 marker
  (`include_soft_delete_marker`) stays request-free — `?include=` hydration
  filters soft-deleted TARGET rows regardless of the toggle.
- **403, never a silent ignore**, when an under-privileged session sends
  `?include_deleted=true`.
- **No UI/POM surface.**
- **API-key sessions bypass the role gate** — the `permission_middleware.tera`
  precedent (`AuthMode::ApiKey { .. } => true`): a key's authority is its RLS
  scopes (#169), and API-key sessions carry `role: "api_key"`, which is not a
  hierarchy role. Without the bypass every generated hurl contract, the ops
  api suite and every machine consumer would 403. JWT sessions rank by the
  Basejump account role.

## 2. The security posture (two layers)

1. **Authentication** — already in place: `scaffold/middleware.tera`'s
   `auth_middleware` authenticates every `/api` request (API key `sk_` or
   JWT) and inserts `AuthInfo` (role, organization_id, api_key_id) into the
   request extensions; handlers receive
   `Extension(api_key_info): Extension<ApiKeyInfo>`. Nothing new.
2. **Role gate (the new half)** — honoring the toggle requires the session's
   JWT role to rank **`manager`-or-better** in the domain's role hierarchy:
   `codegraph_config::config::default_roles_hierarchy()` =
   `[owner, manager, member, employee]`, overridden by
   `[rbac] roles_hierarchy` (`DomainConfig.rbac` — the same source the
   DB-level role policies read, fetched in the handler generator exactly the
   way `db/ddl/query/context.rs` fetches it). Rank = index in the hierarchy
   (lower index = higher privilege); a role the hierarchy does not name ranks
   below everything. The handler renders a generated helper
   (`include_deleted_role_allows(&ApiKeyInfo)`) + a pre-rendered guard block
   answering 403 `AppError::forbidden` naming the requirement. Fail closed:
   absent/`false` needs no gate (today's behaviour); a hierarchy without
   `manager` refuses every JWT role.

## 3. What changed, per plane

### The generator (`api/handler/include_deleted.rs`)

ONE spec function consumed by the generator so the emitted expression and
the KATs derive from one place:

```rust
pub fn include_deleted_role_allows(role: &str, hierarchy: &[String]) -> bool {
    let Some(min_rank) = hierarchy.iter().position(|r| r == "manager") else { return false };
    hierarchy.iter().position(|r| r == role).is_some_and(|rank| rank <= min_rank)
}
```

`build_include_deleted_gate(is_auditable, has_read, has_list, has_fts,
hierarchy)` pre-renders two whitespace-exact artifacts (the #338 block
convention):

- **helper_fn** — the emitted `include_deleted_role_allows(&ApiKeyInfo)`:
  the api-key bypass, the hierarchy as a `const HIERARCHY: &[&str]`
  (verbatim from config), and the same rank comparison as the spec fn. A
  hierarchy without `manager` keeps the `let Some(...) else { return false }`
  arm — fail closed at runtime with one emitted shape.
- **guard_block** —
  `if params.include_deleted == Some(true) && !include_deleted_role_allows(&api_key_info)`
  → `AppError::forbidden("?include_deleted=true requires a
  manager-or-better role")`. Rendered inline after the handler's
  `let correlation_id = …` line (starts with a newline, no trailing
  newline).

Active only when the entity is auditable AND at least one GET/LIST/SEARCH
surface exists (`has_read or has_list or has_fts`) — otherwise the helper
would be dead code and the guard would reference a `params` no handler binds.
Empty strings keep the handler byte-identical.

### The handler context (`api/handler/mod.rs`)

`HandlerContext` gains `include_deleted_helper_fn` + `include_deleted_guard_block`
(`#[serde(default)]` — always serialized, empty when inactive). The
hierarchy is fetched from `config.rbac.hierarchy_or_default()` with the
`default_roles_hierarchy()` fallback.

### The templates

- **list.tera** — `ListParams` gains `include_deleted: Option<bool>` (gated
  `is_auditable`, `{%- / -%}` trims so inactive renders byte-identical); the
  struct + default-page fns' gate widens from `has_list` to
  `has_list or is_auditable` (an auditable entity without a list surface
  still binds `params` on GET); the consts stay `has_list`-gated; the helper
  fn renders before the struct; the guard renders after `correlation_id`;
  `list_filtered` and the `?q=` delegation forward
  `params.include_deleted.unwrap_or(false)`.
- **read.tera** — the extractor + utoipa `params(ListParams)` gate widens
  from `has_include` to `has_include or is_auditable`; both load arms
  (`find_by_id` / `find_by_id_scoped`) forward the resolved bool; the guard
  renders after `correlation_id`.
- **search.tera** — `SearchParams` gains the field (same trims), the guard
  renders, the dedicated `/search` forwards.

`unwrap_or(false)` preserves today's default-exclude for every
flag-absent request.

### OpenAPI

`ListParams`/`SearchParams` derive `IntoParams` — the field documents itself
(description names the `manager` minimum and the 403). Verified by
`openapi_combined_registration_tests` (7 green, no template change needed).

### hurl contract (`api/hurl_contract/include_deleted.rs`)

For auditable entities with create+delete+read, the entity contract gains
one deterministic leg: POST a SECOND row (the source's deferred delete stays
untouched — the #784 soft-delete-leg precedent), DELETE it (204 — soft
delete on both providers since #791/#788), GET it WITH
`?include_deleted=true` asserting `$.data.id == "{{module}}_deleted_id"`
(the toggle serves the row), GET it WITHOUT the param asserting 404 (the
default read hides the row). The leg runs under the contract's `api_key` —
an API-key session, which bypasses the role gate — so the leg is
deterministic for exactly the surface the handler serves. Omission
discipline (`#662 may_refuse`): an auditable entity missing a gate emits its
omission comment naming the gap; a non-auditable entity emits NOTHING.

The gate reads `resolve_is_auditable` — the entity's OWN read-plane
decision — so the contract cannot pin a toggle the handler does not serve.

### Provider parity

SeaORM (`repository_emitter/crud.rs` branch) and cornucopia (#791 statement
switch) need NO changes: the handler forwards the flag identically for both
— pinned by `handler_matrix_tests`, where
`root_auditable_include_deleted_toggle` (SeaORM project) and
`cornucopia_auditable_include_deleted_toggle` render **byte-identical
handlers (equal sha256)**; only the persistence plane below http differs.

## 4. Strict bool parse

`Option<bool>` under the existing `serde_qs` `QsQuery` machinery: a garbage
value (`?include_deleted=banana`) fails extraction → `QsQueryRejection` →
400. There is no KAT home for the generated `qs_query.rs` (it is scaffold
template code, exercised by every generated app); the emitted-handler pins
(`list_params_carries_the_include_deleted_toggle`) hold the strict shape.

## 5. Tests

Red-first (`test:` pins watched failing before each slice):

| Test | File | Pins |
|---|---|---|
| `auditable_get_forwards_the_include_deleted_param` | `template_harness/handler_include_deleted.rs` | GET forwards the resolved bool into `find_by_id` |
| `list_params_carries_the_include_deleted_toggle` | 〃 | `Option<bool>` field, strict serde shape |
| `auditable_list_forwards_the_include_deleted_param` | 〃 | `list_filtered` + `?q=` delegation + dedicated `/search` + `SearchParams` doc |
| `auditable_handlers_render_the_role_gate` | 〃 | guard fires on `Some(true)`, consults the helper, 403 names `manager`, api-key bypass, hierarchy const |
| `auditable_get_without_include_still_extracts_list_params` | 〃 | the widened extractor arm (no include surface) |
| `non_auditable_handler_does_not_reference_the_param` | 〃 | accept-and-ignore (negative, passes before and after) |
| `default_hierarchy_ranks_the_manager_minimum` | `api/handler/include_deleted.rs` KATs | owner/manager pass, member/employee/`api_key`/unknown refuse |
| `custom_hierarchies_rank_by_their_own_order` | 〃 | `["admin","manager","clerk"]` ranks by its own order |
| `hierarchy_without_manager_fails_closed` | 〃 | no `manager` ⇒ every role refuses; empty hierarchy too |
| `emitted_helper_embeds_the_hierarchy_and_the_bypass` | 〃 | the emitted helper = spec fn + bypass + const |
| `guard_block_names_the_manager_minimum` | 〃 | the 403 block, #338 whitespace convention |
| `inactive_shapes_render_nothing` | 〃 | non-auditable / no-read-surface ⇒ empty |
| `custom_hierarchy_renders_into_the_emitted_const` | 〃 | config hierarchy verbatim; no-manager keeps the fail-closed arm |
| `provable_facts_emit_the_deterministic_toggle_leg` | `api/hurl_contract/include_deleted.rs` KATs | the leg byte-exact |
| `missing_gates_emit_the_omission_comment` | 〃 | omission names every missing op |
| `non_auditable_facts_emit_nothing` | 〃 | silence |
| `facts_follow_the_entity_auditable_decision_and_ops` | 〃 | db-backed: auditable → leg; non-auditable → silence; no-delete → omission |
| `handler_output_matrix_is_byte_identical` | `handler_matrix_tests.rs` | 3 new permutations (below), 5 auditable moved, 15 byte-identical |

Matrix permutations added:

- `root_auditable_include_deleted_toggle` — auditable, no include surface:
  the widened extractor + forward + guard on GET/LIST.
- `root_auditable_toggle_no_manager_hierarchy` — the fail-closed helper
  bytes (hierarchy `["owner","admin","member"]`).
- `cornucopia_auditable_include_deleted_toggle` — the provider-parity KAT
  (hashes identical to the SeaORM twin).

## 6. Byte identity — the enumerated movement

**Handler matrix** (`HANDLER_MATRIX_BLESS=1`, reviewed against the pre-bless
fixture):

- MOVED (auditable — the toggle plumbing): `child_full_crud`,
  `root_fts_dedicated`, `root_media_cleanup_auditable`, `root_mox_ops`,
  `root_semantic_search`.
- BYTE-IDENTICAL (15): every non-auditable permutation
  (`root_read_list_only`, `root_full_crud`, `root_ux_sort`,
  `root_fts_query_param`, `root_hierarchy_tree{,_typed}`,
  `root_workflow_status`, `root_include_single_segment`,
  `root_include_dot_siblings`, `cornucopia_include`,
  `grandchild_full_crud_with_service_ops`, `root_eligibility_guards`,
  `child_eligibility_guards`, `root_service_ops`,
  `service_ops_child_roles`) — the byte-identity contract holds.
- Process note: the FIRST bless attempt moved all 20 permutations — the
  `ListParams`/`SearchParams` field blocks used standalone `{% if %}` lines
  that inject a newline each when inactive. Fixed with `{%- / -%}` trims and
  re-blessed; the pre-bless fixture comparison then showed exactly the
  auditable set.

**Whole-tree canaries** — the init fixture's two entities are auditable, so
4 files moved (enumerated by regenerating the tree with base-commit
templates and diffing per-file normalized hashes):

- `src/api/common/common_todo_item_handler.rs`
- `src/api/common/common_todo_list_handler.rs`
- `hurl/10_common_common_todo_item.hurl`
- `hurl/11_common_common_todo_list.hurl`

Re-blessed: `ux_rules_pre_feature_tree.sha256` (`UX_RULES_BLESS=1`),
`forms_pre_feature_tree.sha256` (`FORMS_BLESS=1`). All 231 other files in
both trees byte-identical.

**policy_rls full-tree snapshot** (`policy_rls_tests__flag_off_full_output_
hashes.snap`, reviewed `.new`-vs-baseline diff before accepting): 27 keys —
6 hurl contracts + 20 handler files. The fixture's domain default
(`auditable` unset = true) makes every fixture entity auditable, INCLUDING
the `common` primitive-wrapper handlers — their pre-change handlers already
carried the `, false` slot (same positional arg), so this is the same
auditable-plumbing movement, not a derivation change. Nothing outside
`hurl/*` + `*_handler.rs` moved (migrations, DDL, RLS, domain types all
byte-identical).

**Review fixture** (`crates/review/generated-candidate/`) — regenerated with
the documented recipe
(`review_fixture_drift_tests -- --ignored
regenerate_committed_fixture_in_place`). Diff = 26 files, exactly the
policy_rls enumeration (6 hurl + 20 handlers).

**Pins updated deliberately** (not weakened — the intent survives):
`ux_rules_tests`'s two `list_filtered(..., false, ux_sort/api_key_info)`
shape pins → the resolved-bool spelling; the pins still hold the sort-slot
ordering they were written for (#306).

## 7. Compile evidence

`grafeo_e2e_tests::grafeo_generated_code_compiles` (the whole-tree
backstop, path deps, no protoc needed after the grpc filter) — green.
The review fixture app's own `cargo check` fails identically before and
after this change (269 E0433s: the committed fixture pins codegraph deps by
git rev, unresolvable offline) — pre-existing environment limitation, not
this slice.

## 8. Deferred / follow-ups

- **hurl leg for `?include_deleted=false`** — the explicit-false leg adds
  nothing the default GET already proves (both are the default read); not
  emitted.
- **utoipa 403 documentation** — the toggle's 403 is documented in the
  param description, not as a per-endpoint 403 response (every auditable
  GET/LIST/SEARCH would gain a conditional 403 line; the OpenAPI noise is
  not worth it for a param-gated denial — revisit if a consumer wants
  machine-readable 403s).
- **`semantic_search` toggle** — the POST-body embedding search keeps its
  hardcoded `false` (the toggle is a query-param surface; a body field is a
  different contract decision).
- **The `?q=` FTS delegation + dedicated `/search` share one guard** — both
  forward the same resolved bool; a dedicated-per-endpoint hierarchy is a
  non-goal.
- **Unknown-role 403 message** — names the requirement, not the session
  role (debuggability vs payload-minimality; the correlation id carries the
  trace).
