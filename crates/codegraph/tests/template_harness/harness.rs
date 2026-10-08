//! Test harness for template output correctness.
//!
//! Uses MockEngine from hr-graph-core to ingest schema data, then runs
//! generators against it and asserts template output correctness
//! per-schema, per-template.

use codegraph::generate::GenerationEntry;
use codegraph::generate::template_engine;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};
use std::path::Path;

pub(crate) fn test_domain_config() -> codegraph_config::DomainConfig {
    codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml")).unwrap()
}

pub(crate) fn test_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    template_engine::create_tera(&template_dir).unwrap()
}

pub(crate) fn test_generation_order() -> Vec<GenerationEntry> {
    vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }]
}

pub(crate) fn test_project_config() -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig::default()
}

pub(crate) fn sqlite_project_config() -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig {
        database: codegraph::generate::DatabaseConfig {
            database_target: codegraph::generate::db::dialect::DatabaseTarget::Sqlite,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(crate) fn gender_codelist_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: "common/json/codelist/GenderCodeList.json".to_string(),
        title: "GenderCodeList".to_string(),
        description: Some("Gender codes".to_string()),
        schema_type: "object".to_string(),
        classification: "codelist".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/codelist/GenderCodeList.json".to_string(),
        pg_type: "TEXT".to_string(),
        rust_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        rust_type_name: "GenderCode".to_string(),
        pg_table_name: "gender_code".to_string(),
        api_path_segment: String::new(),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

pub(crate) fn candidate_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: "recruiting/json/CandidateType.json".to_string(),
        title: "CandidateType".to_string(),
        description: Some("A person requesting consideration for a position".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("recruiting".to_string()),
        rel_path: "recruiting/json/CandidateType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Candidate".to_string(),
        pg_table_name: "candidate".to_string(),
        api_path_segment: "candidates".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: true,
        has_one_of: false,
        has_any_of: false,
        has_definitions: true,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

pub(crate) fn candidate_properties() -> Vec<PropertyNode> {
    vec![
        PropertyNode {
            is_id: false,
            name: "givenName".to_string(),
            prop_type: "string".to_string(),
            description: Some("The person's given name".to_string()),
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
            pg_column_name: "given_name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "given_name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
        PropertyNode {
            is_id: false,
            name: "familyName".to_string(),
            prop_type: "string".to_string(),
            description: Some("The person's family name".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "family_name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "family_name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
    ]
}

pub(crate) async fn setup_mock() -> MockEngine {
    MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties("CandidateType", candidate_properties())
        .build()
}

// === Handler Template Tests (Child Entity) ===

/// Build a mock with Compensation (parent) + Reward (child) for handler tests.
pub(crate) fn parent_child_mock() -> (MockEngine, Vec<codegraph_core::types::ParentCandidate>) {
    let parent_schema = SchemaNode {
        namespace: None,
        schema_id: "compensation/json/CompensationType.json".to_string(),
        title: "CompensationType".to_string(),
        description: Some("Compensation package".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/CompensationType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Compensation".to_string(),
        pg_table_name: "compensation".to_string(),
        api_path_segment: "compensation".to_string(),
        parent_schema: None,
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
    let child_schema = SchemaNode {
        namespace: None,
        schema_id: "compensation/json/RewardType.json".to_string(),
        title: "RewardType".to_string(),
        description: Some("A reward within compensation".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/RewardType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Reward".to_string(),
        pg_table_name: "reward".to_string(),
        api_path_segment: "reward".to_string(),
        parent_schema: None,
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

    let candidates = vec![codegraph_core::types::ParentCandidate {
        child_title: "RewardType".to_string(),
        parent_title: "CompensationType".to_string(),
        field_name: "compensationType".to_string(),
        source: codegraph_core::types::DetectionSource::ScalarRef,
    }];

    // The child entity needs an EntityReference property whose rust_field_name
    // generates a FK column matching the inferred parent_ref (compensation_type_id).
    let child_fk_property = PropertyNode {
        is_id: false,
        name: "compensationType".to_string(),
        prop_type: "object".to_string(),
        description: Some("FK to parent compensation".to_string()),
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "compensation_type_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "compensation_type".to_string(),
        rust_field_type: "Option<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("CompensationType".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: Some(codegraph_type_contracts::RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };

    let mock = MockEngine::builder()
        .with_schema(parent_schema)
        .with_schema(child_schema)
        .with_properties("RewardType", vec![child_fk_property])
        .build();

    (mock, candidates)
}

pub(crate) fn worker_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: "hr/json/WorkerType.json".to_string(),
        title: "WorkerType".to_string(),
        description: Some("A worker".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("hr".to_string()),
        rel_path: "hr/json/WorkerType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Worker".to_string(),
        pg_table_name: "worker".to_string(),
        api_path_segment: "workers".to_string(),
        parent_schema: None,
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
    }
}

pub(crate) fn person_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: "hr/json/PersonType.json".to_string(),
        title: "PersonType".to_string(),
        description: Some("A person".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("hr".to_string()),
        rel_path: "hr/json/PersonType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Person".to_string(),
        pg_table_name: "person".to_string(),
        api_path_segment: "persons".to_string(),
        parent_schema: None,
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
    }
}

pub(crate) fn worker_properties_with_person_ref() -> Vec<PropertyNode> {
    vec![PropertyNode {
        is_id: false,
        name: "person".to_string(),
        prop_type: "object".to_string(),
        description: Some("FK to person".to_string()),
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "person_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "person".to_string(),
        rust_field_type: "Option<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("PersonType".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: Some(codegraph_type_contracts::RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }]
}

pub(crate) fn setup_include_mock() -> MockEngine {
    MockEngine::builder()
        .with_schema(worker_schema())
        .with_schema(person_schema())
        .with_properties("WorkerType", worker_properties_with_person_ref())
        .build()
}

pub(crate) fn setup_include_mock_with_refs() -> MockEngine {
    MockEngine::builder()
        .with_schema(worker_schema())
        .with_schema(person_schema())
        .with_ref_target("person", "WorkerType", person_schema())
        .with_properties("WorkerType", worker_properties_with_person_ref())
        .build()
}

pub(crate) fn include_domain_config() -> codegraph_config::DomainConfig {
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

pub(crate) fn no_include_domain_config() -> codegraph_config::DomainConfig {
    let toml_str = r#"
[defaults]
operations = ["create", "read", "update", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType", "PersonType"]

[domains.hr.entity_config.WorkerType]
operations = ["create", "read", "update", "list"]
"#;
    codegraph_config::config::parse_domain_config_str(toml_str).unwrap()
}
