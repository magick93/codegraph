# LexoRank Kanban — a codegraph recipe

A thought-experiment turned reference: build a Trello-like Kanban board with
codegraph where cards keep their drag-and-drop order via the **LexoRank**
algorithm (Atlassian JIRA's lexicographic ranking). The companion runnable
example lives at `examples/kanban-lexorank/`.

This doc is the capabilities analysis (what codegraph gives you, what it
doesn't, and the workaround for every gap) followed by the recipe itself.

No codegraph crates are modified by this recipe — every artifact is
consumer-side: model files, config, a template override, hand-written files
that survive regeneration, and one consumer migration.

---

## 1. LexoRank in one paragraph

Every card carries a string `rank` key. Cards sort lexicographically by
`rank` within their list. "Insert between A and B" never renumbers the
list — it computes a key strictly between `rank(A)` and `rank(B)`. Keys
grow in length only when the space between two keys is exhausted, and a
rare **rebalance** re-spaces a whole list back to short keys. The recipe
uses the [`lexorank` crate (2.0)](https://crates.io/crates/lexorank) — a
231-line port of JIRA's algorithm:

```rust
use lexorank::LexoRank;

let a = LexoRank::from_string("0|a1")?;          // "{bucket}|{rank}"
let b = LexoRank::from_string("0|a3")?;
let mid = a.between(&b).unwrap();               // "0|a2" — Option (None iff equal)
let next = LexoRank::from_string("0|h9")?.next(); // "0|ha" (append-to-end)
```

Facts that matter for storage:

- Stored format is `{bucket}|{rank}` — this recipe fixes the bucket at
  `0` (JIRA uses buckets to reconcile concurrent re-ranks; one bucket is
  the standard simplification).
- Rank charset is `[0-9a-z]`, must not end in `0`. ASCII ordering of that
  charset is monotonic, so **byte-wise** string comparison is correct.
- Postgres `TEXT` comparison is collation-dependent by default — see the
  `COLLATE "C"` workaround in §3 (gap 3). With `COLLATE "C"`, Postgres
  `ORDER BY rank`, `rank > $1`, and the crate's internal `Ord` all agree.
- Licensing note: crates.io lists the crate's license as "non-standard"
  (repo LICENSE); review before commercial adoption. Swapping in a
  hand-rolled ~100-line midpoint is trivial (§7).

---

## 2. What codegraph gives you for free (model-driven)

| Need | Mechanism | Where it lands |
|---|---|---|
| `rank` storage | `String [0..1] rank` in the `.mox` model → `PgType::Text` | `board.card.rank TEXT` column, SeaORM entity field, `Option<String>` on create/update/response DTOs, hurl contract fields |
| Key format validation on create | mox constraint `pattern "…"` → `#[garde(pattern(r"…"))]` on the **create** DTO | `crates/codegraph-generate/templates/domain_types/dto_create.tera:26` |
| Cards under lists (URL nesting + automatic FK filter) | `parent`/`parent_ref`/`role = "child"` in `domains.toml` | `GET /api/v1/board/lists/{list_id}/cards` — the parent id is injected into every list/read call (`templates/api/handler.tera:790`) |
| Ad-hoc filtering | `filter_fields` per entity in `domains.toml` → `ALLOWED_FILTER_KEYS` + `?filter[list_id]=…` | `templates/api/handler.tera:658-666` |
| Ordered reads | `ux_rules = true` (ON in the init scaffold) → `?sort=rank&order=asc`, validated against a per-entity allow-list, always appended with `, id ASC` as tiebreaker | `crates/codegraph-generate/src/ux/sort.rs`, `templates/api/handler.tera:690`, `src/ddd/repository_emitter/query_search.rs:15-57` |
| Typed UI data plane | `#lib/api/client.ts` (browser) and `$lib/server/api.ts` `apiFetch` (server-side, credentials never reach the client — issue #574) | emitted if-absent, hand edits survive |

`rank` infers the `Text` ux dimension → sortable by default. If a
consumer's ux-rules pack ever demotes it, a `[[column]]` rule in a
`--ux-rules <file>` forces `dimension = "text"` back on.

---

## 3. Gaps and workarounds

These are the seven things codegraph cannot express today, each with the
workaround this recipe ships. All workarounds are consumer-owned artifacts
that **survive regeneration**.

### Gap 1 — no compute-and-persist operation ("move card")

The behavior seams, in the order this recipe evaluated them:

| Seam | Why it doesn't fit |
|---|---|
| mox `op move(before, after)` | op bodies are **compute-only**: the `Resource` arena is an in-memory mirror, there is no DB handle, no async, and the route never persists (`crates/codegraph-generate/src/ddd/mox_ops.rs`) |
| function bindings (`implementation = "manual"`) | manual impls receive typed inputs only — no DB handle, no tx |
| lifecycle hooks | `TxContext::execute` is rows-affected-only (write, not query), and `before_create`/`before_update` receive **immutable** payloads (`&CreateXRequest`) — a hook can validate/abort but never rewrite the command (`templates/hooks/lifecycle_trait.tera:35`) |
| `custom_routes = true` domains | entity-less by design (`ordering.rs:75`); in the monolith topology the generated router file isn't even emitted and the server's custom-routes fold is `ext_points`-gated with the `custom_routes` variable unbound in the render context — effectively inert in the stock driver |

**Workaround (server-authoritative, primary path):** a one-line template
override of `api/router.tera` that merges a hand-written sibling module
into the domain router:

```
Router::new().merge(super::handwritten_routes::routes())   ← the whole override
```

`generated/src/api/board/handwritten_routes.rs` is:

- **never swept** — `clean_generated_output` removes only `src/api/{domain}/*_handler.rs` files (`crates/codegraph-generate/src/output.rs:285`), not sibling modules;
- **auto-declared** — the post-generation `generate_mod_files` filesystem scan declares `pub mod handwritten_routes;` (`crates/codegraph-generate/src/output.rs:593-655`);
- a full `Router<AppState>` participant — same `State<AppState>`, `Extension<ApiKeyInfo>`, and `PermissionGuard` vocabulary as generated routes (unlike `HookRegistry::add_routes`, whose `Router<()>` constraint exists because extension crates can't see `AppState`).

It hosts `POST /api/v1/board/cards/{card_id}/move` and
`POST /api/v1/board/lists/{list_id}/rebalance` (full code in the example).

### Gap 2 — no initial-rank assignment on create

There is no column-default mechanism from the model (`column_info_to_ddl`
hardcodes `default: None`), the create command writes an explicit value
per DTO field, and hooks can't mutate the payload (gap 1's seam table). A
NULL `rank` would sort inconsistently (`NULLS LAST` on ASC) and break
neighbor math.

**Workaround:** a `BEFORE INSERT` trigger in the consumer migration
assigns the first/next key. Triggers run before CHECK constraints, fire
inside the same statement, and a consumer migration is the one SQL
artifact codegraph explicitly preserves (see gap 4).

### Gap 3 — collation

`TEXT` sorts by database collation; LexoRank needs byte order. The
generated `?sort=rank` path emits a plain `ORDER BY "rank"` — it would
follow whatever collation the column carries.

**Workaround:** one `ALTER TABLE … ALTER COLUMN rank TYPE text COLLATE "C"`
in the same consumer migration. Column-level collation applies to every
generated query without touching templates.

### Gap 4 — no UNIQUE/index/constraint vocabulary for entity columns

`ColumnDef` has no unique flag and `IndexDef.unique` is dead code (only
the SQLite template reads it; nothing sets it). Nothing in the model
produces an index on a user column.

**Workaround:** the same consumer migration adds the
`(list_id, rank)` index, the rank-format `CHECK` (belt-and-braces beside
the create-DTO garde pattern — the **update** DTO doesn't carry the
pattern guard, see §6 quirks), and the trigger. Consumer migrations are
the strongest survival story in the tree: `clean_stale_migrations`
deletes only files whose leading numeric prefix is **≥ 10**
(`crates/codegraph-generate/src/pipeline.rs:355`). An **unnumbered**
`migrations/custom.sql` sorts after the entity band (`0005…`) and runs
last — exactly what an `ALTER` on a generator-created table needs.

### Gap 5 — rebalancing

Nothing in the generated surface can rewrite a whole list's keys.

**Workaround:** rides gap 1's handwritten module —
`POST /lists/{list_id}/rebalance` re-spaces the list's keys by walking
`between()` towards a far-right sentinel (yielding `0|1, 0|2, …` and only
growing when the space demands). Trigger it on a `409 rank_exhausted`
from the move endpoint, or when keys grow past a length threshold you
choose.

### Gap 6 — no drag-and-drop UI

The generated UI plane is table/form/list pages with column-header
sorting; there is no dnd vocabulary anywhere (`ui/src/`,
`ifml/route_generator/`). Two constraints shape the workaround:

- **Sweep collision:** the regen sweep removes stale route dirs under
  `ui/src/routes/(app)/{domain}/{path_segment}/` (`output.rs:300-334`).
  A hand-authored page parked at `(app)/board/kanban/` would be swept —
  the domain in this recipe is literally named `board`. Put hand routes
  **outside** any modeled domain's path segment (the example uses
  `(app)/kanban/`).
- **`package.json` is regenerated every run** (`ui/scaffold.rs` renders
  `ui/scaffold/package_json.tera` unconditionally), so adding
  `svelte-dnd-action` by editing the file is lost on the next regen.

**Workaround:** a hand-authored SvelteKit route set (`+page.server.ts`
load, `+page.svelte` board, and a `api/+server.ts` move/rebalance proxy
through `$lib/server/api` — a child route, since `+page.server.ts` and
`+server.ts` cannot share a route), parked at `(app)/kanban/`; the dnd dep
comes from a second template override (`ui/scaffold/package_json.tera`,
copy + one dependency line). Hand-authored dirs outside modeled domain
segments survive the sweep.

### Gap 7 — extra Cargo/npm dependencies

`CargoConfig.extra_dependencies` exists and is rendered verbatim into the
generated `[dependencies]` (`templates/scaffold/cargo_toml.tera`) — but the
stock driver builds `CargoConfig::default()` and never sets it. There is no
CLI/profile path to it.

**Workaround (shipped):** the recipe needs exactly one new Rust dep
(`lexorank = "2.0"`) and one npm dep (`svelte-dnd-action`), and both ride
template overrides:

- `templates-override/scaffold/cargo_toml.tera` — a copy of the built-in
  `scaffold/cargo_toml.tera` plus the `lexorank = "2.0"` line. Owning a
  full copy of a scaffold template is the cost; when codegraph ships a
  template change you re-copy and re-apply your one-line diff.
- `templates-override/ui/scaffold/package_json.tera` — same pattern for
  the npm dep.

Dep-free alternative: drop the `cargo_toml.tera` override and implement
the two LexoRank primitives the handlers actually need (`midpoint` via a
~40-line `between`, `next`) inside `handwritten_routes.rs` — see §7.

### Quirks worth knowing (non-blockers)

- The **update** DTO doesn't render the `pattern` garde attribute (only
  `dto_create.tera` does) — the DB `CHECK` constraint is the real
  backstop for the write path.
- The cornucopia repository adapter accepts `?sort=` but ignores it
  (`let _ = sort;` — static SQL files can't do dynamic ORDER BY). The ux
  sort plane is a SeaORM feature; the cornucopia variant would lean on
  the handwritten routes for ordering.
- Handwritten routes don't appear in the generated OpenAPI document.
- `HookRegistry::add_routes(prefix, Router)` exists but is never wired in
  the stock monolith (the `server.tera` fold is `ext_points`-gated and
  `custom_routes` is unbound in the render context) — don't build on it
  yet.
- Two concurrent moves can produce **equal** ranks; the generated sort's
  `, id ASC` tiebreaker keeps rendering stable, and `between(equal,
  equal)` returning `None` makes the next move on that pair answer
  `409 rank_exhausted` until a rebalance.

---

## 4. The recipe (server-authoritative, primary path)

### Step 0 — scaffold

```bash
codegraph init kanban --domains board     # or --codegraph-path ~/git/codegraph for local dev
cd kanban
```

### Step 1 — the model (`model/board.mox`)

```mox
package board

/// A kanban board.
class BoardType {
    /// Display name of the board.
    String name
}

/// A column on a board.
class BoardListType {
    /// Column title (e.g. "Todo", "In Progress", "Done").
    String name
    /// Column order left-to-right (columns are not drag-reordered in v1).
    int position
    refers BoardType board
}

/// A card on a board.
class CardType {
    /// Card title.
    String title
    /// Optional free-form description.
    String [0..1] description
    /// LexoRank ordering key within the list ("{bucket}|{rank}", e.g.
    /// "0|hzzzzz"). Assigned by the DB trigger on insert; rewritten only
    /// by the move/rebalance endpoints. Never client-supplied on v2.
    String [0..1] rank { pattern "^0[|][0-9a-z]*[1-9a-z]$" }
    refers BoardListType list
}
```

Notes:

- Titles carry the `Type` suffix (init convention; keeps bare names
  unambiguous in the multi-package union namespace).
- The `pattern` uses `[|]` (character class) instead of `\|` so the regex
  survives mox string literals without escaping questions; it renders as
  `#[garde(pattern(r"^0[|][0-9a-z]*[1-9a-z]$"))]` on `CreateCardTypeRequest`.
- `rank` is optional: the create body omits it and the trigger assigns.

### Step 2 — domains.toml

```toml
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.board]
label = "Board"
schema_dir = "board"
postgres_schema = "board"

[domains.board.entities.BoardListType]
filter_fields = ["board_id"]

[domains.board.entities.CardType]
parent = "BoardListType"
parent_ref = "list"
role = "child"
filter_fields = ["list_id"]
```

`parent`/`parent_ref` give the nested route
`GET /api/v1/board/lists/{list_id}/cards` with the parent filter applied
server-side on every read.

### Step 3 — first generate

```bash
just generate    # or the raw wrapper command with --mox-files + --config
```

Expect the usual tree: `000500_board_board.sql`, `…_board_list.sql`,
`…_card.sql` (+RLS/trigger/event-trigger siblings), handlers, DTOs,
`/api/v1/board/...` routes. At this point `rank` is a plain TEXT column
with no index, no collation fix, and no assignment — keep going.

### Step 4 — deploy the consumer-owned files (one-time bootstrap)

Copy from the example (`examples/kanban-lexorank/`) into the generated
tree:

| From | To | Survives regen because |
|---|---|---|
| `handwritten/custom.sql` | `generated/migrations/custom.sql` | unnumbered ⇒ never swept (≥10 rule), runs last |
| `handwritten/handwritten_routes.rs` | `generated/src/api/board/handwritten_routes.rs` | sweep removes only `*_handler.rs` |
| `ui/kanban/+page.server.ts`, `+page.svelte`, `api/+server.ts` | `generated/ui/src/routes/(app)/kanban/` (+ `api/`) | outside `(app)/{domain}/{seg}` |

Then copy the template overrides **next to the project** (not into the
generated tree) — `templates-override/api/router.tera`,
`templates-override/scaffold/cargo_toml.tera` (adds `lexorank = "2.0"`),
and `templates-override/ui/scaffold/package_json.tera` (adds
`svelte-dnd-action`) — and add `--template-dir templates-override` to the
`generate` recipe in the justfile (see `justfile-additions.txt` in the
example).

### Step 5 — regenerate

```bash
just generate
```

The mod scan declares `pub mod handwritten_routes;`, the overridden
router merges its `routes()` at the domain root, `svelte-dnd-action`
lands in `ui/package.json`. From here on, **every future regen preserves
all of it** — edit the model, rerun, keep shipping.

### Step 6 — apply migrations & smoke the API

```bash
# With ops_backend enabled: testkit api (migrate + hurl contracts + RLS checks).
# Without it: apply generated/migrations with any runner, boot the server, curl.
```

Contract spot-checks (any HTTP client; API-key auth per the scaffold):

```bash
# Create a list and three cards (rank omitted → trigger assigns 0|1, 0|2, 0|3).
curl -X POST "$API/api/v1/board/lists" -H "Authorization: Bearer $KEY" \
  -d '{"name":"Todo","position":0,"board_id":"…"}'
curl -X POST "$API/api/v1/board/lists/$LIST/cards" -H "Authorization: Bearer $KEY" \
  -d '{"title":"First"}'
# …create two more, then read back ordered by rank:
curl "$API/api/v1/board/lists/$LIST/cards?sort=rank&order=asc" -H "Authorization: Bearer $KEY"

# Move card C between A (after_id) and B (before_id) — server computes the midpoint:
curl -X POST "$API/api/v1/board/cards/$CARD_C/move" -H "Authorization: Bearer $KEY" \
  -d '{"after_id":"'$CARD_A'","before_id":"'$CARD_B'"}'

# Drag to another list's end:
curl -X POST "$API/api/v1/board/cards/$CARD_C/move" -H "Authorization: Bearer $KEY" \
  -d '{"list_id":"'$LIST2'"}'

# Re-space a list whose keys grew too long:
curl -X POST "$API/api/v1/board/lists/$LIST/rebalance" -H "Authorization: Bearer $KEY"
```

### Step 7 — the UI

`npm install && npm run dev` inside `generated/ui`; the board lives at
`/kanban`. The sketch fetches server-side via `$lib/server/api` (issue
#574 contract: credentials never reach the browser), renders columns via
`svelte-dnd-action` (the canonical multi-zone kanban pattern — one dnd
zone per column over a flat item array tagged with `listId`), and on
drop derives `{after_id?, before_id?, list_id?}` from the finalized
order, POSTing to the same-origin `/kanban/api/move` proxy.

### Step 8 — operational notes

- **When to rebalance:** keys grow only on between-exhaustion. Warn at
  length > ~11 chars, rebalance on `409 rank_exhausted`.
- **Concurrency:** the move handler locks the moving row (`FOR UPDATE`)
  inside the tx; two racers on the same pair can still land equal keys —
  the `, id ASC` sort tiebreaker keeps output stable and the next move
  on that pair 409s into a rebalance. That's the honest v1 semantics.
- **Authz:** handwritten routes apply the same `PermissionGuard`
  vocabulary as generated ones (`"board:cards", "update"`); org isolation
  remains DB-level — the handler sets the RLS session context exactly
  like the generated command plane (`set_config` batch +
  `SET LOCAL ROLE app_user`, mirroring `templates/ddd/command.tera:34`).

---

## 5. Variant V1 — client-computed ranks (zero custom Rust)

Trello itself computes keys client-side. If you accept client-supplied
ordering:

1. Drop `handwritten_routes.rs` and the `router.tera` override.
2. Keep `custom.sql` — the trigger and `COLLATE "C"` still earn their
   keep (initial key on create; byte order for the generated
   `?sort=rank`).
3. The UI computes `between(first.visible, last.visible)` (or
   `next()` for end-of-list) in TypeScript — port the ~40-line `between`
   from the lexorank crate (charset `[0-9a-z]`, append `1` on overflow,
   grow a `0…01` suffix between adjacent keys) — and PATCHes it through
   the generated `PUT /api/v1/board/cards/{id}`.
4. Keep the `CHECK` constraint as the only server-side format gate
   (the update DTO has no pattern guard).

Trade-offs: no server-side neighbor math, so concurrent movers can
produce equal keys more easily (same tiebreaker story), and the server
trusts clients on ordering. V2 exists because the move semantics are
server-trivial once the seam (gap 1 workaround) is in place.

---

## 6. The move/rebalance algorithm (V2, as implemented)

```
move(card, after_id?, before_id?, list_id?):
  BEGIN; lock card row FOR UPDATE
  target = list_id or card.list_id
  lower  = rank(after_id)  if after_id        # direct bound
  upper  = rank(before_id) if before_id       # direct bound
  (lower, upper) both  → between(lower, upper)      (None ⇒ 409 rank_exhausted)
  after_id only       → between(lower, successor(lower)) or next(lower)
  before_id only      → between(predecessor(upper), upper) or prev(upper)
  neither             → next(max(target)) or "0|1"        # drag to empty/end
  UPDATE card SET list_id, rank; COMMIT
```

`successor/predecessor` are list-scoped, exclude the moving card, and
compare with `COLLATE "C"`. Keys grow only when the gap between
neighbours is exhausted — the entire LexoRank trick.

`rebalance(list)`: read cards ordered by rank, assign `spread_keys(n)`
(repeated `between(cursor, far-right sentinel)` — short keys that grow
only as `n` demands), write in one tx.

---

## 7. Swapping the algorithm

Everything above is algorithm-agnostic except `custom.sql`'s trigger
next-key function and the key-math helpers in `handwritten_routes.rs`
(`parse_key`/`midpoint`/`next_key`/`prev_key`/`spread_keys`). Alternatives:

- **Hand-rolled LexoRank** — ~100 lines, no dep, no license question.
  The trigger body stays as-is (it's already hand-rolled).
- **Figma-style fractional indexing** (`fractional-index` crate) —
  shorter average keys, different charset; change the `CHECK`/`pattern`
  and the trigger's next-key rule accordingly. Collation and index
  workarounds are identical.

---

## 8. Upstream issue candidates

Gaps that would retire workarounds if fixed in codegraph (documentation
only — none implemented here):

1. **Op-plane persistence** — a sanctioned compute-AND-persist verb
   (tx + repo access) would replace the `router.tera` override +
   handwritten module entirely (the mox `op` route machinery already
   exists; it needs a write-capable context).
2. **Column defaults / indexes / unique from the model** — `String rank
   { default "0|1" }` plus index/unique vocabulary would retire most of
   `custom.sql`.
3. **Hook payload mutability** — `before_create(&mut CreateXRequest)`
   would allow server-side initial-rank assignment without a trigger.
4. **Monolith `custom_routes` wiring** — bind the `custom_routes`
   variable in `server.tera`'s fold (or drop the `ext_points` gate) so
   `HookRegistry::add_routes` / entity-less custom route domains work
   out of the box.
5. **`extra_dependencies` reachability** — a CLI/profile surface for
   `CargoConfig.extra_dependencies` would retire the `cargo_toml.tera`
   override for dep injection.
6. **UI dnd vocabulary** — an `ifml-components.toml`/ux-rules story for
   column-relative ordering views would retire the hand-authored board
   page.
