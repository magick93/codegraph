use crate::fixtures::{mock_schema, test_domain_config};
use codegraph::generate;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::SchemaNode;

// === Generation Ordering Tests ===

#[tokio::test]
async fn test_generation_ordering_with_empty_graph() {
    let mock = MockEngine::new();
    let config = test_domain_config();
    let order = generate::compute_generation_order(&mock, &config)
        .await
        .unwrap();
    assert!(order.is_empty());
}

#[tokio::test]
async fn test_generation_ordering_respects_domain_order() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_schema(mock_schema(
            "common/json/NameType.json",
            "NameType",
            "name",
            "common",
            "entity_reference",
        ))
        .build();

    let config = test_domain_config();
    let order = generate::compute_generation_order(&mock, &config)
        .await
        .unwrap();

    assert_eq!(order.len(), 2);
    // Common should come before recruiting
    assert_eq!(order[0].domain, "common");
    assert_eq!(order[1].domain, "recruiting");
}

/// Issue #64: a title graph-discovered in a higher-priority domain (cross-domain
/// allOf reference) but EXPLICITLY configured in a lower-priority domain must be
/// assigned to the configured domain. The old global `seen_titles` dedup let the
/// first domain claim it, so per-domain generators (openapi domain files, CLI,
/// links) silently lost the entity for its real domain.
#[tokio::test]
async fn test_generation_ordering_configured_domain_wins_over_discovery() {
    // PositionType physically lives in "common" (cross-domain allOf extension
    // schema), so it is graph-discovered by common. Screening explicitly
    // configures it via entities = ["PositionType"].
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "common/json/PositionType.json",
            "PositionType",
            "position",
            "common",
            "entity_reference",
        ))
        .build();

    let config_str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []

[domains.screening]
label = "Screening"
schema_dir = "screening"
postgres_schema = "screening"
depends_on = ["common"]
entities = ["PositionType"]
"#;
    let config = codegraph_config::config::parse_domain_config_str(config_str).unwrap();
    let order = generate::compute_generation_order(&mock, &config)
        .await
        .unwrap();

    // The entity must appear exactly once, assigned to its configured domain.
    assert_eq!(
        order.len(),
        1,
        "PositionType should appear exactly once. Got: {order:?}"
    );
    assert_eq!(
        order[0].domain, "screening",
        "configured domain must win over graph discovery. Got: {order:?}"
    );
    assert_eq!(order[0].schema_title, "PositionType");
}

/// Issue #64 companion: when NO domain explicitly configures a cross-domain
/// title, the first (highest-priority) domain claiming it keeps it — entity
/// generators must still run it exactly once.
#[tokio::test]
async fn test_generation_ordering_undiscovered_title_stays_in_first_domain() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "common/json/PositionType.json",
            "PositionType",
            "position",
            "common",
            "entity_reference",
        ))
        .build();

    let config_str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []

[domains.screening]
label = "Screening"
schema_dir = "screening"
postgres_schema = "screening"
depends_on = ["common"]
entities = []
"#;
    let config = codegraph_config::config::parse_domain_config_str(config_str).unwrap();
    let order = generate::compute_generation_order(&mock, &config)
        .await
        .unwrap();

    assert_eq!(
        order.len(),
        1,
        "PositionType should appear exactly once. Got: {order:?}"
    );
    assert_eq!(
        order[0].domain, "common",
        "first-discovering domain keeps the title when nothing configures it. Got: {order:?}"
    );
}

#[tokio::test]
async fn test_generation_ordering_excludes_inline_def_schemas() {
    // Build an inline-def schema (parent_schema is set — like #/definitions/AssessmentScoreType)
    let inline_schema = SchemaNode {
        namespace: None,
        schema_id: "assessments/json/ReportType.json#/definitions/AssessmentScoreType".into(),
        title: "AssessmentScoreType".into(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("assessments".into()),
        rel_path: "assessments/json/ReportType.json#/definitions/AssessmentScoreType".into(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "AssessmentScoreType".into(),
        pg_table_name: "assessment_score".into(),
        api_path_segment: "assessment-score".into(),
        parent_schema: Some("ReportType".into()), // marks this as an inline/local definition
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    // Also add a top-level schema for the same domain to ensure the domain
    // itself is not excluded (only the inline def should be filtered).
    let top_schema = mock_schema(
        "assessments/json/ReportType.json",
        "ReportType",
        "report",
        "assessments",
        "entity_reference",
    );

    let config_str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.assessments]
label = "Assessments"
schema_dir = "assessments"
postgres_schema = "assessments"
entities = ["ReportType", "AssessmentScoreType"]
"#;
    let config = codegraph_config::config::parse_domain_config_str(config_str).unwrap();

    let mock = MockEngine::builder()
        .with_schema(inline_schema)
        .with_schema(top_schema)
        .build();

    let order = generate::compute_generation_order(&mock, &config)
        .await
        .unwrap();

    // Only the top-level schema should be in the order — the inline def must be excluded
    assert_eq!(
        order.len(),
        1,
        "inline def schema should be excluded from generation order"
    );
    assert_eq!(order[0].schema_title, "ReportType");
    assert!(
        !order
            .iter()
            .any(|e| e.schema_title == "AssessmentScoreType"),
        "AssessmentScoreType (inline def) must not appear in generation order"
    );
}
