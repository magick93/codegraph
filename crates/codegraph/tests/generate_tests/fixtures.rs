//! Shared fixtures for the `generate_tests` target.
//! `mock_composition_tree`/`mock_schema` pin generator behavior — frozen verbatim.

use codegraph_core::types::{
    ColumnInfo, CompositionNode, CompositionTree, PropertyNode, SchemaNode,
};
use codegraph_type_contracts::RefClassificationKind;
use std::path::Path;

pub(crate) fn mock_schema(
    schema_id: &str,
    title: &str,
    table_name: &str,
    schema_name: &str,
    classification: &str,
) -> SchemaNode {
    let rust_type_name = title.replace("Type", "");
    SchemaNode {
        namespace: None,
        schema_id: schema_id.to_string(),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: classification.to_string(),
        domain: Some(schema_name.to_string()),
        rel_path: schema_id.to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name,
        pg_table_name: table_name.to_string(),
        api_path_segment: table_name.replace('_', "-"),
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

pub(crate) fn mock_properties() -> Vec<PropertyNode> {
    vec![
        PropertyNode {
            name: "given_name".to_string(),
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
            name: "family_name".to_string(),
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

pub(crate) fn mock_composition_tree(
    schema_title: &str,
    table_name: &str,
    domain: &str,
) -> CompositionTree {
    CompositionTree {
        root: CompositionNode {
            field_name: table_name.to_string(),
            schema_title: schema_title.to_string(),
            table_schema: domain.to_string(),
            table_name: table_name.to_string(),
            fk: None,
            is_collection: false,
            columns: vec![
                ColumnInfo {
                    name: "given_name".to_string(),
                    description: Some("The person's given name".to_string()),
                    rust_type: "String".to_string(),
                    postgres_type: "TEXT".to_string(),
                    is_optional: false,
                    is_codelist_fk: false,
                    composite_columns: vec![],
                    is_array: false,
                    classification: Some(RefClassificationKind::PrimitiveWrapper),
                    fk_target: None,
                    check_values: vec![],
                },
                ColumnInfo {
                    name: "family_name".to_string(),
                    description: Some("The person's family name".to_string()),
                    rust_type: "String".to_string(),
                    postgres_type: "TEXT".to_string(),
                    is_optional: true,
                    is_codelist_fk: false,
                    composite_columns: vec![],
                    is_array: false,
                    classification: Some(RefClassificationKind::PrimitiveWrapper),
                    fk_target: None,
                    check_values: vec![],
                },
            ],
            jsonb_columns: vec![],
            children: vec![],
            composite_range: None,
            consumed_fields: vec![],
        },
    }
}

pub(crate) fn test_domain_config() -> codegraph_config::DomainConfig {
    codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml")).unwrap()
}

pub(crate) fn test_project_config() -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig::default()
}

/// Helper: build a PropertyNode with defaults, overriding key fields.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prop(
    name: &str,
    rust_type: &str,
    pg_type: &str,
    required: bool,
    classification: Option<&str>,
    classification_kind: Option<RefClassificationKind>,
    ref_target: Option<&str>,
    is_array: bool,
) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: required,
        is_nullable: !required,
        is_array,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: pg_type.to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: rust_type.to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: classification.unwrap_or("direct_column").to_string(),
        ref_target: ref_target.map(|s| s.to_string()),
        classification: classification.map(|s| s.to_string()),
        projection: None,
        classification_kind,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}

/// Like `prop()` but allows split `rust_field_name` ≠ `pg_column_name`,
/// matching real ingestion where strip_code_suffix_safe strips _code from
/// rust_field_name but pg_column_name retains it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prop_split(
    name: &str,
    rust_field_name: &str,
    pg_column_name: &str,
    rust_type: &str,
    pg_type: &str,
    required: bool,
    classification_kind: Option<RefClassificationKind>,
    ref_target: Option<&str>,
    is_array: bool,
) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: required,
        is_nullable: !required,
        is_array,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: pg_column_name.to_string(),
        pg_column_type: pg_type.to_string(),
        rust_field_name: rust_field_name.to_string(),
        rust_field_type: rust_type.to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: ref_target.map(|s| s.to_string()),
        classification: None,
        projection: None,
        classification_kind,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}
