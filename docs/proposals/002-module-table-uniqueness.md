# Proposal: module/table name uniqueness (compile-time)

Status: draft (codegraph lifecycle hardening phase 2, Nov 2026)
Target: rexlang (rex-driver ddd/graph integrity; mox compiler naming)

## Problem

Two classes/enums in one package whose names strip (Type-suffix + snake_case)
to the same module — e.g. `PayRun` and `PayRunType` — generate the same module
path and pg table. Downstream, codegraph silently overwrote one entity with
the other (a 229-file, 0-error generation that dropped an aggregate), and the
colliding FK constraint then surfaced as a confusing duplicate-FK error far
from the cause (`fk_deployment_job_id` class). codegraph now hard-errors at
generation time (entity-producing titles stripping to the same module or
pg_table_name, naming both titles), but the compiler is the right place: the
ambiguity exists in the model, before any generator runs.

## Proposed rule

New integrity rule (suggest **Rule 17 — stripped-name uniqueness**):

- compile error when two classes/enums in one package strip to the same
  module identifier (Type-suffix-stripped snake_case), or map to the same
  default table name;
- the error names BOTH declarations with spans;
- value objects are exempt: an entity + a contained value object sharing a
  stripped name is legal (the VO projects as `{parent}_{feature}` child
  tables) — the rule applies only when BOTH declarations are entity-bearing
  (or both are enums landing in the same codelist module).

## Proposed escape hatch

```
class PayRunType table "pay_run_type" { ... }
class PayRun   table "pay_run_head" { ... }   // or rename the class
```

A `table "..."` annotation (already the spelling used by codegraph's config
layer) overrides the derived table name; module uniqueness still applies
unless a companion `module "..."` annotation is also provided. Keep the
annotation pair minimal: `table` alone resolves table collisions; add
`module` only if real models need same-table/different-module (not known to
exist).

## Acceptance

- Compile error naming both declarations for `PayRun` + `PayRunType`
  (both entities) in one package.
- Entity + contained VO sharing the stripped name compiles.
- `table "..."` disambiguation compiles and flows through rex-ir
  (`ClassDef.table_name: Option<String>`, serde-defaulted) so consumers stop
  re-deriving table names.
- Conformance fixtures for all three shapes.

## Consumer impact

- Models that today silently collide fail compilation with a precise message
  — strictly better than the downstream duplicate-FK surprise or the silent
  overwrite.
- The `table_name` IR field lets codegraph delete its own stripped-name
  table derivation over time (config layer keeps precedence).
- codegraph's generation-time hard error stays as defense-in-depth (it also
  covers non-rex sources: JSON Schema `$id`s).
