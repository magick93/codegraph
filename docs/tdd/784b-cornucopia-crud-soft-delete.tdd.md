# TDD — cornucopia mox-first CRUD soft delete (issue #788)

Status: **LANDED** on `ws/784b-cornucopia-crud-soft-delete` (stacked on the
open `ws/784-soft-delete-includes` PR #787, whose
`include_soft_delete_marker` predicate this fix reuses at a new consumer).

## 0. The defect, verified

`crates/codegraph-generate/src/db/cornucopia_queries.rs::render_entity_sql`
(line ~228) read:

```rust
let soft_delete_col = tree.soft_delete_column.as_deref();
```

`EntityTree::soft_delete_column` is **policy-derived** (from a `SoftDelete`
policy node) and `ingest_policies_and_relationships`
(`crates/codegraph/src/ingest/policy_ingest.rs`) has **zero callers** on any
driver path — so the field is `None` for EVERY project today. That one local
feeds every CRUD query writer in the file, so the cornucopia persistence
plane had FOUR behavioural defects for auditable entities (tables whose DDL
carries `deleted_at`):

| # | Defect | Site | What it did |
|---|---|---|---|
| 1 | Reads return soft-deleted rows | `write_get_queries`, `write_get_scoped_queries`, `write_list_queries` (+count), `write_search_queries`, `write_embedding_queries` | filtered only under `if !include_deleted && let Some(sd)` — `Some` unreachable |
| 2 | **DELETE hard-deletes auditable entities** | `write_delete_query`'s `None` arm | emitted `DELETE FROM` while the SeaORM twin soft-deletes and the DDL emits `deleted_at` — the data-loss class |
| 3 | UPDATE touches deleted rows | `write_update_query` | the `deleted_at IS NULL` where-guard was dead |
| 4 | The `include_deleted` toggle is silently dead | `ddd/cornucopia_repo.rs` | keyed the parameter on `tree.is_auditable` (correct) and switched to the `get_/list_/count_..._including_deleted` statements — but both statement variants were byte-identical |

Also: design-finder/DD9 SQL (`write_design_finder_queries`,
`write_design_query_sql`) documents "soft-deleted rows always stay invisible
to design finders" — that contract was false on this plane for the same
reason. (The `write_tree_workers_query`'s hardcoded `deleted_at IS NULL`
joins were already correct and are unrelated to this local.)

## 1. The consumer map — why the one line is total

Every `soft_delete_col` read in the file flows from the ONE local in
`render_entity_sql`:

| Consumer | Fed from | Effect of the fix |
|---|---|---|
| `write_list_queries` (line ~292 call) | the local | `WHERE "deleted_at" IS NULL` on `list_{entity}` + `count_{entity}`; `_including_deleted` twins stay unfiltered |
| `write_get_queries` (line ~301) | the local | the default `get_{entity}` gains `AND "deleted_at" IS NULL`; twin stays unfiltered |
| `write_get_scoped_queries` (line ~313) | the local | the parent-scoped get gains the same conjunct; twin stays unfiltered |
| `write_update_query` (line ~337 call) | the local | `AND "deleted_at" IS NULL` joins the where-guard |
| `write_delete_query` (line ~347) | the local | **flips `DELETE FROM` → `UPDATE SET "deleted_at" = NOW()`** for auditable trees; non-auditable keeps the hard delete |
| `write_search_queries` (line ~368) | the local | `AND "deleted_at" IS NULL` on the default search/count; twin unfiltered |
| `write_embedding_queries` (line ~380) | the local | same shape for `semantic_{entity}` |
| `write_design_finder_queries` / `write_design_query_sql` (lines ~511/~563) | the local | the documented "deleted rows invisible to finders" contract now holds |
| `render_include_sql` (line ~1047) | NOT this local — already derived through `include_soft_delete_marker(target)` since #784 | unchanged |

Nothing else in the file reads `soft_delete_column`, and no statement NAME
changes (the `--!` annotations were already emitted in both variants, so the
worker wasm gate's tripwire — names vs stub constructor set — holds
verbatim).

## 2. The fix

```rust
let soft_delete_col = include_soft_delete_marker(tree);
```

The predicate (`ddd/repository_emitter/include_soft_delete.rs`, landed with
#784) returns `Some("deleted_at")` iff `tree.is_auditable` — the read
plane's auditable decision (design → audit-policy → domains.toml,
`&& !append_only`). The import already existed in the file from #784.

Why this is the right derivation, not a new one: it is the SAME decision the
query/repository planes gate their `include_deleted` parameters on
(`ddd/query.rs`, `ddd/cornucopia_repo.rs`), the decision the SeaORM twin
filters on (`repository_emitter/crud.rs`), and the decision the DDL emits
the column for — so the filter can never name a column the table lacks, and
both persistence providers now agree on soft-delete semantics by
construction.

## 3. The no-regression argument

`EntityTree::soft_delete_column` is `None` for every project today (the
policy ingest has no caller), so the old local was ALWAYS `None`: no
existing consumer could have depended on a filter that never existed. The
fix only ADDS predicates where the column provably exists (`is_auditable`
⇒ the DDL emits `deleted_at`, pinned by the auditable-DDL contract), and
never invents one (a stale policy field over a non-auditable tree stays
marker-free — the #784 posture carried to the CRUD plane). Non-auditable
and append-only trees are byte-identical output (marker `None`).

## 4. Consumer migration

- **DELETE on an auditable cornucopia entity is now a SOFT delete** — the
  row keeps `deleted_at`; consumers that relied on the (buggy) hard delete
  to physically remove rows must use the row's visibility contract instead.
- Reads (`list/count/get/search/semantic`) exclude soft-deleted rows.
- The escape hatch is pre-existing surface: the generated repository
  methods' `include_deleted: bool` parameter and the
  `get_/list_/count_/search_..._including_deleted` query variants (the
  toggle now branches real statements — it was dead before).
- hr-specs is the external acceptance gate.

## 5. Tests

Red-first (`test(crud): pin cornucopia soft-delete SQL contract (failing,
#788)` — all eight watched failing before the one-line fix):

| Test | File | Pins |
|---|---|---|
| `list_and_count_sql_filters_soft_deleted_rows_when_auditable` | `db/cornucopia_crud_soft_delete_tests.rs` | defect 1 + 4: default arms filter, `_including_deleted` twins don't, arms differ |
| `get_sql_filters_and_including_deleted_stays_unfiltered` | 〃 | defect 1 |
| `scoped_get_sql_filters_when_auditable` | 〃 | defect 1 (nested reads) |
| `search_sql_filters_when_auditable` | 〃 | defect 1 (FTS) |
| `delete_sql_soft_deletes_when_auditable_and_hard_deletes_only_without_a_column` | 〃 | **defect 2 — the data-loss flip**, both arms |
| `update_sql_guards_against_deleted_rows_when_auditable` | 〃 | defect 3 |
| `non_auditable_and_append_only_sql_invents_no_marker` | 〃 | the stale-policy-field negative (byte-absence of `deleted_at`), append-only byte-absence + no update path |
| `crud_and_include_sql_share_one_marker` | 〃 | the parity pin: CRUD list SQL ⟺ include SQL ⟺ the predicate, both arms |

All KATs drive `render_entity_sql` itself (the derivation site), with trees
whose `is_auditable` is set explicitly and `soft_delete_column` left `None`
— the mox-first shape — except the stale-field negative.

Runtime gate (`cornucopia_includes_hydrate_under_rls` section 10): the
emitted `list_customer` / `list_customer_including_deleted` statements bound
positionally and executed as `app_user` under the session bundle — the
soft-deleted org-A customer is ABSENT from the default list, PRESENT in the
`_including_deleted` twin, and org B's rows are absent from both. The
existing include legs are unchanged. Helper refactor is
behavior-preserving: `assert_no_placeholders` extracted out of
`bind_source_param` (identical panic text), plus `bind_named_params` and
`query_list_scoped` on the same contract.

## 6. Collateral pins verified green

- `cornucopia_include_tests` (2) — include shapes only, no CRUD SQL pinned.
- `cornucopia_include_runtime_tests` PR-CI (15) — the annotation contract
  reads `include_*` statements only.
- `worker_wasm_compile_tests` (ignored) — the `--!` tripwire + stub hold
  (names unchanged).
- `mox_equivalence_tests` (8) — JSON vs mox full-tree byte equality; both
  runs move identically.
- `cornucopia_include_compile_tests` (13), `repository_compile_tests` (6) —
  names unchanged; no emitted-Rust shape change on the SeaORM plane.

## 7. Deferred / follow-ups

- An endpoint-level `include_deleted` query parameter — deliberately not
  invented (the repository plane's parameter is the escape hatch; #784's
  recorded non-goal stands).
- DELETE legs in generated hurl contracts still pin 204 + GET 404 — correct
  under soft delete too (the read plane hides the row); no contract change
  was needed.
- The policy plane (`ingest_policies_and_relationships`) remains unwired;
  its other consumers (`soft_delete_visibility`, cascade, retention, RLS)
  are untouched by this fix.
