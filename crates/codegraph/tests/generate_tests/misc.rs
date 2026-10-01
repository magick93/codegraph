//! Stragglers nearest cohesive: GenerationReport summaries and the
//! template-engine load check.

use codegraph::generate;
use std::path::Path;

// === GenerationReport Tests ===

#[test]
fn generation_report_summary_with_errors() {
    use codegraph::error::Error;
    use codegraph::generate::report::{GenerationError, GenerationReport, GenerationWarning};

    let mut report = GenerationReport::new();
    report.errors.push(GenerationError {
        entity: "CandidateType".into(),
        generator: "ddl".into(),
        source: Error::SchemaNotFound("missing".into()),
    });
    report.warnings.push(GenerationWarning {
        entity: "OrderType".into(),
        generator: "codelist".into(),
        check: "empty_codelist",
        message: "no enum values".into(),
    });

    assert!(report.has_errors());
    let summary = report.summary();
    assert!(summary.contains("1 error"));
    assert!(summary.contains("1 warning"));
    assert!(summary.contains("CandidateType"));
}

#[test]
fn generation_report_summary_clean() {
    use codegraph::generate::report::GenerationReport;

    let report = GenerationReport::new();
    assert!(!report.has_errors());
    let summary = report.summary();
    assert!(summary.contains("0 errors"));
}

// === Template Engine Tests ===

#[test]
fn test_template_engine_loads_all_templates() {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    // Should have loaded templates from all subdirectories
    let names: Vec<&str> = tera.get_template_names().collect();
    assert!(
        names.iter().any(|n| n.starts_with("db/")),
        "Should have db/ templates"
    );
    assert!(
        names.iter().any(|n| n.starts_with("ddd/")),
        "Should have ddd/ templates"
    );
    assert!(
        names.iter().any(|n| n.starts_with("api/")),
        "Should have api/ templates"
    );
    assert!(
        names.iter().any(|n| n.starts_with("scaffold/")),
        "Should have scaffold/ templates"
    );
}
