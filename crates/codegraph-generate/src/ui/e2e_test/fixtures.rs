use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use super::common::collect_ui_fields;
use super::page::UiField;

/// Build a JS object-literal body (without outer braces) for creating an entity
/// via the API.  Used for parent and grandparent entity creation in tests.
pub(super) async fn build_test_data_json(
    db: &dyn GraphQuerier,
    schema_title: &str,
    domain: Option<&str>,
    config: &DomainConfig,
) -> String {
    let fields = match collect_ui_fields(db, schema_title, &[], domain, config).await {
        Ok(f) => f,
        Err(_) => {
            // Fallback: if collect_ui_fields failed (e.g. codelist entities with
            // no UI fields in the graph), generate a minimal payload with a
            // code placeholder to satisfy NOT NULL constraints.
            // Only common-domain codelists have code columns.
            if !schema_title.is_empty() && (domain == Some("common")) {
                return "code: `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`"
                    .to_string();
            }
            return String::new();
        }
    };
    // Also handle empty-success: collect_ui_fields may return Ok(vec![]) when
    // the schema has no properties (e.g. enum-only code-list schemas).
    // Only common-domain codelists have code columns.
    if fields.is_empty() && !schema_title.is_empty() && (domain == Some("common")) {
        return "code: `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`"
            .to_string();
    }
    let mut entries = Vec::new();
    for f in &fields {
        if f.is_entity_ref || f.name == "id" {
            continue;
        }
        if f.name.ends_with("_id") && !f.is_codelist {
            continue;
        }
        // ValueObject fields: omit non-array nested types entirely.
        // All are Option<T> or Vec<T> with #[serde(default)] — omitting
        // the key lets serde use None / empty vec.  Only emit [] for arrays.
        if f.nested_type_name.is_some() {
            if f.is_array {
                entries.push(format!("'{}': []", f.name));
            } else {
                // Omitted — serde uses #[serde(default)] → None
            }
            continue;
        }
        let value = test_value_for_field(f);
        if !value.is_empty() {
            entries.push(format!("'{}': {}", f.name, value));
        }
    }
    entries.join(", ")
}

/// A runtime-valid v4 UUID as a TypeScript template literal (fixed version
/// nibble `4` and variant nibble `8`, random 12-hex tail).
fn uuid_literal() -> String {
    "`00000000-0000-4000-8000-${Math.random().toString(16).slice(2, 14).padStart(12, '0')}`"
        .to_string()
}

/// Generate a JS literal value for a UiField, matching the same logic used in
/// test templates' testData() function.
pub(super) fn test_value_for_field(field: &UiField) -> String {
    // StructuredWrapper fields emit JSONB objects
    if !field.structured_sub_fields.is_empty() {
        if field.is_array {
            return format!("[{{ value: 'Test {}' }}]", field.label);
        }
        return format!("{{ value: 'Test {}' }}", field.label);
    }
    if field.is_codelist && !field.codelist_values.is_empty() {
        if field.is_array {
            return format!("[{{ code: '{}' }}]", field.codelist_values[0]);
        }
        return format!("'{}'", field.codelist_values[0]);
    }
    // Plain UUID columns (resolved entity refs are handled above) must serialize
    // as a valid v4 UUID; a literal like `'Test Reviewer Id'` fails validation.
    if field.pg_type == "UUID" && !field.is_entity_ref {
        return uuid_literal();
    }
    match field.input_type.as_str() {
        "number" => "42".to_string(),
        "checkbox" => "true".to_string(),
        "date" => "'2025-01-15'".to_string(),
        "datetime-local" => "'2025-01-15T10:30:00Z'".to_string(),
        "date-range" => "'[2025-01-15T00:00:00Z,2025-12-31T23:59:59Z)'".to_string(),
        "code" => {
            // code column in codelist entities: must be globally unique at
            // runtime across parallel test invocations. Use a TS template
            // literal with Date.now() + random suffix.
            "`TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`".to_string()
        }
        _ => {
            if field.pg_type.contains("GEOMETRY") {
                // Geometry fields omitted — plain WKT strings are not accepted without ST_GeomFromText
                String::new()
            } else if field.is_array {
                format!("['Test {}']", field.label)
            } else if field.is_range {
                "'[2025-01-01T00:00:00Z,2025-12-31T23:59:59Z]'".to_string()
            } else if field.name == "code" && !field.is_codelist {
                // code column in codelist entities: must be globally unique at
                // runtime across parallel test invocations. Use a TS template
                // literal with Date.now() + random suffix.
                "`TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`".to_string()
            } else {
                format!("'Test {}'", field.label)
            }
        }
    }
}
