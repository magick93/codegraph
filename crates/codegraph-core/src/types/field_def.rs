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
///   Array-of-entity-ref properties (junction tables) keep their raw names —
///   the junction table is `<parent>_<field>` and the DTO field must match it.
/// - For all other kinds: returns the field names as-is from the property.
pub fn resolve_field(prop: &PropertyNode) -> FieldDefinition {
    match prop.effective_kind() {
        Some(RefClassificationKind::EntityReference) if prop.is_array => FieldDefinition {
            rust_field_name: prop.rust_field_name.clone(),
            column_name: prop.pg_column_name.clone(),
        },
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
    // Array-of-entity-ref properties are junction tables, not FK columns —
    // the raw names are the junction field/table identity (see resolve_field).
    if prop.is_array {
        return Ok((prop.rust_field_name.clone(), prop.pg_column_name.clone()));
    }
    // Direct $ref target is a known entity.
    if let Ok(Some(target)) = db.get_property_ref_target(&prop.name, source_title).await {
        if entity_titles.contains(&target.title) {
            return Ok((
                ensure_id_suffix(&prop.rust_field_name),
                ensure_id_suffix(&prop.pg_column_name),
            ));
        }
        // ValueObject whose allOf chain reaches an entity.
        if let Ok(Some(entity)) = crate::traits::find_entity_extended_by_vo(db, &target.title).await
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

/// Extract the codelist enum name from a property's `ref_target` path.
///
/// Handles both clean names (`"GenderCodeList"`) and path-style references
/// (`"common/json/codelist/GenderCodeList.json"`).
///
/// Returns `None` when `ref_target` is `None` or empty — the caller should
/// fall back to `"String"`.
pub fn codelist_enum_name_from_ref(ref_target: &Option<String>) -> Option<String> {
    let target = ref_target.as_deref()?.trim();
    if target.is_empty() {
        return None;
    }
    // Take the last path segment and strip .json or .json# extension
    let filename = target.rsplit('/').next().unwrap_or(target);
    let name = filename
        .strip_suffix(".json#")
        .or_else(|| filename.strip_suffix(".json"))
        .unwrap_or(filename);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity_ref_prop(name: &str, is_array: bool) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: "array".into(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: false,
            is_array,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: name.to_string(),
            pg_column_type: "TEXT".into(),
            rust_field_name: name.to_string(),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "flat".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::EntityReference),
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    #[test]
    fn scalar_entity_ref_gets_id_suffix() {
        let def = resolve_field(&entity_ref_prop("settlor", false));
        assert_eq!(def.rust_field_name, "settlor_id");
        assert_eq!(def.column_name, "settlor_id");
    }

    #[test]
    fn scalar_entity_ref_id_suffix_is_idempotent() {
        let def = resolve_field(&entity_ref_prop("tenant_id", false));
        assert_eq!(def.rust_field_name, "tenant_id");
        assert_eq!(def.column_name, "tenant_id");
    }

    #[test]
    fn junction_array_entity_ref_keeps_raw_name() {
        let def = resolve_field(&entity_ref_prop("settlor_ids", true));
        assert_eq!(def.rust_field_name, "settlor_ids");
        assert_eq!(def.column_name, "settlor_ids");
    }

    #[test]
    fn junction_array_without_ids_stem_keeps_raw_name() {
        let def = resolve_field(&entity_ref_prop("parties", true));
        assert_eq!(def.rust_field_name, "parties");
        assert_eq!(def.column_name, "parties");
    }

    #[test]
    fn non_entity_ref_unchanged() {
        let mut prop = entity_ref_prop("status", false);
        prop.classification_kind = Some(RefClassificationKind::PrimitiveWrapper);
        let def = resolve_field(&prop);
        assert_eq!(def.rust_field_name, "status");
        assert_eq!(def.column_name, "status");
    }

    #[test]
    fn ensure_id_suffix_is_idempotent() {
        assert_eq!(ensure_id_suffix("trust"), "trust_id");
        assert_eq!(ensure_id_suffix("trust_id"), "trust_id");
    }
}
