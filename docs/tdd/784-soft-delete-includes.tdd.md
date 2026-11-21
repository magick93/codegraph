# TDD — `?include=` excludes soft-deleted related rows (issue #784)

Status: **LANDED** on `ws/784-soft-delete-includes` (stacked on the open
`ws/781-cornucopia-includes` PR #785 — rebase onto `master` once that merges).

## 0. The defect, verified

`?include=` returned related rows whose `deleted_at` is set, on BOTH providers:

| Plane | File | What it did |
|---|---|---|
| SeaORM include fetch | `ddd/repository_emitter/tree.rs` (`emit_single_fetch_method` / `emit_batch_fetch_method` / `emit_dot_fetch_method`) | Built `Entity::find().filter(...)` with NO soft-delete predicate, while the list/get/search CRUD paths pass `include_deleted` (handlers hardcode `false`) and append `DeletedAt.is_null()` |
| cornucopia include SQL | `db/cornucopia_queries.rs::render_include_sql` | Filtered on `target.soft_delete_column` / `leaf.soft_delete_column` — plumbing that WORKS, fed by an input that is always `None` on the mox-first path |
| The marker's derivation | `ddd/repository_emitter/emitter.rs` | `EntityTree.soft_delete_column` came from a `SoftDelete` **policy** node; `ingest_policies_and_relationships` has **no caller** on the mox-first driver path — so the marker was unreachable exactly where the tables ARE auditable |
| The runtime pin | `cornucopia_include_runtime_tests` | Asserted the bug as designed behaviour ("a soft-deleted target IS still returned") |

The #781 runtime pin also proved the marker suffix, hand-applied to the same
statements, excluded the seeded rows — the SQL semantics were never in doubt;
the derivation was.

**Adjacent finding (reported, NOT fixed here — non-include output):** the same
root cause leaves the cornucopia GET/LIST/SEARCH query SQL unfiltered on the
mox-first path — `cornucopia_queries.rs` reads the same policy-derived
`soft_delete_column` for `write_list_query`/`write_get_query`/`write_search`
(line ~227). The SeaORM query plane is unaffected (it keys on
`EntityTree.is_auditable`). Fixing it changes non-include output, which #784's
scope discipline forbids — tracked as a follow-up.

## 1. The decision: the read plane's auditable decision (option A, refined)

The repo carries FOUR auditable-ish derivations:

1. **DDL context** (`db/ddl/query/context.rs`): domain `auditable` + `!append_only`
   → emits the `deleted_at` column.
2. **Read plane** (`ddd/query.rs`, `ddd/repository.rs`, `repository_emitter/crud.rs`):
   design → audit-policy → domains.toml fallback, `&& !append_only` → gates the
   `include_deleted` parameters AND the `DeletedAt.is_null()` filters. This is
   `EntityTree.is_auditable`.
3. **Media #699** (`media/plan.rs`): deliberately mirrors the DDL decision —
   the gate must never name a column the table lacks.
4. `EntityTree.soft_delete_column`: policy-only — the #784 root cause.

**Chosen: #2 (`EntityTree.is_auditable`)**, exposed as ONE predicate consumed
by both providers:

```rust
// ddd/repository_emitter/include_soft_delete.rs
pub(crate) fn include_soft_delete_marker(tree: &EntityTree) -> Option<&'static str> {
    tree.is_auditable.then_some("deleted_at")
}
```

Why the read plane's decision and not the DDL-only one:

- **Semantic agreement by construction** — the handlers pass `include_deleted
  = false` against exactly this decision (`query.tera` gates the parameter on
  it), so includes agree with the CRUD read contract for every shape.
- **Compile safety** — the entity model carries the `DeletedAt` column exactly
  when the read plane filters on it (`db/entity.rs`'s auditable push; the
  invariant is documented there: "the repository emitter's auditable
  soft-delete filter compiles only when the entity model has the column"). A
  DDL-only derivation could emit a filter on a target whose entity model lacks
  the variant (an `Audit` policy with `track_deleted = false`) → E0599.
- **Runtime safety** — `is_auditable ⇒ !append_only`, and the DDL emits
  `deleted_at` for the same default domain decision, so the column exists for
  every auditable tree. The divergent shapes (a `.ddd` design's
  `auditable = false` over an auditable domain, the audit-policy split) are
  PRE-EXISTING tensions shared with the read plane — includes introduce no new
  risk class.

Why NOT option B (wiring `ingest_policies_and_relationships` on the mox-first
path): the policy plane feeds `soft_delete_visibility`, `soft_delete_cascade`,
retention and RLS consumers; wiring it on would ripple into every one of them
(non-include output). Rejected without even a trial — the derivation above is
strictly smaller.

## 2. The consumer map (additive change; nothing existing moved)

- `EntityTree.soft_delete_column` / `soft_delete_visibility` /
  `soft_delete_cascade` consumers (policy-driven query SQL shapes, policy
  surfaces): **untouched** — the include planes stopped reading
  `soft_delete_column`; no other reader changed.
- `EntityTree.is_auditable` consumers (query.tera context, repository.tera
  trait, crud.rs/finders.rs/query_search.rs filters, the delete guard at
  `emitter.rs`): **unchanged**; includes are one NEW reader.
- `EntityTree` itself: NO field added — the predicate reads the existing
  `is_auditable`. The dot emitter gained a resolved INTERMEDIATE tree
  (`include_intermediate_trees`, segments[0]) beside the existing
  last-segment (leaf) tree.

## 3. What changed, per plane

### SeaORM (`ddd/repository_emitter/tree.rs`)

The emitted fetches AND the filter when the marker is present — the exact
fragment:

```rust
.filter(crate::entity::recruiting_candidate::Column::ReferredByApplicationId.eq(source_id))
.filter(crate::entity::recruiting_candidate::Column::DeletedAt.is_null())
```

Sites: the scalar arm (single: after `Id.eq(fk_value)`; batch: after
`Id.is_in(fk_values)`), the reverse-FK array arm (single + batch, after the
reverse-FK predicate), and BOTH dot hops (intermediate load + leaf load). A
filtered scalar answers `Ok(None)` (the include field stays null); a filtered
collection yields the empty vec — the handler's `Some(vec![])` shape, never an
error. The VO→entity `child_table_override` arm stays unfiltered (a VO child
table's rows inherit the parent's visibility; outside #784's acceptance list).

### cornucopia (`db/cornucopia_queries.rs::render_include_sql`)

Same shapes as #781 shipped, now reachable — the marker suffix on the scalar
(`AND "deleted_at" IS NULL` before `LIMIT 1`), the batch (`WHERE "t".…`), the
array (`AND "deleted_at" IS NULL`), and both dot-hop conds (`"i".…`/`"l".…`)
all derive from the shared predicate.

### hurl contracts (`api/hurl_contract/include.rs`)

A provable ARRAY include gains the deterministic exclusion leg:

```
# ?include=parts — a soft-deleted part is EXCLUDED (issue #784)
POST {{base_url}}/api/v1/inventory/part
Authorization: Bearer {{api_key}}
{ ...target minimal body with the reverse FK wired to {{source_id}}... }

HTTP 201
[Captures]
deleted_part_id: jsonpath "$.data.id"

DELETE {{base_url}}/api/v1/inventory/part/{{deleted_part_id}}
Authorization: Bearer {{api_key}}

HTTP 204

GET {{base_url}}/api/v1/inventory/widget/{{widget_id}}?include=parts
Authorization: Bearer {{api_key}}

HTTP 200
[Asserts]
jsonpath "$.included.part" == []
```

Gates (all must hold; every unmet gate gets its own omission comment — the
`#662 may_refuse` discipline):

- **array shape only** — the collection wrapper keeps `$.included.<seg>`
  present when empty, so `== []` proves the FILTER rather than a missing key;
- the target is **auditable per the repository emitter's OWN tree**
  (`RepositoryImplEmitter::query_entity_tree` → `is_auditable`) — one
  derivation, zero re-implementation; a non-auditable target hard-deletes on
  DELETE, which would make the leg pass with OR without the filter
  (`OmittedNonAuditable`);
- the target has **create + delete ops** and a resolvable schema property
  carrying the reverse FK (the JSON key the target's create DTO expects — the
  same `resolve_field` spelling `payload_fields` emits);
- the reverse FK is injected into the target's minimal body wired to the
  source's captured id (`inject_fk_capture`).

Scalar and dot paths keep the #781 provability rules (no new legs for
never-wired optional scalars; dot stays omitted).

## 4. Semantics recorded

- **Default-exclude**: no endpoint exposes an `include_deleted` toggle today
  (handlers hardcode `false` — `handler/read.tera:46,88`); none was invented.
- **Soft-deleted SOURCE still hydrates** — the include marker filters the
  TARGET hop only; the source's visibility belongs to the CRUD read that
  produced it. Pinned live (runtime gate leg 9b/9c) and in the hurl
  hydration legs.
- **Arrays yield `Some([])`** — the empty collection is a present value, never
  `None` and never an error.
- **Fail closed**: a marker is never invented for a tree the derivation calls
  non-auditable — the table may lack the column, and a marker naming a column
  a table lacks would 500 every include request.
- **Tenancy untouched**: no tenant predicate was added to the include SQL; the
  cross-tenant fence stays RLS-owned (the runtime pin's cross-tenant legs are
  unchanged and still pass).

## 5. One derivation, two providers — the parity pin

`both_providers_derive_the_include_marker_from_one_predicate`
(`db/cornucopia_queries_tests.rs`): for the same tree, auditable and not, the
SeaORM emission contains `Column::DeletedAt.is_null()` ⟺ the cornucopia SQL
contains `"deleted_at" IS NULL` ⟺ the predicate returns `Some`. Neither plane
can drift from the derivation or from each other.

## 6. Tests

Red-first (commit `test(includes): pin soft-delete exclusion for ?include=
(failing, #784)` — all watched failing before the fix):

| Test | File | Pins |
|---|---|---|
| `marker_follows_the_read_plane_auditable_decision` | `include_soft_delete.rs` | auditable default → Some; explicit non-auditable (even with a stale policy field) → None; append-only → None |
| `scalar_include_fetch_filters_soft_deleted_target_when_auditable` | `repository_emitter/tests.rs` | single + batch scalar arms carry the filter |
| `scalar_include_fetch_invents_no_filter_when_target_not_auditable` | 〃 | negative |
| `include_fetch_without_a_target_tree_invents_no_filter` | 〃 | unresolvable tree → unchanged shape |
| `reverse_fk_include_fetch_filters_soft_deleted_target_when_auditable` | 〃 | single + batch array arms |
| `dot_include_fetch_filters_the_leaf_when_auditable` / `…invents_no_leaf_filter…` | 〃 | leaf hop |
| `dot_include_fetch_filters_the_intermediate_when_auditable` | 〃 | intermediate hop (needs the new tree plumbing) |
| `include_sql_marker_follows_the_auditable_decision_not_the_policy_field` | `cornucopia_queries_tests.rs` | the #784 root cause, both arms |
| `append_only_target_omits_updated_at_and_invents_no_marker` (flipped from `…and_keeps_aliases`) | 〃 | append-only targets have no audit column at all |
| `dot_include_sql_markers_follow_the_auditable_decision_per_hop` | 〃 | each hop independent |
| `both_providers_derive_the_include_marker_from_one_predicate` | 〃 | the parity pin |
| `emitted_include_queries_extract_bind_and_carry_the_soft_delete_marker` (renamed from `…_carry_no_marker`) | `cornucopia_include_runtime_tests.rs` | PR-CI: every emitted statement carries the marker, exactly one predicate per auditable hop |

Live gate flips (`cornucopia_includes_hydrate_under_rls`, real Postgres):
soft-deleted target EXCLUDED (scalar + reverse-FK + dot intermediate + a NEW
dot-leaf seed trio: `organization_a_deleted` / `customer_deleted_org` /
`invoice_deleted_leaf`), soft-deleted source still hydrates (unchanged legs),
cross-tenant legs untouched. The manual `scalar_marker_sql` /
`array_marker_suffix` helpers were retired — the emitted SQL IS the marked SQL
now.

Hurl KATs: `soft_delete_exclusion_leg_is_emitted_for_provable_arrays`,
`soft_delete_leg_is_omitted_for_a_non_auditable_target`,
`soft_delete_leg_is_omitted_when_facts_do_not_resolve`,
`fk_injection_prepends_the_reverse_fk_member`.

Compile pins (#551): `repository_compile_tests` (6 — the filtered fetches
compile in the scratch crate) and `cornucopia_include_compile_tests` (13 —
the `--!` statement NAMES are unchanged, so the stub surface stays exact).

## 7. Byte identity — the enumerated movement

- `handler_matrix_tests`: **byte-identical** — the matrix pins `handler.tera`
  bytes; the filters live in the repository plane.
- `ux_rules_byte_identity_tests` / `forms_byte_identity_tests`: hold, no
  re-bless (their trees carry no auditable include surface).
- `policy_rls_tests::flag_off_full_output_hashes.snap`: re-blessed. Diff =
  exactly two keys — `src/domain/recruiting/application/repository_impl.rs`
  and `src/domain/recruiting/candidate/repository_impl.rs` (the two entities
  whose include fetches the review fixture carries). Verified by diffing the
  produced `.new` against the baseline before blessing; no blanket
  `INSTA_UPDATE`.
- Review fixture (`crates/review/generated-candidate/`): regenerated with the
  documented recipe (`review_fixture_drift_tests -- --ignored
  regenerate_committed_fixture_in_place`). Diff = 4 inserted lines, the two
  `DeletedAt.is_null()` filters per entity (single + batch each).

## 8. Deferred / follow-ups

- **cornucopia GET/LIST/SEARCH mox-first soft-delete gap** — the adjacent
  finding above; same one-line derivation at the `write_*_query` sites, but
  non-include output (workers fixtures move), so its own issue.
- **SeaORM dot intermediate addressing** — `emit_dot_fetch_method` loads the
  intermediate with `Id.eq(source_id)` (the SOURCE's PK) where the cornucopia
  twin uses a correlated subquery through the source's FK. Pre-existing,
  unpinned, no live consumer (no fixture carries a SeaORM dot path); reported,
  untouched by #784 (the new filters sit on the load as it exists).
- An endpoint-level `include_deleted` query parameter — deliberately not
  invented (no caller today).
- VO→entity child-table include filtering — the override arm inherits the
  parent's visibility; a separate decision if it ever models its own
  lifecycle.
