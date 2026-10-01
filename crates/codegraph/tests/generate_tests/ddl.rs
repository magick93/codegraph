use crate::fixtures::{
    mock_composition_tree, mock_properties, mock_schema, test_domain_config, test_project_config,
};
use codegraph::generate;
use codegraph::generate::traits::EntityGenerator;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{ColumnInfo, FkTarget, PropertyNode};
use codegraph_type_contracts::RefClassificationKind;
use std::path::Path;

// === DDL Generator Tests ===

#[tokio::test]
async fn test_ddl_generator_produces_table_sql() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties("CandidateType", mock_properties())
        .with_composition_tree(
            "CandidateType",
            mock_composition_tree("CandidateType", "candidate", "recruiting"),
        )
        .build();

    let config = test_domain_config();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-ddl");
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "DDL generator should produce files");

    let table_file = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("recruiting_candidate.sql")
        })
        .expect("Should have a table SQL file");

    assert!(
        table_file
            .content
            .contains("CREATE TABLE IF NOT EXISTS recruiting.candidate"),
        "Should contain CREATE TABLE"
    );
    assert!(
        table_file.content.contains("given_name TEXT NOT NULL"),
        "Should contain given_name column"
    );
    assert!(
        table_file.content.contains("family_name TEXT"),
        "Should contain family_name column"
    );
}

/// Issue #311: a workflow entity's status column carries the configured
/// initial_state as its DDL DEFAULT, so API creates (whose INSERTs omit the
/// workflow-managed column) materialize the initial state and list badges
/// work on created rows. Pins the nullable-column shape — the gate fixture's
/// `status` is a non-required codelist ref, which the pre-fix nullability
/// guard silently skipped.
#[tokio::test]
async fn test_ddl_generator_defaults_workflow_status_column_to_initial_state() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "refunds/json/RefundRequestType.json",
            "RefundRequestType",
            "refund_request",
            "refunds",
            "entity_reference",
        ))
        .with_properties("RefundRequestType", mock_properties())
        .with_composition_tree("RefundRequestType", {
            let mut tree = mock_composition_tree("RefundRequestType", "refund_request", "refunds");
            tree.root.columns = vec![ColumnInfo {
                name: "status".to_string(),
                description: Some("Workflow status of the request".to_string()),
                rust_type: "String".to_string(),
                postgres_type: "TEXT".to_string(),
                is_optional: true,
                is_codelist_fk: false,
                composite_columns: vec![],
                is_array: false,
                classification: Some(RefClassificationKind::CodelistReference),
                fk_target: Some(FkTarget {
                    schema: "common".to_string(),
                    table: "refund_status_code_list".to_string(),
                    column: "code".to_string(),
                    on_delete: "RESTRICT".to_string(),
                }),
                check_values: vec![],
            }];
            tree
        })
        .build();

    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[domains.refunds]
label = "Refunds"
schema_dir = "refunds"
postgres_schema = "refunds"
entities = ["RefundRequestType"]

[domains.refunds.entity_config.RefundRequestType.workflow]
status_field = "status"
states = ["draft", "submitted", "approved", "rejected"]
initial_state = "draft"
terminal_states = ["approved", "rejected"]
generate_action_endpoints = true
"#,
    )
    .unwrap();

    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-ddl-workflow");
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "RefundRequestType",
            "refunds",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let table_file = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("refunds_refund_request.sql")
        })
        .expect("Should have a table SQL file");

    assert!(
        table_file.content.contains("status TEXT DEFAULT 'draft'"),
        "the nullable status column must DEFAULT to the workflow initial_state:\n{}",
        table_file.content
    );
    assert!(
        !table_file.content.contains("status TEXT NOT NULL"),
        "the default must not change the column's nullability:\n{}",
        table_file.content
    );
}

#[tokio::test]
async fn generate_policy_driven_ddl_with_soft_delete() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "test/json/TestEntityType.json",
            "TestEntityType",
            "test_entity",
            "test",
            "entity",
        ))
        .with_properties(
            "TestEntityType",
            vec![PropertyNode {
                name: "name".to_string(),
                prop_type: "string".to_string(),
                description: Some("The entity name".to_string()),
                format: None,
                is_required: true,
                is_nullable: false,
                is_array: false,
                min_items: None,
                max_items: None,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pg_column_name: "name".to_string(),
                pg_column_type: "TEXT".to_string(),
                rust_field_name: "name".to_string(),
                rust_field_type: "String".to_string(),
                sea_orm_type: "Text".to_string(),
                render_strategy: "direct_column".to_string(),
                ref_target: None,
                classification: Some("primitive_wrapper".to_string()),
                projection: None,
                classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
                type_expr: None,
            }],
        )
        .build();

    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.test]
label = "Test"
schema_dir = "test"
postgres_schema = "test"
entities = ["TestEntityType"]
auditable = true
"#,
    )
    .unwrap();

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-policy-ddl");
    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "TestEntityType",
            "test",
            &config,
            &tera,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "DDL generator should produce files");

    let table_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("test_test_entity.sql"))
        .expect("Should have a table SQL file");

    let content = &table_file.content;

    assert!(
        content.contains("CREATE TABLE IF NOT EXISTS test.test_entity"),
        "Should contain CREATE TABLE statement"
    );
    assert!(
        content.contains("\"name\" TEXT NOT NULL"),
        "Should contain quoted name column (PG reserved word)"
    );
    assert!(
        content.contains("deleted_at TIMESTAMPTZ NULL"),
        "Should contain deleted_at audit column"
    );
    assert!(
        content.contains("created_at TIMESTAMPTZ NOT NULL"),
        "Should contain created_at column"
    );
    assert!(
        content.contains("id UUID NOT NULL"),
        "Should contain primary key id column"
    );
    assert!(
        content.contains("PRIMARY KEY"),
        "Should contain PRIMARY KEY constraint"
    );
}
