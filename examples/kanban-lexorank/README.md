# kanban-lexorank — a codegraph LexoRank example

A Trello-style Kanban board whose cards keep drag-and-drop order via
**LexoRank** keys, built with codegraph. The full capabilities analysis and
step-by-step recipe live in [`docs/lexorank-kanban.md`](../../docs/lexorank-kanban.md);
this directory is the runnable companion.

Everything here is **consumer-side** — no codegraph crates change. The
recipe's primary path is server-authoritative: the client drops a card and
tells the server its new neighbours; the server computes the midpoint rank
key transactionally.

## Layout

| Path | Purpose | Copied to (generated tree) |
|---|---|---|
| `model/board.mox` | The model: Board / BoardList / Card with `rank { pattern … }` | source of truth (stays put) |
| `domains.toml` | Domain config; cards nest under lists (`parent`/`parent_ref`) | stays put |
| `profiles.toml` | Init-scaffold-equivalent default profile (`ux_rules` on) | stays put |
| `templates-override/api/router.tera` | ONE-line override: merges `handwritten_routes::routes()` into the board domain router | `--template-dir` input |
| `templates-override/scaffold/cargo_toml.tera` | Adds `lexorank = "2.0"` (dep-injection workaround, gap 7) | `--template-dir` input |
| `templates-override/ui/scaffold/package_json.tera` | Adds `svelte-dnd-action` | `--template-dir` input |
| `handwritten/custom.sql` | `COLLATE "C"`, `(list_id, rank)` index, format CHECK, initial-rank trigger | `generated/migrations/custom.sql` |
| `handwritten/handwritten_routes.rs` | `POST /cards/{id}/move` + `POST /lists/{id}/rebalance` (lexorank crate, tx + RLS context + `FOR UPDATE`) | `generated/src/api/board/handwritten_routes.rs` |
| `ui/kanban/+page.server.ts` | Board load via `$lib/server/api` (issue #574 contract) | `generated/ui/src/routes/(app)/kanban/` |
| `ui/kanban/+page.svelte` | Board UI (svelte-dnd-action multi-zone recipe) | ditto |
| `ui/kanban/api/+server.ts` | Same-origin move/rebalance proxy | ditto |

Why these locations survive regeneration (sweep rules): unnumbered
migrations, non-`*_handler.rs` siblings under `src/api/{domain}/`, and UI
route dirs outside `(app)/{domain}/{path_segment}/` are never removed —
see the recipe doc, §3 gaps 1/4/6.

## Quickstart

```bash
# 0. Scaffold a project (or use this dir's model/config directly)
codegraph init kanban --domains board
cd kanban

# 1. Replace the starter model + config with this example's
cp ../kanban-lexorank/model/board.mox model/
cp ../kanban-lexorank/domains.toml .
cp ../kanban-lexorank/profiles.toml .

# 2. Copy the template overrides next to the project and wire them into
#    the justfile generate recipe (see justfile-additions.txt)
cp -r ../kanban-lexorank/templates-override .

# 3. First generate
just generate

# 4. One-time bootstrap: deploy the consumer-owned files, then regen once
#    so the mod scan declares handwritten_routes
cp ../kanban-lexorank/handwritten/custom.sql generated/migrations/
cp ../kanban-lexorank/handwritten/handwritten_routes.rs generated/src/api/board/
mkdir -p 'generated/ui/src/routes/(app)/kanban/api'
cp ../kanban-lexorank/ui/kanban/+page.server.ts ../kanban-lexorank/ui/kanban/+page.svelte 'generated/ui/src/routes/(app)/kanban/'
cp ../kanban-lexorank/ui/kanban/api/+server.ts 'generated/ui/src/routes/(app)/kanban/api/'
just generate

# 5. Apply migrations + smoke the API (needs Postgres)
cargo build --manifest-path generated/Cargo.toml
testkit api    # if ops_backend is enabled; otherwise apply generated/migrations manually

# 6. UI
cd generated/ui && npm install && npm run dev   # board at /kanban
```

From here, editing `model/board.mox` and re-running `just generate` keeps
every custom file in place — that's the point of the recipe.

## Try it

```bash
# Cards created WITHOUT a rank get '0|1', '0|2', … from the insert trigger.
curl -X POST "$API/api/v1/board/lists/$LIST/cards" -H "Authorization: Bearer $KEY" \
  -d '{"title":"First"}'

# Ordered read (generated ux sort plane):
curl "$API/api/v1/board/lists/$LIST/cards?sort=rank&order=asc" -H "Authorization: Bearer $KEY"

# Move between neighbours — server computes the midpoint:
curl -X POST "$API/api/v1/board/cards/$C/move" -H "Authorization: Bearer $KEY" \
  -d "{\"after_id\":\"$A\",\"before_id\":\"$B\"}"

# Re-space a list whose keys grew (409 rank_exhausted, or length > ~11):
curl -X POST "$API/api/v1/board/lists/$LIST/rebalance" -H "Authorization: Bearer $KEY"
```

## Variant V1 (client-computed, zero custom Rust)

Skip steps 2 (router override), 4's `handwritten_routes.rs` line, and the
`cargo_toml.tera` override. The UI computes `between()` client-side (port
the ~40-line algorithm from the lexorank crate) and PATCHes `rank` through
the generated `PUT /api/v1/board/cards/{id}`. Keep `custom.sql` — the
trigger and `COLLATE "C"` still earn their keep. Trade-offs in the recipe
doc, §5.

## Known limitations

- The handwritten routes are not in the generated OpenAPI document.
- Concurrent moves can land equal keys; the generated `, id ASC` sort
  tiebreaker keeps reads stable and the next move on that pair 409s into a
  rebalance (v1 semantics, recipe §8).
- The `lexorank` crate's license is listed as "non-standard" on crates.io —
  review before commercial adoption; §7 of the recipe shows the swap to a
  hand-rolled implementation.
- The board page sketch assumes one board and uses `svelte-dnd-action`'s
  `use:dndzone`/`on:finalize` recipe; check the crate's current Svelte-5
  API when you wire it for real.
