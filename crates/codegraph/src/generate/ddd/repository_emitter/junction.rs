use crate::generate::code_writer::{wln, CodeWriter};

use super::helpers::q;
use super::JunctionTableInfo;

/// Build the junction INSERT statement text (shared by insert and replace paths).
fn junction_insert_sql(j: &JunctionTableInfo) -> String {
    format!(
        "INSERT INTO {}.{} ({}, {}) VALUES ($1, $2)",
        j.sql_schema_name,
        q(&j.sql_table_name),
        q(&j.parent_fk_column),
        q(&j.child_fk_column),
    )
}

/// Emit junction (many-to-many) INSERT statements for array-of-entity-ref
/// properties. The DTO carries `Vec<uuid::Uuid>` (required) or
/// `Option<Vec<uuid::Uuid>>` (optional).
pub(crate) fn emit_junction_inserts(
    code: &mut CodeWriter,
    junctions: &[JunctionTableInfo],
    parent_id_var: &str,
    item_accessor: &str,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    for j in junctions {
        wln!(code);
        wln!(
            code,
            "{pad}// Insert junction rows: {}.{}",
            j.sql_schema_name,
            j.sql_table_name
        );
        let sql = junction_insert_sql(j);
        if j.is_required {
            wln!(
                code,
                "{pad}for item in &{item_accessor}.{field} {{",
                field = j.field_name
            );
            wln!(code, "{pad}    let stmt = Statement::from_sql_and_values(");
            wln!(code, "{pad}        DatabaseBackend::Postgres,");
            wln!(code, "{pad}        \"{sql}\",");
            wln!(
                code,
                "{pad}        vec![{parent_id_var}.into(), (*item).into()],"
            );
            wln!(code, "{pad}    );");
            wln!(code, "{pad}    tx.execute(stmt).await?;");
            wln!(code, "{pad}}}");
        } else {
            wln!(
                code,
                "{pad}if let Some(ref ids) = {item_accessor}.{field} {{",
                field = j.field_name
            );
            wln!(code, "{pad}    for item in ids {{");
            wln!(
                code,
                "{pad}        let stmt = Statement::from_sql_and_values("
            );
            wln!(code, "{pad}            DatabaseBackend::Postgres,");
            wln!(code, "{pad}            \"{sql}\",");
            wln!(
                code,
                "{pad}            vec![{parent_id_var}.into(), (*item).into()],"
            );
            wln!(code, "{pad}        );");
            wln!(code, "{pad}        tx.execute(stmt).await?;");
            wln!(code, "{pad}    }}");
            wln!(code, "{pad}}}");
        }
    }
}

/// Emit junction replace logic for UPDATE: when the update request carries the
/// field, delete existing junction rows and re-insert the supplied ids.
pub(crate) fn emit_junction_replace(
    code: &mut CodeWriter,
    junctions: &[JunctionTableInfo],
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    for j in junctions {
        wln!(code);
        wln!(
            code,
            "{pad}// Replace junction rows: {}.{}",
            j.sql_schema_name,
            j.sql_table_name
        );
        wln!(
            code,
            "{pad}if let Some(ref ids) = cmd.{field} {{",
            field = j.field_name
        );
        let del_sql = format!(
            "DELETE FROM {}.{} WHERE {} = $1",
            j.sql_schema_name,
            q(&j.sql_table_name),
            q(&j.parent_fk_column),
        );
        wln!(code, "{pad}    let stmt = Statement::from_sql_and_values(");
        wln!(code, "{pad}        DatabaseBackend::Postgres,");
        wln!(code, "{pad}        \"{del_sql}\",");
        wln!(code, "{pad}        vec![id.into()],");
        wln!(code, "{pad}    );");
        wln!(code, "{pad}    tx.execute(stmt).await?;");
        let ins_sql = junction_insert_sql(j);
        wln!(code, "{pad}    for item in ids {{");
        wln!(
            code,
            "{pad}        let stmt = Statement::from_sql_and_values("
        );
        wln!(code, "{pad}            DatabaseBackend::Postgres,");
        wln!(code, "{pad}            \"{ins_sql}\",");
        wln!(code, "{pad}            vec![id.into(), (*item).into()],");
        wln!(code, "{pad}        );");
        wln!(code, "{pad}        tx.execute(stmt).await?;");
        wln!(code, "{pad}    }}");
        wln!(code, "{pad}}}");
    }
}

/// Emit junction SELECT statements for response construction: fetch child ids
/// for the parent row and expose them as `Vec<Uuid>` variables named
/// `<field>_rows`.
pub(crate) fn emit_junction_reads(
    code: &mut CodeWriter,
    junctions: &[JunctionTableInfo],
    parent_id_expr: &str,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    for j in junctions {
        wln!(code);
        let select_sql = format!(
            "SELECT {} FROM {}.{} WHERE {} = $1 ORDER BY created_at",
            q(&j.child_fk_column),
            j.sql_schema_name,
            q(&j.sql_table_name),
            q(&j.parent_fk_column),
        );
        wln!(code, "{pad}let {field}_rows = {{", field = j.field_name);
        wln!(code, "{pad}    let stmt = Statement::from_sql_and_values(");
        wln!(code, "{pad}        DatabaseBackend::Postgres,");
        wln!(code, "{pad}        \"{select_sql}\",");
        wln!(code, "{pad}        vec![{parent_id_expr}.into()],");
        wln!(code, "{pad}    );");
        wln!(code, "{pad}    let rows = db.query_all(stmt).await?;");
        wln!(
            code,
            "{pad}    let mut items = Vec::with_capacity(rows.len());"
        );
        wln!(code, "{pad}    for row in &rows {{");
        wln!(code, "{pad}        use sea_orm::TryGetable;");
        wln!(
            code,
            "{pad}        let v: Uuid = Uuid::try_get_by(row, \"{}\").map_err(|e| format!(\"{{e:?}}\"))?;",
            j.child_fk_column
        );
        wln!(code, "{pad}        items.push(v);");
        wln!(code, "{pad}    }}");
        wln!(code, "{pad}    items");
        wln!(code, "{pad}}};");
    }
}

/// Populate junction fields into the response struct construction.
pub(crate) fn emit_junction_field_population(
    code: &mut CodeWriter,
    junctions: &[JunctionTableInfo],
    pad: &str,
) {
    for j in junctions {
        if j.is_required {
            wln!(code, "{pad}{field}: {field}_rows,", field = j.field_name);
        } else {
            wln!(
                code,
                "{pad}{field}: Some({field}_rows),",
                field = j.field_name
            );
        }
    }
}
