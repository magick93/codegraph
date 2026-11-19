# TDD: injected-scope `when` guards (issue #663, `condition_paths`)

Siblings: #659 (typed queries), #660 (transition binding), #661 (child ops), #662 (statement bodies) — same treatment.

## Red

`cargo test -p codegraph --test service_ops_tests injected_scope` over the guard fixture (PayRun submits when its Timecard is Approved, `inject TimecardRepository`):

```
Config("capability `submit`: `when` condition uses field access
`TimecardRepository`.timecardStatus, which is not an own-record field;
only bare field references are supported, which cannot be lowered to a
Rust guard on `PayRunType`; ...")
```

Fixture authoring notes (two upstream constraints the first attempt hit):
- the module's state-literal namespace is GLOBAL — `Approved` declared by two machines is E0354; the fixture's PayRun machine uses `Posted`;
- the `when (...)` clause takes ONE raw-parens span — the whole condition sits inside a single paren pair;
- `refers X holder` (no multiplicity) is a LIST; `refers HolderType[1] holder` is the required-single the deep-path chain needs to compile upstream past R3.

## Green

- Ingest: `DddServiceOperation.condition_paths` typed mirrors + the `service_operation_node` mapping; `ddd_ingest_tests` pins the readback.
- Lowering: scope table (`InjectedScope`) threaded through `lower`/`comparison`/`field_vs_literal`/`resolve_operand`; own-record output byte-identical (`response.` unchanged). The authorize fn goes async (`&AppState, &ApiKeyInfo, &response`) only when injected paths exist; FK extraction (Option-aware), the sibling `find_by_id` (auditable arity from the same `resolve_is_auditable` the query plane uses — cannot drift), the loaded-record comparison.
- Multi-machine: the RELEASE fixture gains the op-level `when` pin (passes pre-change — the gate lived upstream and is lifted at 4f497e1; the pin is the regression net + the `condition_paths` own-record evidence).

## Review-driven hardening (the adversarial pass — REQUEST_CHANGES, both majors fixed)

1. **The hurl planner's "conservative 403" was unsound in both directions.**
   - Optional FK + eq-initial guard: planner 200 (satisfiable), handler 403 (fresh FK NULL — the create body omits nullable props). THE most natural spelling.
   - Non-machine injected field (`hours > 8`): fresh value 42 → handler 200, planner pinned 403. "Conservative denial" is backwards for contract ASSERTIONS — a wrong pin fails the suite either way.
   - Blind spots (negation polarity, `||`, float/bool/date literals) each landed in the 403 bucket.
   - **Fix**: a three-verdict model — `Satisfied` (every FK required + every comparison machine-decided and passing on the fresh row) → normal legs; `Denied` (optional FK ⇒ NULL ⇒ the gate denies before evaluating; or a machine-decided comparison failing under plain AND) → 403 leg; `Unverifiable` (anything the collector cannot decide: negation, `||`, non-machine fields, exotic literals) → the leg is OMITTED (the `Body { may_refuse }` precedent). The collector only pushes comparisons under PLAIN AND from the root — the first cut collected negated comparisons and could have decided one upside down (caught by the KAT before push).
   - KATs: `fresh_fixture_verdicts_are_provable_never_guessed` (6 cases incl. the reviewer's exact 200-vs-403 shape), `undecided_comparisons_omit_the_leg_never_guess` (4).
2. **Cross-domain injected guards broke worker crates silently.** The #590 handlers run in per-domain worker crates whose `AppState` carries only its own domain's slots — `state.{other_domain}_…` is E0599 at consumer build with no generation-time refusal. **Fix**: `BodyRenderContext.workers_topology` (rides `from_project`, so every `routes_for_entity` caller sees it) + a named refusal under workers. Monolith keeps lowering — pinned both ways in `injected_guards_refuse_under_workers_topology`.
3. The rust-name alias over-stripped (`trim_end_matches("Type")` strips repeatedly; the rust-name rule strips ONCE via `strip_suffix`) — `TaskTypeType` would alias wrong. Fixed to strip-once; the sanitize step's digit/length edges documented as uncovered (those titles miss the alias and fall back exactly as before).

## Teeth

- Compile gate (its own binary — #746): first run caught the design-op override missing rust-name lookups (E0599 on a design without `delete`); reverted-gate run fails with the emitted file list.
- The planner verdict KATs are hand-built scopes + conditions — breaking `collect_injected_comparisons`' plain-AND context (the first cut) fails `undecided_comparisons_omit_the_leg_never_guess`.

## Deliberately not done

- `field_type` rides the graph unread (the documented lowering contract; the lowering re-parses the source).
- Mixed entities can surface one refusal twice (`check_service_op_bodies` + `check_service_op_guards` both run `routes_for_entity`; identical text, different check names).
- Child-op-with-injected-path refusal is defensive (the upstream compiler rejects injected paths in a `children` block — no service means no injections).
- A seeded-FK alternative (create the injected parent with the required state in the contract preamble) — bigger fixture machinery; the verdict model is sound without it.
