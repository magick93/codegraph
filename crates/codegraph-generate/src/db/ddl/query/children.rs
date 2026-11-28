use std::collections::HashSet;

use codegraph_core::types::CompositionNode;

use crate::db::ddl::types::{ChildTableDef, ColumnDef};

use super::artifacts::{GeneratedTables, retain_generated_fks};
use super::columns::{child_parent_fk_column, column_info_to_ddl};

/// Recursively ensure every child table (and nested child) carries the
/// tenant column, so org-isolation RLS policies can filter on it.
pub(super) fn inject_child_tenant_columns(children: &mut [ChildTableDef], tenant_col: &ColumnDef) {
    for child in children {
        if !child.columns.iter().any(|c| c.name == tenant_col.name) {
            child.columns.insert(0, tenant_col.clone());
        }
        inject_child_tenant_columns(&mut child.child_tables, tenant_col);
    }
}

/// Convert a child `CompositionNode` into a `ChildTableDef`, recursively processing
/// nested children.
pub(in crate::db::ddl) fn composition_node_to_child_table(
    node: &CompositionNode,
    parent_table_name: &str,
    parent_schema_name: &str,
    parent_display_name: &str,
    generated_tables: &GeneratedTables,
) -> ChildTableDef {
    let child_table_name = codegraph_naming::child_table_name(parent_table_name, &node.field_name);
    let child_display_name = format!("{} {}", parent_display_name, node.field_name);

    let mut columns = Vec::new();
    let mut foreign_keys = Vec::new();
    let mut check_constraints = Vec::new();
    let mut comments = Vec::new();

    // Emit composite range column if present
    if let Some(ref range) = node.composite_range {
        columns.push(ColumnDef {
            name: range.pg_column_name.clone(),
            pg_type: range.pg_type.clone(),
            nullable: true,
            default: None,
            is_primary_key: false,
            is_array: false,
        });
    }

    // Convert ColumnInfo → DDL artifacts
    for col in &node.columns {
        if col.name == "id" {
            continue;
        }
        if let Some((cols, fks, checks, cmts)) = column_info_to_ddl(col, &child_table_name) {
            columns.extend(cols);
            foreign_keys.extend(fks);
            check_constraints.extend(checks);
            comments.extend(cmts);
        }
    }

    // Deduplicate check constraints by name
    {
        let mut seen = HashSet::new();
        check_constraints.retain(|chk| seen.insert(chk.name.clone()));
    }

    // Deduplicate columns
    {
        let mut seen = HashSet::new();
        columns.retain(|col| seen.insert(col.name.clone()));
    }

    // Remove any column that collides with the parent FK column
    let parent_fk_col = child_parent_fk_column(parent_table_name);
    columns.retain(|col| col.name != parent_fk_col);

    // The child-table template appends standard created_at/updated_at audit
    // columns; schema-derived timestamps (e.g. provenance.createdAt) would
    // declare them twice, so schema-derived standards are dropped here.
    columns.retain(|col| {
        let normalized = codegraph_naming::to_snake_case(&col.name).to_lowercase();
        normalized != "created_at" && normalized != "updated_at"
    });

    // Filter FK constraints: only keep FKs whose target table belongs to a
    // generated entity (same logic as the parent table FK filter).
    retain_generated_fks(&mut foreign_keys, generated_tables);

    // Recursively convert nested children
    let nested_children: Vec<ChildTableDef> = node
        .children
        .iter()
        .map(|child| {
            composition_node_to_child_table(
                child,
                &child_table_name,
                parent_schema_name,
                &child_display_name,
                generated_tables,
            )
        })
        .collect();

    ChildTableDef {
        schema_name: parent_schema_name.to_string(),
        table_name: child_table_name,
        parent_fk_column: parent_fk_col,
        parent_schema: parent_schema_name.to_string(),
        parent_table: parent_table_name.to_string(),
        columns,
        display_name: child_display_name,
        comments,
        foreign_keys,
        check_constraints,
        child_tables: nested_children,
        // Set by the DDL generator from the media plan (issue #701).
        media_reference: false,
    }
}

/// Flatten a recursive child table tree into a depth-first ordered Vec.
/// Each child table already carries its own parent_schema/parent_table for FK references.
pub(super) fn flatten_child_tables(children: Vec<ChildTableDef>) -> Vec<ChildTableDef> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    flatten_child_tables_inner(children, &mut result, &mut seen);
    result
}

fn flatten_child_tables_inner(
    children: Vec<ChildTableDef>,
    result: &mut Vec<ChildTableDef>,
    seen: &mut std::collections::HashSet<String>,
) {
    for mut child in children {
        let nested = std::mem::take(&mut child.child_tables);
        if seen.insert(child.table_name.clone()) {
            result.push(child);
        } else {
            eprintln!(
                "DDL: skipping duplicate child table '{}' (same VO type referenced by multiple properties)",
                child.table_name
            );
        }
        flatten_child_tables_inner(nested, result, seen);
    }
}
