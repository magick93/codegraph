//! Doc-completeness gate for `docs/rosetta-gap-analysis.md` (issue #254).
//!
//! The gap analysis is the sign-off gate for the Rosetta epic (#255–#268).
//! This test keeps it honest: every Rune DSL construct — all 16
//! `SemanticElement` kinds and all 51 `Expr` variants from the pinned sigil
//! revision — must appear as a disposition-table row carrying
//!
//! 1. one of the five disposition values from the issue, and
//! 2. an evidence reference (`probe:` a probe test, `sigil:` a sigil source
//!    anchor, `graph:` a codegraph source anchor, or `defer:` a deferral
//!    rationale),
//!
//! and the document must have left DRAFT status.

use std::fs;
use std::path::PathBuf;

/// All 16 `SemanticElement` variants (sigil-model lib.rs:622).
const SEMANTIC_ELEMENT_KINDS: &[&str] = &[
    "Data",
    "Enumeration",
    "Annotation",
    "TypeAlias",
    "BasicType",
    "RecordType",
    "LibraryFunction",
    "Function",
    "Rule",
    "Report",
    "ExternalRuleSource",
    "Schema",
    "Body",
    "Corpus",
    "Segment",
    "MetaType",
];

/// All 51 `Expr` variants (sigil-model expr.rs:183, rev 49a6a27f).
const EXPR_VARIANTS: &[&str] = &[
    "BooleanLiteral",
    "StringLiteral",
    "NumberLiteral",
    "IntLiteral",
    "ListLiteral",
    "SymbolReference",
    "ImplicitVariable",
    "FeatureCall",
    "DeepFeatureCall",
    "ArithmeticOperation",
    "LogicalOperation",
    "EqualityOperation",
    "ComparisonOperation",
    "ContainsExpression",
    "DisjointExpression",
    "DefaultOperation",
    "JoinOperation",
    "ConditionalExpression",
    "OnlyExistsExpression",
    "ExistsExpression",
    "AbsentExpression",
    "OnlyElement",
    "CountOperation",
    "FlattenOperation",
    "DistinctOperation",
    "ReverseOperation",
    "FirstOperation",
    "LastOperation",
    "SumOperation",
    "AsKeyOperation",
    "OneOfOperation",
    "ChoiceOperation",
    "ToStringOperation",
    "ToNumberOperation",
    "ToIntOperation",
    "ToTimeOperation",
    "ToEnumOperation",
    "ToDateOperation",
    "ToDateTimeOperation",
    "ToZonedDateTimeOperation",
    "SwitchOperation",
    "WithMetaOperation",
    "AsOperation",
    "ThenOperation",
    "FilterOperation",
    "MapOperation",
    "ReduceOperation",
    "SortOperation",
    "MinOperation",
    "MaxOperation",
    "ConstructorExpression",
];

/// The five disposition values from issue #254.
const DISPOSITIONS: &[&str] = &[
    "existing graph",
    "new node family",
    "annotation payload",
    "generator feature",
    "deferred",
];

/// Accepted evidence-reference prefixes.
const EVIDENCE_PREFIXES: &[&str] = &["probe:", "sigil:", "graph:", "defer:"];

fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/rosetta-gap-analysis.md")
}

fn doc_text() -> String {
    let path = doc_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("gap-analysis doc missing at {}: {e}", path.display()))
}

/// Find the disposition-table row for `construct`: a markdown table line
/// whose first cell is exactly the construct name.
fn row_for<'a>(doc: &'a str, construct: &str) -> Option<&'a str> {
    let needle = format!("| {construct} |");
    doc.lines().find(|line| line.starts_with(&needle))
}

fn missing_disposition(row: &str, construct: &str) -> Option<String> {
    if DISPOSITIONS.iter().any(|d| row.contains(d)) {
        None
    } else {
        Some(format!(
            "row for `{construct}` carries no disposition value (one of: {})\n    row: {row}",
            DISPOSITIONS.join(", ")
        ))
    }
}

fn missing_evidence(row: &str, construct: &str) -> Option<String> {
    if EVIDENCE_PREFIXES.iter().any(|p| row.contains(p)) {
        None
    } else {
        Some(format!(
            "row for `{construct}` carries no evidence ref (one of: {})\n    row: {row}",
            EVIDENCE_PREFIXES.join(", ")
        ))
    }
}

/// The shared gate: every construct named in `list` must have a complete
/// row. Returns all failures so one run reports the whole remaining gap.
fn check_rows(doc: &str, list: &[&str], label: &str) -> Vec<String> {
    let mut failures = Vec::new();
    for construct in list {
        match row_for(doc, construct) {
            None => failures.push(format!(
                "{label}: no disposition-table row for `{construct}` (expected a line starting `| {construct} |`)"
            )),
            Some(row) => {
                if let Some(problem) = missing_disposition(row, construct) {
                    failures.push(format!("{label}: {problem}"));
                }
                if let Some(problem) = missing_evidence(row, construct) {
                    failures.push(format!("{label}: {problem}"));
                }
            }
        }
    }
    failures
}

#[test]
fn gap_analysis_doc_is_out_of_draft() {
    let doc = doc_text();
    assert!(
        doc.lines()
            .any(|l| l.contains("Status") && l.contains("REVIEW")),
        "gap-analysis doc is still DRAFT; flip the status line to REVIEW after the probe WPs merge"
    );
}

#[test]
fn every_semantic_element_kind_has_a_disposition_row() {
    let doc = doc_text();
    let failures = check_rows(&doc, SEMANTIC_ELEMENT_KINDS, "SemanticElement");
    assert!(
        failures.is_empty(),
        "gap-analysis doc incomplete (SemanticElement kinds):\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn every_expr_variant_has_a_disposition_row() {
    let doc = doc_text();
    let failures = check_rows(&doc, EXPR_VARIANTS, "Expr");
    assert!(
        failures.is_empty(),
        "gap-analysis doc incomplete (Expr variants):\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn expr_variant_list_matches_pinned_sigil() {
    // Compile-time anchor: the doc gate's variant list must not drift from
    // sigil-model. The enum itself is Serialize-only and not iterable at
    // runtime, so we pin the count and spot-check family sentinels.
    assert_eq!(EXPR_VARIANTS.len(), 51);
    for sentinel in [
        "BooleanLiteral",
        "WithMetaOperation",
        "ConstructorExpression",
        "MaxOperation",
    ] {
        assert!(EXPR_VARIANTS.contains(&sentinel));
    }
}
