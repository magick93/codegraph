# TDD — SeaORM dot-include intermediate addressing (issue #789)

Status: **LANDED** on `ws/784c-dot-intermediate-addressing` (stacked on the
open `ws/784-soft-delete-includes` PR #787). This is the recorded §8 finding
of `784-soft-delete-includes.tdd.md`, now fixed under its own issue.

## 0. The defect, verified

`crates/codegraph-generate/src/ddd/repository_emitter/tree.rs` —
`emit_dot_fetch_method` loaded the DOT-include INTERMEDIATE with the
SOURCE's primary key:

```rust
let intermediate = crate::entity::{intermediate_module}::Entity::find()
    .filter(crate::entity::{intermediate_module}::Column::Id.eq(source_id))   // WRONG
    .one(db).await?;
```

The handler calls `repo.{fetch_method}(&tx, id)` with the SOURCE row's PK
(an invoice id, say). `intermediate.id == source_pk` never matches across
entities, so **every SeaORM dot include answered `Ok(None)`** —
always-empty, silent, no error on any surface.

**The cornucopia twin was already correct**: `db/cornucopia_queries.rs::
render_include_sql`'s `Dot` shape addresses the intermediate through the
SOURCE's FK via a correlated subquery:

```sql
WHERE "i"."id" = (SELECT "customer_id" FROM invoice WHERE "id" = :source_id)
```

Two providers, one shape, one of them wrong — the exact drift class the
include planes exist to avoid.

**Why it survived**: no pin held the wrong bytes. `template_harness`
pins handler call-site names only; `include_snapshot_tests` pins DTOs; the
#784 KATs pin the intermediate's soft-delete FILTER, not the load
predicate. No in-repo generated tree carries a dot fetch; the live
consumer is hr-specs (`worker?include=deployment.position`).

## 1. The fix — mirror the scalar include arm's own idiom

The scalar arm of `emit_single_fetch_method` already loads the source row
by PK, reads its FK (required: plain `Uuid`; optional: the
`Some(v) => v, None => return Ok(None)` idiom), and loads the target by
that value. The dot arm now does the same for its first hop:

```rust
let source = crate::entity::staff_worker::Entity::find()
    .filter(crate::entity::staff_worker::Column::Id.eq(source_id))
    .one(db).await?;
let source = match source { Some(s) => s, None => return Ok(None) };
let fk_value = match source.deployment_id {          // required: `let fk_value = source.deployment_id;`
    Some(v) => v,
    None => return Ok(None),
};
let intermediate = crate::entity::staff_deployment::Entity::find()
    .filter(crate::entity::staff_deployment::Column::Id.eq(fk_value))
    .filter(crate::entity::staff_deployment::Column::DeletedAt.is_null())   // #784, kept
    .one(db).await?;
```

- **#784 filters survive verbatim** — the intermediate's
  `DeletedAt.is_null()` rides the (moved) load; the LEAF load and its
  filter were already correct and are untouched.
- **The dead `seg0.is_array` arm is DELETED** (operator-approved): the
  include classifier already refuses dot notation whose first hop is a
  collection (`api/include_plan.rs::CornucopiaIncludeShape::classify` —
  "dot-notation includes whose first hop is a collection are not
  implemented on the cornucopia SQL-first plane yet"), so the branch was
  unreachable. Its deletion is what makes the else arm the only path.
- **`emit_dot_batch_fetch_method` needs no change**: it delegates to the
  single fetch per source id (`self.{fetch_method}(db, *source_id)`), so
  the fix lands transitively — the batch is fixed by construction.
- **Call shape unchanged**: no handler-side or classifier change
  (`handler.tera`, `include_plan.rs` untouched); no cornucopia SQL change;
  no new hurl legs (dot paths stay omission-commented per the may_refuse
  discipline).

## 2. Tests

Red-first (commit `test(includes): pin dot-include intermediate addressing
through the source's FK (failing, #789)` — all four watched failing; the
three #784 dot-filter KATs passed before the fix and stayed as the guard):

| Test | File | Pins |
|---|---|---|
| `dot_fetch_addresses_the_intermediate_through_the_source_fk` | `repository_emitter/tests.rs` | source loads by its own PK, `source.job_id` is read, the intermediate loads by `Id.eq(fk_value)`, and the defect shape (`hr_job::Column::Id.eq(source_id)`) is absent |
| `dot_fetch_returns_none_when_the_source_row_is_missing` | 〃 | the source match (early-return `Ok(None)`) precedes the intermediate load |
| `dot_fetch_optional_fk_uses_the_option_idiom` | 〃 | `let fk_value = match source.job_id { Some(v) => v, None => return Ok(None) }` |
| `dot_fetch_required_fk_reads_the_plain_uuid` | 〃 | `let fk_value = source.job_id;` with no Option unwrapping |
| `dot_include_fetch_compiles` | `repository_compile_tests.rs` | the #551 compile pin (below) |

## 3. The compile pin (#551)

`repository_compile_tests::dot_include_fetch_compiles` — the mox-fixture/
scratch-crate pattern (`seaorm_repository_impl_compiles_in_isolation`'s
shape): a worker→deployment→position scalar-refers chain (**explicit
`[0..1]` multiplicities — a bare `refers` materializes as a junction**,
which the include resolver skips), the source's `allow_include` declaring
`deployment.position`, generated through the production driver, then
assembled into a scratch crate carrying only the repository plane's
dependency set and `cargo check`ed.

Presence guards fail BEFORE cargo check (template drift must be named,
not buried in a compile error): the source load by PK, the FK read
(`source.deployment_id`), the intermediate by `Id.eq(fk_value)`, and the
defect shape asserted absent. The mox-first fixture's entities are
auditable by default, so the compiled plane also carries BOTH #784
`DeletedAt` filters — the filters and the fixed addressing compile
together.

**Teeth** (à la #550): reverting the emitter's intermediate addressing to
`Id.eq(source_id)` fails the guard naming the defect. Reverted; all 7
legs green.

## 4. Blast radius

- **In-repo: zero movement.** No committed fixture or snapshot carries a
  SeaORM dot fetch (the review fixture's includes are single-segment).
  All byte-identity canaries hold with no re-bless; the cornucopia dot
  SQL is untouched.
- **hr-specs (the live consumer): behaviour flips.** Dot includes
  (`worker?include=deployment.position`) hydrate for the first time on
  the SeaORM plane. **Migration note**: any e2e spec that accidentally
  pinned the always-empty result (`$.included.deployment` absent/`None`)
  needs updating to the hydrated shape; specs that legitimately never
  received dot data now will.
- The batch list envelope is unchanged in shape
  (`included.<segments[0]>[<source_id>]`) — sources whose dot fetch
  answers `None` are skipped there exactly as before, they just no longer
  answer `None` for every row.
