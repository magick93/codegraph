//! Per-property DTO field resolution (split from `dto.rs`, issue #751 size
//! budget).
//!
//! `resolve_dto_fields` lowers each graph property into the `DtoField`s the
//! response/create/update templates render. The scalar entity-reference arm
//! resolves `uuid::Uuid` UNCONDITIONALLY (issue #459) — matching
//! `dot_field_type`'s include-plane hardcode and the child-DTO plane — so a
//! stale graph (`rust_field_type = "String"`) can never drift the FK's DTO
//! type away from the Uuid column the repository hydrates.

use std::collections::HashSet;

use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;

use super::dto::{
    ChildDtoContext, DtoField, codelist_enum_name_from_ref, strip_vec_wrapper,
    supports_garde_length,
};

/// Maximum nesting depth for recursive child DTO building.
const MAX_CHILD_DTO_DEPTH: usize = 10;

/// Recursively build a `ChildDtoContext` for a ValueObject property.
pub(super) async fn build_child_dto(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    parent_schema_title: &str,
    parent_struct_name: &str,
    visited: &mut HashSet<String>,
    depth: usize,
    suffix: &str,
) -> Option<ChildDtoContext> {
    if depth >= MAX_CHILD_DTO_DEPTH {
        return None;
    }

    let target = if prop.is_array {
        db.get_array_item_schema(&prop.name, parent_schema_title)
            .await
            .ok()
            .flatten()
    } else {
        db.get_property_ref_target(&prop.name, parent_schema_title)
            .await
            .ok()
            .flatten()
    };

    let target_schema = target?;

    // Cycle guard
    if !visited.insert(target_schema.title.clone()) {
        return None;
    }

    let raw_child_props = db
        .get_properties(&target_schema.title)
        .await
        .unwrap_or_default();
    let child_props = {
        let mut seen = HashSet::new();
        raw_child_props
            .into_iter()
            .filter(|p| p.rust_field_name != "id" && seen.insert(p.rust_field_name.clone()))
            .collect::<Vec<_>>()
    };

    let child_struct_name = format!(
        "{}{}",
        parent_struct_name,
        codegraph_naming::strip_suffix(&target_schema.rust_type_name, suffix)
    );

    let mut child_fields: Vec<DtoField> = Vec::new();
    let mut nested_child_dtos: Vec<ChildDtoContext> = Vec::new();

    // Composite range: collapse start/end fields into a single range column (same as parent DTOs)
    let consumed_fields: HashSet<String> = db
        .get_consumed_fields(&target_schema.title)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(prop, _role)| prop.name)
        .collect();

    // Add composite range field to child DTO (e.g. affiliation_period: String)
    if let Ok(Some(range)) = db.get_composite_range(&target_schema.title).await {
        child_fields.push(DtoField {
            name: range.pg_column_name,
            rust_type: "String".to_string(),
            is_required: false,
            is_array: false,
            description: String::new(),
            render_strategy: "composite_range".to_string(),
            is_entity_ref: false,
            is_hierarchy_field: false,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pattern: None,
            format: None,
        });
    }

    for c in child_props
        .iter()
        .filter(|c| !consumed_fields.contains(&c.name))
    {
        match c.effective_kind() {
            Some(RefClassificationKind::CompositeWrapper)
            | Some(RefClassificationKind::MediaWrapper) => {
                if let Ok(comp_cols) = db
                    .get_composite_columns(&c.name, &target_schema.title)
                    .await
                {
                    for col in &comp_cols {
                        let rust_type = col
                            .dto_rust_type
                            .clone()
                            .unwrap_or_else(|| col.rust_type.clone());
                        child_fields.push(DtoField {
                            name: format!("{}{}", c.rust_field_name, col.suffix),
                            rust_type,
                            is_required: c.is_required,
                            is_array: false,
                            description: String::new(),
                            render_strategy: "composite_column".to_string(),
                            is_entity_ref: false,
                            is_hierarchy_field: false,
                            min_length: None,
                            max_length: None,
                            minimum: None,
                            maximum: None,
                            pattern: None,
                            format: None,
                        });
                    }
                }
            }
            Some(RefClassificationKind::EntityReference) => {
                let fd = codegraph_core::types::resolve_field(c);
                child_fields.push(DtoField {
                    name: fd.rust_field_name.clone(),
                    rust_type: "uuid::Uuid".to_string(),
                    is_required: c.is_required,
                    is_array: false,
                    description: String::new(),
                    render_strategy: "entity_ref".to_string(),
                    is_entity_ref: true,
                    is_hierarchy_field: false,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pattern: None,
                    format: None,
                });
            }
            Some(RefClassificationKind::StructuredWrapper) => {
                let fd = codegraph_core::types::resolve_field(c);
                // StructuredWrappers are stored as a single JSONB column inline.
                child_fields.push(DtoField {
                    name: fd.rust_field_name.clone(),
                    rust_type: "serde_json::Value".to_string(),
                    is_required: c.is_required,
                    is_array: false,
                    description: String::new(),
                    render_strategy: "direct_column".to_string(),
                    is_entity_ref: false,
                    is_hierarchy_field: false,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pattern: None,
                    format: None,
                });
            }
            Some(RefClassificationKind::ValueObject) => {
                // Recurse: nested VOs become nested child DTOs
                let nested_dto = Box::pin(build_child_dto(
                    db,
                    c,
                    &target_schema.title,
                    &child_struct_name,
                    visited,
                    depth + 1,
                    suffix,
                ))
                .await;
                if let Some(nested) = nested_dto {
                    nested_child_dtos.push(nested);
                }
            }
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::CodelistCheck) => {
                if c.is_array {
                    // Codelist array → nested child DTO with single "code" field
                    let nested_struct = format!(
                        "{}{}",
                        child_struct_name,
                        codegraph_naming::to_pascal_case(&c.rust_field_name)
                    );
                    let code_type = codelist_enum_name_from_ref(&c.ref_target)
                        .unwrap_or_else(|| "String".to_string());
                    nested_child_dtos.push(ChildDtoContext {
                        field_name: c.rust_field_name.clone(),
                        struct_name: nested_struct,
                        fields: vec![DtoField {
                            name: "code".to_string(),
                            rust_type: code_type,
                            is_required: true,
                            is_array: false,
                            description: String::new(),
                            render_strategy: "codelist".to_string(),
                            is_entity_ref: false,
                            is_hierarchy_field: false,
                            min_length: None,
                            max_length: None,
                            minimum: None,
                            maximum: None,
                            pattern: None,
                            format: None,
                        }],
                        is_array: true,
                        child_dtos: vec![],
                        is_media: false,
                    });
                } else {
                    let fd = codegraph_core::types::resolve_field(c);
                    let rust_type = codelist_enum_name_from_ref(&c.ref_target)
                        .unwrap_or_else(|| "String".to_string());
                    child_fields.push(DtoField {
                        name: fd.rust_field_name.clone(),
                        rust_type,
                        is_required: c.is_required,
                        is_array: false,
                        description: String::new(),
                        render_strategy: "codelist".to_string(),
                        is_entity_ref: false,
                        is_hierarchy_field: false,
                        min_length: None,
                        max_length: None,
                        minimum: None,
                        maximum: None,
                        pattern: None,
                        format: None,
                    });
                }
            }
            Some(RefClassificationKind::PrimitiveWrapper)
            | Some(RefClassificationKind::ArrayWrapper)
            | Some(RefClassificationKind::RangeWrapper)
            | Some(RefClassificationKind::InlineEnum) => {
                let fd = codegraph_core::types::resolve_field(c);
                let len_ok = supports_garde_length(&c.rust_field_type);
                child_fields.push(DtoField {
                    name: fd.rust_field_name.clone(),
                    rust_type: c.rust_field_type.clone(),
                    is_required: c.is_required,
                    is_array: false,
                    description: String::new(),
                    render_strategy: "direct_column".to_string(),
                    is_entity_ref: false,
                    is_hierarchy_field: false,
                    min_length: if len_ok { c.min_length } else { None },
                    max_length: if len_ok { c.max_length } else { None },
                    minimum: c.minimum,
                    maximum: c.maximum,
                    pattern: c.pattern.clone(),
                    format: c.format.clone(),
                });
            }
            None => {
                let t = &c.rust_field_type;
                if t.contains("::")
                    || t.starts_with("Vec<")
                    || matches!(
                        t.as_str(),
                        "String" | "bool" | "i16" | "i32" | "i64" | "f32" | "f64" | "u32" | "u64"
                    )
                {
                    let fd = codegraph_core::types::resolve_field(c);
                    let len_ok = supports_garde_length(t);
                    child_fields.push(DtoField {
                        name: fd.rust_field_name.clone(),
                        rust_type: t.clone(),
                        is_required: c.is_required,
                        is_array: false,
                        description: String::new(),
                        render_strategy: "direct_column".to_string(),
                        is_entity_ref: false,
                        is_hierarchy_field: false,
                        min_length: if len_ok { c.min_length } else { None },
                        max_length: if len_ok { c.max_length } else { None },
                        minimum: c.minimum,
                        maximum: c.maximum,
                        pattern: c.pattern.clone(),
                        format: c.format.clone(),
                    });
                }
            }
        }
    }

    // Deduplicate child fields by name
    {
        let mut seen_fields = HashSet::new();
        child_fields.retain(|f| seen_fields.insert(f.name.clone()));
    }

    Some(ChildDtoContext {
        field_name: prop.rust_field_name.clone(),
        struct_name: child_struct_name,
        fields: child_fields,
        is_array: prop.is_array,
        child_dtos: nested_child_dtos,
        // The top-level caller knows whether this property is a media field
        // (the plan is per entity/domain); nested children are never media.
        is_media: false,
    })
}

/// Shared resolution inputs threaded through the per-property DTO field
/// lowering (extracted from `build_dto_context`, issue #751).
pub(super) struct DtoFieldResolution<'a> {
    pub db: &'a dyn GraphQuerier,
    pub schema_title: &'a str,
    pub entity_name: &'a str,
    pub module_name: &'a str,
    pub type_suffix: &'a str,
    pub entity_titles: &'a HashSet<String>,
    pub hierarchy_field_name: &'a Option<String>,
    pub media_fields: &'a HashSet<String>,
}

/// Resolve every schema property into DTO fields / child DTOs. Extracted
/// verbatim from `build_dto_context` (issue #751); the caller keeps the
/// preamble (schema fetch, entity config, media plan) and the post-loop
/// context assembly.
pub(super) async fn resolve_dto_fields(
    r: &DtoFieldResolution<'_>,
    props: &[PropertyNode],
    consumed_fields: &HashSet<String>,
    fields: &mut Vec<DtoField>,
    child_dtos: &mut Vec<ChildDtoContext>,
) {
    let mut seen_child_structs = HashSet::new();

    for prop in props {
        // Skip fields consumed by composite ranges
        if consumed_fields.contains(&prop.name) {
            continue;
        }
        let is_entity_ref = prop.effective_kind() == Some(RefClassificationKind::EntityReference);

        if prop.effective_kind() == Some(RefClassificationKind::CompositeWrapper)
            || prop.effective_kind() == Some(RefClassificationKind::MediaWrapper)
        {
            if let Ok(comp_cols) = r.db.get_composite_columns(&prop.name, r.schema_title).await {
                for col in &comp_cols {
                    let rust_type = col
                        .dto_rust_type
                        .clone()
                        .unwrap_or_else(|| col.rust_type.clone());
                    fields.push(DtoField {
                        name: format!("{}{}", prop.rust_field_name, col.suffix),
                        rust_type,
                        is_required: prop.is_required,
                        is_array: false,
                        description: String::new(),
                        render_strategy: "composite_column".to_string(),
                        is_entity_ref: false,
                        is_hierarchy_field: false,
                        min_length: None,
                        max_length: None,
                        minimum: None,
                        maximum: None,
                        pattern: None,
                        format: None,
                    });
                }
            }
            continue;
        }

        if prop.effective_kind() == Some(RefClassificationKind::ValueObject) {
            // When a non-array VO targets a known entity, the DDL emits an
            // FK column instead of a child table. Emit a UUID field to match.
            let is_entity_fk = if !prop.is_array {
                r.db.get_property_ref_target(&prop.name, r.schema_title)
                    .await
                    .ok()
                    .flatten()
                    .map(|t| r.entity_titles.contains(&t.title))
                    .unwrap_or(false)
            } else {
                false
            };
            if is_entity_fk {
                let fd = codegraph_core::types::resolve_field(prop);
                fields.push(DtoField {
                    name: fd.rust_field_name.clone(),
                    rust_type: "Uuid".to_string(),
                    is_required: false,
                    is_array: false,
                    description: prop.description.clone().unwrap_or_default(),
                    render_strategy: "entity_ref".to_string(),
                    is_entity_ref: true,
                    is_hierarchy_field: false,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pattern: None,
                    format: None,
                });
            } else {
                let mut visited = HashSet::new();
                visited.insert(r.schema_title.to_string());
                if let Some(mut child_dto) = Box::pin(build_child_dto(
                    r.db,
                    prop,
                    r.schema_title,
                    r.entity_name,
                    &mut visited,
                    0,
                    r.type_suffix,
                ))
                .await
                {
                    // Issue #707: a media reference stays in the tree (the
                    // response DTO renders it; the media handlers address
                    // the child row) but the create/update templates omit
                    // it, so the upload handler is its only writer.
                    child_dto.is_media = r.media_fields.contains(&child_dto.field_name)
                        || r.media_fields.contains(&prop.rust_field_name);
                    if seen_child_structs.insert(child_dto.struct_name.clone()) {
                        child_dtos.push(child_dto);
                    }
                }
            }
            continue;
        }

        // Array of entity refs:
        //  - Junction (no back-reference on the target schema): the DTO carries
        //    a Vec<uuid::Uuid> of related ids, persisted into the junction table.
        //  - FK-on-child (target has a `<parent>_id` column, e.g. party.case_id
        //    for case.party_ids): no field on the parent DTO — children are
        //    managed through their own endpoints.
        if prop.is_array && prop.effective_kind() == Some(RefClassificationKind::EntityReference) {
            let target =
                r.db.get_array_item_schema(&prop.name, r.schema_title)
                    .await
                    .ok()
                    .flatten();
            let has_back_ref = match &target {
                Some(t) => {
                    r.db.get_properties(&t.title)
                        .await
                        .map(|ps| {
                            ps.iter().any(|p| {
                                codegraph_naming::is_back_ref_column(
                                    &p.pg_column_name,
                                    r.module_name,
                                    r.schema_title,
                                )
                            })
                        })
                        .unwrap_or(false)
                }
                None => false,
            };
            // The repository junction writer requires a resolvable target
            // (`if !has_back_ref && let Some(t) = target` in the repository
            // emitter). Emitting the DTO field when the target is dangling
            // would accept ids no query ever writes — the #539 silent-drop
            // shape. Both planes skip together.
            if !has_back_ref && target.is_some() {
                let field_def = codegraph_core::types::resolve_field(prop);
                fields.push(DtoField {
                    name: field_def.rust_field_name.clone(),
                    rust_type: "uuid::Uuid".to_string(),
                    is_required: prop.is_required,
                    is_array: true,
                    description: prop.description.clone().unwrap_or_default(),
                    render_strategy: "entity_ref".to_string(),
                    is_entity_ref: true,
                    is_hierarchy_field: false,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pattern: None,
                    format: None,
                });
            }
            continue;
        }

        // Codelist array properties → synthetic child DTO with a single "code" field.
        if prop.is_array
            && matches!(
                prop.effective_kind(),
                Some(RefClassificationKind::CodelistReference)
                    | Some(RefClassificationKind::CodelistCheck)
            )
        {
            let child_struct_name = format!(
                "{}{}",
                r.entity_name,
                codegraph_naming::to_pascal_case(&prop.rust_field_name)
            );
            let code_type = codelist_enum_name_from_ref(&prop.ref_target)
                .unwrap_or_else(|| "String".to_string());
            if seen_child_structs.insert(child_struct_name.clone()) {
                child_dtos.push(ChildDtoContext {
                    field_name: prop.rust_field_name.clone(),
                    struct_name: child_struct_name,
                    fields: vec![DtoField {
                        name: "code".to_string(),
                        rust_type: code_type,
                        is_required: true,
                        is_array: false,
                        description: prop.description.clone().unwrap_or_default(),
                        render_strategy: "codelist".to_string(),
                        is_entity_ref: false,
                        is_hierarchy_field: false,
                        min_length: None,
                        max_length: None,
                        minimum: None,
                        maximum: None,
                        pattern: None,
                        format: None,
                    }],
                    is_array: true,
                    child_dtos: vec![],
                    is_media: false,
                });
            }
            continue;
        }

        // Map codelist references to the generated codelist enum type
        // (e.g. GenderCodeList) for compile-time validation
        let rust_type = match prop.effective_kind() {
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::CodelistCheck) => {
                codelist_enum_name_from_ref(&prop.ref_target)
                    .unwrap_or_else(|| "String".to_string())
            }
            Some(RefClassificationKind::RangeWrapper) => {
                // Range types mapped to JSONB; use the graph's rust_field_type
                // (serde_json::Value) so DTOs match entity model types.
                prop.rust_field_type.clone()
            }
            // Issue #459: a scalar entity reference's DTO type is ALWAYS the
            // FK uuid — matching `dot_field_type`'s include-plane hardcode and
            // the child-DTO plane — never the graph's `rust_field_type` (a
            // stale graph could carry "String" while the repository hydrates
            // the Uuid FK column). Both producers already set `uuid::Uuid`,
            // so no reachable fixture output changes.
            Some(RefClassificationKind::EntityReference) => "uuid::Uuid".to_string(),
            _ => prop.rust_field_type.clone(),
        };
        // When is_array is true, the template wraps the type in Vec<>.
        // The ingester already sets rust_field_type to Vec<T> for arrays,
        // so strip the outer Vec<> to avoid double-wrapping (Vec<Vec<T>>).
        let rust_type = if prop.is_array {
            strip_vec_wrapper(&rust_type)
        } else {
            rust_type
        };

        let field_def = codegraph_core::types::resolve_field(prop);
        // Use field_def.rust_field_name for DTO field names
        let len_ok = supports_garde_length(&rust_type);

        fields.push(DtoField {
            name: field_def.rust_field_name.clone(),
            rust_type,
            // Entity references in DTOs respect the schema's required/optional
            // constraint — required FKs produce non-Option uuid::Uuid fields.
            is_required: prop.is_required,
            is_array: prop.is_array,
            description: prop.description.as_deref().unwrap_or("").to_string(),
            render_strategy: prop.render_strategy.clone(),
            is_entity_ref,
            is_hierarchy_field: r.hierarchy_field_name.as_deref()
                == Some(&field_def.rust_field_name),
            min_length: if len_ok { prop.min_length } else { None },
            max_length: if len_ok { prop.max_length } else { None },
            minimum: prop.minimum,
            maximum: prop.maximum,
            pattern: prop.pattern.clone(),
            format: prop.format.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::traits::GraphIngestor;

    /// Issue #459 pin: a scalar entity-reference property whose graph row
    /// carries `rust_field_type = "String"` (the stale-graph drift shape)
    /// resolves dto type `uuid::Uuid` under the `_id`-suffixed FK name —
    /// never the stale "String" the `_` arm used to copy.
    #[tokio::test]
    async fn scalar_entity_ref_resolves_uuid_even_when_the_graph_says_string() {
        let engine = codegraph_core::mock::MockEngine::new();
        let prop = PropertyNode {
            name: "badge_ref".to_string(),
            prop_type: "object".to_string(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: true,
            is_id: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "badge_ref".to_string(),
            pg_column_type: String::new(),
            rust_field_name: "badge_ref".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: String::new(),
            render_strategy: "entity_reference".to_string(),
            ref_target: Some("BadgeType.json".to_string()),
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::EntityReference),
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        };
        engine
            .ingest_property("CrewType", "crew", &prop)
            .await
            .unwrap();

        let entity_titles: HashSet<String> = ["CrewType".to_string(), "BadgeType".to_string()]
            .into_iter()
            .collect();
        let hierarchy_field_name: Option<String> = None;
        let media_fields: HashSet<String> = HashSet::new();
        let r = DtoFieldResolution {
            db: &engine,
            schema_title: "CrewType",
            entity_name: "CrewType",
            module_name: "crew",
            type_suffix: "",
            entity_titles: &entity_titles,
            hierarchy_field_name: &hierarchy_field_name,
            media_fields: &media_fields,
        };
        let props = vec![prop];
        let consumed_fields: HashSet<String> = HashSet::new();
        let mut fields = Vec::new();
        let mut child_dtos = Vec::new();
        resolve_dto_fields(&r, &props, &consumed_fields, &mut fields, &mut child_dtos).await;

        assert_eq!(fields.len(), 1, "{fields:?}");
        let field = &fields[0];
        // resolve_field appends the `_id` suffix on the FK name (issue #459).
        assert_eq!(field.name, "badge_ref_id", "{fields:?}");
        assert_eq!(field.rust_type, "uuid::Uuid", "{fields:?}");
        assert!(field.is_entity_ref, "{fields:?}");
        assert!(!field.is_array, "{fields:?}");
    }
}
