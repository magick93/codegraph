use crate::api::include_path::ResolvedIncludePath;
use crate::code_writer::{w, wln, CodeWriter};

use super::dto::emit_child_field_population;
use super::helpers::{
    is_copy_type, is_vec_string, is_vec_type, null_value_for_type, q, turbofish, typed_value_expr,
    vec_array_type_and_ctor,
};
use super::{ChildColumn, ChildTableInfo, EntityTree};

/// Emit a single child column value expression for an INSERT statement.
///
/// When `dto_rust_type` is set, the DTO field is a codelist enum that needs
/// `.to_string()` before being stored as a String column.
pub(crate) fn emit_child_col_write_value(code: &mut CodeWriter, col: &ChildColumn) {
    let clone_suffix = if is_copy_type(&col.rust_type) {
        ""
    } else {
        ".clone()"
    };
    let has_enum = col.dto_rust_type.is_some();

    if col.is_nullable {
        if is_vec_string(&col.rust_type) || (is_vec_type(&col.rust_type) && has_enum) {
            // Vec<String> or Vec<CodelistType> — store as TEXT[] with element-level conversion
            let map_fn = if is_vec_string(&col.rust_type) {
                "s"
            } else {
                "s.to_string()"
            };
            w!(
                code,
                ", item.{field}.clone().map(|v| sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(v.into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))).unwrap_or({null})",
                field = col.field_name,
                null = null_value_for_type("Vec<String>"),
            );
        } else if is_vec_type(&col.rust_type) {
            // Vec<NaiveDate> or other non-string Vec — use typed array
            let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
            w!(
                code,
                ", item.{field}.clone().map(|v| sea_orm::Value::Array({array_type}, Some(Box::new(v.into_iter().map(|s| {value_ctor}).collect())))).unwrap_or(sea_orm::Value::Array({array_type}, None))",
                field = col.field_name,
            );
        } else if has_enum {
            w!(
                code,
                ", item.{field}.as_ref().map(|v| sea_orm::Value::String(Some(Box::new(v.to_string())))).unwrap_or({null})",
                field = col.field_name,
                null = null_value_for_type(&col.rust_type),
            );
        } else {
            let typed_value = typed_value_expr(&col.rust_type, "v");
            w!(
                code,
                ", item.{field}{clone}.map(|v| {typed_value}).unwrap_or({null})",
                field = col.field_name,
                clone = clone_suffix,
                typed_value = typed_value,
                null = null_value_for_type(&col.rust_type),
            );
        }
    } else if is_vec_string(&col.rust_type) || (is_vec_type(&col.rust_type) && has_enum) {
        let map_fn = if is_vec_string(&col.rust_type) {
            "s"
        } else {
            "s.to_string()"
        };
        w!(
            code,
            ", sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(item.{field}.clone().into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))",
            field = col.field_name,
        );
    } else if is_vec_type(&col.rust_type) {
        let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
        w!(
            code,
            ", sea_orm::Value::Array({array_type}, Some(Box::new(item.{field}.clone().into_iter().map(|s| {value_ctor}).collect())))",
            field = col.field_name,
        );
    } else if has_enum {
        w!(
            code,
            ", sea_orm::Value::String(Some(Box::new(item.{field}.to_string())))",
            field = col.field_name,
        );
    } else {
        let item_expr = format!("item.{}{}", col.field_name, clone_suffix);
        let typed_value = typed_value_expr(&col.rust_type, &item_expr);
        w!(code, ", {}", typed_value);
    }
}

/// Flatten a nested child table tree into a depth-first ordered list.
/// Each entry retains its correct `parent_fk_column` and `sql_table_name`.
pub(crate) fn flatten_child_tables(children: &[ChildTableInfo]) -> Vec<&ChildTableInfo> {
    let mut result = Vec::new();
    for child in children {
        result.push(child);
        result.extend(flatten_child_tables(&child.child_tables));
    }
    result
}

/// Build the parameterized INSERT statement text for a child-table row.
/// Shared by `emit_child_inserts` (create) and the update replace path in
/// `emit_update_body`; both previously duplicated this if/else verbatim.
pub(crate) fn child_insert_sql(
    child: &ChildTableInfo,
    col_names: &str,
    placeholders: &str,
) -> String {
    if col_names.is_empty() {
        format!(
            "INSERT INTO {}.{} (id, {}) VALUES ($1, $2)",
            child.sql_schema_name,
            q(&child.sql_table_name),
            child.parent_fk_column,
        )
    } else {
        format!(
            "INSERT INTO {}.{} (id, {}, {}) VALUES ($1, $2, {})",
            child.sql_schema_name,
            q(&child.sql_table_name),
            child.parent_fk_column,
            col_names,
            placeholders,
        )
    }
}

/// Emit the shared tail of the child INSERT branches: the per-row id, the
/// INSERT statement, and recursion into grandchildren. The array and
/// optional branches differ only in how iteration is opened, so both
/// delegate here.
#[allow(clippy::too_many_arguments)]
fn emit_child_insert_body(
    code: &mut CodeWriter,
    child: &ChildTableInfo,
    sql: &str,
    row_id_var: &str,
    parent_id_var: &str,
    indent: usize,
    pad: &str,
) {
    wln!(code, "{pad}    let {row_id_var} = Uuid::new_v4();");
    wln!(code, "{pad}    let stmt = Statement::from_sql_and_values(");
    wln!(code, "{pad}        DatabaseBackend::Postgres,");
    wln!(code, "{pad}        \"{sql}\",", sql = sql);
    w!(
        code,
        "{pad}        vec![{row_id_var}.into(), {parent_id_var}.into()"
    );
    for col in &child.columns {
        emit_child_col_write_value(code, col);
    }
    wln!(code, "],");
    wln!(code, "{pad}    );");
    wln!(code, "{pad}    tx.execute(stmt).await?;");
    emit_child_inserts(code, &child.child_tables, row_id_var, "item", indent + 1);
    wln!(code, "{pad}}}");
}

/// Recursively emit INSERT statements for child tables and their nested children.
///
/// * `parent_id_var` — Rust variable name holding the parent's UUID (e.g. `"id"`, `"child_id"`)
/// * `item_accessor` — DTO access prefix (e.g. `"cmd"`, `"item"`)
/// * `indent` — indentation level (number of 4-space units)
pub(crate) fn emit_child_inserts(
    code: &mut CodeWriter,
    children: &[ChildTableInfo],
    parent_id_var: &str,
    item_accessor: &str,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    for child in children {
        // Skip child tables with no data columns — nothing meaningful to insert
        if child.columns.is_empty() && child.child_tables.is_empty() {
            continue;
        }

        wln!(code);
        let col_names: Vec<String> = child.columns.iter().map(|c| q(&c.pg_column_name)).collect();
        let placeholders: Vec<String> = child
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                if let Some(ref cast) = col.pg_cast {
                    if crate::is_geometry_cast(cast) {
                        format!("ST_GeomFromGeoJSON(${})", i + 3)
                    } else {
                        format!("${}::{}", i + 3, cast)
                    }
                } else {
                    format!("${}", i + 3)
                }
            })
            .collect();
        let sql = child_insert_sql(child, &col_names.join(", "), &placeholders.join(", "));

        // Use a unique variable name for this level's row ID to prevent
        // shadowing when grandchild inserts reference the parent FK.
        let row_id_var = format!("child_id_{}", child.sql_table_name.replace('.', "_"));

        if child.is_array {
            wln!(
                code,
                "{pad}// Insert child rows: {}.{}",
                child.sql_schema_name,
                child.sql_table_name
            );
            let item_var = if child.columns.is_empty() && child.child_tables.is_empty() {
                "_item"
            } else {
                "item"
            };
            wln!(
                code,
                "{pad}for {item_var} in &{item_accessor}.{field} {{",
                field = child.field_name
            );
            emit_child_insert_body(code, child, &sql, &row_id_var, parent_id_var, indent, &pad);
        } else {
            wln!(
                code,
                "{pad}// Insert optional child row: {}.{}",
                child.sql_schema_name,
                child.sql_table_name
            );
            let item_var = if child.columns.is_empty() && child.child_tables.is_empty() {
                "_item"
            } else {
                "item"
            };
            wln!(
                code,
                "{pad}if let Some(ref {item_var}) = {item_accessor}.{field} {{",
                field = child.field_name
            );
            emit_child_insert_body(code, child, &sql, &row_id_var, parent_id_var, indent, &pad);
        }
    }
}

/// Recursively emit SELECT + Response-building code for child tables.
///
/// For each child table, emits code that:
/// 1. Queries child rows by parent FK
/// 2. For each child row, recursively queries nested grandchild tables
/// 3. Builds the child Response struct including nested children
///
/// * `parent_id_expr` — Rust expression for the parent row's ID (e.g. `"id"`, `"row.id"`)
/// * `indent` — indentation level (number of 4-space units)
pub(crate) fn emit_child_reads(
    code: &mut CodeWriter,
    children: &[ChildTableInfo],
    parent_id_expr: &str,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    for child in children {
        wln!(code);

        // Child tables with no data columns and no nested children — emit empty vec.
        if child.columns.is_empty() && child.child_tables.is_empty() {
            wln!(
                code,
                "{pad}let {field}_rows: Vec<{struct_name}Response> = Vec::new();",
                field = child.field_name,
                struct_name = child.struct_name,
            );
            continue;
        }

        // Child tables with no data columns but with nested children —
        // still need to query for ids to resolve grandchildren.
        let col_names: Vec<String> = child
            .columns
            .iter()
            .map(|c| {
                if let Some(ref cast) = c.pg_cast {
                    if crate::is_geometry_cast(cast) {
                        format!(
                            "ST_AsGeoJSON({})::text AS {}",
                            q(&c.pg_column_name),
                            q(&c.pg_column_name)
                        )
                    } else {
                        format!(
                            "{}::{} AS {}",
                            q(&c.pg_column_name),
                            cast,
                            q(&c.pg_column_name)
                        )
                    }
                } else {
                    q(&c.pg_column_name)
                }
            })
            .collect();
        let select_cols = if col_names.is_empty() {
            "id".to_string()
        } else {
            format!("id, {}", col_names.join(", "))
        };
        let select_sql = format!(
            "SELECT {} FROM {}.{} WHERE {} = $1 ORDER BY created_at",
            select_cols,
            child.sql_schema_name,
            q(&child.sql_table_name),
            child.parent_fk_column,
        );

        wln!(code, "{pad}let {field}_rows = {{", field = child.field_name);
        wln!(code, "{pad}    let stmt = Statement::from_sql_and_values(");
        wln!(code, "{pad}        DatabaseBackend::Postgres,");
        wln!(code, "{pad}        \"{}\",", select_sql);
        wln!(code, "{pad}        vec![{parent_id_expr}.into()],");
        wln!(code, "{pad}    );");
        wln!(code, "{pad}    let rows = db.query_all(stmt).await?;");
        wln!(
            code,
            "{pad}    let mut items = Vec::with_capacity(rows.len());"
        );
        let child_row_var = if child.columns.is_empty() && child.child_tables.is_empty() {
            "_child_row"
        } else {
            "child_row"
        };
        wln!(code, "{pad}    for {} in &rows {{", child_row_var);
        if !child.columns.is_empty() || !child.child_tables.is_empty() {
            wln!(code, "{pad}        use sea_orm::TryGetable;");
        }

        // If there are nested children, extract the child row's id for sub-queries.
        // Only emit child_row_id when at least one grandchild actually needs
        // a parent-id query (has columns or its own children). Grandchildren
        // with neither are emitted as `Vec::new()` and never consume the id.
        if !child.child_tables.is_empty() {
            let needs_id = child
                .child_tables
                .iter()
                .any(|gc| !gc.columns.is_empty() || !gc.child_tables.is_empty());
            if needs_id {
                wln!(
                    code,
                    "{pad}        let child_row_id: Uuid = Uuid::try_get_by(child_row, \"id\").map_err(|e| format!(\"{{e:?}}\"))?;"
                );
            }
            // Recursively query nested grandchild tables using child_row_id.
            emit_child_reads(code, &child.child_tables, "child_row_id", indent + 2);
        }

        wln!(
            code,
            "{pad}        items.push({}Response {{",
            child.struct_name
        );
        for col in &child.columns {
            emit_child_col_read_value(code, col, &format!("{pad}            "));
        }
        // Wire nested children into the response struct.
        emit_child_field_population(code, &child.child_tables, &format!("{pad}            "));
        // DTO fields the child columns do not cover (e.g. scalar `*_id` mirrors
        // of array entity-ref properties that the DDL stores as junction
        // tables) default to None instead of failing E0063.
        wln!(code, "{pad}        ..Default::default()");
        wln!(code, "{pad}        }});");
        wln!(code, "{pad}    }}");
        wln!(code, "{pad}    items");
        wln!(code, "{pad}}};");
    }
}

/// Nested children of the source-tree child subtree matching a VO→entity
/// child-table override.
///
/// The override fetch queries the VO child table directly (e.g.
/// `worker_person`) and builds the scoped response (e.g.
/// `WorkerPersonLegalResponse`). Its nested children must hydrate from the
/// same subtree the main response hydration uses, so struct names align with
/// the scoped prefix by construction (`WorkerPersonLegal*`).
fn override_subtree_children<'a>(
    tree: &'a EntityTree,
    over: &crate::api::include_path::ChildTableOverride,
) -> &'a [ChildTableInfo] {
    tree.child_tables
        .iter()
        .find(|c| c.sql_table_name == over.child_table_name)
        .map(|c| c.child_tables.as_slice())
        .unwrap_or(&[])
}

/// Which child tables an include-fetch response should hydrate.
///
/// Two cases hydrate (everything else yields no children):
/// - `child_table_override` (VO→entity): the SOURCE tree's matching subtree's
///   nested children — struct names align with the scoped response prefix
///   (`WorkerPersonLegal*`) by construction (#162 phase 2).
/// - entity-native responses (`{Target}Response`): the TARGET tree's own
///   child tables (#161).
pub(crate) fn include_hydration_children<'a>(
    tree: &'a EntityTree,
    path: &ResolvedIncludePath,
    target_tree: Option<&'a EntityTree>,
) -> &'a [ChildTableInfo] {
    let seg = &path.segments[0];
    if let Some(ref over) = seg.child_table_override {
        return override_subtree_children(tree, over);
    }
    let entity_native = path.response_rust_type == format!("{}Response", seg.entity_name);
    match target_tree {
        Some(ttree) if entity_native => &ttree.child_tables,
        _ => &[],
    }
}

/// Emit a single child column read expression for response struct construction.
fn emit_child_col_read_value(code: &mut CodeWriter, col: &ChildColumn, pad: &str) {
    if col.dto_rust_type.is_some() {
        if col.is_nullable {
            wln!(
                code,
                "{pad}{field}: Option::<String>::try_get_by(child_row, \"{pg}\").ok().flatten().and_then(|v| v.parse().ok()),",
                field = col.field_name,
                pg = col.pg_column_name,
            );
        } else {
            wln!(
                code,
                "{pad}{field}: String::try_get_by(child_row, \"{pg}\").map_err(|e| format!(\"{{e:?}}\"))?.parse().unwrap_or_default(),",
                field = col.field_name,
                pg = col.pg_column_name,
            );
        }
    } else if col.is_nullable {
        wln!(
            code,
            "{pad}{field}: Option::<{typ}>::try_get_by(child_row, \"{pg}\").ok().flatten(),",
            field = col.field_name,
            typ = col.rust_type,
            pg = col.pg_column_name,
        );
    } else {
        wln!(
            code,
            "{pad}{field}: {typ}::try_get_by(child_row, \"{pg}\").map_err(|e| format!(\"{{e:?}}\"))?,",
            field = col.field_name,
            typ = turbofish(&col.rust_type),
            pg = col.pg_column_name,
        );
    }
}
