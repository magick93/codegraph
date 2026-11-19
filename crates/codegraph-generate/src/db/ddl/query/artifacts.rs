use std::collections::{HashMap, HashSet};

use codegraph_core::traits::GraphQuerier;
use codegraph_type_contracts::RefClassificationKind;

use crate::db::ddl::accumulators::DdlAccumulators;
use crate::db::ddl::types::{ChildTableDef, ColumnDef, ForeignKeyDef, IndexDef};
use crate::error::{Error, Result};
use codegraph_config::DomainConfig;

use super::children::inject_child_tenant_columns;
use super::columns::column_info_to_ddl;
use super::enrich::is_global_entity;
use super::keywords::strip_rsharp;

pub(super) async fn add_parent_fk(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
    artifacts: &mut DdlAccumulators,
) {
    let ddl_props = db.get_properties(schema_title).await.unwrap_or_default();
    if let Some(fk_col) = crate::resolve_parent_fk_column(
        schema_title,
        parent_candidates,
        entity_cfg,
        &config.defaults.type_suffix,
    ) {
        let is_required = ddl_props.iter().any(|p| {
            codegraph_core::types::resolve_field(p).rust_field_name == fk_col
                || p.pg_column_name == fk_col
        }) && ddl_props
            .iter()
            .find_map(|p| {
                let fd = codegraph_core::types::resolve_field(p);
                if fd.rust_field_name == fk_col || p.pg_column_name == fk_col {
                    Some(p.is_required)
                } else {
                    None
                }
            })
            .unwrap_or(false);
        artifacts.columns.push(ColumnDef {
            name: fk_col.clone(),
            pg_type: "UUID".to_string(),
            nullable: !is_required,
            default: None,
            is_primary_key: false,
            is_array: false,
        });
        // Resolve the parent's schema and table for the FK constraint.
        // Manual config takes priority over graph detection.
        let mut fk_resolved = false;
        if let Some(ec) = entity_cfg
            && ec.role.as_deref() == Some("child")
            && let Some(ref parent_title) = ec.parent
            && let Ok(Some(parent_schema)) = db.get_schema_in_domain(parent_title, domain).await
        {
            let parent_domain = if config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(parent_title))
                .unwrap_or(false)
            {
                domain
            } else {
                parent_schema.domain.as_deref().unwrap_or(domain)
            };
            artifacts.foreign_keys.push(ForeignKeyDef {
                column_name: strip_rsharp(&fk_col),
                column: fk_col.clone(),
                references_schema: parent_domain.to_string(),
                references_table: parent_schema.pg_table_name.clone(),
                references_column: "id".to_string(),
                on_delete: "CASCADE".to_string(),
                is_codelist: false,
            });
            fk_resolved = true;
        }
        if !fk_resolved {
            let stripped =
                crate::api::router::strip_suffix(schema_title, &config.defaults.type_suffix);
            if let Some(pc) = parent_candidates.iter().find(|pc| {
                crate::api::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                    == stripped
            }) && let Ok(Some(parent_schema)) =
                db.get_schema_in_domain(&pc.parent_title, domain).await
            {
                let parent_domain = if config
                    .domains
                    .get(domain)
                    .map(|d| d.entities.contains(&pc.parent_title))
                    .unwrap_or(false)
                {
                    domain
                } else {
                    parent_schema.domain.as_deref().unwrap_or(domain)
                };
                artifacts.foreign_keys.push(ForeignKeyDef {
                    column_name: strip_rsharp(&fk_col),
                    column: fk_col,
                    references_schema: parent_domain.to_string(),
                    references_table: parent_schema.pg_table_name.clone(),
                    references_column: "id".to_string(),
                    on_delete: "CASCADE".to_string(),
                    is_codelist: false,
                });
            }
        }
    }
}

pub(super) fn add_hierarchy_artifacts(
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    artifacts: &mut DdlAccumulators,
    schema_name: &str,
    table_name: &str,
) {
    if let Some(ec) = entity_cfg
        && let Some(ref hierarchy_field) = ec.hierarchy_field
    {
        artifacts.columns.push(ColumnDef {
            name: hierarchy_field.clone(),
            pg_type: "UUID".to_string(),
            nullable: true,
            default: None,
            is_primary_key: false,
            is_array: false,
        });
        artifacts.foreign_keys.push(ForeignKeyDef {
            column: hierarchy_field.clone(),
            column_name: strip_rsharp(hierarchy_field),
            references_schema: schema_name.to_string(),
            references_table: table_name.to_string(),
            references_column: "id".to_string(),
            is_codelist: false,
            on_delete: "SET NULL".to_string(),
        });
        artifacts.indexes.push(IndexDef {
            name: format!("idx_{}_{}", table_name, hierarchy_field),
            columns: vec![hierarchy_field.clone()],
            unique: false,
        });
    }
}

pub(super) fn add_tree_columns(
    root: &codegraph_core::types::CompositionNode,
    table_name: &str,
    artifacts: &mut DdlAccumulators,
) {
    for col in &root.columns {
        if col.name == "id" {
            continue;
        }
        if let Some((cols, fks, checks, cmts)) = column_info_to_ddl(col, table_name) {
            artifacts.columns.extend(cols);
            artifacts.foreign_keys.extend(fks);
            artifacts.check_constraints.extend(checks);
            artifacts.comments.extend(cmts);
        }
    }

    // Deduplicate check constraints by name — duplicate ColumnInfo entries
    // from cross-domain schema merging produce duplicate constraints.
    {
        let mut seen = HashSet::new();
        artifacts
            .check_constraints
            .retain(|chk| seen.insert(chk.name.clone()));
    }
}

/// The set of (schema, table) pairs that will actually have migrations:
/// derived from [`compute_generation_order`] — the exact table universe the
/// DDL generator emits for (issue #448). The legacy config `entities` lists
/// ride inside the generation order, as does every graph schema with a
/// `pg_table_name` (mox/rosetta-first projects carry no `entities` key, and
/// their FK targets keep VO-shaped defaults until a classification pass
/// runs — the pre-#448 `is_entity`-only union dropped exactly those
/// targets, leaving reference columns constraint-less).
pub(super) async fn generated_table_set(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<HashSet<(String, String)>> {
    let order = crate::ordering::compute_generation_order(db, config).await?;
    let table_by_title: HashMap<String, String> = db
        .list_schemas(None)
        .await
        .map_err(|e| Error::Config(e.to_string()))?
        .into_iter()
        .filter(|s| !s.pg_table_name.is_empty())
        .map(|s| (s.title, s.pg_table_name))
        .collect();
    Ok(order
        .into_iter()
        .filter_map(|entry| {
            table_by_title
                .get(&entry.schema_title)
                .map(|table| (entry.domain, table.clone()))
        })
        .collect())
}

pub(super) fn retain_generated_fks(
    foreign_keys: &mut Vec<ForeignKeyDef>,
    generated_tables: &HashSet<(String, String)>,
) {
    foreign_keys.retain(|fk| {
        fk.is_codelist
            || generated_tables
                .contains(&(fk.references_schema.clone(), fk.references_table.clone()))
    });
}

pub(super) async fn add_property_ref_columns(
    db: &dyn GraphQuerier,
    schema_title: &str,
    schema_name: &str,
    generated_tables: &HashSet<(String, String)>,
    artifacts: &mut DdlAccumulators,
) {
    if let Ok(props) = db.get_properties(schema_title).await {
        let existing_names: std::collections::HashSet<String> =
            artifacts.columns.iter().map(|c| c.name.clone()).collect();

        for prop in &props {
            let kind = prop.effective_kind();
            if kind != Some(RefClassificationKind::EntityReference) {
                continue;
            }
            // Array entity refs are junction/FK-on-child relationships
            // materialized elsewhere (child tables, child-side FKs) — never
            // columns on this table.
            if prop.is_array {
                continue;
            }
            let base = prop
                .rust_field_name
                .strip_prefix("r#")
                .unwrap_or(&prop.rust_field_name);
            let col_name = if base.ends_with("_id") {
                base.to_string()
            } else {
                format!("{}_id", base)
            };
            if !existing_names.contains(col_name.as_str()) {
                // Try to resolve the FK target for the constraint.
                // Only emit the FK if the target schema is an entity
                // configured in some domain (has its own table/migration).
                // VOs, excluded types, and entity types not in any domain's
                // entities list don't have tables, so FK constraints to them
                // would fail with "undefined_table".
                if let Ok(Some(target)) = db.get_property_ref_target(&prop.name, schema_title).await
                    && !target.pg_table_name.is_empty()
                    && target.is_entity
                {
                    artifacts.columns.push(ColumnDef {
                        name: col_name.clone(),
                        pg_type: "UUID".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_array: false,
                    });
                    let fk_schema = target.domain.as_deref().unwrap_or(schema_name);
                    artifacts.foreign_keys.push(ForeignKeyDef {
                        column_name: strip_rsharp(&prop.rust_field_name),
                        column: col_name,
                        references_schema: fk_schema.to_string(),
                        references_table: target.pg_table_name.clone(),
                        is_codelist: false,
                        references_column: "id".to_string(),
                        on_delete: "SET NULL".to_string(),
                    });
                }
            }
        }

        // Re-filter after adding cross-domain FKs (same logic as above)
        retain_generated_fks(&mut artifacts.foreign_keys, generated_tables);
    }
}

pub(super) fn add_timestamp_and_tenant_columns(
    columns: &mut Vec<ColumnDef>,
    child_tables: &mut [ChildTableDef],
    config: &DomainConfig,
    table_name: &str,
    append_only: bool,
) -> (bool, bool) {
    // Add standard timestamp columns
    columns.push(ColumnDef {
        name: "created_at".to_string(),
        pg_type: "TIMESTAMPTZ".to_string(),
        nullable: false,
        default: Some("now()".to_string()),
        is_primary_key: false,
        is_array: false,
    });
    // Append-only tables keep created_at (the row's birth) but never gain
    // an updated_at column — states are never mutated in place (#284).
    if !append_only {
        columns.push(ColumnDef {
            name: "updated_at".to_string(),
            pg_type: "TIMESTAMPTZ".to_string(),
            nullable: false,
            default: Some("now()".to_string()),
            is_primary_key: false,
            is_array: false,
        });
    }
    let has_updated_at = !append_only;

    // Determine tenancy
    let is_tenant_scoped = !is_global_entity(table_name, config);

    // Add platform_organization_id for tenant-scoped entities
    if is_tenant_scoped {
        columns.insert(
            1,
            ColumnDef {
                name: "platform_organization_id".to_string(),
                pg_type: "UUID".to_string(),
                nullable: false,
                default: Some("'00000000-0000-0000-0000-000000000000'::UUID".to_string()),
                is_primary_key: false,
                is_array: false,
            },
        );

        // Child tables (junction + VO) inherit the parent's tenancy: they
        // carry the same tenant column so org-isolation RLS policies can
        // be applied to them too. Nested children recurse.
        let tenant_col = ColumnDef {
            name: "platform_organization_id".to_string(),
            pg_type: "UUID".to_string(),
            nullable: false,
            default: Some("'00000000-0000-0000-0000-000000000000'::UUID".to_string()),
            is_primary_key: false,
            is_array: false,
        };
        inject_child_tenant_columns(child_tables, &tenant_col);
    }

    (has_updated_at, is_tenant_scoped)
}

pub(super) fn dedup_ddl_artifacts(artifacts: &mut DdlAccumulators) {
    // Deduplicate columns by name — CompositeWrapper expansion from
    // allOf-inherited properties can produce duplicate expanded columns.
    {
        let mut seen = std::collections::HashSet::new();
        artifacts
            .columns
            .retain(|col| seen.insert(col.name.clone()));
    }

    // Deduplicate foreign keys by constraint name — cross-domain
    // schema merging via allOf produces duplicate FK definitions.
    {
        let mut seen = std::collections::HashSet::new();
        artifacts
            .foreign_keys
            .retain(|fk| seen.insert(fk.column_name.clone()));
    }

    // Deduplicate comments by column name — same root cause.
    {
        let mut seen = std::collections::HashSet::new();
        artifacts.comments.retain(|c| seen.insert(c.column.clone()));
    }
}

/// The `(parent_title, projected_table)` pair when `schema_title`'s parent
/// projects a `{parent}_{feature}` child table equal to the schema's own
/// `pg_table_name` (the standalone emission renders the parent's child
/// shape and the parent skips its duplicate block).
pub(in crate::db::ddl) async fn composition_table_conflict(
    db: &dyn GraphQuerier,
    schema_title: &str,
) -> Result<Option<(String, String)>> {
    Ok(db.composition_table_conflict(schema_title).await?)
}
