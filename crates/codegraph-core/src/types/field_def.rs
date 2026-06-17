use serde::{Deserialize, Serialize};

use crate::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub rust_field_name: String,
    pub column_name: String,
}

/// Resolves a `FieldDefinition` from a `PropertyNode` based on its classification kind.
///
/// - For `EntityReference`: appends `_id` suffix to both field and column names
///   (idempotent — does not double `_id` if already present).
/// - For all other kinds: returns the field names as-is from the property.
pub fn resolve_field(prop: &PropertyNode) -> FieldDefinition {
    match prop.effective_kind() {
        Some(RefClassificationKind::EntityReference) => {
            let rust_field_name = if prop.rust_field_name.ends_with("_id") {
                prop.rust_field_name.clone()
            } else {
                format!("{}_id", prop.rust_field_name)
            };
            let column_name = if prop.pg_column_name.ends_with("_id") {
                prop.pg_column_name.clone()
            } else {
                format!("{}_id", prop.pg_column_name)
            };
            FieldDefinition {
                rust_field_name,
                column_name,
            }
        }
        _ => FieldDefinition {
            rust_field_name: prop.rust_field_name.clone(),
            column_name: prop.pg_column_name.clone(),
        },
    }
}
