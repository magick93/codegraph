//! Level 2: Insta snapshot tests for include DTO generator output.
//! Run with: cargo test -p codegraph --test include_snapshot_tests

use std::path::Path;

use codegraph::generate;
use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};
use codegraph_type_contracts::RefClassificationKind;

fn worker_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "hr/json/WorkerType.json".into(),
        title: "WorkerType".into(),
        description: Some("A worker".into()),
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("hr".into()),
        rel_path: "hr/json/WorkerType.json".into(),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Worker".into(),
        pg_table_name: "worker".into(),
        api_path_segment: "workers".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn person_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "hr/json/PersonType.json".into(),
        title: "PersonType".into(),
        description: Some("A person".into()),
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("hr".into()),
        rel_path: "hr/json/PersonType.json".into(),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Person".into(),
        pg_table_name: "person".into(),
        api_path_segment: "persons".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn worker_properties_with_person_ref() -> Vec<PropertyNode> {
    vec![PropertyNode {
        name: "person".into(),
        prop_type: "object".into(),
        description: Some("FK to person".into()),
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "person_id".into(),
        pg_column_type: "UUID".into(),
        rust_field_name: "person".into(),
        rust_field_type: "Option<Uuid>".into(),
        sea_orm_type: "Uuid".into(),
        render_strategy: "entity_reference".into(),
        ref_target: Some("PersonType".into()),
        classification: Some("entity_reference".into()),
        projection: None,
        classification_kind: Some(RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }]
}

fn setup_include_mock() -> MockEngine {
    MockEngine::builder()
        .with_schema(worker_schema())
        .with_schema(person_schema())
        .with_properties("WorkerType", worker_properties_with_person_ref())
        .build()
}

fn include_domain_config() -> codegraph_config::DomainConfig {
    let toml_str = r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
allow_include = ["person"]
operations = ["create", "read", "update", "list"]
"#;
    codegraph_config::config::parse_domain_config_str(toml_str).unwrap()
}

fn test_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    generate::template_engine::create_tera(&template_dir).unwrap()
}

#[test]
fn snapshot_dto_included_single_level() {
    let engine = setup_include_mock();
    let config = include_domain_config();
    let tera = test_tera();
    let project = ProjectConfig::default();
    let output_dir = tempfile::TempDir::new().unwrap();

    let gen = generate::ddd::dto::DtoGenerator::new(output_dir.path());
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "WorkerType", "hr", &config, &tera, &project))
        .expect("DtoGenerator failed");

    let included_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"))
        .expect("Should have a dto_included.rs file");

    insta::assert_snapshot!("dto_included_worker", &included_file.content);
}

fn dot_notation_schema_for(title: &str, table: &str, rust_type_name: &str) -> SchemaNode {
    SchemaNode {
        schema_id: format!("hr/json/{title}.json"),
        title: title.into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("hr".into()),
        rel_path: format!("hr/json/{title}.json"),
        pg_type: "UUID".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: rust_type_name.into(),
        pg_table_name: table.into(),
        api_path_segment: table.replace('_', "-"),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn setup_dot_notation_mock() -> MockEngine {
    let deployment_schema = dot_notation_schema_for("DeploymentType", "deployment", "Deployment");
    let position_schema = dot_notation_schema_for("PositionType", "position", "Position");

    MockEngine::builder()
        .with_schema(dot_notation_schema_for("WorkerType", "worker", "Worker"))
        .with_schema(deployment_schema)
        .with_schema(position_schema)
        .with_properties(
            "WorkerType",
            vec![PropertyNode {
                name: "deployment".into(),
                prop_type: "object".into(),
                description: Some("Deployments".into()),
                format: None,
                is_required: false,
                is_nullable: true,
                is_array: true,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pg_column_name: "deployment".into(),
                pg_column_type: "UUID".into(),
                rust_field_name: "deployment".into(),
                rust_field_type: "Option<Vec<Uuid>>".into(),
                sea_orm_type: "Uuid".into(),
                render_strategy: "entity_reference".into(),
                ref_target: Some("DeploymentType".into()),
                classification: Some("entity_reference".into()),
                projection: None,
                classification_kind: Some(RefClassificationKind::EntityReference),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
            }],
        )
        .with_properties(
            "DeploymentType",
            vec![
                PropertyNode {
                    name: "assignment_reason_code".into(),
                    prop_type: "string".into(),
                    description: Some("Assignment reason code".into()),
                    format: None,
                    is_required: false,
                    is_nullable: true,
                    is_array: false,
                    pattern: None,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pg_column_name: "assignment_reason_code".into(),
                    pg_column_type: "TEXT".into(),
                    rust_field_name: "assignment_reason_code".into(),
                    rust_field_type: "Option<String>".into(),
                    sea_orm_type: "Text".into(),
                    render_strategy: "codelist_reference".into(),
                    ref_target: Some("AssignmentReasonCodeList".into()),
                    classification: Some("codelist_reference".into()),
                    projection: None,
                    classification_kind: Some(RefClassificationKind::CodelistReference),
                    ui_override_detail: None,
                    ui_override_list_cell: None,
                    ui_override_form: None,
                    ui_override_inline: None,
                },
                PropertyNode {
                    name: "full_time_equivalent_ratio".into(),
                    prop_type: "number".into(),
                    description: Some("FTE ratio".into()),
                    format: None,
                    is_required: false,
                    is_nullable: true,
                    is_array: false,
                    pattern: None,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pg_column_name: "full_time_equivalent_ratio".into(),
                    pg_column_type: "NUMERIC".into(),
                    rust_field_name: "full_time_equivalent_ratio".into(),
                    rust_field_type: "Option<rust_decimal::Decimal>".into(),
                    sea_orm_type: "Decimal".into(),
                    render_strategy: "direct_column".into(),
                    ref_target: None,
                    classification: None,
                    projection: None,
                    classification_kind: None,
                    ui_override_detail: None,
                    ui_override_list_cell: None,
                    ui_override_form: None,
                    ui_override_inline: None,
                },
                PropertyNode {
                    name: "position".into(),
                    prop_type: "object".into(),
                    description: Some("FK to position".into()),
                    format: None,
                    is_required: false,
                    is_nullable: true,
                    is_array: false,
                    pattern: None,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pg_column_name: "position_id".into(),
                    pg_column_type: "UUID".into(),
                    rust_field_name: "position".into(),
                    rust_field_type: "Option<Uuid>".into(),
                    sea_orm_type: "Uuid".into(),
                    render_strategy: "entity_reference".into(),
                    ref_target: Some("PositionType".into()),
                    classification: Some("entity_reference".into()),
                    projection: None,
                    classification_kind: Some(RefClassificationKind::EntityReference),
                    ui_override_detail: None,
                    ui_override_list_cell: None,
                    ui_override_form: None,
                    ui_override_inline: None,
                },
            ],
        )
        .build()
}

fn dot_notation_domain_config() -> codegraph_config::DomainConfig {
    let toml_str = r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "DeploymentType", "PositionType"]

[domains.hr.entity_config.WorkerType]
allow_include = ["deployment.position"]
operations = ["create", "read", "update", "list"]
"#;
    codegraph_config::config::parse_domain_config_str(toml_str).unwrap()
}

#[test]
fn snapshot_dto_included_dot_notation() {
    let engine = setup_dot_notation_mock();
    let config = dot_notation_domain_config();
    let tera = test_tera();
    let project = ProjectConfig::default();
    let output_dir = tempfile::TempDir::new().unwrap();

    let gen = generate::ddd::dto::DtoGenerator::new(output_dir.path());
    let files = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(gen.generate(&engine, "WorkerType", "hr", &config, &tera, &project))
        .expect("DtoGenerator failed");

    let included_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"))
        .expect("Should have a dto_included.rs file");

    let content = &included_file.content;

    assert!(
        content.contains("struct DeploymentWithPositionResponse"),
        "should contain enriched struct for deployment.position"
    );
    assert!(
        content.contains("assignment_reason_code:"),
        "should contain intermediate entity scalar field"
    );
    assert!(
        content.contains("full_time_equivalent_ratio:"),
        "should contain intermediate entity numeric field"
    );
    assert!(
        content.contains("position: Option<PositionResponse>"),
        "should contain nested leaf include field"
    );
    assert!(
        content.contains("deployment_position"),
        "should use deployment_position alias for the dot-notation path"
    );
    assert!(
        content.contains("deployment.position"),
        "should have serde rename for the original dot-notation"
    );

    insta::assert_snapshot!("dto_included_dot_notation", content);
}
