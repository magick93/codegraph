# Flywheel corpus fixture

The real Flywheel consumer model, promoted into the committed fixture corpus
per issue #352 (deliverable E1). The `.rosetta` files were recreated from the
Flywheel Supabase (Postgres) schema by the flywheel consumer and are copied
**verbatim** — do not edit them here; fix upstream and re-promote.

## Contents

- `model/*.rosetta` — 11 files, ~1444 lines, ~156 top-level constructs across
  the `flywheel.<domain>` namespaces (`common`, `reference`, `profiles`,
  `accounts`, `company`, `taxonomy`, `qanda`, `jobs`, `marketplace`, `inbox`,
  `pricing`).
- `domains.toml` — the consumer's domain config. `force_entities` /
  `force_value_objects` pin the postgres table set 1:1 with the original
  schema (rosetta types are otherwise auto-scored).
- `profiles.toml` — the consumer's generation profile (`rosetta_backend =
  true`, which gates the `condition_validations` / `functions` / `rules` /
  `regulatory_reports` generator families).

## Construct shapes exercised

This corpus is the standard heavy rosetta fixture because it packs the
construct shapes that caught five real generator bugs in one consumer session
(commit `c0cab515`):

- cross-namespace **wildcard imports** (`import flywheel.common.*` etc., with
  `depends_on` chains in `domains.toml` satisfying #267 validation),
- **self-referencing FKs** (`jobs.Comment.parentComment`),
- **choices** (`jobs.ProposedBudget`, `jobs.BusinessServiceProvider` — kept
  unstripped and separately addressable),
- **named conditions** (`jobs.ApplicantIdentified`,
  `company.ExactlyOneAuthor`, `pricing.MinBelowMax`, … — one `validations.rs`
  per affected domain via `condition_validations`),
- **~30 enums** (all in `common`, incl. 20-value `PricingModelTypeEnum` and
  `displayName`-renamed codes),
- custom **typeAliases** (`Uuid`, `Money`, `Slug`, `Tsid`, …),
- **number aliases with `digits` / `fractionalDigits`** (postgres
  `numeric(p,s)` mappings).

Note: the corpus intentionally models **no `func` elements** (rosetta
function codegen has its own fixture at `tests/fixtures/rosetta_bridge/`).

## Running the tests

```bash
# Always-on smoke (single file, plan-less — seconds):
cargo test -p codegraph --test flywheel_corpus_tests

# Heavy gates (all 11 files; debug-mode classify over ~156 constructs):
cargo test -p codegraph --test flywheel_corpus_tests -- --ignored --nocapture
```

The heavy gates are wired into the consumer-e2e CI job by #353; the
wall-clock of `flywheel_corpus_generates_full_artifact_set` is the #350 (D5
perf) baseline.
