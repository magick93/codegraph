# WP1.5 — Namespaces vs dir-derived domains — what does the table assume (#267/#268)?

## Verdict

Namespaces are NOT mapped onto domains in codegraph today — there is no
namespace axis at all in the JSON-schema (or mox) pipeline. Domain identity
is derived purely from the schema FILE PATH: `extract_domain_from_path`
(`crates/codegraph/src/ingest/schema_loader.rs:320-330`) takes the segment
immediately before `/json/` (handling both `<domain>/json/...` and
`<version>/<domain>/json/...`, falling back to the first segment), and the
result rides on `SchemaNode.domain`
(`crates/codegraph-core/src/types/schema.rs:17`). Ownership conflicts are
resolved by TITLE dedup, not visibility: in `compute_generation_order`
(`crates/codegraph-generate/src/lib.rs:2176-2332`) the first domain in
topological order to claim a title via `seen_titles` owns it outright, and —
contrary to the "explicit beats discovery" reassignment comment — a title
claimed explicitly is never reassigned, so the losing domain emits nothing.
Two surprises sharpen the pin: (1) the winner is decided by an
alphabetical-sort-plus-reversal artifact of `DomainRegistry`'s Kahn
implementation (`registry.rs:47-48`, `registry.rs:162-164`), NOT by
domains.toml declaration order; (2) the cross-domain visibility gate is
effectively dead — `fk_target_undeclared_dependency`
(`crates/codegraph/src/validate.rs:143-214`) looks up `prop.ref_target` in a
title-keyed map, but JSON-schema ingest stores `ref_target` as the RAW `$ref`
string (`async_ingest.rs:1019`, e.g. `"../common/json/AddressType.json#"`)
so the lookup always misses and the check silently skips
(`validate.rs:209`). A cross-domain FK therefore generates without a declared
`depends_on`, with zero warnings. Separately, a `NamespaceNode` type already
exists with UNRELATED semantics (AT-Protocol repo namespaces,
`crates/codegraph-core/src/types/atproto.rs:4-8`), which the #267/#268
uplift must reconcile by name.

## Evidence

- `crates/codegraph/src/ingest/schema_loader.rs:320-330` —
  `extract_domain_from_path`: domain = the path segment immediately before
  `/json/`; `<version>/<domain>/json/...` handled; fallback = first segment.
  Domain identity is a pure function of file location — schema content is
  never consulted.
- `crates/codegraph/src/ingest/schema_loader.rs:53-89` — the recursive walk
  over the schemas root assigns `rel_path` then derives `domain` per file;
  `domains.toml`'s `schema_dir` is never cross-checked against it.
- `crates/codegraph/src/ingest/schema_loader.rs:142-188` — `load_files`
  (the mox `import schema` path via `SchemaFileSpec`) takes `domain` from
  the SPEC, bypassing the path derivation — the one existing seam where a
  non-dir-derived domain enters the system.
- `crates/codegraph-core/src/types/schema.rs:17` — `SchemaNode.domain:
  Option<String>` is the only scoping/ownership field; no namespace field
  exists anywhere on the schema/property graph model.
- `crates/codegraph-config/src/registry.rs:47-48` — `from_config` sorts
  domain names ALPHABETICALLY before assigning node indices;
  `registry.rs:162-164` reverses the Kahn output so dependencies sort
  first. With no `depends_on` edges the effective generation order for
  unrelated domains is DESCENDING alphabetical (`common` before `billing`).
- `crates/codegraph-generate/src/lib.rs:2228, 2281-2329` — `seen_titles`
  claim logic: first domain in `domain_order` wins; reassignment to a later
  domain happens ONLY when the first claim was graph-discovery-only
  (lib.rs:2298-2329). Two explicit `entities = ["CustomerType"]` claims →
  the second domain's claim is silently dropped.
- probe test:
  `crates/codegraph/tests/rosetta_gap_probes/namespace_probe.rs::domain_is_dir_derived_from_path`
  — fixture content contains no "billing" marker, yet artifacts are
  `src/domain/billing/customer/…` and the DDL reads
  `CREATE TABLE IF NOT EXISTS billing.customer`; no other domain appears in
  `src/domain/` (the umbrella `src/domain/mod.rs` excepted).
- probe test:
  `crates/codegraph/tests/rosetta_gap_probes/namespace_probe.rs::same_title_in_two_domains_deduped_to_first`
  — `CustomerType.json` in `common` AND `billing` (both declared
  explicitly, common first in the TOML): exactly ONE
  `_common_customer.sql` migration and ONE `src/domain/common/customer/`
  module in the whole tree; zero `_billing_customer.sql`, zero
  `src/domain/billing/customer/`. **`common` won** — by descending
  alphabetical order after the registry reversal, not by declaration order.
  Characterization bonus: the loser domain produced ZERO artifacts of any
  kind (not even domain scaffolding) — a domain that loses all its titles
  vanishes from the generated tree entirely.
- probe test:
  `crates/codegraph/tests/rosetta_gap_probes/namespace_probe.rs::versioned_layout_maps_to_domain_segment`
  — `schemas/v1/billing/json/CustomerType.json` → all artifacts under
  `src/domain/billing/…`; nothing ever lands under a `v1` domain.
- `crates/codegraph/src/ingest/async_ingest.rs:1019` — `ref_target` is
  stored as the raw `$ref` string (`Some(ref_path.to_string())`), never
  resolved to the target title (reclassification at
  `async_ingest.rs:1233-1289` resolves titles only to decide
  entity_reference vs value_object, and does not rewrite `ref_target`).
- `crates/codegraph/src/validate.rs:143-214` — `check_fk_targets` keys its
  entity→domain map by TITLE and looks up `prop.ref_target` verbatim
  (validate.rs:192); a path-style ref never matches, so the
  `fk_target_undeclared_dependency` issue is unreachable from the
  JSON-schema pipeline (silently skipped at validate.rs:209).
- `crates/codegraph/src/driver.rs:506, 1043-1068` — `run_validation` runs
  before generation and hard-fails on Error issues — so IF the check could
  fire, an undeclared cross-domain FK would abort the run; the probe shows
  it cannot fire.
- probe test:
  `crates/codegraph/tests/rosetta_gap_probes/namespace_probe.rs::undeclared_cross_domain_fk_is_not_flagged_and_still_generates_fk`
  — billing `CustomerType` → `$ref "../common/json/AddressType.json#"` with
  NO `depends_on`: `driver::run` succeeds and the billing customer DDL
  carries `REFERENCES common.address(id)` anyway. Zero warnings.
- probe test:
  `crates/codegraph/tests/rosetta_gap_probes/namespace_probe.rs::cross_domain_reference_generates_fk`
  — same model WITH `depends_on = ["common"]`: identical artifacts (the
  declaration changes nothing in output), and no generated "DomainDepends"
  artifact exists — the dependency edge lives only in domains.toml and in
  migration ordering (the sole `depends_on` string in the tree is
  incidental docker-compose service wiring in `e2e-tests/`).
- `crates/codegraph-core/src/types/atproto.rs:4-8` — the naming collision:
  `pub struct NamespaceNode { authority, segment, domain }` — AT-Protocol
  repo namespaces, projected into the graph by
  `crates/codegraph/src/ingest/atproto_projection.rs` (populated when
  `project.has_atproto`, driver.rs:474-488) and linked via
  `EdgeType::InNamespace` (`crates/codegraph-core/src/types/edge.rs`). A
  #268 `namespace a.b` node type cannot reuse this name without a migration
  or alias story.

### Harness note (no support.rs change made)

`support::add_domain_entry` appends a full `domains_toml(...)` body —
including a second `[defaults]` header — which TOML rejects as a duplicate
key when appended to a `write_schema_project` config, so it is currently
unusable for two-domain probes. The probes write their two-domain TOML
locally instead; `support.rs` was not modified (WP rules).

## Disposition recommendation

Today's model, for the disposition table: **dir-derived domain ONLY** (path
segment before `/json/`; mox imports may override via `SchemaFileSpec`),
**title dedup as the ownership rule** (topological-order first claim wins;
winner decided by alphabetical-sort + Kahn reversal, not declaration
order; loser domain emits nothing), and **no namespace visibility axis**
(cross-domain `$ref` visibility is unenforced — `depends_on` affects
nothing observable in generation output).

Per construct:

- **`namespace a.b` declarations** — **defer to the #267/#268 uplift
  model**; not representable today and NOT derivable from existing graph
  fields (there is no namespace field on any schema-side node). Do not map
  namespaces onto domains in the bridge: the disposition table's uplift
  model (namespace = first-class visibility/scoping axis; domain =
  ownership/boundary axis, unchanged) is the correct target, and the
  dir-derived domain must keep winning for boundary decisions
  (`src/domain/<domain>/`, pg schema, RLS tenancy) regardless of what the
  Rune source namespaces say.
- **`import` declarations** — same disposition: defer to #267/#268. When
  the bridge lands, it should emit `NamespaceNode`-style nodes + visibility
  edges per #268 (and per the #1.4 precedent, reusing the existing
  cross-domain `$ref` machinery for the reference edges — the pipeline
  already wires cross-schema FKs correctly; what is missing is the
  visibility CONTRACT, not the wire).
- **Flag for #267 (must-do):** reconcile the `NamespaceNode` name
  collision with `crates/codegraph-core/src/types/atproto.rs:4-8` (AT-Proto
  repo namespaces, `EdgeType::InNamespace`). Either rename the new type
  (e.g. a distinct Rosetta/Rune namespace node) or migrate the atproto one;
  reusing the name with different fields/semantics will silently break
  serde round-trips and the atproto projection.
- **Opportunistic defect (out of WP scope, worth filing):** the dead
  `fk_target_undeclared_dependency` check (validate.rs:192 raw-`$ref`
  lookup). Fixing it — resolve `ref_target` to the title, or stem-match as
  `atproto_projection.rs:157-158` does — would make `depends_on` actually
  enforced today and give the #267/#268 visibility model a working
  enforcement point to build on.

### Enablers

None — this WP is test + findings only. No production code, templates,
Cargo.toml, or support.rs changes were made; two-domain fixtures write
their domains.toml locally inside the probe file.
