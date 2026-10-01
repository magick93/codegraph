use super::types::{
    CheckConstraint, ChildTableDef, ColumnComment, ColumnDef, DdlContext, ForeignKeyDef, IndexDef,
};

/// Mutable DDL artifact accumulators shared by the `query_ddl_context` phases.
pub(super) struct DdlAccumulators {
    pub(super) columns: Vec<ColumnDef>,
    pub(super) foreign_keys: Vec<ForeignKeyDef>,
    pub(super) check_constraints: Vec<CheckConstraint>,
    pub(super) comments: Vec<ColumnComment>,
    pub(super) indexes: Vec<IndexDef>,
}

impl DdlAccumulators {
    pub(super) fn new(root: &codegraph_core::types::CompositionNode) -> Self {
        let mut columns = vec![ColumnDef {
            name: "id".to_string(),
            pg_type: "UUID".to_string(),
            nullable: false,
            default: Some("gen_random_uuid()".to_string()),
            is_primary_key: true,
            is_array: false,
        }];

        // Emit composite range column if present (already resolved on the tree node)
        if let Some(ref range) = root.composite_range {
            columns.push(ColumnDef {
                name: range.pg_column_name.clone(),
                pg_type: range.pg_type.clone(),
                nullable: true,
                default: None,
                is_primary_key: false,
                is_array: false,
            });
        }

        Self {
            columns,
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            comments: Vec::new(),
            indexes: Vec::new(),
        }
    }
}

/// Quote PostgreSQL reserved-word column names throughout a `DdlContext`.
///
/// Applies `codegraph_naming::quote_pg_column` to all column names, FK column references,
/// comment column references, and check constraint expressions so that reserved
/// words like `order`, `start`, `end` are rendered as `"order"`, `"start"`, `"end"`.
pub(super) fn quote_ddl_identifiers(ctx: &mut DdlContext) {
    quote_columns(&mut ctx.columns);
    quote_fks(&mut ctx.foreign_keys);
    quote_comments(&mut ctx.comments);
    quote_checks(&mut ctx.check_constraints);
    quote_child_tables(&mut ctx.child_tables);
}

fn quote_columns(columns: &mut [ColumnDef]) {
    for col in columns {
        col.name = codegraph_naming::quote_pg_column(&col.name);
    }
}

fn quote_fks(fks: &mut [ForeignKeyDef]) {
    for fk in fks {
        fk.column = codegraph_naming::quote_pg_column(&fk.column);
    }
}

fn quote_comments(comments: &mut [ColumnComment]) {
    for c in comments {
        c.column = codegraph_naming::quote_pg_column(&c.column);
    }
}

fn quote_checks(checks: &mut [CheckConstraint]) {
    for chk in checks {
        let quoted = codegraph_naming::quote_pg_column(&chk.column);
        if quoted != chk.column {
            chk.expression = chk.expression.replace(&chk.column, &quoted);
        }
        chk.column = quoted;
    }
}

fn quote_child_tables(children: &mut [ChildTableDef]) {
    for child in children {
        quote_columns(&mut child.columns);
        quote_fks(&mut child.foreign_keys);
        quote_comments(&mut child.comments);
        quote_checks(&mut child.check_constraints);
        quote_child_tables(&mut child.child_tables);
    }
}
