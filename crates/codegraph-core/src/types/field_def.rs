use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::error::GraphError;
use crate::traits::GraphQuerier;
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

/// Resolve the FK column identifiers `(rust_field_name, column_name)` for a
/// property that references another entity — either through a direct `$ref` to an
/// entity or through a ValueObject whose allOf chain reaches an entity.
///
/// Single source of truth for FK column naming: both the entity generator and the
/// include-path FK resolver use this function so they always agree on the column
/// identifiers.
///
/// - `EntityReference` or VO→entity → returns names with `_id` suffix
/// - Otherwise → returns names as-is (child table, no FK column on the parent)
pub async fn resolve_fk_column_name(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    source_title: &str,
    entity_titles: &HashSet<String>,
) -> Result<(String, String), GraphError> {
    // Direct $ref target is a known entity.
    if let Ok(Some(target)) = db.get_property_ref_target(&prop.name, source_title).await {
        if entity_titles.contains(&target.title) {
            return Ok((
                ensure_id_suffix(&prop.rust_field_name),
                ensure_id_suffix(&prop.pg_column_name),
            ));
        }
        // ValueObject whose allOf chain reaches an entity.
        if let Ok(Some(entity)) =
            crate::traits::find_entity_extended_by_vo(db, &target.title).await
        {
            if entity_titles.contains(&entity.title) {
                return Ok((
                    ensure_id_suffix(&prop.rust_field_name),
                    ensure_id_suffix(&prop.pg_column_name),
                ));
            }
        }
    }
    // Not an entity reference — return as-is (child table, no parent FK column).
    Ok((prop.rust_field_name.clone(), prop.pg_column_name.clone()))
}

/// Append `_id` suffix to a field/column name if not already present.
pub fn ensure_id_suffix(name: &str) -> String {
    if name.ends_with("_id") {
        name.to_string()
    } else {
        format!("{}_id", name)
    }
}
