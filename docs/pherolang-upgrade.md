# Upgrading to pherolang 0.3 (rexlang rebrand + the DDD/IR epic)

Status: planned (epic + sub-issues filed)
Pinned before: `rex-*` @ `1fe0ca90006613dfc9bd8ee6c7feee0241f2b4cf`
Pinned after: `phero-*` @ `298267e32a5112dc6fd84708868b9bc59d65ade7`

This document is the durable record of *why* the dependency moves, *what*
breaks, and *in what order* the work lands. The GitHub epic (see
"Tracking") carries the issue-level detail; this file carries the review,
the migration contract and the gates.

---

## 1. Upstream: what changed

`yestechgroup/rexlang` now 301-redirects to **`yestechgroup/pherolang`**.

| | |
|---|---|
| Releases | `v0.2.0` (foundation/security), `v0.3.0` "Phero Lang" (rename + the one deliberate wire break), 2026-11-14 |
| Default branch | `298267e` — `v0.3.0` **plus the DDD epic** (DD1–DD9, PRs #307–#311) |
| Crates | all 22 `rex-*` → `phero-*`; `RexBackend`/`RexError`/`RexExpr` → `Phero*` |
| Env | `REX_*` → `PHERO_*` (`REXFMT` → `PHEROFMT`) |
| Binary | `rexlang` → `pherolang`; LSP identity + diagnostic `source` → `pherolang` |

We pin **`298267e`** (default-branch HEAD), not `v0.3.0`: the DDD epic is where
the value is, and the repo pins revs rather than tags.

### 1.1 The wire break

0.3 does **not** read 0.2 artifacts — there is no dual-accept window.

| Surface | Before | After |
|---|---|---|
| Provenance key | `rexVersion` | `pheroVersion` |
| Instance discriminator | `"$type": "rex.instance"` | `"$type": "phero.instance"` |
| JSON Schema `$id` URN | `urn:rex:model:<pkg>:<profile>` | `urn:phero:model:<pkg>:<profile>` |
| Artifact file convention | `.rex.json` | `.phero.json` |
| Format markers | all `1` | all **`2`** (`FORMAT_VERSION`, `ACTOR_`, `DDD_`, `IFML_`, `IFML_MANIFEST_`, `EVENT_`, `DEPLOY_`, `USE_CASE_`, `CMMN_`, `DMN_`, `INSTANCE_`) |

Three markers deliberately stay at `1`: `GRAPH_MODEL_FORMAT_VERSION`, the
`graph query` envelope, and the evidence receipt/proof-register formats.

### 1.2 Dependency and security fallout (net positive)

- **salsa 0.23.0 → 0.28.5** — clears `RUSTSEC-2026-0308` (unsound interned
  values) and retires the codegraph#535 watch item.
- **tokio 1.53.1 → 1.54.1** — clears `RUSTSEC-2026-0386` (retires #533) once
  the lockfile refresh lands with the bump.
- **pest/pest_derive leave the graph** — `.ifml` moved to logos + chumsky
  with byte-identical artifacts and a normalised EBNF as the single grammar
  authority.
- `tower-lsp-server` 0.23 replaced dormant `tower-lsp` (upstream LSP only;
  our own LSP crate is unaffected).
- New transitive git deps already anonymous-fetchable: `xstate-ir`
  (restingstate), `dsntk-*` (yestechgroup fork), `sigil-*` @ `69115a7`
  (unchanged — lockstep is a no-op).

### 1.3 New IR surfaces we now receive

`.mox` (additive):

- `Feature.collection` — `"set"` for `T[] { set }` (also lowers
  `constraints.unique: true`).
- `ClassDef.table_name` — `class X table "explicit_name" { … }`.
- `ClassDef.kind` — `ClassKind::{Entity, Register}` (`register Role { … }`).
- `SessionKind` on grants (`human` / `agent` / `any`).
- `phero_syntax::words` — the public word tables (`KEYWORDS`,
  `MOX_CONTEXTUAL`, …) that our hand-maintained `migrate` list duplicates.

`.ddd` (the DD1–DD9 epic):

| # | Surface | Ir field(s) |
|---|---|---|
| DD1 | resolved identities + provenance | `pheroVersion`, `Design.typeRef`, `Delegation.targetRef`, `Service.dependencyRefs`/`decisionDependencyRefs`, `ServiceOperation.*Refs`, `SearchDef.entityRef` |
| DD2 | strict value semantics | `Design.semantics` (`identity`/`immutable`/`deepImmutable`/`equality`, no opt-out) |
| DD3 | DTO boundaries | `Design.dtoShape`, `DddTypeRef` (untagged: domain `TypeRef` \| `{"dto":…}`), `DddParam`; **`Design.class` becomes `Option<String>`** |
| DD4 | aggregate boundaries | `Design.isAggregateRoot`, `Design.aggregate {root, origin}`; `extend` renamed `children` (`extend` warns E0387) |
| DD5 | invariants | `Design.invariants: [{expression}]` |
| DD6 | collections | (the `.mox` `collection` marker above) |
| DD7 | error vocabulary | `DddModel.errors`, `Module.errors`, `ErrorDecl`, `throws` on ops |
| DD8 | multi-file applications | `compile_ddd_app_str`, `DddFragment`, `gather_ddd_app_imports` |
| DD9 | typed repository queries | `RepositoryOperation.query` (`predicate`/`sort`/`pagination`/`readOnly`) + `capabilities` |

Also: `ServiceOperation.{condition_paths, transition, child_scope, throws,
bodies, body_receiver}`, `Design.child_operations`, `TargetRef`/`CapabilityRef`/
`EventRef`/`DecisionRef`.

`.ifml`: grammar unchanged; `check_ifml_with_decisions`, `.dmn` decision data
(`data:` may name a decision service), lint channel W0150+.

New surfaces beyond the nine we consume: `.dmn`, `.uc`, `.cmmn`, a unified
`GraphModel` with `pherolang graph build|query` analytics, `pherolang artifact
check`, and `pherolang verify` receipts.

### 1.4 Our six upstream proposals all shipped

`docs/proposals/001–006` were asks against rexlang. Every one landed:

| Proposal | Shipped as |
|---|---|
| 001 explicit service-op transition binding | `ServiceOperation.transition` → `TransitionRef {trigger, machine}` |
| 002 module/table name uniqueness | `table "…"` annotation + upstream E0301 |
| 003 executable service bodies | `bodies["stmt"]` = `phero_expr::stmt::StmtBody` JSON + `body_receiver` |
| 004 aggregate child operations | `Design.child_operations` + `ChildRef {feature, class, action}` |
| 005 multi-machine / cross-entity `when` scoping | `ServiceOperation.condition_paths` (`ConditionScope::OwnRecord` / `Injected{repository, class}`) |
| 006 register properties + session grants | `ClassKind::Register` + `SessionKind` |

The codegraph-side work is therefore *deleting workarounds*, which is what
makes P5 worth doing in the same epic.

---

## 2. What breaks here (the migration contract)

### 2.1 Compile-level

| # | Site | Break | Fix |
|---|---|---|---|
| 1 | 54 `.rs` files, 2 crate `Cargo.toml`s, ~109 files incl. docs | `rex_*`/`rex-*` → `phero_*`; repo URL → `pherolang.git` | mechanical rename; `ingest/rex_imports.rs` → `ingest/phero_imports.rs` |
| 2 | `codegraph-generate/src/mox_expr.rs:827-925` | exhaustive `match AlgebraKind` — new `AlgebraKind::All` | add the arm (`iter().all(…)`), or refuse explicitly |
| 3 | `codegraph/src/expr_json.rs:164-173` | same | add `"all"` |
| 4 | `ingest/ddd_ingest.rs:391,436` | `Design.class: Option<String>` | standalone DTOs (`class == None`) skip ingest with one WARN; mirror keeps `String` |
| 5 | `init/commands.rs:921-929` | `p.type_` / `op.return_type` are `DddTypeRef` | `as_domain()` for class detection |
| 6 | `tests/ddd_ingest_tests.rs` | DDD struct literals (new fields, `DddParam`, `DddTypeRef`) | rebuild via the upstream builders (`RepositoryOperation::builtin/declared`, `DddModel::new().module(…)`) |
| 7 | `ddd/service_ops.rs:2624-2732`, `ddd/design.rs:469-510` | signature helpers + `from_value::<TypeRef>` on a `DddTypeRef` payload | route through one `ddd_signature_type()` helper; DTO refs get a **named** rejection, not "unresolvable" |

Everything else we consume is byte-compatible: `TypeRef`, `Multiplicity`,
`Feature`/`ClassDef`/`Model`, `ActorModel`/grants, `EventModel`, `DeployModel`,
`IfmlModel`, `IfmlManifest` shapes are unchanged (additive at most), which is
why generated output must stay **byte-identical** through P1–P4.

### 2.2a The one deliberate behaviour change: a `.mox` compile that lowers no model

pherolang 0.3 rejects models it used to accept (E0300 duplicate stripped
module names, Rule 16 typed lifecycles, …). `ingest_mox_files` used to turn
*any* compile failure into a warning and an empty ingest — so a rejected
model produced a zero-entity, zero-error app (the WS5 failure mode). The
failure of `module_collision_tests::colliding_entity_titles_hard_error_naming_both`
is what surfaced it: upstream already rejected the collision, so codegraph's
own gate never ran.

Now: **no model ⇒ `Error::MoxModel { reason }`** with the rendered
per-file diagnostics (the `DddModel`/`EvtModel` precedent), while
file-level problems that still leave a partial model keep warning. Issue
#675. No generated bytes change; a run that *should* have failed now does.

### 2.2 Runtime / gate-level

- **`GRAPH_FORMAT_VERSION` 4 → 5 (mandatory).** The graph-cache inputs hash
  covers model/config file contents + graph format version + grafeo storage
  generation — **not** the compiler rev. Without the bump every consumer with
  a warm `.graph-cache/` silently reuses the old ingested graph. The bump also
  drags `tests/fixtures/artifact_kit/graph_document_v4.md` → `v5` (and the
  four doc-comment references + `artifact_kit_tests::CASES`).
- Goldens: `tests/fixtures/ddd/library.ddd.json` (`DDD_GOLDEN_BLESS=1`),
  `tests/fixtures/evt/orders.evt.json` (**no** bless switch today — add
  `EVT_GOLDEN_BLESS`, or hand-bump `formatVersion` 1 → 2 and pin the rest as
  unchanged).
- `tests/fixtures/workers_gate/Cargo.lock` regenerates (rex entries → phero).
- Every `.mox`/`.ddd`/`.evt`/`.deploy`/`.ifml`/`.actor` fixture must
  recompile: the epic added hard rules (Rule 16 typed lifecycles, DD2 value
  rules R-V1..V3, DD3 "a DTO carries no op", DD4, DD7 `throws` completeness),
  and our ingest hard-fails on error diagnostics by design. **The pre-flight
  probe already swept them: all pass, with `ddd/invalid.ddd` failing by
  design** (see §3/P0) — the work here is the cache/golden mechanics, not
  model surgery.
- `codegraph migrate`'s hand-maintained `KEYWORDS` list (`migrate.rs:24-62`)
  is short several real keywords (`event`, `channel`, …) — replace it with
  `phero_syntax::words::KEYWORDS` (and treat `register` as contextual).
- User-facing strings: ~48 in `init/commands.rs`, plus `migrate.rs`,
  `ingest/rex_imports.rs`, `ifml_actor_import.rs`, `cli.rs` help text,
  `driver.rs` pass banners, `templates/project/{wrapper_main,readme}.tera`,
  `templates/db/policy_rls.tera`, issue references `rexlang #114/#118/#49`.
- `cargo audit`: retire the `RUSTSEC-2026-0308` + `RUSTSEC-2026-0386` watch
  entries in AGENTS.md (#535/#533) once salsa 0.28 / tokio 1.54 are in the
  lock.
- VS Code extension: `codegraph-vscode/grammar-mox` description, plus grammar
  gaps for `table "…"`, `register`, `{ set }`; regenerate the committed
  tree-sitter parser.

---

## 3. Phases

### P0 — pre-flight evidence

- Baseline: `RUSTFLAGS="-C debuginfo=0" cargo test --workspace` green on the
  old pin (record the run).
- Fixture sweep: run the new compiler (`pherolang check`, or an in-repo probe
  over `phero-driver`) across `crates/codegraph/tests/fixtures/**` and the
  known consumer models before writing code, so the fixture repairs are
  enumerated up front.

**Sweep result (done — probe over `phero-driver` @ `298267e`, compiling each
fixture the way the pipeline does: `.mox` as one union per fixture group,
`.ddd`/`.evt` with their transitively gathered domains, `.ifml` through
`parse_ifml` + `compile_ifml_str` + `check_ifml_with_actors`):**

```
PASS  mox   ddd/{equivalence,library}.mox, evt/orders.mox, ifml_gate/refunds.mox,
            workers_gate/model/{billing,common,dispatch,fleet,inventory,leave,
                                payroll2,scheduling,security,vendor}.mox
PASS  ddd   ddd/{equivalence,library}.ddd, ifml_gate/refunds.ddd, workers_gate/design/leave.ddd
PASS  evt   evt/orders.evt
PASS  ifml  ifml_gate/app.ifml
FAIL  ddd   ddd/invalid.ddd  (intentional: the hard-error fixture —
             "entity 'Chapter' is contained by 'Library'; only aggregate roots
              may declare a repository")
```

i.e. **no fixture repairs are required**; the only failure is the fixture that
is *supposed* to fail, and it fails with the wording the hard-error test
already pins. The remaining P3 work is the cache/golden mechanics, not model
surgery. Consumer models (crewbase, hr-specs) still need their own
sweep — they were not available to the probe.

### P1 — rebrand + rev bump (mechanical, one PR)

- Root `Cargo.toml`: `pherolang.git` + `rev = "298267e…"`, `phero-ir` /
  `phero-driver` / `phero-expr` / `phero-ifml`, rewritten pin comment
  (rebrand, the wire break, the audit retirements, the DDD epic).
- `crates/codegraph/Cargo.toml`, `crates/codegraph-generate/Cargo.toml`.
- Mechanical `rex_` → `phero_`; `ingest/rex_imports.rs` →
  `ingest/phero_imports.rs`; user-facing `rexlang` → `pherolang`; `Cargo.lock`.

### P2 — API alignment (compile)

The seven rows of §2.1, plus one shared `ddd_signature_type()` helper in the
ingest/generate boundary so the `DddTypeRef` split is handled in exactly one
place.

### P3 — green suite

Bless the two goldens, regen the fixture lock, bump `GRAPH_FORMAT_VERSION` +
kit doc (no fixture repairs expected — the probe sweep is clean), then: canaries
(`ux_rules_byte_identity`, `forms_byte_identity`, `policy_rls` snapshot) with
an **enumerated** diff (expected: none), and the full gate list of §4.

### P4 — docs, policy, proposals

`AGENTS.md` (rename + a "Pherolang 0.3 upgrade" note: wire break,
`formatVersion` 2, cache v5, `.ifml` parser change, audit entries);
`docs/proposals/001–006` → status "implemented upstream" with the field each
became; `docs/rosetta*.md`, `docs/ux-rules.md`, `docs/systems-and-generators.md`,
`docs/mox-simplification-ledger.md`, `README.md`.

### P5 — feature adoption (one PR per item, ordered by value)

Each item **deletes** a heuristic or seam we wrote as a workaround.

1. **DD9 typed queries — LANDED (#659).** A `query`-carrying declared
   operation now lowers through `ddd/repository_query.rs`: the predicate is
   a **parameterized** SeaORM `Condition` / cornucopia named-bind set
   (literals bound too — `:_lit0`), `sort` becomes the ORDER BY with the
   `, "id" ASC` identity tiebreaker, and `page limit/max` the runtime
   `limit`/`offset` window (default = `limit`, cap = `max`; `page cursor`
   lowers LIMIT/OFFSET with a one-time WARN — keyset is recorded, not
   silently dropped). The closed subset is `self.x`/bare stored scalars,
   parameters, string/number/boolean/date literals, `== != < <= > >=`,
   `&& || !` (negation pushed down), parentheses, and null comparisons;
   EVERY other construct — arithmetic, calls, `?.`, `?:`, `let`, lambdas,
   collection algebra, `if`, class-typed parameter navigation,
   column-to-column comparisons — is a **named hard rejection**
   (`query \`Repo.op\`: … cannot be lowered …`) raised by
   `ValidationPass::check_repository_queries` BEFORE generation, never a
   broader query. Gate: `cargo test -p codegraph --test ddd_query_tests`
   + the in-crate `ddd::repository_query` predicate matrix + the extended
   `repository_compile_tests` / `worker_wasm_compile_tests` compile pins.
   The equality-finder heuristic (`lower_finder` + the contained-column
   EXISTS machinery) is **retained only for query-less declared
   operations** — consumer models still declare `findByX(x)` — and is
   marked deprecated in the module docs. Remaining: a generated HTTP route
   carrying the query's page (the repository plane first, by design),
   runtime `capabilities` enforcement (needs that route), the keyset
   predicate, class-typed parameter navigation, DTO-boundary returns, and
   lexicographic `spec.sort` ordering (the `?sort=` allow-list plane is
   unrelated — a design query's ORDER BY is generation-time, fixed).
2. **001 transition binding** — `ServiceOperation.transition` replaces the
   op-name→trigger / suffix-stripping / capability-sharing /
   `condition_source_states` heuristics in `ddd/service_ops.rs` and the hurl
   state-aware contract planner; multi-machine qualifier supported.
3. **004 child ops** — `Design.child_operations` + `ChildRef` replaces the
   cross-entity 501 seams (`addTimecardItem`, `punch`, child assignment ops).
4. **003 executable bodies** — lower `bodies["stmt"]` (`StmtBody`) instead of
   the 501 seam; `body_receiver` binds `self`.
5. **005 when-scoping** — `condition_paths` lifts the machine-bearing-module
   restriction and admits `Injected{repository, class}` guards.
6. **DD6 sets** — `Feature.collection` → `BTreeSet` in domain types (Ord
   requirements; refuse `float`/`double` sets with the documented fallback),
   `uniqueItems` + DB uniqueness, DTO/UI surfaces.
7. **DD7 errors** — declared vocabulary + `throws` → generated error enums and
   precise HTTP mapping (prelude `NotFound`/`Conflict` → 404/409).
8. **DD5 invariants + DD4 aggregate boundaries** — invariant checks at value
   construction, aggregate save (root + members) and after transitions
   (`InvariantViolation`); repository placement + child routing read the
   authored boundary instead of re-deriving containment.
9. **DD3 DTO boundaries** — model `dto` designs → generated boundary types;
   `DddTypeRef::Dto` signatures resolve to them; refuse output-DTO params
   through the API plane.
10. **`.mox` surface** — honour `table "…"` (pg table name), map
    `ClassKind::Register`, and switch `migrate` to `phero_syntax::words`.
11. **Editor** — `grammar-mox` + committed tree-sitter parser for
    `table`/`register`/`{ set }`; LSP completion contexts.
12. **006 Part B (decision-gated)** — `SessionKind` grant predicates in
    `policy_rls` (Postgres group roles per session kind).

### P6 — new surfaces (optional epics)

- `.dmn` decision plane: ingest, `invokes` resolution, `.ifml` decision data.
- `.uc` / `.cmmn` ingest.
- Upstream `GraphModel` + `graph query` analytics (separation-of-duty, dead
  capabilities, reachability) as a validation plane over our graph.
- `pherolang artifact check` / `verify` receipts as CI gates over committed
  artifacts.

---

## 4. Gates

| Phase | Gate |
|---|---|
| P1/P2 | `cargo build --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --check` |
| P3 | `RUSTFLAGS="-C debuginfo=0" cargo test --workspace`; `--test mox_ingest_tests`, `mox_equivalence_tests`, `mox_identity_tests`, `mox_ops_tests`, `mox_plane_compile_tests`, `ddd_golden_tests`, `ddd_ingest_tests`, `ddd_pipeline_tests`, `evt_golden_tests`, `evt_ingest_tests`, `evt_pipeline_tests`, `deploy_ingest_tests`, `ifml_dsl_features_tests`, `ifml_e2e_tests`, `ifml_scaffold_tests`, `ifml_actor_import_tests`, `artifact_kit_tests`, `ux_rules_byte_identity_tests`, `forms_byte_identity_tests`, `policy_rls_snapshot_tests`, `repository_compile_tests`, `command_compile_tests` |
| P3 (nightly lane) | `ifml_codegen_gate`, `entity_codegen_gate`, `service_ops_compile_gate`, `service_ops_hurl_gate`, `worker_wasm_compile_tests`, `workers_gateway_compile_tests`, `review_api_suite`, `workers_live_gate`, `events_runtime_tests`, `hooks_cornucopia_runtime_tests` |
| P4 | `cargo test -p codegraph --test wrapper_drift_tests`, `doctor` smoke |
| P5 | per-item gates in each issue |

Local full-suite runs must mirror CI's `RUSTFLAGS: -C debuginfo=0` (see
AGENTS.md "Testing").

---

## 5. Risks

1. **Consumer-model lockstep.** Our DDD ingest hard-fails on compile errors and
   the epic added many. A consumer (crewbase, hr-specs) must fix its `.ddd`
   before or with the upgrade. Mitigation: the P0 sweep, a doctor-visible
   summary, and a migration section in the release note.
2. **Silent cache staleness.** Forgetting the `GRAPH_FORMAT_VERSION` bump is
   the worst failure mode here (old graph, new generators, no error).
   Mitigation: bump in the same commit as the rev, plus the kit-doc rename.
3. **Byte-identity drift.** P1–P4 must change no generated byte; every
   canary re-bless requires an enumerated diff. P5 items *do* change output —
   they must carry their own blessed diff and consumer migration note.
4. **Scope creep.** P5 is twelve separable workstreams; none may ride the
   upgrade PR.
5. **Two settled decisions** (recorded so they are not re-litigated):
   - the DDD golden's `pheroVersion` line is a deliberate **rev canary** —
     every `phero-*` bump re-blesses `library.ddd.json`, which is how a
     dependency move stays visible in review. Do not normalize it away.
   - **no `doctor` hint** for the one-time cache re-ingest: the format
     version is already printed (`graph format vN`) and the `INFO`/`PASS`
     cache line names cold vs warm, so a dedicated hint would be noise.
6. **Upstream churn.** 0.3 is pre-1.0 with a stated wire break policy; treat
   the rev pin as the contract and re-run P0/P3 on the next bump.

---

## 6. Tracking

Epic: [#654](https://github.com/magick93/codegraph/issues/654).

| Phase | Issues |
|---|---|
| P1 | [#655](https://github.com/magick93/codegraph/issues/655) rebrand + rev bump |
| P2 | [#656](https://github.com/magick93/codegraph/issues/656) API alignment |
| P3 | [#657](https://github.com/magick93/codegraph/issues/657) fixtures, graph-format v5, green suite |
| P4 | [#658](https://github.com/magick93/codegraph/issues/658) docs, policy, proposals |
| P5 | [#659](https://github.com/magick93/codegraph/issues/659) DD9 queries · [#660](https://github.com/magick93/codegraph/issues/660) transition binding · [#661](https://github.com/magick93/codegraph/issues/661) child ops · [#662](https://github.com/magick93/codegraph/issues/662) bodies · [#663](https://github.com/magick93/codegraph/issues/663) when-scoping · [#664](https://github.com/magick93/codegraph/issues/664) sets · [#665](https://github.com/magick93/codegraph/issues/665) errors · [#666](https://github.com/magick93/codegraph/issues/666) invariants/aggregates · [#667](https://github.com/magick93/codegraph/issues/667) DTO boundaries · [#668](https://github.com/magick93/codegraph/issues/668) `.mox` surface · [#669](https://github.com/magick93/codegraph/issues/669) editor · [#670](https://github.com/magick93/codegraph/issues/670) session kinds |
| P6 | [#671](https://github.com/magick93/codegraph/issues/671) `.dmn` · [#672](https://github.com/magick93/codegraph/issues/672) `.uc`/`.cmmn` · [#673](https://github.com/magick93/codegraph/issues/673) semantic graph · [#674](https://github.com/magick93/codegraph/issues/674) artifact/verify gates |

