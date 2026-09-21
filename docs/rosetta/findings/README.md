# Rosetta gap-analysis findings (issue #254)

One file per probe work package. These notes are the raw evidence the
disposition table (`docs/rosetta-gap-analysis.md`) is synthesized from; the
directory stays after #254 closes as the citable evidence trail.

## Format

```markdown
# WP1.x — <question>

## Verdict
<one paragraph>

## Evidence
- <file:line> — <what it shows>
- probe test: `crates/codegraph/tests/rosetta_gap_probes/<wp>_probe.rs::<test_name>`

## Disposition recommendation
<per construct: disposition value + rationale>
```

## Rules

- Evidence must be observable behavior (generated output, graph query results,
  sigil API behavior) with `file:line` references — never recollection.
- If a probe test fails against expectations, the surprise **is** the finding:
  record it, do not force the test green.
- Tiny enablers (a minimal accessor/field exposed purely so a probe can
  demonstrate behavior) must be flagged in a dedicated `## Enablers` section.
  They change no generator output.
- Sigil pin: `yestechgroup/sigil@49a6a27f589fa89cbb20641504923c911700ee67`
  (test-only dev-dependency; production wiring is #255).

## Files

| WP | File | Question |
|----|------|----------|
| 1.1 | `wp1.1-inheritance.md` | Does generation need an inheritance merge, or does the ingest-time flatten cover Rune `extends`? |
| 1.2 | `wp1.2-cardinality.md` | Is `min>1` collection cardinality representable? |
| 1.3 | `wp1.3-choice.md` | How are choice / one-of constructs represented today? |
| 1.4 | `wp1.4-meta-annotations.md` | Where do `[metadata]` / `[ruleReference]` / `[docReference]` land? |
| 1.5 | `wp1.5-namespaces.md` | Namespaces vs dir-derived domains — what does the table assume (#267/#268)? |
| 1.6 | `wp1.6-expr-json-typing.md` | `Expr::to_json` round-trip stability + honest expression-typing scope |
| 1.7 | `wp1.7-data-plane.md` | Does the draft "existing graph" Data-plane disposition hold row by row? |
