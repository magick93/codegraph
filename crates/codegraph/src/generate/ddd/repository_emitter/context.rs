use codegraph_core::traits::GraphQuerier;
use codegraph_type_contracts::RefClassificationKind;

use crate::error::Result;
use crate::generate::pg_cast_for_type;

use super::{ChildColumn, ChildTableInfo, JunctionTableInfo, TreeColumn};

/// Resolve worker detail JOINs from the parent entity's DDL-defined child tables.
/// Uses the entity's module/table name to derive child table names following
/// the DDL naming convention: {parent_table}_person, {parent_table}_person_name.
pub(crate) async fn resolve_worker_detail_joins(
    db: &dyn GraphQuerier,
    parent_entity_name: &str,
    domain: &str,
) -> Vec<(String, String, String)> {
    let parent_schema = match db.get_schema_in_domain(parent_entity_name, domain).await {
        Ok(Some(s)) => s,
        _ => return Vec::new(),
    };
    let parent_table = parent_schema.pg_table_name;
    let schema = parent_schema.domain.as_deref().unwrap_or("public");
    // DDL convention for WorkerType → worker → worker_person → worker_person_name:
    // parent_table = "worker"
    // person_table = "common.worker_person", FK: worker_id
    // name_table = "common.worker_person_name", FK: worker_person_id
    let person_table = format!("{}.{}_{}", schema, parent_table, "person");
    let person_fk = format!("{}_id", parent_table);
    let name_table = format!("{}.{}_{}_{}", schema, parent_table, "person", "name");
    let name_fk = format!("{}_{}_{}", parent_table, "person", "id");
    let joins = vec![
        (name_table, name_fk, "wp".to_string()),
        (person_table, person_fk, "w".to_string()),
    ];
    let mut joins = joins;
    joins.reverse();
    joins
}

/// Maximum nesting depth for recursive child table building.
/// Prevents stack overflow on deeply-nested or degenerate schemas.
const MAX_CHILD_DEPTH: usize = 10;

/// Recursively build a `ChildTableInfo` for a ValueObject property.
/// Resolves the target schema, classifies its properties, and recurses
/// for any nested ValueObject properties (creating nested child tables).
#[allow(clippy::too_many_arguments)]
async fn build_child_table_info(
    db: &dyn GraphQuerier,
    prop: &codegraph_core::types::PropertyNode,
    parent_schema_title: &str,
    parent_table_name: &str,
    schema_name: &str,
    parent_struct_name: &str,
    visited: &mut std::collections::HashSet<String>,
    depth: usize,
    suffix: &str,
) -> Option<ChildTableInfo> {
    if depth >= MAX_CHILD_DEPTH {
        return None;
    }

    // Resolve the target schema (handles array vs non-array)
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

    let prop_field_def = codegraph_core::types::resolve_field(prop);

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

    let child_table_name = codegraph_naming::truncate_pg_identifier(&format!(
        "{}_{}",
        parent_table_name, prop_field_def.column_name
    ));
    let child_struct_name = format!(
        "{}{}",
        parent_struct_name,
        codegraph_naming::strip_suffix(&target_schema.rust_type_name, suffix)
    );

    let mut child_columns: Vec<ChildColumn> = Vec::new();
    let mut nested_child_tables: Vec<ChildTableInfo> = Vec::new();

    // Composite range: collapse start/end fields into a single range column (same as DDL generator)
    let consumed_fields: std::collections::HashSet<String> = db
        .get_consumed_fields(&target_schema.title)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(prop, _role)| prop.name)
        .collect();
    let composite_range = db
        .get_composite_range(&target_schema.title)
        .await
        .ok()
        .flatten();
    if let Some(ref range) = composite_range {
        child_columns.push(ChildColumn {
            field_name: range.pg_column_name.clone(),
            pg_column_name: range.pg_column_name.clone(),
            rust_type: "String".to_string(),
            is_nullable: true,
            dto_rust_type: None,
            pg_cast: pg_cast_for_type(&range.pg_type),
        });
    }

    for c in child_props
        .iter()
        .filter(|c| c.pg_column_name != "id" && !consumed_fields.contains(&c.name))
    {
        let field_def = codegraph_core::types::resolve_field(c);
        match c.effective_kind() {
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::CodelistCheck) => {
                let enum_name =
                    crate::generate::ddd::dto::codelist_enum_name_from_ref(&c.ref_target);
                if c.is_array {
                    // Codelist array within a child VO → nested child table
                    let nested_table = codegraph_naming::truncate_pg_identifier(&format!(
                        "{}_{}",
                        child_table_name, c.pg_column_name
                    ));
                    let nested_struct = format!(
                        "{}{}",
                        child_struct_name,
                        codegraph_naming::to_pascal_case(&c.rust_field_name)
                    );
                    nested_child_tables.push(ChildTableInfo {
                        field_name: field_def.rust_field_name.clone(),
                        struct_name: nested_struct,
                        sql_table_name: nested_table,
                        sql_schema_name: schema_name.to_string(),
                        parent_fk_column: codegraph_naming::truncate_pg_identifier(&format!(
                            "{}_id",
                            child_table_name
                        )),
                        is_array: true,
                        columns: vec![ChildColumn {
                            field_name: "code".to_string(),
                            pg_column_name: "code".to_string(),
                            rust_type: "String".to_string(),
                            is_nullable: false,
                            dto_rust_type: enum_name,
                            pg_cast: None,
                        }],
                        child_tables: vec![],
                    });
                } else {
                    child_columns.push(ChildColumn {
                        field_name: field_def.rust_field_name.clone(),
                        pg_column_name: field_def.column_name.clone(),
                        rust_type: "String".to_string(),
                        is_nullable: !c.is_required,
                        dto_rust_type: enum_name,
                        pg_cast: None,
                    });
                }
            }
            Some(RefClassificationKind::PrimitiveWrapper)
            | Some(RefClassificationKind::ArrayWrapper)
            | Some(RefClassificationKind::RangeWrapper)
            | Some(RefClassificationKind::InlineEnum) => {
                let pg_cast = if c.effective_kind() == Some(RefClassificationKind::RangeWrapper) {
                    pg_cast_for_type(&c.pg_column_type)
                } else {
                    None
                };
                child_columns.push(ChildColumn {
                    field_name: field_def.rust_field_name.clone(),
                    pg_column_name: field_def.column_name.clone(),
                    rust_type: c.rust_field_type.clone(),
                    is_nullable: !c.is_required,
                    dto_rust_type: None,
                    pg_cast,
                });
            }
            Some(RefClassificationKind::EntityReference) => {
                // Array entity refs are junction tables, never columns — the
                // same rule the DDL generator applies (ddl.rs: array
                // EntityReference => return None, junction CompositionNode).
                // Emitting them as scalar UUID columns made INSERT/SELECT
                // reference columns the DDL never created (e.g.
                // assessment_report."results", worker_person.employment_permits
                // → "column ... does not exist" at runtime).
                if c.is_array {
                    continue;
                }
                child_columns.push(ChildColumn {
                    field_name: field_def.rust_field_name,
                    pg_column_name: field_def.column_name,
                    rust_type: "Uuid".to_string(),
                    is_nullable: !c.is_required,
                    dto_rust_type: None,
                    pg_cast: None,
                });
            }
            Some(RefClassificationKind::CompositeWrapper)
            | Some(RefClassificationKind::MediaWrapper) => {
                if let Ok(comp_cols) = db
                    .get_composite_columns(&c.name, &target_schema.title)
                    .await
                {
                    for col in &comp_cols {
                        let dto_rust_type = col
                            .dto_rust_type
                            .as_ref()
                            .filter(|dt| *dt != &col.rust_type)
                            .cloned();
                        let pg_cast = pg_cast_for_type(&col.pg_type);
                        child_columns.push(ChildColumn {
                            field_name: format!("{}{}", field_def.rust_field_name, col.suffix),
                            pg_column_name: format!("{}{}", field_def.column_name, col.suffix),
                            rust_type: col.rust_type.clone(),
                            is_nullable: !c.is_required,
                            dto_rust_type,
                            pg_cast,
                        });
                    }
                }
            }
            Some(RefClassificationKind::StructuredWrapper) => {
                // StructuredWrappers are stored as a single JSONB column inline.
                child_columns.push(ChildColumn {
                    field_name: field_def.rust_field_name.clone(),
                    pg_column_name: field_def.column_name.clone(),
                    rust_type: "serde_json::Value".to_string(),
                    is_nullable: !c.is_required,
                    dto_rust_type: None,
                    pg_cast: None,
                });
            }
            Some(RefClassificationKind::ValueObject) => {
                // Recurse: nested ValueObjects become nested child tables
                let nested = Box::pin(build_child_table_info(
                    db,
                    c,
                    &target_schema.title,
                    &child_table_name,
                    schema_name,
                    &child_struct_name,
                    visited,
                    depth + 1,
                    suffix,
                ))
                .await;
                if let Some(nested_info) = nested {
                    nested_child_tables.push(nested_info);
                }
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
                    child_columns.push(ChildColumn {
                        field_name: field_def.rust_field_name.clone(),
                        pg_column_name: field_def.column_name.clone(),
                        rust_type: t.clone(),
                        is_nullable: !c.is_required,
                        dto_rust_type: None,
                        pg_cast: None,
                    });
                }
            }
        }
    }

    // Deduplicate child_columns by field_name, then by physical column name.
    // A DTO column can collide with the parent-FK column (e.g. a notification
    // DTO carrying its own `interview_id` next to the parent binding); the
    // table has that column once, bound to the parent FK, so DTO duplicates
    // are dropped and the first occurrence of any repeated column wins.
    let parent_fk_column =
        codegraph_naming::truncate_pg_identifier(&format!("{}_id", parent_table_name));
    {
        let mut seen_fields = std::collections::HashSet::new();
        child_columns.retain(|c| seen_fields.insert(c.field_name.clone()));
        let mut seen_columns = std::collections::HashSet::new();
        child_columns.retain(|c| {
            c.pg_column_name != parent_fk_column && seen_columns.insert(c.pg_column_name.clone())
        });
    }

    Some(ChildTableInfo {
        field_name: prop_field_def.rust_field_name.clone(),
        struct_name: child_struct_name,
        sql_table_name: child_table_name,
        sql_schema_name: schema_name.to_string(),
        parent_fk_column,
        is_array: prop.is_array,
        columns: child_columns,
        child_tables: nested_child_tables,
    })
}

/// Classify properties into direct columns and child tables.
/// Bundles the context needed by `build_columns_and_children` to classify properties.
pub(crate) struct ClassificationContext<'a> {
    pub(crate) schema_title: &'a str,
    pub(crate) module_name: &'a str,
    pub(crate) schema_name: &'a str,
    pub(crate) entity_name: &'a str,
    pub(crate) composite_range: &'a Option<codegraph_core::types::CompositeRange>,
    pub(crate) consumed_fields: &'a std::collections::HashSet<String>,
    pub(crate) all_field_names: &'a std::collections::HashSet<String>,
    pub(crate) entity_titles: &'a std::collections::HashSet<String>,
    pub(crate) workflow_managed: &'a std::collections::HashSet<String>,
    pub(crate) suffix: &'a str,
}

///
/// This is the core property-classification loop extracted from `query_entity_tree`.
/// It walks each property and, based on its `RefClassificationKind`, decides whether
/// to emit a direct column (TreeColumn) or a child table (ChildTableInfo).
pub(crate) async fn build_columns_and_children(
    db: &dyn GraphQuerier,
    props: &[codegraph_core::types::PropertyNode],
    ctx: &ClassificationContext<'_>,
) -> Result<(Vec<TreeColumn>, Vec<ChildTableInfo>, Vec<JunctionTableInfo>)> {
    let schema_title = ctx.schema_title;
    let module_name = ctx.module_name;
    let schema_name = ctx.schema_name;
    let entity_name = ctx.entity_name;
    let consumed_fields = ctx.consumed_fields;
    let _all_field_names = ctx.all_field_names;
    let entity_titles = ctx.entity_titles;
    let workflow_managed = ctx.workflow_managed;

    let mut direct_columns = Vec::new();

    // Add composite range column (if present) so DDL has it, but mark as
    // composite so create/update/response code skips DTO references.
    if let Some(ref range) = ctx.composite_range {
        direct_columns.push(TreeColumn {
            field_name: range.pg_column_name.clone(),
            pg_column_name: range.pg_column_name.clone(),
            dto_field_name: None,
            rust_type: "String".to_string(),
            is_nullable: true,
            is_entity_ref: false,
            dto_rust_type: None,
            is_workflow_managed: false,
            is_array: false,
            pg_cast: pg_cast_for_type(&range.pg_type),
            is_composite_range: true,
            is_structured_wrapper: false,
            is_media: false,
        });
    }
    let mut child_tables = Vec::new();
    let mut junction_tables = Vec::new();
    let mut seen_child_structs = std::collections::HashSet::new();

    for prop in props {
        if prop.rust_field_name == "id" {
            continue;
        }
        if consumed_fields.contains(&prop.name) {
            continue;
        }
        let is_workflow_field = workflow_managed.contains(&prop.rust_field_name);
        let field_def = codegraph_core::types::resolve_field(prop);
        if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::CompositeWrapper)
                | Some(RefClassificationKind::MediaWrapper)
        ) {
            let is_media = prop.effective_kind() == Some(RefClassificationKind::MediaWrapper);
            if let Ok(comp_cols) = db.get_composite_columns(&prop.name, schema_title).await {
                for col in &comp_cols {
                    let dto_rust_type = col
                        .dto_rust_type
                        .as_ref()
                        .filter(|dt| *dt != &col.rust_type)
                        .cloned();
                    let suffix_name = format!("{}{}", field_def.rust_field_name, col.suffix);
                    let suffix_pg = format!("{}{}", field_def.column_name, col.suffix);
                    direct_columns.push(TreeColumn {
                        field_name: suffix_name,
                        pg_column_name: suffix_pg,
                        dto_field_name: None,
                        rust_type: col.rust_type.clone(),
                        is_nullable: !prop.is_required,
                        is_entity_ref: false,
                        dto_rust_type,
                        is_workflow_managed: is_workflow_field,
                        is_array: false,
                        pg_cast: None,
                        is_composite_range: false,
                        is_structured_wrapper: false,
                        is_media,
                    });
                }
            }
        } else if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::PrimitiveWrapper)
                | Some(RefClassificationKind::ArrayWrapper)
                | Some(RefClassificationKind::RangeWrapper)
                | Some(RefClassificationKind::InlineEnum)
        ) {
            let pg_cast = if prop.effective_kind() == Some(RefClassificationKind::RangeWrapper) {
                pg_cast_for_type(&prop.pg_column_type)
            } else {
                None
            };
            direct_columns.push(TreeColumn {
                field_name: field_def.rust_field_name.clone(),
                pg_column_name: field_def.column_name.clone(),
                dto_field_name: None,
                rust_type: prop.rust_field_type.clone(),
                is_nullable: !prop.is_required,
                is_entity_ref: false,
                dto_rust_type: None,
                is_workflow_managed: is_workflow_field,
                is_array: prop.is_array,
                pg_cast,
                is_composite_range: false,
                is_structured_wrapper: false,
                is_media: false,
            });
        } else if matches!(
            prop.effective_kind(),
            Some(RefClassificationKind::CodelistCheck)
                | Some(RefClassificationKind::CodelistReference)
        ) {
            if prop.is_array {
                let enum_name =
                    crate::generate::ddd::dto::codelist_enum_name_from_ref(&prop.ref_target);
                let child_table_name = codegraph_naming::truncate_pg_identifier(&format!(
                    "{}_{}",
                    module_name, prop.pg_column_name
                ));
                let child_struct = format!(
                    "{}{}",
                    entity_name,
                    codegraph_naming::to_pascal_case(&prop.rust_field_name)
                );
                if seen_child_structs.insert(child_struct.clone()) {
                    child_tables.push(ChildTableInfo {
                        field_name: field_def.rust_field_name.clone(),
                        struct_name: child_struct,
                        sql_table_name: child_table_name,
                        sql_schema_name: schema_name.to_string(),
                        parent_fk_column: codegraph_naming::truncate_pg_identifier(&format!(
                            "{}_id",
                            module_name
                        )),
                        is_array: true,
                        columns: vec![ChildColumn {
                            field_name: "code".to_string(),
                            pg_column_name: "code".to_string(),
                            rust_type: "String".to_string(),
                            is_nullable: false,
                            dto_rust_type: enum_name,
                            pg_cast: None,
                        }],
                        child_tables: vec![],
                    });
                }
            } else {
                let codelist_type =
                    crate::generate::ddd::dto::codelist_enum_name_from_ref(&prop.ref_target);
                direct_columns.push(TreeColumn {
                    field_name: field_def.rust_field_name.clone(),
                    pg_column_name: field_def.column_name.clone(),
                    dto_field_name: None,
                    rust_type: "String".to_string(),
                    is_nullable: !prop.is_required,
                    is_entity_ref: false,
                    dto_rust_type: codelist_type,
                    is_workflow_managed: is_workflow_field,
                    is_array: false,
                    pg_cast: None,
                    is_composite_range: false,
                    is_structured_wrapper: false,
                    is_media: false,
                });
            }
        } else if prop.effective_kind() == Some(RefClassificationKind::StructuredWrapper) {
            direct_columns.push(TreeColumn {
                field_name: field_def.rust_field_name.clone(),
                pg_column_name: field_def.column_name.clone(),
                dto_field_name: None,
                rust_type: "serde_json::Value".to_string(),
                is_nullable: !prop.is_required,
                is_entity_ref: false,
                dto_rust_type: Some(prop.rust_field_type.clone()),
                is_workflow_managed: is_workflow_field,
                is_array: prop.is_array,
                pg_cast: None,
                is_composite_range: false,
                is_structured_wrapper: true,
                is_media: false,
            });
        } else if prop.effective_kind() == Some(RefClassificationKind::EntityReference) {
            // Array entity refs: junction table (no back-ref on target) or
            // FK-on-child (target has <parent>_id). Either way the parent model
            // has no column for this property.
            if prop.is_array {
                let target = db
                    .get_array_item_schema(&prop.name, schema_title)
                    .await
                    .ok()
                    .flatten();
                let back_ref = format!(
                    "{}_id",
                    codegraph_naming::truncate_pg_identifier(module_name)
                );
                let has_back_ref = match &target {
                    Some(t) => db
                        .get_properties(&t.title)
                        .await
                        .map(|ps| {
                            ps.iter().any(|p| {
                                p.pg_column_name == back_ref
                                    || p.pg_column_name
                                        == format!(
                                            "{}_id",
                                            codegraph_naming::to_snake_case(entity_name)
                                        )
                            })
                        })
                        .unwrap_or(false),
                    None => false,
                };
                if !has_back_ref {
                    if let Some(t) = target {
                        junction_tables.push(JunctionTableInfo {
                            field_name: field_def.rust_field_name.clone(),
                            sql_table_name: codegraph_naming::truncate_pg_identifier(&format!(
                                "{}_{}",
                                module_name, field_def.column_name
                            )),
                            sql_schema_name: schema_name.to_string(),
                            parent_fk_column: codegraph_naming::truncate_pg_identifier(&format!(
                                "{}_id",
                                module_name
                            )),
                            child_fk_column: codegraph_naming::truncate_pg_identifier(&format!(
                                "{}_id",
                                t.pg_table_name
                            )),
                            is_required: prop.is_required,
                        });
                    }
                }
                continue;
            }
            // Nullability honors the schema's `required` (JSON schema is the source
            // of truth): required refs emit Set(v) against a Uuid model column,
            // optional refs emit Set(Some(v)) against Option<Uuid>.
            let is_nullable = !prop.is_required;
            direct_columns.push(TreeColumn {
                field_name: field_def.rust_field_name,
                pg_column_name: field_def.column_name,
                dto_field_name: None,
                rust_type: "Uuid".to_string(),
                is_nullable,
                is_entity_ref: true,
                dto_rust_type: None,
                is_workflow_managed: is_workflow_field,
                is_array: false,
                pg_cast: None,
                is_composite_range: false,
                is_structured_wrapper: false,
                is_media: false,
            });
        } else if prop.effective_kind() == Some(RefClassificationKind::ValueObject) {
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
                direct_columns.push(TreeColumn {
                    field_name: format!("{}_id", field_def.rust_field_name),
                    pg_column_name: format!("{}_id", field_def.column_name),
                    dto_field_name: None,
                    rust_type: "Uuid".to_string(),
                    is_nullable: true,
                    is_entity_ref: true,
                    dto_rust_type: None,
                    is_workflow_managed: is_workflow_field,
                    is_array: false,
                    pg_cast: None,
                    is_composite_range: false,
                    is_structured_wrapper: false,
                    is_media: false,
                });
            } else {
                let mut visited = std::collections::HashSet::new();
                visited.insert(schema_title.to_string());
                if let Some(child_info) = Box::pin(build_child_table_info(
                    db,
                    prop,
                    schema_title,
                    module_name,
                    schema_name,
                    entity_name,
                    &mut visited,
                    0,
                    ctx.suffix,
                ))
                .await
                {
                    if seen_child_structs.insert(child_info.struct_name.clone()) {
                        child_tables.push(child_info);
                    }
                }
            }
        }
    }

    // Deduplicate direct_columns by field_name — composite wrappers and allOf
    // composition can produce duplicate column names. Keep the first occurrence.
    {
        let mut seen_fields = std::collections::HashSet::new();
        direct_columns.retain(|c| seen_fields.insert(c.field_name.clone()));
    }

    Ok((direct_columns, child_tables, junction_tables))
}
