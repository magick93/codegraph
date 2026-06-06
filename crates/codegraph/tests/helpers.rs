//! Shared test helpers for gRPC tests.
//! Included via `mod helpers;` in other test files.

use std::collections::HashMap;
use std::path::Path;

use codegraph_core::mock::MockEngine;
use codegraph_core::types::*;
use codegraph_config::config::{DefaultsConfig, DomainConfig, DomainEntry};
use codegraph_type_contracts::RefClassificationKind;

/// Create a MockEngine pre-loaded with a CandidateType schema and properties.
pub fn mock_engine_with_candidate() -> MockEngine {
    MockEngine::builder()
        .with_schema(SchemaNode {
            schema_id: "candidate".into(),
            title: "CandidateType".into(),
            description: Some("A job candidate".into()),
            schema_type: "object".into(),
            classification: "entity".into(),
            domain: Some("recruiting".into()),
            rel_path: "recruiting/json/CandidateType.json".into(),
            pg_type: "entity".into(),
            rust_type: "CandidateType".into(),
            sea_orm_type: "Entity".into(),
            rust_type_name: "Candidate".into(),
            pg_table_name: "candidate".into(),
            api_path_segment: "candidates".into(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
        })
        .with_properties("CandidateType", vec![
            PropertyNode {
                name: "given_name".into(),
                prop_type: "string".into(),
                description: Some("Given name".into()),
                format: None,
                is_required: true,
                is_nullable: false,
                is_array: false,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pg_column_name: "given_name".into(),
                pg_column_type: "TEXT".into(),
                rust_field_name: "given_name".into(),
                rust_field_type: "String".into(),
                sea_orm_type: "String".into(),
                render_strategy: "flat".into(),
                ref_target: None,
                classification: None,
                projection: None,
                classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
            },
            PropertyNode {
                name: "family_name".into(),
                prop_type: "string".into(),
                description: Some("Family name".into()),
                format: None,
                is_required: true,
                is_nullable: false,
                is_array: false,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pg_column_name: "family_name".into(),
                pg_column_type: "TEXT".into(),
                rust_field_name: "family_name".into(),
                rust_field_type: "String".into(),
                sea_orm_type: "String".into(),
                render_strategy: "flat".into(),
                ref_target: None,
                classification: None,
                projection: None,
                classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
            },
            PropertyNode {
                name: "email".into(),
                prop_type: "string".into(),
                description: Some("Email address".into()),
                format: Some("email".into()),
                is_required: true,
                is_nullable: false,
                is_array: false,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pg_column_name: "email".into(),
                pg_column_type: "TEXT".into(),
                rust_field_name: "email".into(),
                rust_field_type: "String".into(),
                sea_orm_type: "String".into(),
                render_strategy: "flat".into(),
                ref_target: None,
                classification: None,
                projection: None,
                classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
            },
            PropertyNode {
                name: "status".into(),
                prop_type: "string".into(),
                description: Some("Application status".into()),
                format: None,
                is_required: true,
                is_nullable: false,
                is_array: false,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pg_column_name: "status".into(),
                pg_column_type: "TEXT".into(),
                rust_field_name: "status".into(),
                rust_field_type: "String".into(),
                sea_orm_type: "String".into(),
                render_strategy: "inline_enum".into(),
                ref_target: None,
                classification: None,
                projection: None,
                classification_kind: Some(RefClassificationKind::InlineEnum),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
            },
        ])
        .build()
}

/// Create a DomainConfig with a recruiting domain.
pub fn domain_config() -> DomainConfig {
    let mut domains = HashMap::new();
    domains.insert(
        "recruiting".to_string(),
        DomainEntry {
            label: "Recruiting".into(),
            schema_dir: "schemas/recruiting".into(),
            postgres_schema: "recruiting".into(),
            depends_on: vec!["common".into()],
            entities: vec!["CandidateType".into()],
            entity_config: HashMap::new(),
            auto_discover: None,
            exclude_entities: vec![],
            force_entities: vec![],
            force_value_objects: vec![],
            exclude: vec![],
            auditable: None,
            tier: "extended".into(),
        },
    );

    DomainConfig {
        defaults: DefaultsConfig {
            operations: vec![
                "create".into(),
                "read".into(),
                "update".into(),
                "delete".into(),
                "list".into(),
            ],
            auto_discover: false,
            split_openapi_by_domain: false,
            app_name: "test-app".into(),
            max_bulk_size: 100,
            type_suffix: "Type".into(),
            types_import_prefix: "codegraph_type_contracts".into(),
        },
        domains,
    }
}

/// Create a Tera engine with the project's templates.
pub fn create_test_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    codegraph::generate::template_engine::create_tera(&template_dir).unwrap()
}
