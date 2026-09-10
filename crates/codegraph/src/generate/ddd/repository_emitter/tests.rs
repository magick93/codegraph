use crate::generate::code_writer::CodeWriter;

use super::*;

fn make_column(
    field: &str,
    rust_type: &str,
    nullable: bool,
    dto_rust_type: Option<&str>,
    is_array: bool,
    is_structured_wrapper: bool,
) -> TreeColumn {
    TreeColumn {
        field_name: field.to_string(),
        pg_column_name: field.to_string(),
        dto_field_name: None,
        rust_type: rust_type.to_string(),
        is_nullable: nullable,
        is_entity_ref: false,
        dto_rust_type: dto_rust_type.map(|s| s.to_string()),
        is_workflow_managed: false,
        is_array,
        pg_cast: None,
        is_composite_range: false,
        is_structured_wrapper,
        is_media: false,
    }
}

// --- emit_entity_to_dto_field tests ---

#[test]
fn entity_to_dto_plain_required() {
    let col = make_column("given_name", "String", false, None, false, false);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert_eq!(code.as_str(), "    given_name: row.given_name,\n");
}

#[test]
fn entity_to_dto_codelist_required() {
    let col = make_column(
        "gender_code",
        "String",
        false,
        Some("GenderCodeList"),
        false,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains(".parse().unwrap_or_default()"));
}

#[test]
fn entity_to_dto_codelist_nullable() {
    let col = make_column(
        "currency_code",
        "String",
        true,
        Some("CurrencyCodeList"),
        false,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains(".and_then(|v| v.parse().ok())"));
}

#[test]
fn entity_to_dto_codelist_array_required() {
    let col = make_column(
        "codes",
        "Vec<String>",
        false,
        Some("StatusEnum"),
        true,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains("filter_map"));
    assert!(!code.as_str().contains(".map(|v|"));
}

#[test]
fn entity_to_dto_codelist_array_nullable() {
    let col = make_column(
        "codes",
        "Vec<String>",
        true,
        Some("StatusEnum"),
        true,
        false,
    );
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code.as_str().contains(".map(|v| v.into_iter()"));
}

#[test]
fn entity_to_dto_jsonb_required() {
    let col = make_column("address", "serde_json::Value", false, None, false, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code
        .as_str()
        .contains("serde_json::from_value(row.address).unwrap_or_default()"));
}

#[test]
fn entity_to_dto_jsonb_nullable() {
    let col = make_column("metadata", "serde_json::Value", true, None, false, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code
        .as_str()
        .contains(".and_then(|v| serde_json::from_value(v).ok())"));
}

#[test]
fn entity_to_dto_jsonb_array_required() {
    let col = make_column("tags", "serde_json::Value", false, None, true, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code
        .as_str()
        .contains("serde_json::from_value(row.tags).unwrap_or_default()"));
}

#[test]
fn entity_to_dto_jsonb_array_nullable() {
    let col = make_column("prefs", "serde_json::Value", true, None, true, true);
    let mut code = CodeWriter::new();
    emit_entity_to_dto_field(&mut code, &col, "row", "    ");
    assert!(code
        .as_str()
        .contains(".and_then(|v| serde_json::from_value(v).ok())"));
}

// --- emit_child_field_population tests ---

#[test]
fn child_population_array() {
    let children = vec![ChildTableInfo {
        field_name: "addresses".to_string(),
        struct_name: "CandidateAddress".to_string(),
        sql_table_name: "candidate_address".to_string(),
        sql_schema_name: "recruiting".to_string(),
        parent_fk_column: "candidate_id".to_string(),
        is_array: true,
        columns: vec![],
        child_tables: vec![],
    }];
    let mut code = CodeWriter::new();
    emit_child_field_population(&mut code, &children, "    ");
    assert_eq!(code.as_str(), "    addresses: addresses_rows,\n");
}

#[test]
fn child_population_single() {
    let children = vec![ChildTableInfo {
        field_name: "profile".to_string(),
        struct_name: "CandidateProfile".to_string(),
        sql_table_name: "candidate_profile".to_string(),
        sql_schema_name: "recruiting".to_string(),
        parent_fk_column: "candidate_id".to_string(),
        is_array: false,
        columns: vec![],
        child_tables: vec![],
    }];
    let mut code = CodeWriter::new();
    emit_child_field_population(&mut code, &children, "    ");
    assert_eq!(
        code.as_str(),
        "    profile: profile_rows.into_iter().next(),\n"
    );
}

#[test]
fn child_population_empty() {
    let mut code = CodeWriter::new();
    emit_child_field_population(&mut code, &[], "    ");
    assert_eq!(code.as_str(), "");
}
