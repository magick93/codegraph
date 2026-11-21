# TDD — cornucopia `?include=` hydration (issue #781 P1)

Status: **P1 LANDED — cornucopia `?include=` hydration is DEFAULT ON** on
`ws/781-cornucopia-includes` (branch stacked on the open `ws/778-handler-refactor` PR).
The `cornucopia_include_hydration` preview flag is **deleted** (`ProjectConfig.codegen`
field, `driver::feature_bool`, the `[features]` key and the profile pair are gone), so
every cornucopia project hydrates includes by default:

* **supported shapes hydrate** — single-segment scalar `refers`, single-segment reverse-FK
  collection, and one hop of dot notation (`customer.org`);
* **every other SHAPE still fails closed per path** with a guarded
  `501 NOT_IMPLEMENTED` naming the requested alias and the construct (depth-3 dot,
  collection hops, VO→entity child tables, no-back-FK defensive arms) — never a
  provider-level 501, never a silently empty include;
* **hurl contracts carry the provable legs** for the supported shapes (array/junction
  unconditional; scalar when the create body wires the FK) and per-path omission
  comments otherwise.

The compile pin, the real-Postgres runtime pin, the flag-ON workers wasm gate and the
single-permutation handler-matrix re-bless all landed with the flip.

Related records: `docs/tdd/778-handler-refactor.tdd.md` §6 (the P0 landing and the P1
hand-off), `AGENTS.md` → "API handler generation plan (issue #778)".

---

## 0. What exists at HEAD, and what the P1 gap actually is

The `?include=` decision plane was already provider-agnostic:

| Layer | File | Today |
|---|---|---|
| Path resolution | `crates/codegraph-generate/src/api/include_path.rs` | Fully implemented, provider-agnostic (`ResolvedIncludePath` = alias + segments + `fetch_method` / `batch_fetch_method` / `response_rust_type`) |
| Handler blocks | `crates/codegraph-generate/src/api/include_plan.rs` | SeaORM renders real hydration; cornucopia renders real hydration for its implemented shapes and a per-path guarded 501 for every other SHAPE |
| SeaORM execution | `ddd/repository_emitter/tree.rs::emit_include_fetch_methods` | Inherent `fetch_*_for_*` / `fetch_*_batch_for_*` on the source `{Entity}RepositoryImpl` |
| Cornucopia execution | `db/cornucopia_queries.rs` + `ddd/cornucopia_repo_include.rs` + `ddd/query.rs` | Include SQL pairs, inherent adapter fetch methods, and session-owning query wrappers — **landed** (#781 P1) |

So P1 was never "wire a flag": it is three emission sites (SQL, adapter, query handler)
plus a per-path capability verdict so a shape the plane does not implement still fails
closed instead of returning empty data.

### The switch (deleted in the flip PR)

The plane shipped behind `[features] cornucopia_include_hydration = true` while it was
built. That scaffolding is **gone**:

* `ProjectConfig.codegen.cornucopia_include_hydration` (field, `Default`, the serialized
  shape key) is removed;
* `driver.rs::feature_bool` (its only caller) and both driver assignments are removed;
  the review-fixture regen recipe and the workers wasm gate no longer set it;
* the fixture's `[features]` key and its ON/OFF profile pair collapse to ONE profile
  (`cornucopia-includes`), and the wiring pin asserts the default plane plus the
  per-path 501s;
* `include_support_for` returns `IncludeSupport::CornucopiaHydration` for every cornucopia
  project, unconditionally. The historical `IncludeSupport::Cornucopia` empty-data arm is
  deleted; `Unsupported` remains for a provider that cannot hydrate at all (no
  production provider — reachable only through `build_include_plan_with_support`).

This is a **deliberate behavioural change** for cornucopia consumers: a request that
sends `?include=` now hydrates the supported shapes instead of answering the P0 501, and
an unimplemented shape answers a per-path 501 naming the alias (never a silent empty
include).

---

## 1. SQL shapes

### 1.1 Taxonomy — what the resolver can even model

`api/include_path.rs` resolves three shapes, and it **refuses junction arrays before
they reach the plan** (`parent_holds_child_as_junction` / the "no back-FK column"
gates): a junction-only relationship never becomes a `ResolvedIncludePath`. The
taxonomy is therefore:

| Shape | `ResolvedIncludePath` discriminant | Status |
|---|---|---|
| scalar FK on the source | `segments.len() == 1`, `!is_array`, no override | **hydrates** |
| reverse FK on the target (one-to-many) | `segments.len() == 1`, `is_array` | **hydrates** (§1.4) |
| one-hop dot notation (the target's own child) | `segments.len() == 2` | **hydrates** (§1.4b) |
| VO→entity child table | `segments[0].child_table_override.is_some()` | **per-path 501** (deferred) |
| dot notation deeper than one hop | `segments.len() > 2` | **per-path 501** (deferred) |

### 1.2 Single-segment scalar — GET (`include_{alias}_for_{source} (source_id)`)

```sql
--! include_customer_for_work_order (source_id) : (id, name, org_id?, created_at, updated_at)
--- Eager-load `customer` for one work_order row through its own FK (issue #781 P1).
SELECT "id", "name", "org_id", "created_at", "updated_at"
FROM "shop"."customer"
WHERE "id" = (SELECT "customer_id" FROM "shop"."work_order" WHERE "id" = :source_id)
LIMIT 1;
```

Deliberate choices:

* **Correlated subquery instead of a JOIN.** The SELECT list is the shared
  `row_col_list` (unqualified `"col"` with `AS "field"` aliases + the `::text` casts from
  `needs_text_cast`). A JOIN makes `"id"` ambiguous and would force a second, prefixed
  column renderer on the single-row path. The subquery also keeps RLS symmetric: a
  source row the session cannot see yields no FK, therefore no include.
* **Columns/hints come from the target's own `EntityTree`** (the same tree
  `find_by_id` renders from), so renames, codelist-ref column renames, append-only
  `updated_at` suppression and range/numeric text casts are identical by construction.
* **Soft delete** filters on the target tree's `soft_delete_column` when it has one
  (a `SoftDelete` policy), exactly like `get_{entity}`/`list_{entity}`. When the tree
  carries no marker (the default auditable entity — the marker comes from the policy
  node, not from auditable-ness), no filter is invented: the table may not have the
  column. There is no per-include `include_deleted` request parameter (the SeaORM
  include fetch has none either); a relation of a soft-deleted source still hydrates,
  because the `include_deleted` decision belongs to the CRUD read that produced the
  source row.
* **`LIMIT 1`** because a scalar FK can only match one row; a stale FK is `Ok(None)`.

### 1.3 Single-segment scalar — LIST (`include_{alias}_batch_for_{source} (source_ids)`)

```sql
--! include_customer_batch_for_work_order (source_ids) : (source_id, id, name, org_id?, ...)
--- Eager-load `customer` for many work_order rows, keyed by source id (issue #781 P1).
SELECT "src"."source_id" AS "source_id", "t"."id", "t"."name", "t"."org_id", ...
FROM (
    SELECT "id" AS "source_id", "customer_id" AS "target_id"
    FROM "shop"."work_order"
    WHERE "id" = ANY(:source_ids::uuid[])
) AS "src"
JOIN "shop"."customer" AS "t" ON "t"."id" = "src"."target_id";
```

* One round trip, keyed by source id — the LIST envelope's `included.<field>` is a
  `HashMap<Uuid, …>` keyed by the SOURCE row id (the SeaORM batch contract), not by the
  target id.
* The derived table carries the source id + FK under distinct names, so the target
  column list is unqualified-but-unambiguous and reuses the same `select_col` renderer;
  `prefixed_row_col_list("t", …)` exists for the JOIN-qualified variant and is used here.
* `ANY(:source_ids::uuid[])` mirrors the existing `tree_{entity}_workers` query, which
  pins the explicit cast for cornucopia's type inference.
* The target's soft-delete marker appends `WHERE "t"."{marker}" IS NULL`.
* Sources with a NULL FK simply produce no target row; the adapter pre-seeds every
  requested id with `None`.

### 1.4 Single-segment array / reverse FK — LANDED (the second shape)

The reverse (one-to-many) shape landed as `CornucopiaIncludeShape::SingleArray`.
The predicate is the TARGET's own FK column — no source subquery, no JOIN (the
target table alone carries every selected column):

```sql
--! include_deployment_for_customer (source_id) : (id, customer_id?, reference, …)
--- Eager-load the `deployment` collection for one customer row through the target's reverse FK (issue #781 P1).
SELECT "id", "customer_id", "reference", "total", "created_at", "updated_at"
FROM "shop"."deployment"
WHERE "customer_id" = :source_id;                                  -- + AND marker IS NULL

--! include_deployment_batch_for_customer (source_ids) : (source_id?, id, customer_id?, reference, …)
--- Eager-load the `deployment` collection for many customer rows, grouped by the target's reverse FK (issue #781 P1).
SELECT "customer_id" AS "source_id", "id", "customer_id", "reference", "total", "created_at", "updated_at"
FROM "shop"."deployment"
WHERE "customer_id" = ANY(:source_ids::uuid[]);                    -- + AND marker IS NULL
```

Deliberate choices:

* **The reverse FK is selected FIRST under the fixed alias `"source_id"`** in
  the batch query, so the adapter groups by `row.source_id` exactly like the
  scalar batch. This is a deliberate strengthening of the design sketch's
  `row.{reverse_fk}`: a config-declared child's `parent_ref` column is exempt
  from the resolver's property gate (`resolve_child_fk_column`), so it need NOT
  appear in the target's schema properties — and therefore need not be in
  `direct_columns`/`row_col_list`. Selecting it explicitly makes the key column
  always present; the predicate column must exist physically or the query
  itself is invalid.
* **`reverse_fk_is_required` decides the key type.** Required ⇒ the selected
  alias hints `source_id` and types as `Uuid` (`let key = row.source_id;`); optional ⇒
  the hint is `source_id?`, the field types as `Option<Uuid>`
  (`let key = match row.source_id { Some(v) => v, None => continue }`).
  `= ANY(...)` never matches NULL, so the `None` arm is unreachable by
  construction but the row field type demands it.
* **No `ORDER BY`.** The SeaORM include fetch (`Entity::find().all(db)`) has no
  explicit order either; ordering parity is deliberate rather than a new
  behavioural difference between providers.
* **No `LIMIT`** — a collection is the whole matching set.
* **Soft delete** is the target tree's `soft_delete_column`, `AND`-ed onto the
  reverse-FK predicate in both queries (the scalar batch frees its `WHERE` for
  the marker; here the FK predicate owns it).
* **GET returns `Vec<{Target}Response>`** (the handler assigns
  `included.<seg> = Some(fetch…)?`; the DTO field is `Option<Vec<…>>`, and an
  empty vec is still a present `[]`), **LIST returns
  `HashMap<Uuid, Vec<{Target}Response>>`** seeded with an empty vec per
  requested id — the SeaORM array-batch contract, so the `included` envelope
  serialises identically for both providers.
* **Resolution note.** The resolver only derives `is_array` for the reverse
  direction in AUTO-discovery (a parent candidate whose child holds a scalar
  reference). An explicit `allow_include` entry derives `is_array` from a
  SOURCE-side array property, which a scalar child reference never has — so a
  reverse include is configured with `role = "root"` + no explicit
  `allow_include`, not by listing the child.

### 1.4b One-hop dot notation — LANDED (the third shape)

`customer.org` (the target's own scalar FK child) landed as
`CornucopiaIncludeShape::Dot`. Two queries per path, plus the first hop's pair
when the first hop is not modelled on its own:

```sql
--! include_customer_for_invoice (source_id) : (id, name, org_id?, …)
--- Eager-load `customer` for one invoice row through its own FK (issue #781 P1).
SELECT … FROM "shop"."customer" WHERE "id" = (SELECT "customer_id" FROM "shop"."invoice" WHERE "id" = :source_id) LIMIT 1;

--! include_customer_org_for_invoice (source_id) : (id, currency_id?, name, …)
--- Eager-load `customer.org` (the `organization` hop of the `customer` intermediate) for one invoice row through the intermediate's own FK (issue #781 P1).
SELECT "l"."id", "l"."currency_id", "l"."name", …
FROM "shop"."organization" AS "l"
JOIN "shop"."customer" AS "i" ON "i"."org_id" = "l"."id"
WHERE "i"."id" = (SELECT "customer_id" FROM "shop"."invoice" WHERE "id" = :source_id)
LIMIT 1;

--! include_customer_org_batch_for_invoice (source_ids) : (source_id, id, currency_id?, name, …)
SELECT "src"."source_id" AS "source_id", "l"."id", "l"."currency_id", …
FROM (
    SELECT "id" AS "source_id", "customer_id" AS "target_id"
    FROM "shop"."invoice"
    WHERE "id" = ANY(:source_ids::uuid[])
) AS "src"
JOIN "shop"."customer" AS "i" ON "i"."id" = "src"."target_id"
JOIN "shop"."organization" AS "l" ON "l"."id" = "i"."org_id";
```

Deliberate choices:

* **The leaf query joins through the intermediate**, addressed by the source
  relationship — the adapter binds only `source_id`/`source_ids`, never an
  intermediate FK value, so the hop is entirely a SQL fact. The leaf columns
  are qualified by the leaf alias (`prefixed_row_col_list("l", …)`) because the
  join makes unqualified `"id"` ambiguous.
* **Both hops' soft-delete markers apply** (`"i".{marker}` and `"l".{marker}`
  when each tree carries one): the GET probe's target fetch already filters
  the intermediate, but a deleted intermediate must not resurrect through the
  leaf query, and the leaf is filtered like every other read.
* **The first hop's pair is emitted by the SQL plane when it is not modelled
  on its own** (`InvoiceType` configures `customer.org` without `customer`).
  Its alias is the first segment's MODULE name, which is what the adapter
  calls (`include_{seg0_module}_for_{source}`); when a single-segment path
  with that alias exists, it is reused (emitted once, tracked by alias).
  The batch half seeds the LIST map with every source that HAS a target.
* **The combined response is built in the adapter, not SQL**: GET probes the
  intermediate (`include_{seg0}_for_…`; a miss is `Ok(None)`), maps the leaf
  into `{First}CombinedResponse { {leaf}: Some(resp), ..Default::default() }`
  and the handler performs the SAME merge as `render_get_sea_orm`'s dot arm.
  LIST returns `HashMap<Uuid, {First}CombinedResponse>` (no `Option`) —
  the SeaORM dot batch contract — seeded from the intermediate batch and
  filled by SOURCE id, so the handler's accumulation is identical.
* **`{First}CombinedResponse` lives in the SOURCE's `dto_included`** (the
  DTO plane already generates it for any multi-segment path); the adapter and
  the query wrappers import it as a sibling (`use super::dto_included::…`),
  and only the LEAF's `{Leaf}Response` is imported from a foreign
  `dto_response` — the intermediate's response is never constructed.
* **Exactly one hop.** A two-segment path whose first hop is a collection, or
  whose leaf is a collection, has no single combined leaf slot and keeps a
  named refusal; deeper paths keep their own refusal.

### 1.5 Still deferred shapes (the SQL is designed, not emitted)

* **Junction arrays.** Not reachable: the resolver drops them.
* **Deeper dot notation (depth > 2).** The depth-2 shape is the recorder; a
  three-hop chain needs either a third query or a recursive combined-struct
  mapping, and the resolver caps explicit paths at 3 segments.
* **VO→entity child table.** The fetch queries the child table
  (`segments[0].child_table_override.child_table_name`) by `parent_fk_column`, mapping
  into `{ChildStruct}Response`; already half-modelled by
  `IncludeSegment::child_table_override`, needs the VO child tree rather than the entity
  tree.

### 1.6 Tenancy / RLS — where the session vars are set

**In the query handler, not in the adapter and not in the HTTP handler.** The generated
cornucopia query handler (`templates/ddd/query.tera`) owns the provider's session story:
`self.client_source.client()` → `client.transaction()` → `set_rls_session_vars(&tx, …)`
(the five `app.*` GUCs + `SET LOCAL ROLE app_user` in one execute pair) → repo call →
`commit`. Every cornucopia read works this way, and the tree handler's comment says so
explicitly ("the query handler owns the client, transaction and RLS session vars").

P1's wrappers follow that shape exactly (§4), so the include SQL is subject to the SAME
policies as the source read:

* the target table's RLS (org isolation + scope) filters returned rows;
* the source subquery is equally fenced, so a cross-tenant `source_id` yields no include;
* a scope/role denial surfaces as `{Domain}Error::from_repo_err` → `403` via
  `AppError::from_domain_error`, the same classification as every other read.

No SQL in the include queries mentions the tenant column: like the rest of the
cornucopia plane, filtering is the policy's job, and DDL triggers stamp writes.

---

## 2. Mapping rows to `{Target}Response` (incl. child-table hydration)

The adapter reuses the CRUD read's mapping rather than a second one:

* `ddd/cornucopia_repo.rs::emit_response_expr(tree, code, "row", pad, prefix)` — emits
  `{Entity}Response { id: row.id, <column → DTO projections>, <child fields>,
  created_at, updated_at, ..Default::default() }` via
  `emit_entity_to_dto_field` (codelist `.parse()`, Decimal-as-String re-parse,
  structured-wrapper `from_value`, UUID/String adaptation) and
  `emit_child_field_population`.
* `emit_child_reads_cornucopia` — per-row `{target_table}::list_{target}_{child}()`
  reads (and junction id reads), recursively for grandchildren. That is why the
  adapter needs the TARGET's query module in scope, not just its DTOs.
* Facts needed: the target's `EntityTree` (`RepositoryImplEmitter::query_entity_tree`),
  built in the generator when the flag is on. Its `direct_columns` drive the SELECT list,
  its `child_tables`/`junction_tables` drive hydration, `append_only` suppresses
  `updated_at`, `has_workflow` emits `workflow_state: None` (populated later by the
  query plane's SELECT-time derived step, same as CRUD reads).
* Import surface emitted by `emit_include_imports`:
  `use crate::domain::{target_domain}::{target_module}::dto_response::{Target}Response;`
  plus, per flattened target child, `…::dto_response::{ChildStruct}Response;`, plus
  `use cornucopia_queries::queries::{target_domain}::{target_module};` for the child
  reads. Names already imported by the source's own header (self-target, or a child
  struct name collision) are skipped — `ChildTableInfo::struct_name` is parent-prefixed
  (`CandidateAddress`), so cross-entity collisions are rare but the skip-first rule
  matches the SeaORM emitter's behaviour.
* **Known cost, inherited deliberately:** child-table hydration is one query per
  row/child (N+1). The SeaORM include fetch has the same shape
  (`emit_child_reads` inside the row loop); a batched child fetch is a shared
  improvement, not a P1 regression.

---

## 3. Adapter surface

Emitted into `src/domain/{domain}/{module}/cornucopia_repository_impl.rs` as an
**inherent** impl block after the trait impl (the provider-generic trait stays CRUD-only;
the SeaORM fetch methods are inherent too):

```rust
impl WorkOrderRepositoryImpl {
    pub async fn fetch_customer_for_work_order<C: GenericClient + Sync>(
        &self, db: &C, source_id: Uuid,
    ) -> Result<Option<CustomerResponse>, Box<dyn std::error::Error>> { … }

    pub async fn fetch_customer_batch_for_work_order<C: GenericClient + Sync>(
        &self, db: &C, source_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Option<CustomerResponse>>, Box<dyn std::error::Error>> { … }

    // Reverse-FK (array) shape: the whole collection on GET, grouped on LIST.
    pub async fn fetch_work_order_for_customer<C: GenericClient + Sync>(
        &self, db: &C, source_id: Uuid,
    ) -> Result<Vec<WorkOrderResponse>, Box<dyn std::error::Error>> { … }

    pub async fn fetch_work_order_batch_for_customer<C: GenericClient + Sync>(
        &self, db: &C, source_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Vec<WorkOrderResponse>>, Box<dyn std::error::Error>> { … }
}
```

* Method names are **exactly** `ResolvedIncludePath::fetch_method` /
  `batch_fetch_method` — the handler block and the adapter are generated from the same
  resolved path, so the call cannot desync (the SeaORM emitter's contract).
* Binding: `{source_table}::include_{alias}_for_{table}().bind(db, &source_id)` and
  `…_batch_…().bind(db, source_ids)` (`&[Uuid]` is what cornucopia generates for a
  `uuid[]` parameter; the tree adapter passes `&Vec` into the same slot).
* Batch keying: seed `result[*id] = None` for every requested id, then
  `result.insert(row.source_id, Some(resp))` per row — identical to the SeaORM
  `HashMap<Uuid, Option<T>>` contract, so `serde_json::to_value` in the LIST block yields
  the same JSON for both providers.
* Array keying: seed `result.entry(*id).or_insert_with(Vec::new)` per requested id,
  then `let key = row.source_id;` (required reverse FK) or
  `let key = match row.source_id { Some(v) => v, None => continue };` (nullable) and
  `result.entry(key).or_default().push(resp)` — the SeaORM array-batch contract.
* A self-referencing include (target module == source module) works: the target tree is
  re-resolved from the same title and the imports are skipped (the header already
  carries them).

---

## 4. Handler blocks

`api/include_plan.rs` renders both arms in Rust (no template branching), mirroring the
tree handler's query-layer call shape.

### GET (`render_get_cornucopia`)

The scalar shape assigns the fetched value directly; the reverse-FK shape
wraps the returned `Vec` in `Some(…)` (the DTO field is `Option<Vec<…>>`).

```rust
for path in &include_paths {
    let unsupported = match path.as_str() {
        "customer.org" => Some("dot-notation includes (a target's child) are not implemented on the cornucopia SQL-first plane yet"),
        _ => None,
    };
    if let Some(reason) = unsupported {
        return Err(AppError {
            status: axum::http::StatusCode::NOT_IMPLEMENTED,
            code: "NOT_IMPLEMENTED",
            message: format!("?include={path} is not supported: {reason} (issue #781)"),
            details: None,
            correlation_id: Some(correlation_id),
        });
    }
    match path.as_str() {
        "customer" => {
            included.customer = state.shop_work_order_queries
                .fetch_customer_for_work_order(id, api_key_info.api_key_id,
                    api_key_info.organization_id, api_key_info.user_id,
                    api_key_info.role.clone()).await
                .map_err(|e: ShopError| {
                    AppError::from_domain_error(e.http_status(), e.to_string(), "Failed to include customer")
                        .with_correlation_id(correlation_id)
                })?;
        }
        "work_order" => {
            // Reverse-FK collection: `Some(vec)` — an empty vec is still `[]`.
            included.work_order = Some(state.shop_customer_queries
                .fetch_work_order_for_customer(id, api_key_info.api_key_id, …).await
                .map_err(|e: ShopError| { … })?);
        }
        _ => {}
    }
}
```

### LIST (`render_list_cornucopia`)

The same per-path guard first (outside the `if`), then the SeaORM-identical envelope
shape:

```rust
let included = if !include_paths.is_empty() {
    let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
    let mut included_map = std::collections::HashMap::<String, serde_json::Value>::new();
    if include_paths.contains(&"customer".to_string()) {
        included_map.insert("customer".to_string(),
            serde_json::to_value(state.shop_work_order_queries
                .fetch_customer_batch_for_work_order(&ids, api_key_info.api_key_id, …).await
                .map_err(|e: ShopError| { … })?).unwrap_or_default());
    }
    included_map
} else { std::collections::HashMap::new() };
```

Session argument plumbing: the wrapper signature is
`(… , api_key_id, organization_id, user_id, role: String)` — the exact order every other
cornucopia query method takes, so the handler block passes
`api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id,
api_key_info.role.clone()` verbatim (what the tree handler passes).

### Query-handler wrappers

`ddd/query.rs::cornucopia_include_query_surface` pre-renders one GET + one LIST method
per implemented path into `QueryContext.include_query_methods`; `templates/ddd/query.tera`
splices them just before the impl's closing brace, and
`QueryContext.include_response_imports` after the error import. Each wrapper is the
tree/CRUD shape: client → transaction → `set_rls_session_vars` → adapter fetch → commit,
mapping errors through `{Domain}Error::from_repo_err`. Both template insertions are
`{%- if … %}`-guarded so a flag-off file is byte-identical.

### Rendered-byte contract

Blocks are whitespace-exact (handler-fn-body indent, no trailing newline), like the
guard/invariant blocks. The pipeline test parses every emitted file with `syn` — this
is how the PoC caught two real defects (a missing closure brace in the `map_err` block
and a missing `)` closing `serde_json::to_value` in the LIST arm).

---

## 5. Capability gating — the per-path support verdict

```rust
pub enum IncludeSupport {
    SeaOrm,                                        // every resolved path hydrates
    CornucopiaHydration,                           // default for cornucopia: per-path verdict
    Unsupported { reason: String },                // provider cannot hydrate at all (injected only)
}

pub enum IncludePathSupport { Supported, Unsupported { reason: String } }

pub enum CornucopiaIncludeShape { SingleScalar, SingleArray }

impl IncludeSupport {
    pub fn path_support(&self, path: &ResolvedIncludePath) -> IncludePathSupport;
    pub fn supported_paths<'a>(&self, paths: &'a [ResolvedIncludePath]) -> Vec<&'a ResolvedIncludePath>;
    pub fn unsupported_paths(&self, paths: &[ResolvedIncludePath]) -> Vec<UnsupportedInclude>;
}
```

`CornucopiaIncludeShape::classify` accepts exactly what the SQL/adapter/query planes
implement and **names the construct** otherwise:

| Path shape | Verdict | Reason text (in the 501) |
|---|---|---|
| 1 segment, `!is_array`, no child-table override | `SingleScalar` | — |
| 1 segment, `is_array`, resolvable reverse FK | `SingleArray` | — |
| 1 segment, `is_array`, no reverse FK (defensive) | unsupported | "array / reverse-FK include carries no back-FK column to scan the target by" |
| 2 segments, scalar hops, no override, leaf FK present | `Dot` | — |
| 2 segments, first hop is a collection | unsupported | "dot-notation includes whose first hop is a collection are not implemented …" |
| 2 segments, leaf is a collection | unsupported | "dot-notation includes whose leaf is a collection are not implemented …" |
| 2 segments, no leaf FK (defensive) | unsupported | "dot-notation include carries no FK column on the first hop to address its leaf by" |
| `child_table_override` | unsupported | "VO→entity child-table includes are not implemented …" |
| >2 segments | unsupported | "dot-notation includes deeper than one hop (a target's child's child) are not implemented …" |
| 0 segments (defensive) | unsupported | "the path resolves to no segment" |

**One predicate, four consumers** — this is the invariant that keeps the planes honest:

1. the handler blocks (what hydrates, what 501s);
2. `db/cornucopia_queries.rs` (which SQL is emitted);
3. `ddd/cornucopia_repo.rs` (which adapter methods exist — a handler must never call a
   method the adapter skipped: E0599);
4. `ddd/query.rs` (which wrappers exist) and `api/hurl_contract/include.rs` (which legs
   the generated contract advertises — an unsupported shape emits the omission comment,
   never a leg).

Adding a shape = add a `CornucopiaIncludeShape` variant + its SQL + its adapter arm + its
wrapper arm + the KATs, in one PR; the classifier is the only switch.

---

## 6. Test plan

### Landed (node-free, PR CI)

| Gate | Covers |
|---|---|
| `cargo test -p codegraph-generate --lib -- api include_plan cornucopia` | 22 include-plan KATs (default-cornucopia hydration, per-path support, classifier reasons + all three accepted shapes + the dot refusals, GET block bytes incl. the `Some(collection)` arm and the dot merge, LIST block bytes incl. the dot accumulation, all-unsupported, injected-`Unsupported` 501, doc lines) · 10 SQL KATs (`db::cornucopia_queries::include_query_tests`: correlated-subquery shape, batch keying/qualification, append-only/aliases, text casts, reverse-FK single predicate, reverse-FK batch keying + `uuid[]` cast, array append-only/marker-free arm, **dot join single + batch with both markers**) · 7 adapter KATs (`ddd::cornucopia_repo_include::include_adapter_tests`: query calls + batch keying, imports, self-target, array collection/grouping, required vs optional reverse-FK key, **dot leaf merge + dot import surface**) · 12 hurl KATs (cornucopia gets the same provable legs as SeaORM; injected-`Unsupported` gets one comment + no legs; per-path omission incl. the supported-but-unprovable dot comment). |
| `cargo test -p codegraph --test cornucopia_include_tests` | The **wiring pin**: one mox fixture, ONE profile, through `driver::run`. Asserts the scalar SQL pair, the reverse-FK SQL pair (optional `work_order` + required `invoice`), the dot plane (the intermediate probe pair emitted for a dot-only source, the leaf join pair keyed by source id, the adapter's combined merge/seed, the wrappers' `Option<Combined>`/`HashMap<Uuid, Combined>` returns, the handler's merge block), and the retained per-path 501 arms (`customer.org.currency`, depth 3) emitting NO SQL / adapter method / wrapper while the one-hop path on the SAME handler hydrates; `syn`-parses nine emitted files. |
| `file_length_contract` | The adapter emission was split into `ddd/cornucopia_repo_include.rs` (issue #751 budget: `cornucopia_repo.rs` 1,780 → 1,829 lines, new module 415) — the test stays green with no allow-list entry. |
| `handler_matrix_tests` | The flipped `cornucopia_include` permutation (see §8) — re-blessed, exactly one line moved. |
| `hurl_contract_tests` / `review_fixture_drift_tests` / `policy_rls_tests` / `service_ops_tests` / `include_junction_tests` | Verified green with no movement (the review and policy_rls fixtures are SeaORM). |

### Landed with the flip (all gates green)

1. **Compile pin (#551 policy) — LANDED.**
   `crates/codegraph/tests/cornucopia_include_compile_tests.rs` assembles a scratch app from
   the emitted pieces (domain-types, target `dto_response`/`dto_included`, `api/links.rs`,
   the sliced include-adapter `impl` with its own imports, the emitted
   `set_rls_session_vars` compiled verbatim + all eight `fetch_*` wrappers hosted on
   `*Queries` structs matching the generated handler's field names/types, and a probe
   awaiting every handler call expression) over a **stubbed** `cornucopia-queries` built
   from `test_framework::cornucopia_queries_stub` plus the eight hint-shaped `include_*`
   query modules. It `cargo check`s the app crate (PR-CI, ~2 s warm). The stub surface is
   pinned EXACTLY to the emitted SQL `--! include_*` names and to the handler call set, so
   a renamed query fails before compiling — the call-set pin now accepts adapter calls
   too (a dot path's first-hop probe pair is called from the adapter, not a handler).
   **Dot notation compiles its MERGE, not just its calls**: each emitted dot arm's
   `let leaf_resp = …; if let Some(val) = leaf_resp { … }` block is sliced verbatim
   into a typed probe over the real `{Entity}IncludedData` and the compiled wrapper
   return type, so a leaf-field rename (E0609) or a first-segment DTO-type collision
   (the `customer` + `customer.org` alias hazard) fails `cargo check`. Teeth validated
   by mutation: a row-shape rename (`E0609`), a bind-arity drop (`E0061`) and a
   `combined.{leaf}` rename (`E0609`) all fail at the emitted adapter/merge. The wasm32
   leg is delegated to the #550 worker gate (the emitters have no `cfg(target_arch)`
   branches; the only wasm questions are `Send`-ness/object safety it already exercises;
   re-run green with the dot changes).
2. **Real-Postgres runtime pin — LANDED.**
   `crates/codegraph/tests/cornucopia_include_runtime_tests.rs` (ignored, the
   `test-ops-integration` lane) generates the SAME shop fixture (the include plane is
   default ON), provisions a unique database through `test_framework::postgres::GateDb`, applies
   the **emitted** table DDL + the emitted `_rls.sql` band (plus a test-owned bootstrap
   for the session-role band a minimal profile does not emit: `app_user`/`api_key`
   roles, `public.get_current_org_id()` verbatim from `api_key_migration.tera`, and a
   tenancy-only `enforce_api_key_scope` stand-in), seeds two orgs, and executes the
   **emitted** `--! include_*` statements with `:source_id`/`:source_ids` bound as `$1`
   (`::uuid[]` preserved) inside a transaction carrying the emitted session bundle
   (`set_config` × 5 + `SET LOCAL ROLE app_user`). Asserted: scalar GET/LIST hydration
   with the batch keyed by SOURCE id; the reverse-FK collection (optional + required FK
   arms) and its batch grouping; the dot hop's leaf; cross-tenant isolation for the
   scalar/batch/dot shapes (an org-A source whose FK points at an org-B target yields
   no row — teeth proven by dropping the session bundle, which fails that leg) plus the
   reverse shape's per-ROW fence; and the soft-delete contract. §9's risk #1 is what
   this gate exists for. **Soft-delete caveat, now recorded:** nothing on the mox-first
   driver path ingests a `SoftDelete` policy (`policy_ingest::
   ingest_policies_and_relationships` has no caller), so the fixture's SQL carries no
   marker; the suite therefore pins the designed marker-less behaviour (a soft-deleted
   target is still returned, a soft-deleted source still hydrates) AND executes the
   emitter's marker suffix (`render_include_sql`) against the same statement to prove
   its semantics on the migrated schema. The end-to-end marker arm needs an input that
   produces the policy — a follow-up, not a guess.
3. **wasm32 worker gate — LANDED.** `worker_wasm_compile_tests` runs the
   workers-topology cornucopia fixture (the test writes its `ProjectConfig`
   directly — no `profiles.toml`; the include plane is default ON, so it sets
   no switch), so the adapter + wrapper + handler call sites compile natively AND on
   `wasm32-unknown-unknown` against the real worker `AppState`. Both implemented
   shapes are covered: the reverse-FK collection (`PayRunLineType`'s config
   `parent_ref` FK, `--!` hint `source_id?`) and a same-domain scalar
   (`PayRunType.compensation`, cross-domain paths are dropped by the resolver
   under workers). The stub `QUERIES_RS` carries the four `include_*` pairs and
   the target's CRUD surface, pinned to the emitted `--!` names by the existing
   `assert_stub_tripwire`; `assert_include_surface` pins the SQL ⇄ adapter ⇄
   wrapper ⇄ handler wiring before the compile phases. **Answer to the open
   question: green on native and wasm32** — the async wrappers stay `Send`
   (`ClientSource` + `GenericClient`), so no emission change was owed. Teeth
   re-validated by mutation: renaming a stub row field fails the wasm check
   (E0308) at the emitted adapter.
4. **Hurl include legs for cornucopia — LANDED.** `sections_for_paths` no longer
   gates cornucopia out: it emits the provable legs for supported shapes. A reverse-FK
   collection is unconditionally provable (`included.<seg>` is `Some(vec)` — `[]` when
   empty), so the array shape gets both legs; a scalar needs a required FK the create
   body wires; a dot path is omitted with the documented comment (the combined leaf is
   an `Option`); an unimplemented SHAPE gets a per-path omission comment. The provider
   gate now only fires for injected `IncludeSupport::Unsupported`.
5. **Matrix re-bless — LANDED** — see §8.

---

## 7. Deferrals (explicit, not guessed)

* Dot notation deeper than one hop (depth > 2, `customer.org.country`); the
  depth-2 shape is landed (§1.4b).
* A same-handler `customer` + `customer.org` pair: the DTO plane's
  `include_fields` dedupes by first-segment field alias (first wins), so a
  single-segment path and a dot path sharing the first segment claim the same
  `included.<first>` slot. This is a PRE-EXISTING DTO-plane limitation shared
  with the SeaORM emitter (the fixture's dot source deliberately does not also
  list `customer`), not something this slice introduced.
* VO→entity child-table includes.
* Batching the child-table hydration (shared N+1 with the SeaORM plane).
* `namespace_layout` target-module paths in the adapter imports (the SeaORM emitter
  threads `namespace_rust_prefix`; cornucopia + namespace + includes is unexercised).
* Cross-domain includes under the workers topology (the resolver already drops them —
  `filter_cross_domain_paths` — so P1 inherits the same limitation).
* `include_deleted`-aware includes (no request parameter exists on any provider).
* Per-include field selection / nested include arguments.
* Collection ordering (`ORDER BY`) — the SeaORM include fetch has none either (§1.4).

---

## 8. Handler-matrix re-bless (EXECUTED — exactly one permutation moved)

With the switch flipped (`include_support_for` returning `CornucopiaHydration` for
cornucopia projects), exactly ONE permutation changed:
`cornucopia_include` in `crates/codegraph/tests/fixtures/handler_matrix.sha256`
(`4963815b…` → `4e27aad4…`). Its three include keys became:

* `include_get_block` — the `for path in &include_paths { … match "person" =>
  included.person = state.common_task_queries.fetch_person_for_task(id, api_key_info…) … }`
  block from §4 (the fixture's one scalar path is `SingleScalar`, so no 501 guard);
* `include_list_block` — the `included_map` batch block from §4;
* `include_doc` — "Eager-load related entities. The cornucopia persistence provider
  hydrates scalar `refers` includes, reverse-FK collections, and one-hop dot-notation
  includes; any other modelled shape answers 501 (issue #781)."

The other 19 permutations (all `ProjectConfig::default()` = SeaORM) did not move. No
committed `crates/review/` tree or `policy_rls` snapshot moved either (both SeaORM
fixtures). Blessed with `HANDLER_MATRIX_BLESS=1` and an enumerated diff.

---

## 9. Dangerous parts, ranked

1. **RLS scoping.** The include SQL relies entirely on the query handler's session
   bundle. A wrapper that forgets `set_rls_session_vars` (or a future refactor that
   calls the adapter from the HTTP handler) silently widens tenancy. The runtime pin's
   cross-tenant leg is the only thing that can prove this — it is the #1 next-slice gate.
2. **Soft delete.** Two failure modes, both silent: naming a marker column the table
   lacks (500 on every include) and ignoring a marker that exists (deleted rows served).
   The filter keys on the target tree's `soft_delete_column`, the same value
   `get_{entity}` uses — one derivation, both queries.
3. **Compile-before-switch.** The adapter/`query.rs`/handler are three files that must
   agree on names, types, and imports. A `syn` parse gate catches syntax only; the
   scratch-crate compile pin must land before the flag flips.
4. **Batch keying.** Keying the map by target id instead of source id compiles and
   silently returns nothing for every row. Pinned by KAT text + the runtime leg.
5. **Child hydration imports.** Cross-entity child struct names, self-targets, and
   duplicate imports are E0252/E0425 land. Skip-first dedupe matches the SeaORM emitter;
   the compile pin is the arbiter.
6. **`ANY(:source_ids::uuid[])` type inference.** Cornucopia infers `uuid[]` from the
   comparison; the explicit cast mirrors the existing `tree_workers` query (the only
   other array-parameter query on this plane).

---

## 10. Recommendation

Continue with this split, in this order:

1. land the compile pin (scratch crate + stub, native) — cheap, catches the class the
   parse gate cannot;
2. extend the runtime harness (real Postgres, two orgs, soft delete) — closes the RLS
   and soft-delete risks;
3. add a flag-ON workers fixture to the wasm gate — **LANDED** (§6 item 3);
4. ~~implement reverse-FK/array~~ — **LANDED** (§1.4): `SingleArray` + its SQL,
   adapter, wrapper, handler arm and KATs. The compile pin (item 1) must now stub the
   array query pair too — see the report's stub list.
5. ~~dot notation~~ — **LANDED** (§1.4b): `Dot` + the intermediate/leaf query
   pair, the adapter's combined-response merge and seed, the wrappers'
   `Option<Combined>`/`HashMap<Uuid, Combined>` returns, and the handler's
   merge/accumulation arms. The compile pin stubs the new `--!` surface and
   compiles it. Depth > 2 stays a named 501.
6. ~~flip the switch by default, then delete the preview flag~~ — **LANDED**: the flag
   field/driver plumbing/profile pair are deleted, `include_support_for` returns
   `CornucopiaHydration` for cornucopia, the matrix is re-blessed (§8), and no review /
   `policy_rls` fixture moved.
