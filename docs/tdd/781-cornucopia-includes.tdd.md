# TDD — cornucopia `?include=` hydration (issue #781 P1)

Status: **contained PoC on `ws/781-cornucopia-includes`** (branch stacked on the open
`ws/778-handler-refactor` PR). The production switch
(`cornucopia_include_hydration`) is **OFF by default**, so every existing cornucopia
project keeps the issue #781 **P0** behaviour (a request that sends `?include=` answers a
guarded `501 NOT_IMPLEMENTED`). The generated surface, the per-path capability verdict,
and the KAT/pipeline pins land here; the compile pin, the live runtime pin, and the
switch flip are the next slices.

Related records: `docs/tdd/778-handler-refactor.tdd.md` §6 (the P0 landing and the P1
hand-off), `AGENTS.md` → "API handler generation plan (issue #778)".

---

## 0. What exists at HEAD, and what the P1 gap actually is

The `?include=` decision plane was already provider-agnostic:

| Layer | File | Today |
|---|---|---|
| Path resolution | `crates/codegraph-generate/src/api/include_path.rs` | Fully implemented, provider-agnostic (`ResolvedIncludePath` = alias + segments + `fetch_method` / `batch_fetch_method` / `response_rust_type`) |
| Handler blocks | `crates/codegraph-generate/src/api/include_plan.rs` | SeaORM renders real hydration; cornucopia renders the P0 guarded 501 |
| SeaORM execution | `ddd/repository_emitter/tree.rs::emit_include_fetch_methods` | Inherent `fetch_*_for_*` / `fetch_*_batch_for_*` on the source `{Entity}RepositoryImpl` |
| Cornucopia execution | — | **Nothing.** `db/cornucopia_queries.rs` and `ddd/cornucopia_repo.rs` have no include surface at all |

So P1 is not "wire a flag": it is three new emission sites (SQL, adapter, query handler)
plus a per-path capability verdict so a shape the plane does not implement still fails
closed instead of returning empty data.

### The switch

`[features] cornucopia_include_hydration = true` in `profiles.toml`.

* `BuildPlan` has **no typed field** for it (it is read from the raw
  `BuildPlan.features` table via `driver.rs::feature_bool`, deliberately: a typed field
  would churn every `BuildPlan` literal in the test suite for one preview bit).
* The driver threads it into `ProjectConfig.codegen.cornucopia_include_hydration`
  (`crates/codegraph/src/driver.rs`, both the plan-built and the plan-mutating site).
* `CodegenConfig.cornucopia_include_hydration` is `#[serde(default)]` = `false`, so a
  plan-less run, a consumer that never sets the feature, and every test fixture are
  byte-identical.
* Non-bool values read as absent (a typo can never turn a preview plane ON). Once the
  feature graduates, this becomes a typed strict bool like `ux_rules`; the deletion is
  part of the flip PR.

---

## 1. SQL shapes

### 1.1 Taxonomy — what the resolver can even model

`api/include_path.rs` resolves three shapes, and it **refuses junction arrays before
they reach the plan** (`parent_holds_child_as_junction` / the "no back-FK column"
gates): a junction-only relationship never becomes a `ResolvedIncludePath`. The
taxonomy is therefore:

| Shape | `ResolvedIncludePath` discriminant | PoC |
|---|---|---|
| scalar FK on the source | `segments.len() == 1`, `!is_array`, no override | **implemented** |
| reverse FK on the target (one-to-many) | `segments.len() == 1`, `is_array` | **implemented** (§1.4) |
| VO→entity child table | `segments[0].child_table_override.is_some()` | deferred |
| dot-notation (target's child) | `segments.len() > 1` | deferred |

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

### 1.5 Still deferred shapes (the SQL is designed, not emitted)

* **Junction arrays.** Not reachable: the resolver drops them.
* **Dot notation.** Per the SeaORM emitter, the combined response is the FIRST
  segment's row plus a merged leaf field (`DeploymentCombinedResponse`). The SQL shape is
  the first hop's single/batch query PLUS the leaf hop addressed through the first
  target's own FK (or reverse FK) — i.e. two queries and a Rust merge identical to
  `render_get_sea_orm`'s accumulation, with the leaf type imported from
  `dto_included`. It needs the combined-struct mapping to be written against
  `dto_included` (the shared response mapping today renders `{Entity}Response`), which is
  why it is a separate slice.
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
    Cornucopia,                                    // historical empty-data arm (explicit injection only)
    CornucopiaHydration,                           // preview flag ON: per-path verdict
    Unsupported { reason: String },                // whole-provider refusal (P0)
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
| `child_table_override` | unsupported | "VO→entity child-table includes are not implemented …" |
| >1 segment | unsupported | "dot-notation includes (a target's child) are not implemented …" |
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

### Landed in this PoC (node-free, PR CI)

| Gate | Covers |
|---|---|
| `cargo test -p codegraph-generate --lib -- api include_plan cornucopia` | 21 include-plan KATs (incl. the cornucopia ones: flag-off P0, flag-on per-path support, classifier reasons + both accepted shapes, GET block bytes incl. the `Some(collection)` arm, LIST block bytes, all-unsupported, doc line) · 7 SQL KATs (`db::cornucopia_queries::include_query_tests`: correlated-subquery shape, batch keying/qualification, append-only/aliases, text casts, reverse-FK single predicate, reverse-FK batch keying + `uuid[]` cast, array append-only/marker-free arm) · 5 adapter KATs (`ddd::cornucopia_repo_include::include_adapter_tests`: query calls + batch keying, imports, self-target, array collection/grouping, required vs optional reverse-FK key) · 1 hurl KAT (per-path omission) |
| `cargo test -p codegraph --test cornucopia_include_tests` | The **wiring pin**: one mox fixture, two profiles (flag ON / OFF) through `driver::run`. ON asserts the scalar SQL pair AND the reverse-FK SQL pair (optional `work_order` + required `invoice`), the inherent adapter methods + imports + collection/grouping shapes, the query wrappers (`Vec`/`HashMap<Uuid, Vec<_>>`) + `set_rls_session_vars`, the handler hydration calls (direct assignment + `Some(…)`), the per-path 501 for `customer.org`, and `syn`-parses six emitted files. OFF asserts no include SQL / adapter / wrappers (scalar or array) and the P0 guarded 501 on both sides. |
| `file_length_contract` | The adapter emission was split into `ddd/cornucopia_repo_include.rs` (issue #751 budget: `cornucopia_repo.rs` 1,780 → 1,829 lines, new module 415) — the test stays green with no allow-list entry. |
| `handler_matrix_tests` | **Unchanged** in this slice (the matrix's `cornucopia_include` permutation uses a default `ProjectConfig`, so the flag is off). |
| `hurl_contract_tests` / `review_fixture_drift_tests` / `service_ops_tests` / `include_junction_tests` | Verified green (no flag-off movement). |

### To add BEFORE flipping the switch

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
   a renamed query fails before compiling. Teeth validated by mutation: a row-shape rename
   (`E0609`) and a bind-arity drop (`E0061`) both fail at the emitted adapter. The wasm32
   leg is delegated to the #550 worker gate (the emitters have no `cfg(target_arch)`
   branches; the only wasm questions are `Send`-ness/object safety it already exercises).
2. **Real-Postgres runtime pin.** Extend `hooks_cornucopia_runtime_tests` (or a sibling
   binary) with the shop fixture from `cornucopia_include_tests`: boot the assembled
   tree against Postgres, seed two orgs, and assert (a) GET hydrates the scalar include,
   (b) LIST keys the map by source id, (c) an unsupported shape answers 501, (d) a
   cross-tenant `source_id` yields no include, (e) a soft-deleted target (policy present)
   is excluded, (f) a soft-deleted source still hydrates (the `include_deleted` decision
   stays with the CRUD read).
3. **wasm32 worker gate — LANDED.** `worker_wasm_compile_tests` runs the
   workers-topology cornucopia fixture with `codegen.cornucopia_include_hydration
   = true` (the test writes its `ProjectConfig` directly — no `profiles.toml`;
   the driver sets the same field from `feature_bool("cornucopia_include_hydration")`),
   so the adapter + wrapper + handler call sites compile natively AND on
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
4. **Hurl include legs for cornucopia.** `sections_for_paths` already omits unsupported
   shapes and emits provable legs for supported ones. A reverse-FK collection is
   unconditionally provable (`included.<seg>` is `Some(vec)` — `[]` when empty), so the
   array shape gets both legs from the same planner; the live proof belongs to the
   runtime gate above.
5. **Matrix re-bless** — see §8.

---

## 7. Deferrals (explicit, not guessed)

* Dot notation / combined responses (needs the `dto_included` mapping surface).
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

## 8. Expected handler-matrix re-bless (NOT blessed here)

When the switch flips by default (feature graduated, `include_support_for` returning
`CornucopiaHydration` for cornucopia projects), exactly ONE permutation changes:
`cornucopia_include` in `crates/codegraph/tests/fixtures/handler_matrix.sha256`. Its three
include keys become:

* `include_get_block` — the `for path in &include_paths { … match "person" =>
  included.person = state.common_task_queries.fetch_person_for_task(id, api_key_info…) … }`
  block from §4 (the fixture's one scalar path is `SingleScalar`, so no 501 guard);
* `include_list_block` — the `included_map` batch block from §4;
* `include_doc` — "Eager-load related entities. The cornucopia persistence provider
  hydrates scalar `refers` includes; any other modelled shape answers 501 (issue #781)."

The other 19 permutations (all `ProjectConfig::default()` = SeaORM) do not move. No
committed `crates/review/` tree moves either (it is a SeaORM fixture). Re-bless only with
`HANDLER_MATRIX_BLESS=1` and an enumerated diff, in the flip PR.

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
5. dot notation last (it drags in the `dto_included` combined-response mapping);
6. flip the switch by default (typed strict feature + profiles.toml + matrix re-bless +
   review-fixture regeneration note), then delete the preview flag.
