// hr-graph/src/generate/playwright/mod.rs
pub mod entity_gen;
pub mod global_gen;

use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use heck::ToLowerCamelCase;
use serde::Serialize;

use super::ui::page::UiField;
use crate::domain_model::{
    EntityField, RustType, example_for_field, parse_rust_type, ts_type_for_field,
};
use crate::error::Result;

/// Per-entity context passed to playwright/entity_page.tera and
/// playwright/test_data_factory.tera.
#[derive(Debug, Serialize)]
pub struct PlaywrightEntityContext {
    /// PascalCase entity name, e.g. "Person"
    pub entity_name: String,
    /// snake_case module name, e.g. "person"
    pub module_name: String,
    /// Domain name, e.g. "common"
    pub domain: String,
    /// URL path segment, e.g. "persons"
    pub path_segment: String,
    pub has_create: bool,
    pub has_read: bool,
    pub has_delete: bool,
    pub has_workflow: bool,
    pub workflow_states: Vec<String>,
    pub initial_state: String,
    /// Fields available for creation forms (excludes workflow-managed fields)
    pub create_fields: Vec<UiField>,
}

/// Summary of one entity — used by the global generator to build mod.rs.
#[derive(Debug, Serialize, Clone)]
pub struct PlaywrightEntitySummary {
    pub module_name: String,
    pub domain: String,
}

/// Per-domain grouping used by crate_lib.tera.
#[derive(Debug, Serialize, Clone)]
pub struct PlaywrightDomainSummary {
    pub name: String,
    pub entities: Vec<PlaywrightEntitySummary>,
}

/// Context for crate_lib.tera — all domains + entities.
#[derive(Debug, Serialize)]
pub struct PlaywrightCrateContext {
    pub domains: Vec<PlaywrightDomainSummary>,
}

/// Scalar (non-array) ValueObject / CompositeWrapper / MediaWrapper properties
/// are stored as flattened child columns on the main table (the Create DTO
/// mirrors this). The canonical entity model keeps them as single child-table
/// fields, so the playwright fixtures/specs expand them into their DTO-shaped
/// flat columns here — without affecting other generators (xrpc, ui, ...).
pub(crate) async fn expand_vo_fields(
    db: &dyn GraphQuerier,
    schema_title: &str,
    model_fields: &[EntityField],
    properties: &[PropertyNode],
) -> Result<Vec<EntityField>> {
    let mut out = Vec::new();
    for field in model_fields {
        let Some(prop) = properties.iter().find(|p| p.name == field.name) else {
            out.push(field.clone());
            continue;
        };
        let kind = prop.effective_kind();
        let expandable = matches!(
            kind,
            Some(RefClassificationKind::CompositeWrapper)
                | Some(RefClassificationKind::MediaWrapper)
                | Some(RefClassificationKind::ValueObject)
                | Some(RefClassificationKind::EntityReference)
        );
        let is_scalar_vo = !prop.is_array && expandable;
        let is_array_vo = prop.is_array
            && matches!(
                kind,
                Some(RefClassificationKind::CompositeWrapper)
                    | Some(RefClassificationKind::MediaWrapper)
                    | Some(RefClassificationKind::ValueObject)
            );
        if !is_scalar_vo && !is_array_vo {
            out.push(field.clone());
            continue;
        }

        // Composite / media wrappers expand into flattened child columns,
        // e.g. `person` (PersonReferenceType) → person_did / person_name / ...
        // Array VOs are flattened the same way when the DDL materializes their
        // columns on the main table (e.g. `recipients` → recipients_did / ...).
        if (matches!(
            kind,
            Some(RefClassificationKind::CompositeWrapper)
                | Some(RefClassificationKind::MediaWrapper)
        ) || (prop.is_array && matches!(kind, Some(RefClassificationKind::ValueObject))))
            && let Ok(cols) = db.get_composite_columns(&prop.name, schema_title).await
        {
            for col in cols {
                let rust_name = format!("{}{}", prop.rust_field_name, col.suffix);
                let column_name = format!("{}{}", prop.pg_column_name, col.suffix);
                if out.iter().any(|f| f.rust_field == rust_name) {
                    continue;
                }
                let base_rt = parse_rust_type(&col.rust_type, prop.is_required);
                let rust_type = if prop.is_required {
                    base_rt
                } else {
                    RustType::Optional {
                        optional: Box::new(base_rt),
                    }
                };
                let is_fk = column_name.ends_with("_id");
                out.push(EntityField {
                    name: rust_name.to_lower_camel_case(),
                    column: column_name.clone(),
                    rust_field: rust_name.clone(),
                    rust_type: rust_type.clone(),
                    sea_orm_type: col.sea_orm_type.clone(),
                    pg_type: col.pg_type.clone(),
                    ts_type: ts_type_for_field(&rust_type),
                    required: prop.is_required,
                    is_pk: false,
                    is_fk,
                    fk_target: if is_fk { col.fk_target.clone() } else { None },
                    fk_table: None,
                    classification: Some("composite_column".to_string()),
                    example_value: example_for_field(&rust_name, &col.rust_type, None),
                    label: field.label.clone(),
                    inherited: false,
                    is_child_table: false,
                    is_model_optional: !prop.is_required,
                });
            }
            continue;
        }

        // Scalar entity references — the DDL emits `{prop}_id` FK columns and
        // the DTO exposes them as flat `{prop}Id` fields. Nullability honors the
        // schema's `required` (JSON schema is the source of truth): a required
        // FK is a plain `campaignId: string` that the fixture must populate;
        // an optional FK stays `campaignId?: string | null`.
        if !prop.is_array && kind == Some(RefClassificationKind::EntityReference) {
            let fd = codegraph_core::types::resolve_field(prop);
            if out.iter().any(|f| f.rust_field == fd.rust_field_name) {
                continue;
            }
            let rust_type = if prop.is_required {
                RustType::Simple("Uuid".to_string())
            } else {
                RustType::Optional {
                    optional: Box::new(RustType::Simple("Uuid".to_string())),
                }
            };
            out.push(EntityField {
                name: fd.rust_field_name.to_lower_camel_case(),
                column: fd.column_name.clone(),
                rust_field: fd.rust_field_name.clone(),
                rust_type: rust_type.clone(),
                sea_orm_type: "Uuid".to_string(),
                pg_type: "UUID".to_string(),
                ts_type: ts_type_for_field(&rust_type),
                required: prop.is_required,
                is_pk: false,
                is_fk: true,
                fk_target: prop.ref_target.clone(),
                fk_table: None,
                classification: Some("entity_reference".to_string()),
                example_value: example_for_field(&fd.rust_field_name, "Uuid", None),
                label: field.label.clone(),
                inherited: false,
                is_child_table: false,
                is_model_optional: !prop.is_required,
            });
            continue;
        }

        // Scalar ValueObjects referencing a known entity — the DDL emits an
        // FK column, so the DTO exposes `{prop}_id`. Pure VOs stay nested in
        // the DTO (and are optional), so they keep their single-field form.
        let vo_target_is_entity = match db.get_property_ref_target(&prop.name, schema_title).await {
            Ok(Some(target)) => {
                if target.is_entity {
                    true
                } else {
                    codegraph_core::traits::find_entity_extended_by_vo(db, &target.title)
                        .await
                        .ok()
                        .flatten()
                        .is_some()
                }
            }
            _ => false,
        };
        if vo_target_is_entity {
            let fd = codegraph_core::types::resolve_field(prop);
            if out.iter().any(|f| f.rust_field == fd.rust_field_name) {
                continue;
            }
            let rust_type = RustType::Optional {
                optional: Box::new(RustType::Simple("Uuid".to_string())),
            };
            out.push(EntityField {
                name: fd.rust_field_name.to_lower_camel_case(),
                column: fd.column_name.clone(),
                rust_field: fd.rust_field_name.clone(),
                rust_type: rust_type.clone(),
                sea_orm_type: "Uuid".to_string(),
                pg_type: "UUID".to_string(),
                ts_type: ts_type_for_field(&rust_type),
                required: false,
                is_pk: false,
                is_fk: true,
                fk_target: prop.ref_target.clone(),
                fk_table: None,
                classification: Some("value_object_fk".to_string()),
                example_value: example_for_field(&fd.rust_field_name, "Uuid", None),
                label: field.label.clone(),
                inherited: false,
                is_child_table: false,
                is_model_optional: true,
            });
            continue;
        }

        out.push(field.clone());
    }
    Ok(out)
}
