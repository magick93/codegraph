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
