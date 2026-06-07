use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;
use codegraph_type_contracts::RefClassificationKind;

#[derive(Debug, Serialize)]
pub struct DtoContext {
    pub module_name: String,
    pub entity_name: String,
    pub domain: String,
    pub fields: Vec<DtoField>,
    pub immutable_fields: Vec<String>,
    pub workflow_excluded_fields: Vec<String>,
    pub list_exclude: Vec<String>,
    pub list_include: Vec<String>,
    pub has_list_fields: bool,
    pub operations: Vec<String>,
    /// First-level child DTOs only — used for parent struct field references.
    pub child_dtos: Vec<ChildDtoContext>,
    /// All child DTOs flattened (including deeply nested) — used for struct definitions.
    pub all_child_dtos: Vec<ChildDtoContext>,
    pub codelist_imports: Vec<String>,
    /// Codelist imports for the update DTO (parent-level fields only, excludes child DTO fields).
    pub codelist_imports_update: Vec<String>,
    /// Whether this entity has a workflow (adds workflow_state section to response).
    pub has_workflow: bool,
    /// Whether this entity has an approval status field.
    pub has_approval_status: bool,
    /// Import paths for structured JSONB wrapper types used by DTO fields.
    pub structured_imports: Vec<String>,
    /// Whether to emit garde validation attributes on DTO fields.
    pub has_validate: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DtoField {
    pub name: String,
    pub rust_type: String,
    pub is_required: bool,
    pub is_array: bool,
    pub description: String,
    pub render_strategy: String,
    pub is_entity_ref: bool,
    pub is_hierarchy_field: bool,
    // Validation fields
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub minimum: Option<rust_decimal::Decimal>,
    pub maximum: Option<rust_decimal::Decimal>,
    pub pattern: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChildDtoContext {
    pub field_name: String,
    pub struct_name: String,
    pub fields: Vec<DtoField>,
    pub is_array: bool,
    /// Nested child DTOs (ValueObject properties within this child DTO)
    pub child_dtos: Vec<ChildDtoContext>,
}

/// Maximum nesting depth for recursive child DTO building.
const MAX_CHILD_DTO_DEPTH: usize = 10;

/// Recursively build a `ChildDtoContext` for a ValueObject property.
async fn build_child_dto(
    db: &dyn GraphQuerier,
    prop: &codegraph_core::types::PropertyNode,
    parent_schema_title: &str,
    parent_struct_name: &str,
    visited: &mut std::collections::HashSet<String>,
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
        let mut seen = std::collections::HashSet::new();
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
    let consumed_fields: std::collections::HashSet<String> = db
        .get_consumed_fields(&target_schema.title)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(prop, _role)| prop.name)
        .collect();

    // Add composite range field to child DTO (e.g. affiliation_period: String)
    if let Ok(Some(range)) = db.get_composite_range(&target_schema.title).await {
        child_fields.push(DtoField {
            name: range.pg_column_name.clone(),
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
                child_fields.push(DtoField {
                    name: format!("{}_id", c.rust_field_name),
                    rust_type: "uuid::Uuid".to_string(),
                    is_required: false,
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
                // StructuredWrappers are stored as a single JSONB column inline.
                child_fields.push(DtoField {
                    name: c.rust_field_name.clone(),
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
                    });
                } else {
                    let rust_type = codelist_enum_name_from_ref(&c.ref_target)
                        .unwrap_or_else(|| "String".to_string());
                    child_fields.push(DtoField {
                        name: c.rust_field_name.clone(),
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
                child_fields.push(DtoField {
                    name: c.rust_field_name.clone(),
                    rust_type: c.rust_field_type.clone(),
                    is_required: c.is_required,
                    is_array: false,
                    description: String::new(),
                    render_strategy: "direct_column".to_string(),
                    is_entity_ref: false,
            is_hierarchy_field: false,
                    min_length: c.min_length,
                    max_length: c.max_length,
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
                    child_fields.push(DtoField {
                        name: c.rust_field_name.clone(),
                        rust_type: t.clone(),
                        is_required: c.is_required,
                        is_array: false,
                        description: String::new(),
                        render_strategy: "direct_column".to_string(),
                        is_entity_ref: false,
            is_hierarchy_field: false,
                        min_length: c.min_length,
                        max_length: c.max_length,
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
        let mut seen_fields = std::collections::HashSet::new();
        child_fields.retain(|f| seen_fields.insert(f.name.clone()));
    }

    Some(ChildDtoContext {
        field_name: prop.rust_field_name.clone(),
        struct_name: child_struct_name,
        fields: child_fields,
        is_array: prop.is_array,
        child_dtos: nested_child_dtos,
    })
}

/// Flatten nested child DTOs into a single list (depth-first).
/// The template iterates a flat list and emits struct definitions for each.
/// Each entry retains its `child_dtos` so templates can emit nested child fields.
fn flatten_child_dtos(children: Vec<ChildDtoContext>) -> Vec<ChildDtoContext> {
    let mut result = Vec::new();
    for child in children {
        let nested = child.child_dtos.clone();
        result.push(child);
        result.extend(flatten_child_dtos(nested));
    }
    result
}

/// Build the template context for DTO generation. Shared between the app DTO generator
/// and the domain-types DTO generator.
pub async fn build_dto_context(
    db: &dyn GraphQuerier,
    schema_title: &str,
    domain: &str,
    config: &DomainConfig,
) -> Result<DtoContext> {
    let schema = db
        .get_schema(schema_title)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

    let entity_name = schema.rust_type_name.clone();
    let module_name = schema.pg_table_name.clone();
    let domain = domain.to_string();

    // Get entity config from domains.toml
    let entity_cfg = config
        .domains
        .get(&domain)
        .and_then(|d| d.get_entity_config(&entity_name));

    let operations = entity_cfg
        .and_then(|ec| ec.operations.clone())
        .unwrap_or_else(|| config.defaults.operations.clone());

    let dto_config = entity_cfg.map(|ec| &ec.dto);
    let mut immutable_fields = dto_config
        .map(|d| d.immutable_fields.clone())
        .unwrap_or_default();
    let list_exclude = dto_config
        .map(|d| d.list_exclude.clone())
        .unwrap_or_default();
    let list_include = dto_config
        .map(|d| d.list_include.clone())
        .unwrap_or_default();

    // Workflow status fields are excluded from Create/Update DTOs —
    // status is set to initial_state on create and changed via workflow transitions.
    let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
    let mut workflow_excluded_fields = Vec::new();
    if let Some(wf) = workflow {
        workflow_excluded_fields.push(wf.status_field.clone());
        if let Some(ref approval_field) = wf.approval_status_field {
            workflow_excluded_fields.push(approval_field.clone());
        }
        // Also mark as immutable so they're excluded from Update DTO
        for f in &workflow_excluded_fields {
            if !immutable_fields.contains(f) {
                immutable_fields.push(f.clone());
            }
        }
    }

    let hierarchy_field_name = entity_cfg
        .and_then(|ec| ec.hierarchy_field.as_ref())
        .cloned();

    let all_props = db.get_properties(schema_title).await?;

    // Deduplicate properties by field name — allOf composition can produce
    // duplicate HasProperty edges (parent + child both contribute the same field).
    let props = {
        let mut seen = std::collections::HashSet::new();
        all_props
            .into_iter()
            .filter(|p| seen.insert(p.rust_field_name.clone()))
            .collect::<Vec<_>>()
    };

    // Media fields are excluded from Create/Update DTOs — uploads happen via
    // separate media endpoints, not the JSON CRUD body.
    for prop in &props {
        if prop.effective_kind() == Some(RefClassificationKind::MediaWrapper) {
            if let Ok(comp_cols) = db.get_composite_columns(&prop.name, schema_title).await {
                for col in &comp_cols {
                    let field_name = format!("{}{}", prop.rust_field_name, col.suffix);
                    if !workflow_excluded_fields.contains(&field_name) {
                        workflow_excluded_fields.push(field_name.clone());
                    }
                    if !immutable_fields.contains(&field_name) {
                        immutable_fields.push(field_name);
                    }
                }
            }
        }
    }

    // Consumed fields from composite range collapsing — skip these from DTOs
    let consumed_fields: std::collections::HashSet<String> = db
        .get_consumed_fields(schema_title)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(prop, _role)| prop.name)
        .collect();

    // Collect all raw field names to detect collisions when stripping _code suffix.
    // E.g. if both "language_code" and "language" exist, don't strip "language_code".
    let all_field_names: std::collections::HashSet<String> =
        props.iter().map(|p| p.rust_field_name.clone()).collect();

    // Build entity titles set so we can detect VO properties that target
    // entities (these become FK columns in the DDL, not child tables).
    let entity_titles: std::collections::HashSet<String> = config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect();

    let mut fields = Vec::new();
    let mut child_dtos = Vec::new();
    let mut seen_child_structs = std::collections::HashSet::new();

    for prop in &props {
        // Skip fields consumed by composite ranges
        if consumed_fields.contains(&prop.name) {
            continue;
        }
        let is_entity_ref = prop.effective_kind() == Some(RefClassificationKind::EntityReference);

        if prop.effective_kind() == Some(RefClassificationKind::CompositeWrapper)
            || prop.effective_kind() == Some(RefClassificationKind::MediaWrapper)
        {
            if let Ok(comp_cols) = db.get_composite_columns(&prop.name, schema_title).await {
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
                db.get_property_ref_target(&prop.name, schema_title)
                    .await
                    .ok()
                    .flatten()
                    .map(|t| entity_titles.contains(&t.title))
                    .unwrap_or(false)
            } else {
                false
            };
            if is_entity_fk {
                fields.push(DtoField {
                    name: format!("{}_id", prop.rust_field_name),
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
                let mut visited = std::collections::HashSet::new();
                visited.insert(schema_title.to_string());
                if let Some(child_dto) = Box::pin(build_child_dto(
                    db,
                    prop,
                    schema_title,
                    &entity_name,
                    &mut visited,
                    0,
                    &config.defaults.type_suffix,
                ))
                .await
                {
                    if seen_child_structs.insert(child_dto.struct_name.clone()) {
                        child_dtos.push(child_dto);
                    }
                }
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
                entity_name,
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

        // Strip _code suffix from codelist field names to match repo emitter's
        // dto_field_name convention (e.g. language_code → language).
        // Guard: don't strip if it would produce a Rust keyword or collide
        // with another existing field name.
        let field_name = match prop.effective_kind() {
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::CodelistCheck) => {
                let stripped = strip_code_suffix_safe(&prop.rust_field_name);
                if stripped != prop.rust_field_name && all_field_names.contains(&stripped) {
                    // Collision: another field already uses the stripped name
                    prop.rust_field_name.clone()
                } else {
                    stripped
                }
            }
            _ => prop.rust_field_name.clone(),
        };

        fields.push(DtoField {
            name: field_name,
            rust_type,
            is_required: prop.is_required,
            is_array: prop.is_array,
            description: prop.description.as_deref().unwrap_or("").to_string(),
            render_strategy: prop.render_strategy.clone(),
            is_entity_ref,
            is_hierarchy_field: hierarchy_field_name.as_deref() == Some(&prop.rust_field_name),
            min_length: prop.min_length,
            max_length: prop.max_length,
            minimum: prop.minimum,
            maximum: prop.maximum,
            pattern: prop.pattern.clone(),
            format: prop.format.clone(),
        });
    }

    // Add synthetic hierarchy field (self-referential FK) when configured.
    // This column is added by the DDL generator but doesn't exist in the schema.
    if let Some(ref hf) = hierarchy_field_name {
        fields.push(DtoField {
            name: hf.clone(),
            rust_type: "uuid::Uuid".to_string(),
            is_required: false,
            is_array: false,
            description: String::new(),
            render_strategy: String::new(),
            is_entity_ref: false,
            is_hierarchy_field: true,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pattern: None,
            format: None,
        });
    }

    // Deduplicate parent-level fields by name
    {
        let mut seen_fields = std::collections::HashSet::new();
        fields.retain(|f| seen_fields.insert(f.name.clone()));
    }

    // Inject hierarchy_field as a synthetic optional UUID field for self-referential
    // tree/hierarchy relationships (e.g. parent_organization_id, reports_to_position_id).
    if let Some(hf) = entity_cfg.and_then(|ec| ec.hierarchy_field.clone()) {
        if !fields.iter().any(|f| f.name == hf) {
            fields.push(DtoField {
                name: hf,
                rust_type: "uuid::Uuid".to_string(),
                is_required: false,
                is_array: false,
                description: "Parent hierarchy reference.".to_string(),
                render_strategy: "hierarchy".to_string(),
                is_entity_ref: false,
                is_hierarchy_field: true,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pattern: None,
                format: None,
            });
        }
    }

    let has_list_fields = !list_include.is_empty();

    // Collect codelist imports: scan fields for types matching known codelist names
    let codelist_names: std::collections::HashSet<String> = db
        .list_codelists()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|cl| cl.name)
        .collect();

    // Flatten nested child DTOs so we scan ALL nesting levels for codelist imports
    // and generate struct definitions for every level.
    let all_child_dtos = flatten_child_dtos(child_dtos.clone());

    // Scan both top-level fields and all child DTO fields (at every nesting level)
    let all_dto_fields = fields
        .iter()
        .chain(all_child_dtos.iter().flat_map(|c| c.fields.iter()));
    let mut codelist_imports: Vec<String> = all_dto_fields
        .filter_map(|f| {
            // Check the raw type and Vec<Type> inner type
            let ty = &f.rust_type;
            let inner = ty
                .strip_prefix("Vec<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(ty);
            if codelist_names.contains(inner) {
                Some(inner.to_string())
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    codelist_imports.sort();

    // Codelist imports for the update DTO: only parent-level fields (child structs
    // are imported from dto_create, so their codelist types aren't needed here).
    let mut codelist_imports_update: Vec<String> = fields
        .iter()
        .filter_map(|f| {
            let ty = &f.rust_type;
            let inner = ty
                .strip_prefix("Vec<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(ty);
            if codelist_names.contains(inner) {
                Some(inner.to_string())
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    codelist_imports_update.sort();

    let has_workflow = workflow
        .map(|wf| wf.generate_action_endpoints)
        .unwrap_or(false);
    let has_approval_status = workflow
        .and_then(|wf| wf.approval_status_field.as_ref())
        .is_some();

    // Collect structured wrapper imports by scanning all DTO fields (parent + child)
    // for types classified as StructuredWrapper. Parent properties have classification
    // info; for child DTO fields, walk their source schemas to check classification.
    let mut structured_type_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // 1. Direct parent-level structured wrapper properties
    for p in &props {
        if p.effective_kind() == Some(RefClassificationKind::StructuredWrapper) {
            // Strip Vec<> wrapper for array properties
            let ty = strip_vec_wrapper(&p.rust_field_type);
            structured_type_names.insert(ty);
        }
    }

    // 2. Walk child VO schemas recursively to find StructuredWrapper properties
    //    at any nesting depth (mirrors the child DTO building recursion).
    let mut vo_visit_queue: Vec<(String, String)> = Vec::new(); // (prop_name, parent_schema)
    let mut vo_visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    vo_visited.insert(schema_title.to_string());
    for p in &props {
        if p.effective_kind() == Some(RefClassificationKind::ValueObject) {
            vo_visit_queue.push((p.name.clone(), schema_title.to_string()));
        }
    }
    while let Some((prop_name, parent)) = vo_visit_queue.pop() {
        let target = match db.get_property_ref_target(&prop_name, &parent).await {
            Ok(Some(t)) => Some(t),
            _ => db
                .get_array_item_schema(&prop_name, &parent)
                .await
                .ok()
                .flatten(),
        };
        if let Some(ts) = target {
            if !vo_visited.insert(ts.title.clone()) {
                continue;
            }
            if let Ok(child_props) = db.get_properties(&ts.title).await {
                for cp in &child_props {
                    if cp.effective_kind() == Some(RefClassificationKind::StructuredWrapper) {
                        structured_type_names.insert(strip_vec_wrapper(&cp.rust_field_type));
                    }
                    if cp.effective_kind() == Some(RefClassificationKind::ValueObject) {
                        vo_visit_queue.push((cp.name.clone(), ts.title.clone()));
                    }
                }
            }
        }
    }

    let import_prefix = &config.defaults.types_import_prefix;
    let structured_imports: Vec<String> = structured_type_names
        .into_iter()
        .map(|t| format!("use {import_prefix}::{t};"))
        .collect();

    Ok(DtoContext {
        module_name,
        entity_name,
        domain,
        fields,
        immutable_fields,
        workflow_excluded_fields,
        list_exclude,
        list_include,
        has_list_fields,
        operations,
        child_dtos,
        all_child_dtos,
        codelist_imports,
        codelist_imports_update,
        has_workflow,
        has_approval_status,
        structured_imports,
        has_validate: true,
    })
}

pub struct DtoGenerator {
    output_dir: PathBuf,
}

impl DtoGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for DtoGenerator {
    fn name(&self) -> &str {
        "dto"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx = build_dto_context(db, schema_title, domain, config).await?;

        if ctx.module_name.is_empty() {
            return Ok(Vec::new());
        }

        let base_dir = self
            .output_dir
            .join("src")
            .join("domain")
            .join(&ctx.domain)
            .join(&ctx.module_name);

        let mut files = Vec::new();

        if ctx.operations.contains(&"create".to_string()) {
            let content = render_template_with_project(tera, "ddd/dto_create.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: base_dir.join("dto_create.rs"),
                content,
            });
        }

        if ctx.operations.contains(&"update".to_string()) {
            let content = render_template_with_project(tera, "ddd/dto_update.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: base_dir.join("dto_update.rs"),
                content,
            });
        }

        let response = render_template_with_project(tera, "ddd/dto_response.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: base_dir.join("dto_response.rs"),
            content: response,
        });

        Ok(files)
    }
}

/// Strips `_code` suffix from a codelist field name, unless the result
/// would be a Rust keyword (e.g. `type_code` → `type` is invalid).
/// Strip outer `Vec<T>` wrapper, returning `T`. If the type doesn't start
/// with `Vec<`, returns it unchanged.
pub(crate) fn strip_vec_wrapper(ty: &str) -> String {
    if let Some(inner) = ty.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
        inner.to_string()
    } else {
        ty.to_string()
    }
}

pub(crate) fn strip_code_suffix_safe(name: &str) -> String {
    const RUST_KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    match name.strip_suffix("_code") {
        Some(stripped) if !RUST_KEYWORDS.contains(&stripped) => stripped.to_string(),
        _ => name.to_string(),
    }
}

/// Extract the codelist enum name from a `ref_target` value.
///
/// Handles both clean names (`"GenderCodeList"`) and path-style references
/// (`"common/json/codelist/GenderCodeList.json"`).
/// Returns `None` when `ref_target` is `None` or empty.
pub(crate) fn codelist_enum_name_from_ref(ref_target: &Option<String>) -> Option<String> {
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
