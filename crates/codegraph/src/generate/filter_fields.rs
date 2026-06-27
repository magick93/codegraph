use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::resolve_field;
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::error::Result;

/// System columns that should never be exposed as filters.
const EXCLUDED_COLUMNS: &[&str] = &["id", "created_at", "updated_at", "platform_organization_id"];

/// Metadata for a single filterable field, used by handler/query/repository generators.
#[derive(Debug, Clone, Serialize)]
pub struct FilterFieldInfo {
    /// snake_case Rust / DTO field name (e.g. `worker_id`).
    pub field_name: String,
    /// PostgreSQL column name (may differ when column is a PG reserved word).
    pub pg_column_name: String,
    /// Rust type string (e.g. `"String"`, `"Uuid"`, `"i64"`).
    pub rust_type: String,
    /// Whether the column is nullable.
    pub is_nullable: bool,
}

/// Resolve which entity fields should be exposed as JSON:API `?filter[field]=value` params.
///
/// - `config_override = Some(&[])` → disable filtering (return empty).
/// - `config_override = Some(&["f1", "f2"])` → only those fields.
/// - `config_override = None` → auto-discover from graph property classifications.
pub async fn resolve_filter_fields(
    db: &dyn GraphQuerier,
    schema_title: &str,
    config_override: Option<&[String]>,
) -> Result<Vec<FilterFieldInfo>> {
    // Explicit empty list disables filtering.
    if let Some(list) = config_override {
        if list.is_empty() {
            return Ok(Vec::new());
        }
    }

    let all_props = db.get_properties(schema_title).await?;

    // Deduplicate by rust_field_name (same as repository_emitter).
    let mut seen = std::collections::HashSet::new();
    let props: Vec<_> = all_props
        .into_iter()
        .filter(|p| seen.insert(resolve_field(p).rust_field_name))
        .collect();

    let mut fields = Vec::new();

    for prop in &props {
        if EXCLUDED_COLUMNS.contains(&resolve_field(prop).rust_field_name.as_str()) {
            continue;
        }
        if prop.is_array {
            continue;
        }
        if prop.rust_field_type.starts_with("Vec<") {
            continue;
        }

        let dominated = match config_override {
            Some(list) => {
                // Explicit list: include only named fields.
                list.iter()
                    .any(|f| f == &prop.name || f == &resolve_field(prop).rust_field_name)
            }
            None => {
                // Auto-discovery heuristic.
                is_auto_filterable(prop)
            }
        };

        if dominated {
            let field_def = resolve_field(prop);
            fields.push(FilterFieldInfo {
                field_name: field_def.rust_field_name,
                pg_column_name: field_def.column_name,
                rust_type: prop.rust_field_type.clone(),
                is_nullable: !prop.is_required,
            });
        }
    }

    Ok(fields)
}

/// Metadata for a filterable field on a child or grandchild table.
///
/// Used to generate `EXISTS` subqueries in the repository `list` method so that
/// parent entities can be filtered by nested value-object or domain-child columns.
///
/// Filter key uses dot notation: `child_field.column` or `child.grandchild.column`.
#[derive(Debug, Clone, Serialize)]
pub struct NestedFilterFieldInfo {
    /// Dot-notation key used in the API, e.g. `"person_name.given_name"`.
    pub filter_key: String,
    /// Fully-qualified SQL child table, e.g. `"common"."person_person_name"`.
    pub sql_table: String,
    /// SQL schema name, e.g. `"common"`.
    pub sql_schema: String,
    /// SQL table name (unqualified), e.g. `"person_person_name"`.
    pub sql_table_name: String,
    /// FK column on the child table that references the parent, e.g. `"person_type_id"`.
    pub parent_fk_column: String,
    /// Column on the child table to filter on, e.g. `"given_name"`.
    pub pg_column_name: String,
    /// Rust type for type-safe value parsing, e.g. `"String"`.
    pub rust_type: String,
    /// Whether the column is nullable.
    pub is_nullable: bool,
    /// For grandchild filters: the intermediate child table through which to join.
    pub intermediate_join: Option<IntermediateJoin>,
}

/// Join metadata for an intermediate child table (used by grandchild filters).
#[derive(Debug, Clone, Serialize)]
pub struct IntermediateJoin {
    /// SQL schema of the intermediate table.
    pub sql_schema: String,
    /// SQL table name (unqualified) of the intermediate table.
    pub sql_table_name: String,
    /// FK column on the intermediate table that references the root parent.
    pub parent_fk_column: String,
}

/// Resolve nested (child / grandchild) filter fields for a parent entity.
///
/// This discovers filterable columns in two categories:
///
/// 1. **Value-object child tables** — walked from the graph via `ChildTableInfo` entries
///    already built by the repository emitter. We re-derive them from the graph here to
///    keep `filter_fields.rs` self-contained.
///
/// 2. **Domain-level child entities** — entities with `role = "child"` and
///    `parent = schema_title` in the domain config.
///
/// Depth is limited to 2 levels (children + grandchildren).
///
/// **Limitation**: Unlike `resolve_filter_fields()`, there is no per-entity config
/// override for nested filter fields. All nested fields are auto-discovered. A future
/// enhancement could add `nested_filter_fields` to `EntityConfig` with the same
/// `None` / `Some(&[])` / explicit-list semantics.
pub async fn resolve_nested_filter_fields(
    db: &dyn GraphQuerier,
    schema_title: &str,
    parent_table_name: &str,
    schema_name: &str,
    config: &DomainConfig,
) -> Result<Vec<NestedFilterFieldInfo>> {
    let mut nested = Vec::new();

    // --- 1. Value-object child tables from the graph ---
    let all_props = db.get_properties(schema_title).await?;
    let mut seen_props = std::collections::HashSet::new();
    let props: Vec<_> = all_props
        .into_iter()
        .filter(|p| seen_props.insert(resolve_field(p).rust_field_name))
        .collect();

    // Collect entity titles so we can skip entity-reference FKs (they aren't child tables).
    let entity_titles: std::collections::HashSet<String> =
        db.get_entity_names().await?.into_iter().collect();

    for prop in &props {
        if EXCLUDED_COLUMNS.contains(&resolve_field(prop).rust_field_name.as_str()) {
            continue;
        }

        let kind = prop.effective_kind();
        let is_vo = matches!(
            kind,
            Some(RefClassificationKind::ValueObject)
                | Some(RefClassificationKind::CompositeWrapper)
        );
        let is_array_vo = prop.is_array && {
            let target = db
                .get_array_item_schema(&prop.name, schema_title)
                .await
                .ok()
                .flatten();
            target.is_some()
        };

        if !is_vo && !is_array_vo {
            continue;
        }

        // Resolve the target schema for this child table.
        let target_schema = if prop.is_array {
            db.get_array_item_schema(&prop.name, schema_title)
                .await
                .ok()
                .flatten()
        } else {
            db.get_property_ref_target(&prop.name, schema_title)
                .await
                .ok()
                .flatten()
        };
        let Some(target) = target_schema else {
            continue;
        };

        // Skip if the target is actually a classified entity (not a VO child table).
        if entity_titles.contains(&target.title) {
            continue;
        }
        // Also skip VO→entity — data lives in entity table, not child table.
        if !target.is_entity || target.pg_table_name.is_empty() {
            if codegraph_core::traits::find_entity_extended_by_vo(db, &target.title).await
                .map(|e| e.is_some()).unwrap_or(false)
            {
                continue;
            }
        }

        let prop_def = resolve_field(prop);
        let child_table_name = codegraph_naming::truncate_pg_identifier(&format!(
            "{}_{}",
            parent_table_name, prop_def.column_name
        ));
        let child_fk = format!("{}_id", parent_table_name);
        let child_field_prefix = prop_def.rust_field_name;

        // Get child properties and find filterable columns.
        let child_props = db.get_properties(&target.title).await.unwrap_or_default();
        let mut seen_child = std::collections::HashSet::new();
        let child_props: Vec<_> = child_props
            .into_iter()
            .filter(|p| {
                let p_def = resolve_field(p);
                p_def.rust_field_name != "id" && seen_child.insert(p_def.rust_field_name)
            })
            .collect();

        for cprop in &child_props {
            let cprop_def = resolve_field(cprop);
            if EXCLUDED_COLUMNS.contains(&cprop_def.rust_field_name.as_str()) {
                continue;
            }
            if cprop.is_array || cprop.rust_field_type.starts_with("Vec<") {
                continue;
            }

            // Check if this child property is itself a VO (grandchild).
            let ckind = cprop.effective_kind();
            let is_child_vo = matches!(
                ckind,
                Some(RefClassificationKind::ValueObject)
                    | Some(RefClassificationKind::CompositeWrapper)
            );
            if is_child_vo {
                // Resolve grandchild table.
                let gc_target = db
                    .get_property_ref_target(&cprop.name, &target.title)
                    .await
                    .ok()
                    .flatten();
                if let Some(gc_schema) = gc_target {
                    if entity_titles.contains(&gc_schema.title) {
                        continue;
                    }
                    // Also skip VO→entity for grandchild.
                    if !gc_schema.is_entity || gc_schema.pg_table_name.is_empty() {
                        if codegraph_core::traits::find_entity_extended_by_vo(db, &gc_schema.title).await
                            .map(|e| e.is_some()).unwrap_or(false)
                        {
                            continue;
                        }
                    }
                    let gc_table_name = codegraph_naming::truncate_pg_identifier(&format!(
                        "{}_{}",
                        child_table_name, cprop_def.column_name
                    ));
                    let gc_fk = format!(
                        "{}_id",
                        codegraph_naming::truncate_pg_identifier(&child_table_name)
                    );
                    let gc_props = db
                        .get_properties(&gc_schema.title)
                        .await
                        .unwrap_or_default();
                    let mut seen_gc = std::collections::HashSet::new();
                    for gprop in gc_props.into_iter().filter(|p| {
                        let p_def = resolve_field(p);
                        p_def.rust_field_name != "id" && seen_gc.insert(p_def.rust_field_name)
                    }) {
                        let gprop_def = resolve_field(&gprop);
                        if EXCLUDED_COLUMNS.contains(&gprop_def.rust_field_name.as_str()) {
                            continue;
                        }
                        if gprop.is_array || gprop.rust_field_type.starts_with("Vec<") {
                            continue;
                        }
                        if !is_auto_filterable(&gprop) {
                            continue;
                        }
                        nested.push(NestedFilterFieldInfo {
                            filter_key: format!(
                                "{}.{}.{}",
                                child_field_prefix, cprop_def.rust_field_name, gprop_def.rust_field_name
                            ),
                            sql_table: format!("\"{}\".\"{}\"", schema_name, gc_table_name),
                            sql_schema: schema_name.to_string(),
                            sql_table_name: gc_table_name.clone(),
                            parent_fk_column: gc_fk.clone(),
                            pg_column_name: gprop_def.column_name,
                            rust_type: gprop.rust_field_type.clone(),
                            is_nullable: !gprop.is_required,
                            intermediate_join: Some(IntermediateJoin {
                                sql_schema: schema_name.to_string(),
                                sql_table_name: child_table_name.clone(),
                                parent_fk_column: child_fk.clone(),
                            }),
                        });
                    }
                }
                continue;
            }

            if !is_auto_filterable(cprop) {
                continue;
            }

            nested.push(NestedFilterFieldInfo {
                filter_key: format!("{}.{}", child_field_prefix, cprop_def.rust_field_name),
                sql_table: format!("\"{}\".\"{}\"", schema_name, child_table_name),
                sql_schema: schema_name.to_string(),
                sql_table_name: child_table_name.clone(),
                parent_fk_column: child_fk.clone(),
                pg_column_name: cprop_def.column_name,
                rust_type: cprop.rust_field_type.clone(),
                is_nullable: !cprop.is_required,
                intermediate_join: None,
            });
        }
    }

    // --- 2. Domain-level child entities (role = "child", parent = schema_title) ---
    for (domain_name, domain_entry) in &config.domains {
        for (config_key, entity_cfg) in &domain_entry.entity_config {
            let is_child = entity_cfg
                .role
                .as_deref()
                .map(|r| r == "child")
                .unwrap_or(false);
            let parent_matches = entity_cfg
                .parent
                .as_deref()
                .map(|p| p == schema_title)
                .unwrap_or(false);
            if !is_child || !parent_matches {
                continue;
            }

            // Resolve the child entity's schema from the graph.
            let child_schema = db.get_schema_in_domain(config_key, schema_name).await.ok().flatten();
            let Some(child_schema) = child_schema else {
                continue;
            };

            let child_module = &child_schema.pg_table_name;
            let child_schema_name = domain_name;
            let child_fk = entity_cfg
                .parent_ref
                .clone()
                .unwrap_or_else(|| format!("{}_id", parent_table_name));

            // Resolve filterable fields on the child entity.
            let child_filter_fields =
                resolve_filter_fields(db, config_key, entity_cfg.filter_fields.as_deref()).await?;

            for ff in child_filter_fields {
                nested.push(NestedFilterFieldInfo {
                    filter_key: format!("{}.{}", child_module, ff.field_name),
                    sql_table: format!("\"{}\".\"{}\"", child_schema_name, child_module),
                    sql_schema: child_schema_name.to_string(),
                    sql_table_name: child_module.to_string(),
                    parent_fk_column: child_fk.clone(),
                    pg_column_name: ff.pg_column_name,
                    rust_type: ff.rust_type,
                    is_nullable: ff.is_nullable,
                    intermediate_join: None,
                });
            }
        }
    }

    // Deduplicate by (sql_table_name, pg_column_name). If two child paths produce
    // the same filter_key but point to different tables, warn and keep the first.
    let mut seen_keys = std::collections::HashSet::new();
    let mut seen_table_col = std::collections::HashSet::new();
    nested.retain(|nf| {
        let table_col = (nf.sql_table_name.clone(), nf.pg_column_name.clone());
        if !seen_table_col.insert(table_col) {
            return false;
        }
        if !seen_keys.insert(nf.filter_key.clone()) {
            tracing::warn!(
                filter_key = %nf.filter_key,
                table = %nf.sql_table_name,
                column = %nf.pg_column_name,
                "duplicate nested filter key — skipping"
            );
            return false;
        }
        true
    });

    Ok(nested)
}

/// Heuristic: should this property be auto-discovered as filterable?
fn is_auto_filterable(prop: &codegraph_core::types::PropertyNode) -> bool {
    match prop.effective_kind() {
        // Codelist / enum fields are natural filter targets.
        Some(RefClassificationKind::CodelistReference)
        | Some(RefClassificationKind::CodelistCheck)
        | Some(RefClassificationKind::InlineEnum) => true,
        // Entity reference FKs are filterable (only actual FK columns, not navigation properties).
        Some(RefClassificationKind::EntityReference) => resolve_field(prop).column_name.ends_with("_id"),
        // Primitive fields whose name ends with `_id` are natural identifiers.
        Some(RefClassificationKind::PrimitiveWrapper) | None => {
            resolve_field(prop).column_name.ends_with("_id")
        }
        // Everything else (ValueObject, CompositeWrapper, ArrayWrapper, RangeWrapper) — skip.
        _ => false,
    }
}
