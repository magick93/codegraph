# TDD — classify database errors by SQLSTATE, not `Display` (issue #828; the #810 diagnosis)

Status: **LANDED** on `fix/828-sqlstate-error-mapping`.

## 0. The defect, verified

Every generated domain error classifier decided from `Display` text:

```rust
let msg = e.to_string();
if msg.contains("INSUFFICIENT_SCOPE") || msg.contains("ROLE_FORBIDDEN") || …
```

On `tokio-postgres` >= 0.7.15 a server error renders as the literal
string `db error`; the typed `DbError` (SQLSTATE, message, detail, hint,
constraint) lives in `source()`. The result on the workers slice:

- a scope-denied INSERT (the generated `enforce_api_key_scope` policy
  raises `P0403` with an `INSUFFICIENT_SCOPE` payload) → `500
  INTERNAL_ERROR "Failed to create …: db error"` instead of 403;
- a duplicate key → 500 instead of 409 (`command.rs`'s own
  `contains("duplicate key")` was dead for the same reason);
- an FK/not-null/check refusal → 500 instead of 422.

codegraph#810 chased that 500 as a machine-bearing create defect for
three days. The consumer's forensics closed it: the probe key had
`scopes = []`, the Postgres log showed `P0403 INSUFFICIENT_SCOPE`, and
the identical body with a wildcard key returned 201. The machine path was
never involved — the create path (`templates/ddd/command.tera`) is
tx → RLS bundle → `repo.create` → commit, with no machine seed and no
kernel call.

### Why every gate stayed green

The workers live gate **skipped** the two hurl contracts that pin this
surface (`03_scope_denial_403.hurl`, `04_cross_tenant_404.hurl`) because
the workers suite passed only `base_url` + `api_key` — no
`api_key_limited`, no `api_key_b`. The monolith api suite ran them (its
SeaORM `Display` happens to carry the message, so it looked healthy), and
the workers slice never exercised a denial at all. A gate that cannot
fail.

## 1. The decision

**One derivation, by SQLSTATE, in the generated APP error plane (`src/error.rs`).**

- `classify_db_error(&impl Display + Debug + 'static) -> DbErrorClass`
  lives in `scaffold/error.tera` (`src/error.rs`) — every consumer reaches
  it there: the domain enum's `from_repo_err`, the tree handler, and the
  media handlers. (The gateway crate renders the same template with
  `is_gateway` set and the classifier omitted: it carries no
  tokio-postgres/deadpool/sea-orm dependencies and its catalog handlers
  only use `AppError`.) It recovers the server facts by downcasting:
  - `tokio_postgres::Error` → `as_db_error()` (workers/cornucopia; the
    cornucopia repository adapter's `db_err` boxes the typed error, so the
    box is downcast and its inner chain walked — `deadpool` wraps a tokio
    error);
  - `sea_orm::DbErr` → `RuntimeErr::SqlxError(Arc<sqlx::Error>)` →
    `as_database_error()` (monolith; `sea_orm::sqlx` is a re-export, so
    no new dependency).
  - The `Box<dyn Error + Send + Sync>` repository shape (issue #813) is
    downcast first, then its chain walked; `Display` is consulted **only**
    when no SQLSTATE is recoverable (hand-built repository sentinels,
    non-Postgres).
- Mapping: `P0403 | 42501 → Forbidden (403)`, `23505 → Conflict (409)`,
  `23503 | 23502 | 23514 → Unprocessable (422)` (the FK arm names the
  constraint), every other code → `InternalError (500)`.
- The signature gained `+ 'static` (needed for the `Any` downcast). Call
  sites are unchanged: concrete errors are `'static`, and the boxed
  repository errors are too.
- Each domain's `from_repo_err` delegates to it (`crate::error::classify_db_error`);
  the `From<…>` conversions route through `from_repo_err`, so the monolith
  and workers cannot disagree. (Why not per-domain? A classifier copy in
  every `errors.rs` is N derivations per app, and the media/tree planes do
  not know a domain module's path — the media compile pin proved exactly
  that.)
- `command.tera`'s inline `contains("duplicate key")` check is deleted;
  only the repository plane's typed **not-found sentinel** is matched, by
  exact string (`"Entity not found or already deleted"` /
  `"not found"`), never by a substring that a DB error could carry.
- `tree.tera` (both provider arms) and `media_route.tera`'s `db_err` call
  the one classifier instead of re-deriving it from `Display`. The
  cornucopia tree arm previously mapped a domain error (including a
  `Forbidden`) to `AppError::internal` — the same class, fixed here.
- The log carries `sqlstate` / `db_message` / `db_detail` / `db_hint` /
  `db_constraint` (ERROR level, visible at the default filter) plus the
  Debug form.

New generic variant `Unprocessable(String)` (422,
`UNPROCESSABLE_ENTITY`) — additive, and `AppError::from_domain_error`
already maps 422 to `AppError::validation`.

## 2. The teeth (written first, watched fail)

`error_mapping_runtime_tests` compiles the **emitted** `errors.rs`
verbatim into a scratch crate and drives real Postgres:

| Leg | SQLSTATE | Expected |
|---|---|---|
| `scope_denial_p0403_is_forbidden` | P0403 (the policy's RAISE) | 403 |
| `scope_denial_survives_the_boxed_repository_shape` | P0403 via `Box<dyn Error + Send + Sync>` | 403 |
| `insufficient_privilege_42501_is_forbidden` | 42501 | 403 |
| `unique_violation_is_conflict` | 23505 | 409 |
| `foreign_key_violation_…` | 23503 | 422, message names `child_parent_fk` |
| `not_null_violation_is_unprocessable` | 23502 | 422 |
| `check_violation_is_unprocessable` | 23514 | 422 |
| `unknown_sqlstate_stays_internal` | 42P01 | 500 (no false denial) |
| `sea_orm_db_err_classifies_by_sqlstate` | sqlx 23505 | 409 + `DbErrorClass::Conflict` |

Mutation proof — the scratch `errors.rs` was patched back to the
pre-#828 `Display` classifier and the suite re-run:

```text
test result: FAILED. 1 passed; 8 failed
  scope_denial_p0403_is_forbidden               FAILED  "db error" → 500
  scope_denial_survives_the_boxed_…             FAILED  "db error" → 500
  insufficient_privilege_42501_is_forbidden     FAILED  "db error" → 500
  unique_violation_is_conflict                  FAILED  "db error" → 500
  foreign_key_violation_…                       FAILED  "db error" → 500
  not_null_violation_is_unprocessable           FAILED  "db error" → 500
  check_violation_is_unprocessable              FAILED  "db error" → 500
  sea_orm_db_err_classifies_by_sqlstate         FAILED  (the monolith's
                                                Display carried the text;
                                                the CLASS was still wrong:
                                                it never produced 409)
  unknown_sqlstate_stays_internal               ok
```

Node-free (PR CI): `generated_error_plane_is_assembly_ready` pins every
SQLSTATE arm in the emitted module and the removal of the Display-based
unique classifier; `template_harness` extends the same assertions;
`media_pipeline_tests` pins the delegation; `handler_matrix_tests` is
re-blessed for exactly the two `root_hierarchy_tree*` permutations.

**The live gate that could not fail now can.**
`workers_live_gate` used to skip the scope-denial + cross-tenant
contracts. The workers suite now provisions `api_key_limited` (opt-in via
`hurl.limited_key`) and `api_key_b`, passes them as hurl variables, and
the fixture's skip list is empty. `03_scope_denial_403.hurl` asserts 403
+ `INSUFFICIENT_SCOPE` through the gateway — on master's classifier it
fails 500. The smoke section also repeat-creates the smoke entity 50×
and fails on any 5xx/transport error (the #810 intermittency class).

## 3. Byte identity (enumerated)

- Review fixture regenerated: `src/error.rs` (the classifier) +
  per-domain `src/domain/{domain}/errors.rs` (delegation; all 5) +
  per-entity `command.rs` (the map_err block; 21 files). No other file
  moved.
- `ux_rules_pre_feature_tree.sha256` + `forms_pre_feature_tree.sha256`
  re-blessed (whole-tree hashes; the only diffs are those files).
- `policy_rls_tests__flag_off_full_output_hashes.snap`: 27 changed
  per-file hashes — exactly `src/error.rs` + the 5 `errors.rs` + 21
  `command.rs`.
- `handler_matrix.sha256`: 2 of 23 permutations (the hierarchy-tree
  pair), the `tree.tera` classifier change only.
- `cornucopia_include_compile_tests`' `ShopError` stand-in is unchanged —
  it implements only the surface the include wrappers call, and the
  wrapper's `from_repo_err(e)` still satisfies `impl Display`.

## 4. Consumer migration / shims retired

- crewbase: the `500 "db error"` envelope becomes the honest status —
  403 (`INSUFFICIENT_SCOPE` / `ROLE_FORBIDDEN`), 409 (unique), 422 (FK/
  not-null/check). Any smoke that asserted 500 on a refused write must
  assert the class instead.
- The consumer's `workers-up` log now carries the SQLSTATE for a genuine
  500 without `RUST_LOG`.
- No shim at issue #806 to delete for this one; the report's "machine
  create 500s" entry closes with the diagnosis, not a workaround.

## 5. Deferred

- Payload-level error `details` for integrity violations (the constraint
  is in the message today, not a `FieldError`).
- The SeaORM `DbErr` chain beyond `RuntimeErr::SqlxError` (a pool-acquire
  timeout stays internal, correctly).
- Typed error classification for the atproto backend (unchanged).
