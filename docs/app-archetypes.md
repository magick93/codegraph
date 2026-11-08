# Beyond the CRUD archetype — app archetypes, custom verbs, headless UX (RFC)

**Status**: proposal (no code in this document). **Evidence base**: the
LexoRank/Kanban capability exercise, `docs/lexorank-kanban.md` (PR #584),
which built a Trello-like board entirely from consumer-side artifacts and
catalogued every workaround it needed. All `file:line` anchors are against
master `d20c2925e`.

---

## 1. Problem statement

Codegraph's generators encode an implicit application archetype: **LOB
CRUD + workflow + forms/tables UI**. That archetype is served extremely
well — model-driven DDL/RLS/repositories/handlers/DTOs/hurl/UI/e2e with
byte-identical discipline. Applications outside the archetype hit the
same two walls, whatever the domain:

**Axis 1 — behavior verbs that aren't CRUD.** "Move card", "allocate
slot", "score application", "publish issue". Every behavior seam fails
identically today (all verified in the LexoRank exercise, §3 gap 1 there):

| Seam | Structural failure |
|---|---|
| mox `op` | compute-only: the `Resource` arena carries no DB handle; routed handlers are read-compute-return (`ddd/mox_ops.rs` `signature()`:730–754 fixes `(&self, res: &Resource, …)`; `templates/api/handler.tera:1158–1195` never writes back) |
| function bindings `manual` | `ManualImpls` is **sync + pure** (`ddd/functions.rs:624–726`) — typed inputs, no tx, no provider handle |
| lifecycle hooks | `TxContext::execute` is rows-affected-only (write-not-query, `templates/hooks/types.tera:32`), and `before_create`/`before_update` receive **immutable** payloads (`templates/hooks/lifecycle_trait.tera:35`) |
| `custom_routes` domains | entity-less by design (`ordering.rs:75`); in the monolith the router file isn't emitted at all (`api/router.rs:225–241`) and the server fold is `ext_points`-gated with the `custom_routes` variable unbound in the render context (`templates/scaffold/server.tera:683`) — inert |
| `HookRegistry::add_routes` | never wired in the stock monolith (same fold), and constrained to `Router<()>` because extension crates can't see `AppState` (`templates/hooks/registry.tera:71–81`) |

The result: consumers drop to hand-written sibling modules that
participate in **none** of the generated contracts — no OpenAPI entry, no
hurl contract, no permission-scope vocabulary, no UI command plane, no
POM/spec surface. Custom code becomes a second-class citizen that every
regeneration could silently strand.

**Axis 2 — interactive UX that isn't tables/forms.** Drag-and-drop
boards, calendars, canvases, maps, timelines-as-interaction. The entity
UI plane owns presentation *and* data plumbing together, so swapping the
widget loses everything downstream (stores, POM page classes, ux specs,
testids). Two seeds of the right idea exist but don't generalize:

- `ifml-components.toml` maps IFML slots to consumer components through a
  tiered selector with a 10-variant `SemanticRole` ontology
  (`codegraph-config/src/ifml_components.rs:9–22`, resolution
  `:93–132`) — but the **entity plane consumes no mappings at all**
  (zero references under `crates/codegraph-generate/src/ui/`), and there
  is no *modeled* way to say "this entity is an ordered collection" —
  the route generator merely derives roles from shapes it already knows
  (`route_generator/roles.rs:24–39`).
- `ui_detail_extensions` (#162) mounts consumer Svelte components in
  generated detail pages — but with exactly one conventional prop
  (`entityId`, `templates/ui/detail_page.tera:289–294`), a flat
  name-keyed list, no selector tiers, no pack merge, no list/detail/form
  slots.

**Axis 3 (observed, scoped out — §9): transport.** Realtime presence,
websocket/sync protocols. Neither of the moves below pretends to cover
it; the archetype matrix in §7 makes the gap explicit.

The strategic claim of this RFC: **the CRUD archetype is not a limitation
of the model — it is a limitation of the contract surface.** The two
moves below (operations plane, headless UX capability layer) plus two
cheap enablers (model vocabulary completion, escape-hatch discipline)
generalize the contract surface so that non-CRUD archetypes ride the same
byte-identical, compile-pinned, contract-complete machinery.

---

## 2. Design principles

Every proposal here instantiates principles the codebase already follows
somewhere — the work is generalizing them, not inventing them.

| # | Principle | Existing precedent |
|---|---|---|
| P1 | **Model the contract, not the widget.** For interactive UX, model the data/interaction *contract*; generate everything except presentation; consumers swap the widget through mappings. | TanStack form layer (#580): generated zod schema + wiring, rendered through components. Charts plane (#583): modeled `ChartDefinition` → generated load + page, consumer can map the component (`charts/mod.rs:201–212`). |
| P2 | **First-class verbs.** Custom behavior written by consumers must join every generated surface: route, OpenAPI, hurl, permission scopes, UI command plane, e2e fixtures. | Function bindings B1 (#337): bound rosetta funcs get routes + OpenAPI + AppState slots + hurl coverage. The gap is that mox ops and handwritten code don't. |
| P3 | **Escape hatches are guarantees.** Write-once files, compile pins (#551 policy), doctor discoverability, regen survival — as *documented invariants*, not folklore. | `functions/manual.rs` write-once + trait-reemit drift guard; `bodyless_op_seams()` doctor section; `every_global_capability_has_a_registry_constructor` audit. |
| P4 | **Ports over hardcoding.** Algorithms and providers are `Arc<dyn Trait>` AppState ports with generated defaults, not baked-in implementations. | `workflow_service: Arc<dyn codegraph_workflow::WorkflowService>` (`templates/scaffold/app_state.tera:55`); `ExternalServiceAdapter` registry. |
| P5 | **Presence-gated byte-identity.** Every new plane is inert when unused; flag off/absent config ⇒ zero files, unchanged output, re-blessed canaries. | The repo's founding discipline (ux-rules, policy_rls, extension points, charts — all shipped this way). |

---

## 3. Proposal A — the operations plane (custom verbs)

**Goal:** a consumer can add a persistence-capable, HTTP-exposable,
contract-complete verb to any modeled entity with a config table plus a
write-once Rust method — and the verb joins OpenAPI, hurl, scopes, the UI
command plane, and the ops api suite automatically.

The design **unifies the two behavior systems** that exist today. They
have complementary strengths and an awkward asymmetry:

- **mox ops** are graph-native: `MoxOperationNode { name, class, params,
  return_type, bodies }` rides the node (`core/src/types/mox.rs:53–64`,
  ingested at `ingest/mox_ingest.rs:594–648`). Routing is *derived from
  shape* (primitive-signature rule, `api/handler.rs:44–74`) with no
  placement control.
- **function bindings** are config-driven over rosetta funcs: the
  `[functions."Name"]` table decides implementation/exposure
  (`codegraph-config/src/config/functions.rs:23–65`) — richer placement,
  but keyed to a separate model source.

The operations plane layers the function-binding vocabulary onto the
mox-op graph nodes.

### A1 — Binding vocabulary

```toml
# domains.toml
[ops."CardType.move"]
implementation = "manual"   # dsl (default) | manual | sql | external
exposure = "http"           # internal (default) | http
```

- New `codegraph-config/src/config/ops.rs` mirroring `functions.rs:23–107`
  (the `FunctionImplementation`/`FunctionExposure` enums are reused
  as-is; the key is `"{ClassTitle}.{op}"`). Deterministic `BTreeMap`.
- Validation mirrors the function path exactly:
  - `OpsBindings::validate_for_target(database_target)` (graph-free
    checks: `sql` ⇒ sqlite refusal — `functions.rs:114–127` pattern);
  - `check_op_bindings(db, config, target)` in `validate.rs` (mirror
    `check_function_bindings`, `validate.rs:665–696`) — unknown op names
    are a hard `Error::Config` naming the offender;
  - driver call sites at `driver.rs:1021–1032` + a new
    `check_exposed_op_types` at the `:1035` site (mirror
    `check_exposed_function_types`, `functions_api.rs:206–244`);
  - doctor parity: a `scan_mox_operations` analog of
    `scan_rosetta_functions` so `codegraph doctor --mox-files` can
    validate bindings before generation.
- **Byte-identity**: absent `[ops]` table (or all-default entries) ⇒
  today's behavior exactly — primitive-signature routing, no OpenAPI
  entries, no AppState slots (the `function_binding_absent_is_byte_identical`
  discipline, pinned by an equivalent `op_binding_absent_is_byte_identical`).

### A2 — Manual ops with a real context

Today's `ManualImpls` is sync+pure because it was designed for
computation. The operations plane needs a tx. Two constraints shape the
fix:

- The domain-types crate is deliberately persistence-agnostic (the
  `mox_plane_compile_tests` wasm32 pin) — so a SeaORM type cannot ride
  `Resource` or the trait.
- The hooks plane already solved erased-provider tx context once
  (`HookTx`/`HookConnSource` SPIs with `native()` hatches,
  `templates/hooks/types.tera` — and the #518 lesson: adapters must be
  bound to locals before `TxContext::new`).

Design: the manual op trait lives **app-side**, in a regenerated module
per domain, with the write-once consumer file beside it:

```
src/domain/{domain}/mox_ops/
├── mod.rs        # regenerated: `pub trait MoxManualOps` + context types
├── manual.rs     # write-once: DefaultMoxManualOps todo! stubs (emit-if-absent)
```

```rust
// mod.rs (regenerated — trait re-emission is the drift guard, the
// functions/manual.rs pattern at ddd/functions.rs:674–680)
#[async_trait]
pub trait MoxManualOps: Send + Sync {
    /// Move a card between/before/after neighbours. Runs inside the
    /// command transaction; Abort rolls back.
    async fn card_move(&self, tx: &TxHandle, actor: &OpContext,
                       card_id: Uuid, req: CardMoveRequest) -> Result<CardMoveResponse, OpError>;
}
```

- `TxHandle` is the erased tx SPI (the `HookTx` shape): `execute`,
  `native() -> &(dyn Any + '_)`, adapted per provider in the generated
  `hook_adapters.rs` (already emitted per crate — the adapters exist).
- `OpContext` carries `organization_id` / `correlation_id` /
  `api_key_id` / `role` — the RLS bundle inputs — so manual ops set
  session vars themselves or call a generated `set_rls_session_vars`
  helper exported from the command module.
- The trait carries async + `Result` — a deliberate widening of the
  functions precedent (`functions.rs`'s sync+pure is unchanged for
  rosetta funcs; the new trait serves mox ops).
- AppState: `ScaffoldDomain.has_manual_ops` (mirror
  `has_manual_functions`, `scaffold/generator.rs:103/:325`) →
  `app_state.tera` slot `{domain}_manual_ops: Option<Arc<dyn MoxManualOps>>`
  constructed `None` in `server.tera` (mirror `:322–323`); the handler
  branch mirrors the functions manual leg (`functions.rs:1662–1700`)
  with `501` until registered.
- Doctor: `bodyless_op_seams()` (`init/commands.rs:565`) extends to list
  manual-op seams (`INFO board::CardType::move — implement MoxManualOps`).

### A3 — HTTP exposure through the shared API plane

`exposure = "http"` promotes an op from today's raw
`serde_json::Value` handler (`handler.tera:1158–1195`) to the typed
chain the functions plane already has:

- Signature slots resolve through **one shared API-plane resolver** —
  `functions_api::resolve_api_type` (`functions_api.rs:150–196`):
  primitives builtin, codelists → `crate::codelist::{Enum}`, entities →
  `{Title}Response`, VOs refused. This retires the primitive-only
  routing rule (`routable_mox_ops`, `handler.rs:44–74`) for bound ops;
  unbound ops keep today's rule (byte-identity).
- Per-op `{Op}Request`/`{Op}Response` DTOs (`Deserialize`/`Serialize` +
  `ToSchema`, camelCase per project), `#[utoipa::path]` handlers — the
  `emit_function_endpoint` template pattern (`functions.rs:1526–1800`)
  instantiated for ops. Route shape stays
  `POST {entity}/{id}/op/{snake}` (`router.tera:121–122`) so existing
  routes don't move; the permission guard upgrades from `"read"` to the
  write-verb vocabulary (`"update"`, or an explicit
  `permissions` sub-table on the binding).
- OpenAPI: `PerDomainOpenApiContext` gains an `ops` vec
  (`openapi.rs:72–82`; **no** `skip_serializing_if` on Tera-context
  vecs); `openapi_domain.tera` registers paths + schemas beside the
  functions entries.
- The **UI command plane** lights up: the entity remote-functions
  emitter (`ui/remote.rs`) gains a `moveCard`-style command per exposed
  op (typed input object → same-origin POST), so generated pages can
  call the verb without hand-written fetch plumbing.
- hurl: exposed ops join the contract band (a `{nn:02d}_{domain}_ops.hurl`
  per domain, minimal valid bodies built from the same fixture data the
  entity contracts use).

### A4 — sql implementation (generalizing #339)

`implementation = "sql"` lowers the op to a PL/pgSQL function in a new
`migrations/023000_ops.sql` band (the `functions_sql` global generator
pattern, AGENTS.md #339 slice): signature slots resolve to
primitives/dates-as-text/codelists, entity-typed slots stay a hard
generation error (the #517 contract), and the generated
`DefaultSqlImpls` call plane dispatches
(`functions.rs:1752–1777` pattern). Worked example in §3.5.

### A5 — compile pins

Per the #551 policy, every emitted Rust carries a compiling pin in the
same PR: the ops plane (mod.rs trait + manual.rs stub + handler
branches) compiles in a scratch crate over the shared stub stack (the
`command_compile_tests` shape). The erased-`TxHandle` leg compiles
wasm32 — that's why it exists.

### 3.5 — Worked example: LexoRank, before and after

The LexoRank exercise shipped the move/rebalance verbs as: a
`router.tera` override, a 500-line handwritten module, a `cargo_toml.tera`
override, and a manual mount (see `docs/lexorank-kanban.md` §3 gap 1/7).
On the operations plane:

```mox
class CardType {
    String [0..1] rank
    refers BoardListType list

    // Body-less op: the seam is now a FIRST-CLASS, contract-complete verb
    // (routed, OpenAPI-documented, scope-guarded) instead of a todo! on a
    // compute-only plane. Class-ref params resolve to Uuid slots through
    // the shared API plane (A3/A4).
    op string move(BoardListType list, CardType after, CardType before)
}
```

```toml
[ops."CardType.move"]
implementation = "manual"
exposure = "http"
```

```rust
// generated/src/domain/board/mox_ops/manual.rs (write-once)
#[async_trait]
impl MoxManualOps for BoardManualOps {
    async fn card_move(&self, tx: &TxHandle, actor: &OpContext, card_id: Uuid,
                       req: CardMoveRequest) -> Result<CardMoveResponse, OpError> {
        let native = tx.native().downcast_ref::<DatabaseTransaction>()
            .ok_or_else(|| OpError::provider("expected the sea_orm transaction handle"))?;
        lexorank_move(native, actor, card_id, req).await.map_err(OpError::from)
    }
}
```

What retires: the router override (the route is generated), the AppState
hand-wiring (the slot is generated), the error-mapping/RLS/session-var
plumbing (OpContext + generated helpers), the absence from OpenAPI/hurl,
and the UI fetch hand-coding (the remote command is generated). What
stays: the algorithm itself (`lexorank_move`, ~80 lines), the DB
migration (until Proposal C lands rank vocabulary), and the UI widget
(Proposal B).

---

## 4. Proposal B — the headless UX capability layer

**Goal:** a consumer models an interaction *contract* ("cards are an
ordered collection within a list") and gets generated plumbing — verb
endpoints (§3), ordering-aware list params, store commands, testids, POM
methods, ux specs — while the *presentation* is a mapped consumer
component. Codegraph never builds a widget; it makes any widget
first-class.

The charts plane (#583) is the end-to-end template for "modeled
capability → generator" — with one deliberate difference: charts are a
read-only plane, and interaction capabilities need the write side. That
write side is exactly Proposal A.

### B1 — Modeled capabilities (charts precedent)

`[ordering."board-cards"]` in domains.toml (naming follows
`[charts."name"]`, `codegraph-config/src/config/charts.rs:321`):

```toml
[ordering."board-cards"]
entity = "CardType"
scope  = "list"          # the refers feature whose FK defines the group
key    = "rank"          # the order-key column
verbs  = ["move", "rebalance"]   # optional; default derives from key presence
component = "kanban-board"        # optional; the mapped widget (B2)
```

Ingest and graph follow the chart pattern verbatim:

- `codegraph-config/src/config/ordering.rs` mirroring `charts.rs`
  (closed vocabulary, strict validation, `BTreeMap` determinism, manual
  `Deserialize` where toml 0.8 mis-derives nested tables — the
  `charts.rs:316–330` lesson);
- `OrderingDefinitionNode { name, domain, entity_title, scope_feature,
  key_field, config_json }` in `codegraph-core/src/types/` + grafeo DDL
  (`schema_ddl.rs:262–264` chart precedent) + the mandatory four-impl
  trait pair (`ingest_ordering_definition` / `get_ordering_definitions`
  on GraphIngestor/GraphQuerier/Mock/CachingQuerier);
- driver **Pass 1g** after charts (`driver.rs:806–819` pattern) in
  `ingest/ordering_ingest.rs` — entity/field refs exact → `+type_suffix`
  → WARN, unresolved = generation hard error naming the entry.

Generation (`codegraph-generate/src/ordering/`, Global, presence-gated —
no definitions ⇒ zero files; `.with_mappings(ctx.ifml_components.cloned())`
at the registry site, `registry.rs:374–380` chart precedent; the
`every_global_capability_has_a_registry_constructor` audit holds the
constructor):

- **Verbs**: `move`/`rebalance` emit as ops on the scope entity
  (Proposal A machinery — the ordering generator writes the `[ops]`
  context, not the handlers). If the consumer already declared the op
  binding, config wins; a duplicate is a validation error.
- **Reads**: the scope entity's list route gains the ordering params
  (`sort=key` is already allow-listed via the ux Text dimension;
  the generator asserts it and hard-errors with a fix hint if a
  ux rule demoted the column — the cook's mistake, named).
- **UI**: a `data.remote.ts` command per verb on the scope entity's
  routes (the `ui/remote.rs` emitter), testids registered through the
  shared vocabulary (`ux/plan.rs::ids` :103–115 entity-side,
  `ifml/selectors.rs::ComponentSelectors` IFML-side — one resolution,
  two consumers, no drift), POM methods (`{comp}Move(card, after?)`) and
  ux-spec assertions (drop persists; order round-trips) in the e2e
  emitters.
- **Standalone route**: like charts, an optional generated page
  (`component = "kanban-board"` renders the mapped widget with the
  conventional props: `columns`, `items`, `onmove`, `onrebalance`,
  testids) — absent mapping ⇒ a documented fallback skeleton, same
  contract as the IFML mapped-component fallback.

### B2 — Interactive roles + entity-plane mapping consumption

Two changes to the mapping machinery:

1. **`SemanticRole` grows the interactive set**: `ranked-collection`,
   `drag-handle`, `time-range`, `map-view`, `canvas` (closed enum,
   `ifml_components.rs:9–22`). The pack-coverage test
   (`ifml_components.rs:484–510` — every role must resolve in the
   built-in pack) forces `packs/shadcn-svelte.toml` entries in the same
   PR. Role computation joins `route_generator/roles.rs` (a scope entity
   with an ordering definition ⇒ `ranked-collection`).
2. **The entity plane consumes mappings.** Today only IFML views and the
   charts generator resolve components (`charts/mod.rs:201–212` is the
   non-IFML precedent). The change: entity page generators gain
   `with_mappings(Option<IfmlComponentMappings>)` (registry threading
   mirrors the charts construction), `UiPageContext` carries a
   mapping-resolved slot per surface (list body, detail body, form
   field, extension block), and `ui_detail_extensions`' flat
   `entityId`-only block (`detail_page.tera:289–294`) generalizes into
   tier-resolved slots carrying the conventional props
   (`data=`/`item=`/`fields=`/`testid=` — the IFML contract at
   `context.rs:1522–1536`). Empty mappings ⇒ today's markup
   byte-identical. Entity e2e/POM emitters read the same resolution
   (`ui/e2e_test/` + `ux/plan.rs::ids`), so specs cannot drift from
   markup — the invariant the IFML side already holds via
   `ComponentSelectors` (`selectors.rs:43–98`).

### 4.5 — Worked example: the board page, before and after

The LexoRank exercise shipped a hand-authored SvelteKit route trio
(`+page.server.ts` / `+page.svelte` / `api/+server.ts`) precisely
because no plane would carry the plumbing. With B1+B2:

- Generated: the `/board/cards` ordering page (load through
  `$lib/server/api`, verb commands, testids, POM, ux specs) and the
  entity routes' ordering params.
- Consumer-owned: exactly one component —

```svelte
<!-- ui/src/lib/components/extensions/KanbanBoard.svelte -->
<script lang="ts">
  // props are the generated contract: no fetch code, no testids, no POM glue
  let { columns, items, onmove, onrebalance, testid }: KanbanBoardProps = $props();
  // svelte-dnd-action multi-zone recipe; onmove({card_id, after_id?, before_id?, list_id?})
</script>
```

plus one `[[component]] role = "ranked-collection"` mapping entry and
the `svelte-dnd-action` dependency (whose injection — until
`extra_dependencies` gains a config surface, §8 phase 1 — stays the
`package_json.tera` override from the recipe).

What retires from the exercise: the page trio, the same-origin proxy,
the hand-derived neighbor math in the UI, the hand-written POM/spec work.

---

## 5. Proposal C — model vocabulary completion

Cheap, high-leverage: most of `custom.sql` in the LexoRank exercise
existed because the model can't say these things (recipe §3 gaps 2–4).

| mox constraint | DDL effect | Today |
|---|---|---|
| `default "0\|1"` | `DEFAULT '0|1'` | no path (`column_info_to_ddl` hardcodes `default: None`) |
| `unique` | `UNIQUE` / unique index | `ColumnDef` has no flag; `IndexDef.unique` dead (never set; PG template ignores it) |
| `index` | `CREATE INDEX` | only hierarchy_field produces an index (`db/ddl/query.rs:1226–1257`) |
| `collate "C"` | `COLLATE "C"` | no path |
| `pattern` (existing) | `CHECK (col ~ …)` | garde on **create** DTO only — update DTO has no pattern guard (`dto_create.tera:26–27`); no DB CHECK |

Design notes:

- Closed, dialect-validated vocabulary: constraints ride the existing
  `feature.constraints` mox lowering (`mox_ingest.rs:1453` already
  carries `pattern`); `ColumnDef` gains `default/unique/index/collation`
  fields; the SqlDialect trait's `validate_column_type` family grows the
  counterpart checks (sqlite: `collate` refused, `default` typed).
- The DDL template (`templates/db/table.tera`) renders them inline —
  **byte-identity**: constraints absent from every existing fixture model
  ⇒ unchanged migrations; the canaries re-bless only if a fixture gains
  a constraint (it shouldn't).
- Pattern-as-CHECK closes the write-path gap the garde-on-create-only
  quirk leaves (LexoRank recipe quirk #1) and makes the DB the backstop
  for every writer, hand-written or generated.

This proposal also removes the *trigger* workaround for gap 2 (initial
rank): with `default`, the create path can omit the column and let the
DB assign — no `BEFORE INSERT` function needed for the common case.

---

## 6. Proposal D — escape-hatch discipline as a guarantee

The LexoRank exercise succeeded *despite* three inert/broken seams.
Fixing them is small and unblocks the hand-written tier for everything
the four proposals don't cover:

1. **Bind the `custom_routes` fold**: `templates/scaffold/server.tera:683`
   renders only under `{% if ext_points %}` and references an unbound
   `custom_routes` variable. Fix: always render the fold, bind
   `custom_routes = hooks_registry.take_custom_routes()` before AppState
   moves the registry, drop the `ext_points` gate. `HookRegistry::
   add_routes(prefix, Router)` (`registry.tera:75–81`) becomes live.
2. **Monolith `custom_routes` domains**: `api/router.rs:225–241` skips
   emission for entity-less custom-routes domains in the monolith, but
   `server.tera:528–532` nests only domains *with* entities — the two
   gates don't join. Emit a `router.rs` for entity-less custom-routes
   domains that delegates wholly to the extension seam (or nest the
   registered prefix server-side), and lift the "entity-less only"
   restriction (`ordering.rs:75`, `scaffold/generator.rs:297`) to allow
   *modeled + custom* domains: generated routes AND a handwritten
   `routes()` merged beside them — the sanctioned form of what the
   LexoRank recipe hand-built with a template override.
3. **Consumer-file inventory**: generation already knows the exact
   expected tree (`compute_generation_order` + the sweep's `expected`
   sets, `output.rs:182–360`). Extend `.codegraph-manifest.json`
   (already emitted for ops freshness) with a
   `consumer_owned: [paths]` section — handwritten files the sweep
   *preserves* — and make `clean_generated_output` warn (not sweep)
   before removing anything matching a consumer-owned pattern recorded
   from the previous run. Folklore ("don't park files in
   `src/api/{domain}/`") becomes a doctor check.
4. **Doctor custom-surfaces section**: one doctor subsection listing
   detected handwritten surfaces (manual.rs impls, custom_routes files,
   template overrides in use, extensions registered) — the single place
   a consumer looks to know what survives and what's wired.

---

## 7. Multi-archetype stress test

Does the contract-surface generalization hold beyond Kanban? Each
archetype: what it needs, which proposals cover it, what remains.

| Archetype | Needs beyond CRUD | A (verbs) | B (UX) | C (vocab) | D (hatches) | Remains |
|---|---|---|---|---|---|---|
| **Kanban / project boards** | move/rank verbs; dnd board | ✅ manual/SQL op | ✅ `ranked-collection` | ✅ default/unique/index | — | — |
| **Resource scheduling** (bookings, capacity) | allocate/release with conflict checks; calendar | ✅ manual op (constraint check in tx); `sql` for conflict predicates | ✅ `time-range` role | ✅ `unique` on booking (resource, interval) | — | interval-overlap constraint vocabulary (exclusion constraints) — small C follow-up |
| **Geospatial asset tracking** | geography types; geofence-entered events; map | ✅ verbs; events via existing event plane | ✅ `map-view` role | ⚠️ needs PgType extension (postgis) — C's dialect path is the slot | ✅ | postgis dialect support = a C-family slice with its own migration band |
| **Offline-first field ops** | sync protocol; merge (LWW/CRDT); conflict UX | ✅ merge op as a port (P4) — `Arc<dyn SyncMerge>` | ✅ conflict-resolution slot | — | ✅ | **transport (sync protocol) — axis 3, §9** |
| **Realtime collaborative whiteboard** | presence, websocket patches, CRDT | partial (patch-apply op) | partial (`canvas` role) | — | ✅ | **transport + presence — the declared non-goal** |
| **Analytics dashboards** | charts, drill-down, read models | — (read-only; charts + reports cover) | ✅ drill-down slots (mapped components over report routes) | — | — | aggregate API plane (charts' documented v1 deferral) |
| **Marketplace / payments** | PSP adapters, idempotency, webhooks | ✅ `external` ops (the #339 slice, keyed to ops) + adapter registry | ✅ checkout slot | ✅ `unique` idempotency keys | ✅ | PSP SDKs stay consumer crates (P4 ports) |
| **Content / CMS** | rich-text blocks, publishing workflow, media | ✅ `publish` verb (workflow transition + verb composition) | ✅ `block-editor` slot | ✅ | — | block-model type (JSONB tree validation) — C follow-up |

Reading the matrix: **every non-transport gap falls into one of the four
proposals' change surfaces** — verbs fall to A, interaction contracts to
B, storage vocabulary to C, and the hand-written remainder becomes safe
via D. The transport axis (realtime/sync) is the one place new
machinery — a connection/presence plane — is genuinely required, and it
is deliberately out of scope (§9) because it needs its own design
(weight: worker topology interplay, auth over websockets, replay) and
no current consumer has forced it.

---

## 8. Roadmap

Sequenced so each phase ships value alone and keeps the byte-identity
discipline. Every phase lands its gates in the same PRs (canaries,
compile pins, AGENTS.md sections) per the #551 policy and byte-identity
discipline.

**Phase 1 — foundations (C + D).**
Model vocabulary completion (`default`/`unique`/`index`/`collate`/
CHECK-from-pattern, update-DTO pattern parity), escape-hatch repair
(bind the `custom_routes` fold, fix monolith custom-routes emission,
consumer-file inventory + pre-sweep warnings, doctor section), and the
small `extra_dependencies` config rider (a CLI/profile path to
`CargoConfig.extra_dependencies` — retires the `cargo_toml.tera`
override for dep injection).
Gates: dialect unit tests, DDL snapshot tests, `custom_routes` monolith
smoke (`profile_smoke_tests` pattern), doctor tests. *This phase alone
shrinks the LexoRank recipe's `custom.sql` to near-zero.*

**Phase 2 — operations plane (A).**
Slices in dependency order: (A1) config + validation + doctor; (A2)
manual ops + erased tx SPI + AppState slots; (A3) http exposure +
OpenAPI + UI command plane + hurl; (A4) routing widening (shared
API-plane resolver); (A5) `sql` ops migration band. Gates:
`op_binding_absent_is_byte_identical`, ops compile pins (native +
wasm32 for the SPI), doctor validation tests, an
`ops_api_gate`-style live test (mirror `review_api_suite`). *LexoRank's
router override + handwritten mount retire here.*

**Phase 3 — UX capability layer (B).**
(B1) the ordering capability end-to-end (config → node → generator →
UI/e2e/POM), (B2) interactive SemanticRoles + entity-plane mapping
consumption + the generalized extension slots. Gates: pack-coverage
test extension, entity-plane byte-identity with empty mappings, POM
parity unit tests, ordering pipeline tests (charts_pipeline_tests
pattern). *The board page trio retires here.*

**Phase 4 — proof.**
Rewrite `examples/kanban-lexorank` on the new planes (it becomes ~1 file
of algorithm + 1 component + config), add it to a nightly conformance
gate (the `ifml_codegen_gate` pattern: migrations apply, axum boots,
move/rebalance over real HTTP, board POM drives a real browser) — the
"second archetype" fixture ensuring these axes cannot rot the way
`ui_orgchart` did for months (`registry.rs:690–710` is the audit that
started that class of guarantee).

---

## 9. Non-goals (this RFC)

- **Transport plane** (websockets, presence, sync protocols): a real
  third axis needing its own design — worker-topology interplay, WS
  auth/replay, conflict reconciliation. Named in §7 so the gap is
  honest; designed when a consumer forces it.
- **Wasm plugin algorithms** as an `implementation =` variant: the
  erased-SPI manual port (A2) covers customization; plugin sandboxing is
  machinery without demand yet.
- **CRDT library selection**: the merge *port* is modeled (§7);
  algorithm choice stays consumer-side.
- **Canvas/whiteboard modeling** (node-edge spatial documents): the
  `canvas` role is named, but the spatial document model is its own epic.

## Appendix — principle violations this RFC retires

For traceability against `docs/lexorank-kanban.md`:

| LexoRank gap | Retired by |
|---|---|
| 1 compute-and-persist | A (ops plane) |
| 2 initial rank | C (`default`) — trigger stays only for `next(max)` semantics |
| 3 collation | C (`collate`) |
| 4 unique/index | C (`unique`/`index`) |
| 5 rebalance | A (rebalance op) |
| 6 dnd UI | B (ordering capability + mapped component) |
| 7 dep injection | D-adjacent: `extra_dependencies` config surface is a small phase-1 rider (CLI/profile path to `CargoConfig.extra_dependencies`) |
