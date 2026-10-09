# UX Rules (ux-rules epic, #286–#306)

The ux-rules plane turns the #286 UX design principles into **codified,
configurable generation rules**. A ux-rules TOML document (the built-in
`ux-default` pack, optionally shadowed by a project file) plus Pass-1
dimension inference resolve into a pure-data `UxPlan` per entity; two
emitters (the entity-scoped UI pipeline and the IFML pipeline) render the
plan into SvelteKit markup and Playwright specs. Flag off ⇒ no plan, no
markup, byte-identical output.

This document is the canonical statement of what shipped (phases 1–4,
issues #292–#306), the traceability audit against the #286 rule table, the
deferred ledger, and the contributor conventions.

## 1. Rule → codified mapping table

Keyed by the #286 rule-table row numbers (row 1 is the sheet header).
Status cells verified against the shipped code on this branch.

| #286 row | Rule | Status | Where |
|---|---|---|---|
| 2 | Right-align numerics | Covered | Quantity/Money → `Align::Right` pack defaults (`dimension_defaults` in `crates/codegraph-generate/src/ux/plan.rs`); `text-right tabular-nums` th+td in `crates/codegraph-generate/templates/ui/list_page.tera` (#297); IFML `crates/codegraph-generate/templates/ifml/svelte/page.tera` (#300) |
| 3 | Left-align text | Covered | default `Align::Left` (`dimension_defaults`); pinned in the #297 integration tests |
| 4 | Header alignment = content | Covered | `Table.Head` carries the same class expression as its cells (`crates/codegraph-generate/templates/ui/list_page.tera`, #297) |
| 5 | Freeze/sticky columns+headers | Deferred | ledger below |
| 6 | Qualitative numbers left-aligned | Covered | TimePoint pack default is `align = "left"`; zip/phone-like columns carry no numeric pg type and fall through to Text |
| 7 | Row height / vertical alignment | Folded into phase 2 | `RowVisuals.vertical_align` (#296), `VerticalAlign::Center` default in `crates/codegraph-generate/templates/ui/list_page.tera` `align-middle` (#297); top-for-dense deferred (the `Top` variant exists unused) |
| 8 | Zebra striping | Folded into phase 2 | `RowVisuals.zebra` (#296) → `bg-muted/50` odd rows via `rowClass` (#297) |
| 9 | Row hover states | Covered | pre-existing `hover:bg-muted/50`, plus the ux-gated `focus-visible:ring-2` keyboard-focus ring in `rowClass` (#297); asserted by the #302 spec |
| 10 | Batch actions + checkboxes | Deferred | ledger below |
| 11 | Wrapping vs truncation | Covered (+a11y) | Pass 4 `truncate_tooltip` (Identifier default); truncation triggers a Tooltip, keyboard/touch-reachable (#296/#297) |
| 12 | Advanced pagination | Deferred | basic prev/next exists pre-epic; page-jump/per-page in ledger |
| 13 | Sorting/filtering | Sorting SHIPPED (#306); filtering deferred | `?sort=`/`?order=` allow-list, `aria-sort` headers, quoted ORDER BY + `, id ASC` tiebreaker (`crates/codegraph-generate/src/ux/sort.rs`, `crates/codegraph-generate/src/api/handler.rs`, `crates/codegraph-generate/templates/ddd/query.tera`, `crates/codegraph-generate/src/ddd/repository_emitter/query_search.rs`); filtering in ledger |
| 14 | Tabular/monospace numerics | Covered | `tabular-nums` on right-aligned cells (#297) and IFML money/quantity cells (#300) |
| 15 | Overflow menu row actions | Covered | DropdownMenu overflow + AlertDialog confirm (#297); child-section tiered menus (#299); IFML event tiering into the per-row menu (#301) |
| 16 | Cell progressive disclosure | Partial | Tooltip + master/detail row-click ship; in-cell "Show More" in ledger |
| 17 | Human-readable first column | Folded into phase 2 | `column_order` readable-first (exact `name` > `title` > `label`, then contains) in `crates/codegraph-generate/src/ux/plan.rs` (#296) |
| 18 | Inline editing | Deferred | ledger below |
| 19 | Table title + description | Partial | page headings exist (pre-epic); schema-description-driven subtitle in ledger |
| 20 | AI labeling | N/A | generated apps contain no AI-generated content; ledger note |
| 21 | Active search | Covered | pre-existing FTS preserved; #306 keeps `q` in the UI query string alongside sort |
| 22 | Nonmodal side panel | Deferred | ledger below |
| 23 | Adjustable row density | Deferred | ledger below |
| 24 | Table orientation | Covered (principle) | tables lay out for horizontal scan; time-ordered data may opt into the timeline (#298 entity, #301 IFML) |
| 25 | Importance-based column order | Folded into phase 2 | `column_order` (audit stamps `created_at`/`updated_at`/`deleted_at`/`*_by` last; author dto pins respected verbatim) (#296) |
| 26 | Card wrap/truncate | N/A v1 | card layouts out of scope; ledger below |
| 27 | Numeric fixed display (no wrap) | Covered | Quantity/Money render raw with no truncate path; alignment/format asserted by the #302 spec |
| 28 | Status/category chips | Covered | StatusCategory/Flag → `Display::Chip` with `ToneMap` keyword→badge-variant mapping (#297 entity, #300 IFML) |
| 29 | Avatars | Deferred | ledger below |
| 30 | Inactive row shading | Folded into phase 2 | `RowVisuals.inactive_shading` from soft-delete marker or workflow terminal states (#296) → `opacity-60` (#297) |
| 31 | Timeline/chart conversion | Covered (timeline) | strictly opt-in `[[collection]] display = "timeline"`; entity rail (`crates/codegraph-generate/templates/ui/list_timeline.tera`, #298), IFML `<ol>` rail (#301); charts in ledger |
| 32 | Minimalist cell borders | Partial | a pack/CSS concern, not a per-column rule; ledger note |
| 33 | Copy chip (hover reveal) | Covered | `Display::CopyChip` renders a copy-to-clipboard chip (#297); hover-reveal refinement in ledger (v1 ships an always-visible chip) |
| 34 | Spanner heads | Deferred | ledger below |
| 35 | Section heads | Deferred | ledger below |

Audit counts: 14 covered · 5 folded into phase 2 · 1 new sub-task (sorting,
#306) · 3 partial · 10 deferred · 1 N/A.

### Corrections vs the prepared #291 table

The prepared table proved accurate against the code on every Status cell.
Two precision notes where the prepared wording could mislead:

- Row 9: the `focus-visible` ring is part of the **ux-gated** `rowClass`
  helper (`crates/codegraph-generate/templates/ui/list_page.tera`, #297), not a standalone pin —
  flag-off rows keep the pre-epic `hover:bg-muted/50` only.
- Row 21: "preserved by #298" is really a #306 property — the sort plane
  keeps `q` in the query string when sorting (`crates/codegraph-generate/templates/ui/list_page.tera`
  sort handler), and the handler validates `sort` even on FTS requests
  (though ts_rank ordering wins over `sort` when `q` is present — see the
  ledger).

## 2. Pass architecture

The plane is a four-pass compiler over per-entity UI inputs, producing a
plan consumed by two emitters:

```
                    UxPlanInput (entity title, UiFields, graph props,
                                 workflow signals, soft-delete, dto pins)
                                   │
   codegraph-generate/src/ux/      ▼
   ┌───────────────────────────────────────────────────────────────┐
   │ Pass 1  dimension.rs   infer_dimension — ordered decision     │
   │         (form follows data: Reference → StatusCategory →      │
   │         Identifier → Flag → TimePoint → Money-heuristic →     │
   │         Quantity → Text; money keyword hints; wrapper         │
   │         sub-field recursion)                                  │
   │                                   │                           │
   │ Pass 2  plan.rs        resolve_column — first-match-wins       │
   │         (rules + ordering)  [[column]] rule over the inferred │
   │         dimension (dimension key = payload on co-selector     │
   │         rules); pack-default fold; column_order;              │
   │         resolve_collection — explicit-rule-only Timeline      │
   │                                   │                           │
   │ Pass 3  plan.rs        build_action_plan — Open/Edit/Delete    │
   │         (action tiering)    partitioned at [actions]          │
   │         inline_max; confirm list                              │
   │                                   │                           │
   │ Pass 4  plan.rs        RowVisuals — zebra, inactive shading,   │
   │         (invisible UI)      vertical center                   │
   │                                   │                           │
   │         sort.rs  sortable projection over column_order        │
   │         diagnostics.rs  advisory warnings (never fatal)       │
   └───────────────────────────────┬───────────────────────────────┘
                                   ▼
                               UxPlan (pure data)
                                   │
              ┌────────────────────┴────────────────────┐
              ▼                                         ▼
   Entity pipeline                            IFML pipeline
   ui/page.rs resolve_ux_context              ifml/route_generator.rs
   → crates/codegraph-generate/               resolve_column_ux +
     templates/ui/list_page.tera              resolve_generation_ux
     (+ _ux_cell.tera, list_timeline.tera,    → crates/codegraph-generate/
       child_section.tera)                       templates/ifml/svelte/page.tera
   ui/e2e_test/ → {seg}.ux.test.ts            ifml/e2e_test/ → {view}.ux.spec.ts
   ui/e2e_test/pom_ctx.rs +                   ifml/e2e_test/{pom,pom_render}.rs →
     ui/test/{page,_pom_kernel}.tera            tests/pages/{view}-page.ts
     → {seg}.page.ts + _support/pom.ts
```

### Module map

| Module | Role |
|---|---|
| `crates/codegraph-config/src/ux/dimension.rs` | Closed 8-value `Dimension` vocabulary (kebab-case TOML spelling) |
| `crates/codegraph-config/src/ux/presentation.rs` | `Display`/`Align`/`FormatConfig`/`ToneMap` + `VALID_TONE_VALUES`, `WORKFLOW_TONE_FALLBACK` |
| `crates/codegraph-config/src/ux/rule.rs` | `ColumnRule`/`CollectionRule`/`ActionRules`, `glob_match`, selector semantics |
| `crates/codegraph-config/src/ux/mod.rs` | `parse_ux_rules_str`/`load_ux_rules` with location-attributed errors + hints, `BUILT_IN_UX_PACK`, `builtin_ux_rules`, `merge` |
| `crates/codegraph-config/src/ux/packs/ux_default.toml` | The built-in pack (doubles as the key reference and user example) |
| `crates/codegraph-generate/src/ux/dimension.rs` | Pass 1 inference + `DimensionHints` + `infer_sub_field_dimensions` |
| `crates/codegraph-generate/src/ux/plan.rs` | `UxPlan`/`ColumnPlan`/`CollectionPlan`/`ActionPlan`/`RowVisuals`, `build_ux_plan`, `resolve_column`, `ids` testid consts |
| `crates/codegraph-generate/src/ux/diagnostics.rs` | `collect_diagnostics`/`report` — the three advisory warnings |
| `crates/codegraph-generate/src/ux/sort.rs` | Sort plane: `sort_plan_from_plan`, `collect_ux_plan_context`, `resolve_ux_sort_plan` |
| `crates/codegraph-generate/src/ui/page.rs` | `resolve_ux_context` — plan → entity list-page context |
| `crates/codegraph-generate/src/ui/scaffold.rs` | `SHADCN_PRIMITIVES` → gated `ui/PRIMITIVES.md` |
| `crates/codegraph-generate/src/api/handler.rs` | `?sort=`/`?order=` validation against the allow-list |
| `crates/codegraph-generate/src/ddd/repository_emitter/query_search.rs` | `emit_sort_ordering` — quoted ORDER BY + `, id ASC` tiebreaker |
| `crates/codegraph-generate/src/ifml/route_generator.rs` | `resolve_column_ux` (lookup tier + shared Pass-1/rules), `resolve_generation_ux`, `TableLayout::Timeline`, event tiering |
| `crates/codegraph-generate/src/ui/e2e_test/` | `{seg}.{family}.test.ts` emitters (11 families), `pom_ctx.rs` (per-entity POM inputs) |
| `crates/codegraph-generate/src/ifml/e2e_test/` | `{view}.spec.ts`/`.workflow.spec.ts`/`.ux.spec.ts` assembly + `pom.rs`/`pom_render.rs` (per-view POM) + `kernel.rs` (IFML half of the shared kernel) |

### Pass 1 decision list (ordered, first hit wins — a pinned contract)

1. entity reference → `Reference`
2. codelist / inline-enum select / workflow `status_field` → `StatusCategory`
3. `uuid` pg type, or `name == "id"` with string ts type → `Identifier`
4. boolean ts type / checkbox input → `Flag`
5. date/timestamp/range pg types, or date-ish inputs → `TimePoint`
6. numeric pg + money name keyword (`amount`, `total`, `subtotal`, `price`,
   `cost`, `fee`, `balance`, `salary`, `rate`, `*_cents`) → `Money`
   (records a `DimensionHints` entry — the inference is honest about guessing)
7. numeric pg → `Quantity`
8. anything else → `Text`

Edge semantics: arrays infer their element dimension; range types infer
TimePoint (v1 formats the lower bound); StructuredWrapper JSONB slots fall
through to Text while their sub-fields infer independently via
`infer_sub_field_dimensions`; synthetic columns (no graph property) infer
from the collected `UiField` alone; name matching is case-insensitive over
`_`-split words.

## 3. Precedence chain

Per column, the resolved contract is:

```
IFML lookup tier  (kind == "lookup" — the DSL named the presentation;
                   StatusCategory + chip, no rule can downgrade it)
      ▼  (everything else)
project [[column]] rules  (first-match-wins over the selector tier;
                           merge() prepends project rules ahead of pack)
      ▼
pack defaults  (ux_default.toml per-dimension rules + dimension_defaults
                fold: display/align/sortable/truncate per dimension)
      ▼
Pass-1 inference  (the inferred dimension selects the pack default row;
                   rules may override the dimension itself)
```

Selector semantics (`column_rule_matches` in `crates/codegraph-generate/src/ux/plan.rs`):

- `dimension`, `classification`, `pg_type`, `name_pattern` AND-combine.
- **Dimension-as-payload**: on a selector-only rule (pack style,
  `dimension = "money"`) the dimension key IS the selector, matched
  against the inferred dimension. When the rule also carries a
  classification/pg_type/name_pattern selector, those establish the match
  and `dimension` becomes payload — the only way a rule can override the
  inferred dimension. A rule with no selectors matches every column.
- `classification` matches best-effort as a normalized substring of the
  property's classification-kind name ("codelist" matches
  `CodelistReference`); `pg_type` is a case-insensitive prefix match on
  the element type ("numeric" matches `NUMERIC(10,2)`); `name_pattern` is
  a `*`-glob, case-sensitive, matched against the field name.
- A rule that OVERRIDES the inferred dimension cancels the column's money
  hint (the author made the call); a rule that confirms the inference does
  not.

Collections resolve separately: the first `[[collection]]` rule whose
`entity_pattern` (glob, `*` legal) matches the entity title wins.
Timeline ONLY when that rule says `display = "timeline"`; anything else —
including no rule at all — is a table. The heuristic never switches a
table into a timeline (it suggests instead, see diagnostics).
`order_by` must name one of the entity's inferred TimePoint fields, else
generation is a hard `Error::Config` naming the candidates.

### Byte-identity contract

`ux_rules` off (or plan-less run without a CLI file) ⇒ `ProjectConfig.ux`
is `None`, `build_ux_plan` returns `None`, every ux key stays out of the
serialized Tera contexts, no sort surface, no `.ux.test.ts`/`.ux.spec.ts`
files — output is byte-identical to pre-feature master. Enforced by
`crates/codegraph/tests/ux_rules_byte_identity_tests.rs`:

1. `flag_off_pipeline_is_deterministic` — full entity pipeline run twice
   hashes identically.
2. `flag_off_output_matches_pre_feature_snapshot` — hashes to the
   committed `crates/codegraph/tests/fixtures/ux_rules_pre_feature_tree.sha256`.
3. `ifml_flag_off_pipeline_is_deterministic` — IFML pipeline canary.
4. `ifml_flag_off_output_matches_pre_feature_snapshot` — committed
   `crates/codegraph/tests/fixtures/ux_rules_pre_feature_ifml_tree.sha256`.

Snapshots normalize the git rev and absolute paths (rev/path/hex-run
replacement) so they survive rev bumps. Re-bless an INTENDED generator
change with:

```bash
UX_RULES_BLESS=1 cargo test -p codegraph --test ux_rules_byte_identity_tests \
  -- flag_off_output_matches_pre_feature_snapshot
# or for the IFML snapshot:
UX_RULES_BLESS=1 cargo test -p codegraph --test ux_rules_byte_identity_tests \
  -- ifml_flag_off_output_matches_pre_feature_snapshot
```

then commit the updated fixture.

## 4. Flag plumbing + config reference

### Flag chain

```
profiles.toml [features] ux_rules = true   (repo profiles: default/ui/fullstack/ci ON;
                                            BuildPlan rejects non-bool with an error)
        ▼
BuildPlan.ux_rules                         (crates/codegraph-generate/src/profile.rs)
        ▼
driver::run effective_ux_rules(ux_rules_cli, build_plan)
  CLI --ux-rules <file> > profile flag (pack only) > none
  - missing CLI file = hard error; parse warnings print as
    `WARN ux-rules: …`; plan-less runs still honor a CLI file
        ▼
GeneratorOpts.ux_rules + ProjectConfig.ux  (templates read project.ux.*;
                                            consumed by ui/page.rs, ui/scaffold.rs,
                                            api/handler.rs, ifml generators)
```

`codegraph init` scaffolds `ux_rules = true` into the generated
`profiles.toml` (`crates/codegraph-generate/templates/project/profiles.tera`) and the wrapper binary
takes `--ux-rules` on its `Run`/`Generate` subcommands
(`crates/codegraph-generate/templates/project/wrapper_main.tera`).

CLI surface: `--ux-rules <file>` on `codegraph generate`, `codegraph run`,
and `codegraph ifml-generate` (`crates/codegraph/src/cli.rs`).

### TOML surface

Every unknown key/value is a parse error; every error names its
`[[column]] #N` / `[[collection]] #N` block (1-based array-of-tables
counting) and carries a `hint:` line where a fix exists.

```toml
[format]
locale = "en-NZ"        # BCP-47 for Intl number/date rendering (default en-NZ)
currency = "NZD"        # ISO-4217; omit for no currency formatting

[[column]]              # selectors (AND):
dimension = "money"     #   text|quantity|money|time-point|status-category|
                        #   identifier|reference|flag
classification = "codelist"   # normalized-substring match on the kind name
pg_type = "numeric"           # case-insensitive prefix on the element type
name_pattern = "*_amount"     # *-glob on the field name; "*" alone rejected
                      # payloads:
display = "chip"      # chip|copy-chip|link|raw
align = "right"       # left|right
[column.tone]         # status keyword → badge variant
active = "default"    #   default|secondary|destructive|outline
pending = "secondary"
sortable = true       # opt in/out of the ?sort= allow-list

[[collection]]
entity_pattern = "refund*"   # glob on the entity title; "*" is legal here
display = "timeline"         # table (default) | timeline (REQUIRES order_by)
order_by = "created_at"      # must be an inferred TimePoint field
title_field = "reference"    # entry title (default: column_order-first)
preview = ["status", "total_amount"]  # entry extras (default: first three
                                      # Text/Quantity column_order fields)

[actions]
inline_max = 1        # >= 1; actions beyond the budget collapse into the menu
confirm = ["delete"]  # lowercase action names requiring confirmation
```

Validation strictness (all hard errors): unknown keys/enum variants
(`deny_unknown_fields` — deliberate divergence from
`ifml_components.rs`, which tolerates unknown keys); empty globs;
`name_pattern = "*"` (would shadow every rule — collections keep bare `*`
legal, coarse opt-in by design); tone values outside the badge-variant
set; `timeline` without `order_by`; unknown collection display values;
`inline_max = 0`. Non-fatal warnings (printed, never fatal): duplicate
identical selectors (first wins), `order_by` without `timeline`.

Pack-as-example: `crates/codegraph-config/src/ux/packs/ux_default.toml`
ships en-NZ/NZD defaults, one per-dimension rule per covered dimension
(quantity/money right; time-point left; status-category/flag chip;
identifier copy-chip; reference link), `inline_max = 1`,
`confirm = ["delete"]`, and NO `[[collection]]` rules — timeline is
strictly opt-in. Its header comment doubles as the key reference; the
documented workflow is to copy it and edit.

Merge semantics (`merge` in `codegraph-config/src/ux/mod.rs`): `[format]`
project wins field-wise (a non-empty locale wins; the currency Option
replaces outright); `[[column]]`/`[[collection]]` project rules PREPEND
(first-match-wins shadows the pack per selector tier; pack fills gaps);
`[actions]` project `inline_max` wins; project `confirm` wins when
non-empty, else the pack's survives.

## 5. Testid contract (canonical)

Single source: `crates/codegraph-generate/src/ux/plan.rs` `pub mod ids` —
Rust constants + `{family}` constructor fns, shared by the UI generators,
the e2e/ux spec emitters, and the POM generators so generated specs never
drift from generated markup.

**The single-source story.** Tera templates keep their literals (Tera
cannot call Rust); the relationship is pinned in the other direction:

1. `ids::` in Rust is the canonical table (the doc comment on the module
   carries the same table with template line refs);
2. Tera literals are pinned by the GENERATED SPECS — a template edit that
   drifts a testid breaks the generated specs (and the byte-identity
   suites pin the specs);
3. the POM kernels mirror the same fragments (`tid('{fragment}')` in both
   kernels; surface parity pinned by
   `pom_kernel_surface_matches_entity_kernel`), and the Rust-built IFML
   page classes format their locators from the same fragment vocabulary.

So there are exactly three places a testid lives — template literal, spec
assertion, POM locator — and each pair is pinned by a suite.

### Entity family (prefix `{module}` = the entity's snake_case module)

| id | Owning template/emitter | POM consumer | Spec assertor |
|---|---|---|---|
| `{module}-search` | `ui/list_page.tera` | page `searchInput()` | search specs, POM fill |
| `{module}-create-btn` | `ui/list_page.tera` | page `createButton()` | crud spec create flow, POM |
| `{module}-empty` | `ui/list_page.tera` | page `emptyState()` | crud spec table-or-empty |
| `{module}-no-results` | `ui/list_page.tera` | page `noResults()` | search spec |
| `{module}-table` | `ui/list_page.tera` | page `tableRoot()` + `UxTable.root()` | crud/ux specs, POM rows |
| `{module}-sort-{field}` | `ui/list_page.tera` (ux sort only) | `UxTable.sortButton/sortHeader/sortBy` | ux spec sort block |
| `{module}-pagination` | `ui/list_page.tera` | page `pagination()` | ux spec hydration waits |
| `{module}-delete-confirm` / `-delete-confirm-confirm` | `ui/list_page.tera`, `ui/child_section.tera` | `UxTable.deleteConfirm()/deleteConfirmConfirm()`, `deleteViaMenu()` | ux spec confirm-cancel block |
| `{module}-form` | `ui/entity_form.tera` | page `formRoot()` | validation/crud specs |
| `{module}-cancel-btn` | `ui/entity_form.tera` | page `cancelButton()`/`cancel()` | POM cancel flow |
| `{module}-submit-btn` | `ui/entity_form.tera` | page `submitButton()`/`submit()` | crud spec, POM submit |
| `{module}-edit-btn` | `ui/detail_page.tera` | page `editButton()`/`edit()` | crud spec edit nav |
| `{module}-delete-btn` | `ui/detail_page.tera` | page `deleteButton()`/`delete()` | crud spec delete flow |
| `{module}-field-{field}` | `ui/detail_page.tera` | page `field(name)` | crud spec, POM detail read |
| `confirm-dialog` / `-confirm` | `ui/detail_page.tera` | page delete flow | crud spec |
| `workflow-panel` / `workflow-state` | `ui/scaffold/workflow_panel.tera` | page `workflowPanel()`/`workflowState()`/`expectState()`/`transitionTo()` | workflow specs |
| `{module}-row` | (not stamped in entity markup yet — rows located via the `{module}-table` root) | reserved for family uniformity | — |
| `{module}-chip` | `ui/_ux_cell.tera` chip branch | `UxTable.chipFor()` | `{seg}.ux.test.ts` chip block |
| `{module}-copy` | `ui/_ux_cell.tera` copy branch | `UxTable.copyTrigger()/copyCell()/tooltipFor()` | `{seg}.ux.test.ts` copy block |
| `{module}-actions` / `-actions-menu` / `-action-{edit,delete,…}` | `ui/_ux_cell.tera`, `ui/child_section.tera` | `UxTable.actionsTrigger()/openActions()/menu()/menuDelete()` | `{seg}.ux.test.ts` actions block |
| `{module}-timeline` / `-timeline-item` / `-timeline-meta` / `-workflow-state` | `ui/list_timeline.tera` | `UxTable.timelineRoot()/timelineItems()/timelineTimes()` | `{seg}.ux.test.ts` timeline block, `ux_rules_tests.rs` |

### IFML family (prefix `{comp}` = the IFML component name)

| id | Owning template/emitter | POM consumer | Spec assertor |
|---|---|---|---|
| `{comp}-table` | `ifml/svelte/page.tera` fallback + `selectors()` (mapped testids) | page `{comp}Row()` / `UxTable` over the module | `{view}.spec.ts` render |
| `{comp}-row` | `page.tera` fallback + `selectors()` | page `{comp}Row()` | click-through specs |
| `{comp}-form` | `page.tera` fallback + `selectors()` | page `{comp}Form()`/`{comp}Input(field)` | validation/CRUD specs |
| `{comp}-submit` | `page.tera` fallback + `selectors()` | page `{comp}Submit()`/`submit{Comp}()`/`save{Comp}()` | CRUD specs |
| `{comp}-error` | `page.tera` fallback | (validation asserted via the form surface) | validation specs |
| `{comp}-details` | `page.tera` fallback + `selectors()` | page `{comp}Details()` + per-field getters (`dd` order) | details/sweep specs |
| `{comp}-state` | `route_generator::workflow_badge_html` | page `{comp}StateBadge()`/`expect{Comp}State()` | `{view}.workflow.spec.ts` |
| `{comp}-transition-{to}` | `route_generator` (kebab via `codegraph-naming`) | page `transition{Comp}To()` (testid map = the same edge enumeration) | `{view}.workflow.spec.ts` |
| `{comp}-chip` / `-copy` | `page.tera` ux cell branches | `UxTable.chipFor()/copyTrigger()/copyCell()` | `{view}.ux.spec.ts` |
| `{comp}-actions` / `-actions-menu` | `page.tera` (event tiering; one per row) | `UxTable.actionsTrigger()/openActions()/menu()` (first-match) | `{view}.ux.spec.ts` |
| `{comp}-timeline` / `-timeline-item` | `page.tera` `<ol>` rail | `UxTable.timelineRoot()/timelineItems()` | `{view}.ux.spec.ts` timeline block |

Framework-level ids outside `ids::` (`side-nav` mapped shell,
`{container}-label` xor headings, `tabs` mapped wrapper) are asserted by
the gate-owned sweep spec and the gate's structural checks — see
`tests/fixtures/ifml_gate/ifml-components.toml` for the mapped testids.

Sorting headers carry `aria-sort` (set only for sortable columns, only
when the ux sort plane is active). The `ids` constants themselves:
`actions`, `actions-menu`, `chip`, `copy`, `timeline`, `timeline-item`.

## 6. Playwright Object Model (POM, #315–#318)

Every generated Playwright spec drives the UI through an emitted
page-object layer — zero raw testid construction in spec bodies (the
#316/#317 rewires took the entity families from 164 raw constructions to
0). Two parallel POM families mirror the two emitters and share one
kernel contract.

### Kernel + page classes

| Pipeline | Kernel (once per run) | Page classes | Consumers |
|---|---|---|---|
| Entity (`ui-e2e-test`) | `ui/tests/generated/_support/pom.ts` — rendered from `templates/ui/test/_pom_kernel.tera` by the FIRST spec-emitting entity (`AtomicBool` once-per-run hook), rewritten every run | `ui/tests/generated/{domain}/{seg}.page.ts` from `templates/ui/test/page.tera` — every any-op entity | all 11 `{seg}.{family}.test.ts` families |
| IFML (`ifml-e2e-test`) | `tests/pages/support/base-page.ts` + `ux-table.ts` — Rust-built constants (`ifml/e2e_test/kernel.rs`) | `tests/pages/{view-kebab}-page.ts` — `ifml/e2e_test/pom_render.rs` renders the `ViewPom` plan | `{view}.spec.ts` / `.workflow.spec.ts` / `.ux.spec.ts` |

`BasePage` carries `goto` + `waitForHydration` (Svelte hydration +
selector visibility). `UxTable` is the shared ux contract component
object: rows/`rowFor`, `chipFor`, `sortBy` (aria-sort aware
click-to-cycle), `openActions`/`menu`/`menuDelete`/`deleteViaMenu`,
`copyCell` (clipboard poll), `timelineItems`/`timelineTimes`, plus the
Intl mirrors — `expectFormatted` runs the SAME
`Intl.DateTimeFormat`/`NumberFormat` (plan locale + rule currency) the
page runs, so format assertions cannot disagree with the markup. Row
visuals close the contract: `expectZebra`, `expectInactive`.

**Kernel parity**: both kernels declare the IDENTICAL public method
surface (names + semantics + `ids::`-mirrored fragments), pinned by
`pom_kernel_surface_matches_entity_kernel`, which renders the entity
template through the real embedded Tera and diff-extracts the surface.
The IFML kernel is Rust-built (not rendered from the shared template)
because the IFML generator must not depend on the passed-in Tera carrying
the ui templates (the external harness passes `Tera::default()`).

### Test-object lowering

The POM is the test-object lowering of the model plane: every
UxPlan/IFML-IR construct lowers to exactly one POM surface, and specs
assert only through it:

| Model construct | POM surface |
|---|---|
| `ColumnPlan { sortable }` + `?sort=` allow-list | `UxTable.sortBy(field, dir)` / `sortButton` / `sortHeader` |
| `Display::Chip` + `ToneMap` | `UxTable.chipFor(label)` |
| `ActionPlan` tiering at `[actions] inline_max` | `UxTable.openActions(row?)` / `menu()` / `menuDelete()` / `deleteViaMenu(row?)` |
| `Display::CopyChip` | `UxTable.copyTrigger(row)` / `copyCell(row, expected)` / `tooltipFor(text)` |
| `CollectionPlan::Timeline` | `UxTable.timelineRoot()` / `timelineItems()` / `timelineTimes()` |
| `[format]` Intl plane | kernel `formatDate/DateTime/Money/Quantity` mirrors (locale/currency from the plan) |
| Row visuals (`RowVisuals`) | `expectZebra()` / `expectInactive(row)` |
| Workflow config (states/edges) | entity page `expectState`/`transitionTo`; IFML page `expect{Comp}State`/`transition{Comp}To` — the transition testid map is the SAME edge enumeration `route_generator` renders as buttons |
| `NavigationFlow` (+ modal views) | IFML page `navigateTo{Target}()` / `close{Target}Modal()` (row click + `waitForURL` pattern; modal wrapper/close/back waits) |
| Mapped components (`ifml-components.toml`) | `ComponentSelectors` resolve the mapping's testids into the page class locators; `UxTable` attaches ONLY to unmapped fallback collections whose entity carries a plan |
| View structure (render/persona payloads) | page `heading()` / `primaryRoot()` / `navRoot()` / `containerRoot()` / `expectDenied()` |
| List row-delete surface (#463) | `{comp}DeleteViaMenu()` — `UxTable.deleteViaMenu` + confirm step |
| `on cancel` navigate event (#463) | `{comp}Cancel()` / `press{Comp}Cancel` |
| Test-side auth personas (#463) | `tests/e2e/personas.ts` — `test.use` storage-state + Bearer header + the ONE `__USER_ROLES__`/`__USER_CAPABILITIES__` seeding place; specs drive personas via the fixture, never inline `addInitScript` |

IFML page classes are built from the SAME resolution the route generator
renders from — `ComponentSelectors`, `workflow_for_entity`,
`html_input_for_dsl` (typed fills: select/check/fill/datetime-local per
input kind), the ux plan mirror — so the POM can never disagree with the
markup about which control does what. Handler parity: the first navigate
event drives the row testid the markup wires; menu events drive the
actions menu.

### Emission gating + lifecycle

- **spec-infra gated, never ux-flag gated** (locked #316 contract): the
  kernel + page classes emit whenever ANY spec emits — entity: any-op
  entities; IFML: any view with payloads. A schema-less IFML run emits
  page classes with NO navigation surface (flows/save/denial are
  payload-gated); the workflow and ux-object surfaces stay model-derived
  (markup parity — badges/chips/buttons render whenever the config/plan
  renders them).
- Generated-fresh every run (never-overwrite applies only to the harness
  stubs); `tests/pages/*-page.ts` of REMOVED views are swept by the next
  regeneration (the generator's staleness pass) — pinned by the gate's
  T4, alongside the surviving page classes and the kernel.

### Canary contract evolution

Flag-off output remains byte-identical **modulo the reviewed test-infra
files**: the POM kernel + page classes ARE part of flag-off output
(their gate is spec-infra, which is flag-independent), so the committed
pre-feature snapshots include them. Rebless discipline is unchanged —
`UX_RULES_BLESS=1` — but every POM-era rebless carries an ENUMERATED
reviewed diff (e.g. #316's entity rebless: exactly `_support/pom.ts`, 2
page classes, 12 spec rewirings, mechanical migration renumbering).

### Validation boundary

The IFML POM runs LIVE in the nightly gate (T3): all spec categories
drive the real API through the page classes. The entity POM is
pin/snapshot-verified (`ui_e2e_test_tests`, insta snapshots,
`ux_rules_tests` content pins, the byte-identity canary) plus a
typecheck over the regenerated review fixture (below); live entity-app
Playwright runs belong to consumer projects — hr-specs' ~4,000-spec
suite is the acceptance gate there.

### IFML test-side auth + journeys (#463)

The `ifml_e2e_auth` feature (default/fullstack ON) extends the IFML
e2e emitter with a test-side auth plane — no new app UI, no Supabase:

- **`tests/e2e/auth.setup.ts`** (Playwright `globalSetup`): provisions
  one API key per human-actor persona via `public.create_api_key`
  (psql over `DATABASE_URL`) and writes `.auth/{persona}.json` storage
  states carrying `apiKey`, roles, and capabilities (from the policy
  context; view-`roles` personas are the no-policy fallback).
- **`tests/e2e/personas.ts`**: per-persona `test.use(...)` fixtures —
  storage state, Bearer header, and the single `__USER_ROLES__`/
  `__USER_CAPABILITIES__` seeding place (the per-spec inline
  `addInitScript` stubs disappear when the flag is on).
- **`{view}.auth.spec.ts`** family: unauthenticated/garbage-key
  behavior; denial = redirect AND denial presentation; per-capability
  control-gating visibility.
- **`tests/journeys/*.journey.spec.ts`** family: the workflow handoff
  (persona A creates via the API, persona B transitions through the POM
  transition map, persistence verified via GET) and the shell-nav
  journey (routes.ts links clicked via `getByRole('link')`, never
  `goto()`).

Everything above is presence-gated on the flag: off ⇒ byte-identical
(the inline persona tests stay). The nightly gate's `auth` + `journeys`
T3 categories run both families against the live fixture app.

## 7. Diagnostics catalog

All diagnostics are advisory warnings printed to stderr as
`warning: ux-rules: {line}` (ui-page generator `report_ux_diagnostics`,
deduplicated across entities; IFML `resolve_generation_ux` returns
deduplicated lines for the same channel). They never fail generation, and
the plan still applies. Pinned at the `report()` boundary in
`crates/codegraph/tests/ux_rules_tests.rs` (in-process stderr capture is
impractical — see the ledger).

| Warning | Trigger | Exact text | Resolution |
|---|---|---|---|
| Money hint | numeric pg column whose name matches a money keyword; skipped when a matching rule pinned a DIFFERENT dimension | `column `total_amount` inferred Money from its name ("amount"); pin intent with a [[column]] rule: dimension = "money" (or "quantity")` | add `[[column]] name_pattern = "*_amount" dimension = "quantity"` (or confirm money) |
| Timeline suggestion | table rendering, ≥1 inferred TimePoint field, a `created_at`/`updated_at`/`due_*`/`*_at`-named field, and NO collection rule matched the entity (explicit table rules are deliberate opt-outs) | `entity "Task" renders as a table but has time-ordered data (field "created_at"); to opt into timeline rendering add:\n[[collection]]\nentity_pattern = "Task*"\ndisplay = "timeline"\norder_by = "created_at"\n(tables never auto-switch)` | add the literal rule block from the message |
| Overflow accounting | actions beyond `[actions] inline_max` fell into the menu | `2 row action(s) collapsed into the overflow menu ([actions] inline_max)` | raise `inline_max` or accept the disclosure |

Two hard (non-advisory) plan errors can also surface at generation time,
both `Error::Config`: a timeline rule whose `order_by` names no TimePoint
field (names the candidates), and a programmatic timeline rule without
`order_by`. The unresolvable-`order_by` case skips the entity page in the
ui pipeline (pinned by `ux_rules_timeline_unresolvable_order_by_skips_entity_page`).

## 8. Test-layers map

Three layers, node-free first:

| Layer | Suite | Command | Covers |
|---|---|---|---|
| PR-CI net (node-free, no Postgres) | `crates/codegraph/tests/ux_rules_tests.rs` (18 tests) | `cargo test -p codegraph --test ux_rules_tests` | full driver runs over the init fixture: default-pack pins, flag-off negatives, project-file override, timeline on/off, child-section tiering, sort-plane wiring (handler + query + page), diagnostics report() pins, `{seg}.ux.test.ts`/`{view}.ux.spec.ts` content |
| | `crates/codegraph/tests/ux_rules_byte_identity_tests.rs` (4 tests) | `cargo test -p codegraph --test ux_rules_byte_identity_tests` | the byte-identity contract: entity + IFML determinism canaries, committed pre-feature snapshots (which now include the POM test-infra files — see §6), `UX_RULES_BLESS=1` rebless |
| Generator-level (in-crate unit) | `crates/codegraph/tests/ui_e2e_test_tests.rs` (23 tests) | `cargo test -p codegraph --test ui_e2e_test_tests` | `{seg}.ux.test.ts` gating: spec absent when flag off / no list+create, per-feature blocks (chip/copy/format/align/sort/actions/first-column/timeline) |
| | `crates/codegraph/tests/ui_e2e_snapshot_tests.rs` | `cargo test -p codegraph --test ui_e2e_snapshot_tests` | insta snapshot of the canonical ux spec |
| | `codegraph-generate` lib tests (`crates/codegraph-generate/src/ux/*`, `crates/codegraph-generate/src/ui/e2e_test/`, `crates/codegraph-generate/src/ifml/e2e_test/`) | `cargo test -p codegraph-generate --lib -- ux e2e_test` | Pass-1 decision table, plan builder, config strictness matrix, sort projection, spec renderers, the POM pins: kernel surface parity (entity↔IFML), page-import↔kernel-emission coherence, `expect`-import gating, plan/payload-driven page-class surface |
| Nightly real-API gate | `crates/codegraph/tests/ifml_codegen_gate.rs` | `cargo test -p codegraph --test ifml_codegen_gate -- --ignored --nocapture` | full pipeline + axum + Playwright; the gate fixture is ux-ON (`fixtures/ifml_gate/ux-rules.toml` merged over the pack), `ux` is an assert_categories entry, dropdown-menu/tooltip stubs back the fallback-table markup. Since #318 every spec category drives the app through the POM page classes; T4 pins stale-POM cleanup |

## 9. Deferred ledger

Each entry: the #286 row it serves, reserved config vocabulary where it
exists, and what it would take.

| Item | #286 row | Reserved vocabulary | What it would take |
|---|---|---|---|
| Sticky headers/columns + scroll orchestration | 5 | `[[collection]] sticky_first_column = true` (unimplemented) | `position: sticky` th/td classes in `list_page.tera` gated on a collection payload; scroll-container testids |
| Batch actions + checkboxes | 10 | `[actions] batch = [...]` (unimplemented) | leftmost checkbox column + selection state + bulk DELETE endpoint; TopBatchToolbar semantics from the #286 discussion |
| Advanced pagination | 12 | `[collection] page_sizes = [...]` (unimplemented) | page-jump + items-per-page controls in the pagination block; API already takes page/page_size |
| Inline editing | 18 | `[[column]] editable = true` (unimplemented) | PATCH-per-field endpoint + optimistic cell forms; conflicts with the dto/permission model — needs a design pass |
| Nonmodal side panel | 22 | `[[collection]] detail = "panel"` (unimplemented) | Sheet-primitive detail route rendered beside the table instead of navigation |
| Adjustable row density | 23 | `[collection] density = "compact|regular|relaxed"` (unimplemented) | row-height CSS variable + a density toggle persisted per user |
| Avatars/icons per identity columns | 29 | `[[column]] display = "avatar"` (unimplemented — reserve the string) | avatar branch in `_ux_cell.tera` over entity-ref/user columns; needs an image/initials source convention |
| Onboarding flows (sequenced guidance) | #286 principle 2 (3:50–4:18) | none | entirely new generator family; not a per-column rule |
| In-cell "Show More" disclosure | 16 | `[[column]] clamp_lines = N` (unimplemented) | line-clamp + expand branch in `_ux_cell.tera` next to the tooltip branch |
| Schema-description table subtitle | 19 | none (read `description` already on SchemaNode) | thread the schema description into `UiPageContext` + a subtitle block in `list_page.tera` |
| AI labeling | 20 | none | N/A until generated apps can carry AI-generated content |
| Cards / teaser layout | 26 | `[[collection]] display = "cards"` (unimplemented) | new `CollectionPlan::Cards` variant + template; shares `column_order` |
| Minimalist borders (pack CSS) | 32 | none (a pack/CSS concern) | shipped pack CSS already trends borderless; full removal is a design pass over `app_css.tera` |
| Hover-reveal copy chips | 33 | none (v1 ships always-visible chips) | opacity-0 group-hover transition on the copy button in `_ux_cell.tera` |
| Spanner heads | 34 | `[[column]] group = "…"` (unimplemented) | header-row grouping in `list_page.tera` keyed on a column payload |
| Section heads | 35 | `[[collection]] section_by = "…"` (unimplemented) | grouped rendering pass over sorted rows in the table template |
| Charts from time-ordered data | 31 (chart half) | `[[collection]] display = "chart"` (unimplemented) | new CollectionPlan variant + a chart template; timeline covers the v1 need |
| LSP UX diagnostics (rule violations in-editor) | — | none | needs the rules parse to run inside the LSP server and a virtual document for the TOML |
| Upstream rexlang `dimension:` DSL extension | — | `dimension` on mox features | the config seam already exists (Dimension vocabulary); rexlang grammar + bridge work is upstream |
| react/vue parity for IFML framework targets | — | none | the IFML svelte template carries the ux markup; port the attr-driven branch chain per framework |

### Known follow-ups (shipped-state caveats)

- **Workflow-status chips are unexercisable on fresh creates**: create DTOs
  carry no status field, so the `{module}-chip` workflow assertions can't
  run against a just-created row. The fix belongs to the workflow/repo
  emitters (return the created row's status), not the ux plane. Surfaced
  by the #303 gate.
- **`get_child_schemas` graph route now derives mox `refers`-array
  children** (fixed in #312; surfaced by #299): the graph route resolves
  inline `#/$defs` children (`parent_schema`) PLUS derived refers children
  (`Schema -[:HasProperty]-> Property {is_array} -[:ItemsOf]-> entity` —
  the FK-on-child lowering). `entity_config` (`role = "child"` + `parent`)
  stays authoritative: consumers merge config children first and dedupe.
  Scalar `refers` remains config-only (intent ambiguous — the same FK
  shape serves plain references). Note: a `refers` without an explicit
  multiplicity defaults to MANY upstream (rexlang `lower_relation`), so
  single-valued back-references like the init starter's `refers
  TodoListType todoList` lower as arrays and DO derive children.
- **Diagnostics are pinned at the `report()` boundary**: in-process
  stderr capture is impractical, so the exact strings are pinned in
  `ux_rules_tests.rs` and the `warning: ux-rules: ` prefix framing is
  mechanical.
- **FTS search ignores sort**: when `?q=` is present the handler
  delegates to `search()` and ts_rank wins; `?sort=` is still validated
  and survives `q` in the UI query string, but the ordering is
  search-ranked. A `q`-aware sort would need ORDER BY composition over
  the FTS ranking.

## Adding a new rule (convention)

1. **Vocabulary**: extend `Dimension`/`Display` in
   `crates/codegraph-config/src/ux/` if needed (kebab-case, closed set —
   unknown values are parse errors by construction).
2. **Pack default**: add or adjust the rule in
   `crates/codegraph-config/src/ux/packs/ux_default.toml`; update the
   `builtin_pack_parses_with_expected_defaults` pin.
3. **Inference or rule path**: extend the Pass-1 decision list
   (`crates/codegraph-generate/src/ux/dimension.rs`, keep the table-driven test
   green) or the selector/payload fold (`plan.rs`).
4. **Both emitters**: render in `crates/codegraph-generate/templates/ui/list_page.tera` +
   `_ux_cell.tera` AND `crates/codegraph-generate/templates/ifml/svelte/page.tera`; keep every new
   block behind a ux-gated `{% if %}` so flag-off stays byte-identical.
5. **Test pins**: `{seg}.ux.test.ts` block in
   `crates/codegraph-generate/src/ui/e2e_test/` + template, a Rust pin
   in `ux_rules_tests.rs`, and generator-level unit tests.
6. **POM surface**: any new ux capability must consider its POM surface
   and the spec assertion that exercises it — a kernel method (`UxTable`),
   a page-class section (`ui/test/page.tera` payload / `ifml/e2e_test/pom.rs`
   plan), or both — so specs keep asserting through the page objects, never
   raw testids (see §6).
7. **Gate category**: extend the gate fixture + `assert_categories` in
   `ifml_codegen_gate.rs` so the nightly real-API run exercises it.
