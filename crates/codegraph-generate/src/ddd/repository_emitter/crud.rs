use crate::code_writer::{w, wln, CodeWriter};

use super::child::{child_insert_sql, emit_child_col_write_value, emit_child_inserts};
use super::dto::emit_response_construction;
use super::helpers::{
    is_copy_type, is_vec_string, is_vec_type, null_value_for_type, q, typed_value_expr,
    vec_array_type_and_ctor,
};
use super::junction::{emit_junction_inserts, emit_junction_replace};
use super::{EntityTree, RepositoryImplEmitter, TreeColumn};

/// Which CRUD operation is being emitted.
/// Centralises column filtering and DTO field name resolution
/// so all paths stay consistent — adding a new variant forces
/// defining its filter and field name at compile time.
#[derive(Clone, Copy)]
enum CrudOp {
    CreateActiveModel,
    CreateRawSql,
    Update,
}

impl CrudOp {
    fn columns<'a>(&self, tree: &'a EntityTree) -> Vec<&'a TreeColumn> {
        match self {
            CrudOp::CreateActiveModel => tree
                .direct_columns
                .iter()
                .filter(|c| !c.is_workflow_managed && !c.is_composite_range && !c.is_media)
                .filter(|c| !Self::is_parent_fk(c, tree))
                .collect(),
            CrudOp::CreateRawSql => tree
                .direct_columns
                .iter()
                .filter(|c| !c.is_workflow_managed && !c.is_composite_range && !c.is_media)
                .filter(|c| !Self::is_parent_fk(c, tree))
                .collect(),
            CrudOp::Update => tree
                .direct_columns
                .iter()
                .filter(|c| !c.is_workflow_managed && !c.is_media && !c.is_composite_range)
                .filter(|c| c.pg_cast.is_none())
                .collect(),
        }
    }

    fn is_parent_fk(c: &TreeColumn, tree: &EntityTree) -> bool {
        tree.parent_ref.as_ref().is_some_and(|pr| {
            c.field_name.eq_ignore_ascii_case(pr)
                || c.pg_column_name.eq_ignore_ascii_case(pr)
                || c.pg_column_name == format!("{}_id", codegraph_naming::to_snake_case(pr))
        })
    }
}

impl RepositoryImplEmitter {
    pub(crate) fn emit_create_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        // Build the body first so we can detect whether the request parameter
        // is actually referenced (flat entities emit an insert that ignores it).
        let mut body = CodeWriter::new();
        let has_range_cols = tree
            .direct_columns
            .iter()
            .any(|c| c.pg_cast.is_some() && !c.is_composite_range);

        // Dispatch to CrudOp::CreateActiveModel or CrudOp::CreateRawSql
        if has_range_cols {
            // Use raw SQL INSERT so range parameters get explicit casts
            self.emit_create_raw_sql(tree, &mut body);
        } else {
            // Use SeaORM ActiveModel insert (no range columns)
            self.emit_create_active_model(tree, &mut body);
        }

        // Insert child table rows (recursively handles nested children).
        emit_child_inserts(&mut body, &tree.child_tables, "id", "cmd", 2);

        // Insert junction rows (many-to-many array-of-entity-ref fields).
        emit_junction_inserts(&mut body, &tree.junction_tables, "id", "cmd", 2);

        let cmd_ident = if body.as_str().contains("cmd.") {
            "cmd"
        } else {
            "_cmd"
        };

        wln!(
            code,
            "    #[tracing::instrument(skip(self, tx), fields(db.operation = \"insert\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn create(");
        wln!(code, "        &self,");
        wln!(code, "        tx: &DatabaseTransaction,");
        wln!(
            code,
            "        {cmd_ident}: Create{}Request,",
            tree.entity_name
        );
        if tree.parent_ref.is_some() {
            wln!(code, "        parent_id: Uuid,");
        }
        wln!(code, "    ) -> Result<Uuid, Box<dyn std::error::Error>> {{");
        wln!(code, "        let id = Uuid::new_v4();");
        wln!(code);
        code.push_str(body.as_str());
        wln!(code);
        wln!(code, "        Ok(id)");
        wln!(code, "    }}");
    }

    /// Emit parent entity INSERT using SeaORM ActiveModel (no range columns).
    fn emit_create_active_model(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(
            code,
            "        // Insert into {}.{} (direct columns)",
            tree.schema_name,
            tree.table_name
        );
        wln!(
            code,
            "        let model = crate::entity::{}::ActiveModel {{",
            tree.entity_module
        );
        wln!(code, "            id: Set(id),");
        if let Some(ref parent_ref) = tree.parent_ref {
            let fk_field = codegraph_naming::to_snake_case(parent_ref);
            wln!(code, "            {fk_field}: Set(Some(parent_id)),");
        }
        let op = CrudOp::CreateActiveModel;
        for col in op.columns(tree) {
            let entity_field = &col.field_name;
            let dto_field = col.dto_name();
            // StructuredWrapper must take priority — serde_json::to_value(), not .to_string().
            if col.is_structured_wrapper {
                // StructuredWrapper (scalar or array): DTO has typed struct/Vec, entity needs JSONB.
                if col.is_nullable {
                    if col.is_array {
                        wln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null))),",
                        );
                    } else {
                        wln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.as_ref().and_then(|v| serde_json::to_value(v).ok())),",
                        );
                    }
                } else {
                    wln!(
                        code,
                        "            {entity_field}: Set(serde_json::to_value(cmd.{dto_field}).unwrap_or(serde_json::Value::Null)),",
                    );
                }
            } else if col.dto_rust_type.is_some() {
                if col.is_array {
                    // Vec<CodelistEnum> → Vec<String>
                    if col.is_nullable {
                        wln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.map(|v| v.into_iter().map(|x| x.to_string()).collect())),",
                        );
                    } else {
                        wln!(
                            code,
                            "            {entity_field}: Set(cmd.{dto_field}.into_iter().map(|v| v.to_string()).collect()),",
                        );
                    }
                } else if col.is_nullable {
                    wln!(
                        code,
                        "            {entity_field}: Set(cmd.{dto_field}.map(|v| v.to_string())),",
                    );
                } else {
                    wln!(
                        code,
                        "            {entity_field}: Set(cmd.{dto_field}.to_string()),",
                    );
                }
            } else {
                wln!(
                    code,
                    "            {}: Set(cmd.{}),",
                    entity_field,
                    dto_field
                );
            }
        }
        wln!(code, "            ..Default::default()");
        wln!(code, "        }};");
        wln!(code, "        model.insert(tx).await?;");
    }

    /// Emit parent entity INSERT using raw SQL with explicit range casts.
    fn emit_create_raw_sql(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(
            code,
            "        // Insert into {}.{} via raw SQL (range columns need explicit casts)",
            tree.schema_name,
            tree.table_name
        );

        // Collect non-workflow columns for the INSERT (exclude composite range
        // columns which exist in DDL but not on DTOs, and exclude parent FK
        // for child entities since it's already set from the route)
        let op = CrudOp::CreateRawSql;
        let insert_cols = op.columns(tree);

        // Build column names: id + optional FK + direct columns (use PG column names for SQL, quoted)
        let mut col_names = vec!["id".to_string()];
        let mut placeholders = vec!["$1".to_string()];
        let mut param_offset = 2usize;
        if let Some(ref parent_ref) = tree.parent_ref {
            col_names.push(q(parent_ref));
            placeholders.push(format!("${param_offset}"));
            param_offset += 1;
        }
        for col in &insert_cols {
            col_names.push(q(&col.pg_column_name));
        }

        // Build placeholders for direct columns with optional casts
        for (i, col) in insert_cols.iter().enumerate() {
            let idx = i + param_offset;
            if let Some(ref cast) = col.pg_cast {
                if crate::is_geometry_cast(cast) {
                    placeholders.push(format!("ST_GeomFromGeoJSON(${idx})"));
                } else {
                    placeholders.push(format!("${idx}::{cast}"));
                }
            } else {
                placeholders.push(format!("${idx}"));
            }
        }

        let sql = format!(
            "INSERT INTO {}.{} ({}) VALUES ({})",
            tree.schema_name,
            q(&tree.table_name),
            col_names.join(", "),
            placeholders.join(", "),
        );

        wln!(code, "        let stmt = Statement::from_sql_and_values(");
        wln!(code, "            DatabaseBackend::Postgres,");
        wln!(code, "            \"{}\",", sql);
        w!(code, "            vec![id.into()");
        if tree.parent_ref.is_some() {
            w!(code, ", parent_id.into()");
        }

        for col in &insert_cols {
            let dto_field = col.dto_name();
            let has_enum = col.dto_rust_type.is_some();
            let clone_suffix = if is_copy_type(&col.rust_type) {
                ""
            } else {
                ".clone()"
            };

            if col.is_structured_wrapper {
                // StructuredWrapper (scalar or array): serialize to JSONB Value.
                if col.is_nullable {
                    w!(
                        code,
                        ", cmd.{dto_field}.as_ref().and_then(|v| serde_json::to_value(v).ok().map(|j| sea_orm::Value::Json(Some(Box::new(j))))).unwrap_or(sea_orm::Value::Json(None))",
                    );
                } else {
                    w!(
                        code,
                        ", sea_orm::Value::Json(Some(Box::new(serde_json::to_value(&cmd.{dto_field}).unwrap_or_default())))",
                    );
                }
            } else if col.is_nullable {
                if is_vec_string(&col.rust_type) || (is_vec_type(&col.rust_type) && has_enum) {
                    let map_fn = if is_vec_string(&col.rust_type) {
                        "s"
                    } else {
                        "s.to_string()"
                    };
                    w!(
                        code,
                        ", cmd.{dto_field}.clone().map(|v| sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(v.into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))).unwrap_or({null})",
                        null = null_value_for_type("Vec<String>"),
                    );
                } else if is_vec_type(&col.rust_type) {
                    let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
                    w!(
                        code,
                        ", cmd.{dto_field}.clone().map(|v| sea_orm::Value::Array({array_type}, Some(Box::new(v.into_iter().map(|s| {value_ctor}).collect())))).unwrap_or(sea_orm::Value::Array({array_type}, None))",
                    );
                } else if has_enum {
                    w!(
                        code,
                        ", cmd.{dto_field}.as_ref().map(|v| sea_orm::Value::String(Some(Box::new(v.to_string())))).unwrap_or({null})",
                        null = null_value_for_type(&col.rust_type),
                    );
                } else {
                    let typed_value = typed_value_expr(&col.rust_type, "v");
                    w!(
                        code,
                        ", cmd.{dto_field}{clone_suffix}.map(|v| {typed_value}).unwrap_or({null})",
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
                    ", sea_orm::Value::Array(sea_orm::sea_query::ArrayType::String, Some(Box::new(cmd.{dto_field}.clone().into_iter().map(|s| sea_orm::Value::String(Some(Box::new({map_fn})))).collect())))",
                );
            } else if is_vec_type(&col.rust_type) {
                let (array_type, value_ctor) = vec_array_type_and_ctor(&col.rust_type);
                w!(
                    code,
                    ", sea_orm::Value::Array({array_type}, Some(Box::new(cmd.{dto_field}.clone().into_iter().map(|s| {value_ctor}).collect())))",
                );
            } else if has_enum {
                w!(
                    code,
                    ", sea_orm::Value::String(Some(Box::new(cmd.{dto_field}.to_string())))",
                );
            } else {
                let item_expr = format!("cmd.{dto_field}{clone_suffix}");
                let typed_value = typed_value_expr(&col.rust_type, &item_expr);
                w!(code, ", {typed_value}");
            }
        }

        wln!(code, "],");
        wln!(code, "        );");
        wln!(code, "        tx.execute(stmt).await?;");
    }

    pub(crate) fn emit_find_by_id_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(code);
        wln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"select\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn find_by_id(");
        wln!(code, "        &self,");
        wln!(code, "        db: &DatabaseTransaction,");
        wln!(code, "        id: Uuid,");
        if tree.is_auditable {
            wln!(code, "        include_deleted: bool,");
        }
        wln!(
            code,
            "    ) -> Result<Option<{}Response>, Box<dyn std::error::Error>> {{",
            tree.entity_name
        );
        if tree.is_auditable {
            wln!(
                code,
                "        let mut query = crate::entity::{}::Entity::find()",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(id));",
                tree.entity_module
            );
            wln!(code);
            wln!(code, "        if !include_deleted {{");
            wln!(
                code,
                "            query = query.filter(crate::entity::{}::Column::DeletedAt.is_null());",
                tree.entity_module
            );
            wln!(code, "        }}");
            wln!(code);
            wln!(code, "        let row = query.one(db)");
            wln!(code, "            .await?;");
        } else {
            wln!(
                code,
                // Use find().filter() instead of find_by_id() because SeaORM's
                // find_by_id() requires the primary key type to impl Into<Value>,
                // which fails for composite keys and custom ID wrappers.
                "        let row = crate::entity::{}::Entity::find()",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(id))",
                tree.entity_module
            );
            wln!(code, "            .one(db)");
            wln!(code, "            .await?;");
        }
        wln!(code);
        wln!(code, "        let row = match row {{");
        wln!(code, "            Some(r) => r,");
        wln!(code, "            None => return Ok(None),");
        wln!(code, "        }};");
        emit_response_construction(code, tree);
    }

    /// Emit `find_by_id_scoped` — same as `find_by_id` but adds a parent FK filter.
    pub(crate) fn emit_find_by_id_scoped_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        let parent_ref = tree.parent_ref.as_deref().unwrap();
        let pascal_col = codegraph_naming::to_pascal_case(parent_ref);
        wln!(code);
        wln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"select_scoped\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn find_by_id_scoped(");
        wln!(code, "        &self,");
        wln!(code, "        db: &DatabaseTransaction,");
        wln!(code, "        id: Uuid,");
        wln!(code, "        parent_id: Uuid,");
        if tree.is_auditable {
            wln!(code, "        include_deleted: bool,");
        }
        wln!(
            code,
            "    ) -> Result<Option<{}Response>, Box<dyn std::error::Error>> {{",
            tree.entity_name
        );
        if tree.is_auditable {
            wln!(
                code,
                "        let mut query = crate::entity::{}::Entity::find()",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(id))",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::{}.eq(parent_id));",
                tree.entity_module,
                pascal_col
            );
            wln!(code);
            wln!(code, "        if !include_deleted {{");
            wln!(
                code,
                "            query = query.filter(crate::entity::{}::Column::DeletedAt.is_null());",
                tree.entity_module
            );
            wln!(code, "        }}");
            wln!(code);
            wln!(code, "        let row = query.one(db)");
            wln!(code, "            .await?;");
        } else {
            wln!(
                code,
                "        let row = crate::entity::{}::Entity::find()",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(id))",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::{}.eq(parent_id))",
                tree.entity_module,
                pascal_col
            );
            wln!(code, "            .one(db)");
            wln!(code, "            .await?;");
        }
        wln!(code);
        wln!(code, "        let row = match row {{");
        wln!(code, "            Some(r) => r,");
        wln!(code, "            None => return Ok(None),");
        wln!(code, "        }};");

        emit_response_construction(code, tree);
    }

    pub(crate) fn emit_update_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        // Build the body first so we can detect whether the request parameter
        // is actually referenced (flat entities emit an update that ignores it).
        let mut body = CodeWriter::new();
        self.emit_update_body(tree, &mut body);
        let cmd_ident = if body.as_str().contains("cmd.") {
            "cmd"
        } else {
            "_cmd"
        };

        wln!(code);
        wln!(
            code,
            "    #[tracing::instrument(skip(self, tx), fields(db.operation = \"update\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn update(");
        wln!(code, "        &self,");
        wln!(code, "        tx: &DatabaseTransaction,");
        wln!(code, "        id: Uuid,");
        wln!(
            code,
            "        {cmd_ident}: Update{}Request,",
            tree.entity_name
        );
        wln!(code, "    ) -> Result<(), Box<dyn std::error::Error>> {{");
        code.push_str(body.as_str());
        wln!(code);
        wln!(code, "        Ok(())");
        wln!(code, "    }}");
    }

    fn emit_update_body(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(
            code,
            "        // Update {}.{} — only set fields present in the update request",
            tree.schema_name,
            tree.table_name
        );
        let has_updatable_cols = tree.direct_columns.iter().any(|c| {
            !c.is_workflow_managed && !c.is_composite_range && !c.is_media && c.pg_cast.is_none()
        });
        let mut_kw = if has_updatable_cols { "mut " } else { "" };
        wln!(
            code,
            "        let {}model = crate::entity::{}::ActiveModel {{",
            mut_kw,
            tree.entity_module
        );
        wln!(code, "            id: Set(id),");
        wln!(code, "            ..Default::default()");
        wln!(code, "        }};");
        let op = CrudOp::Update;
        for col in op.columns(tree) {
            let entity_field = &col.field_name;
            if col.is_structured_wrapper {
                // StructuredWrapper (scalar or array): serialize to JSONB.
                if col.is_nullable {
                    if col.is_array {
                        wln!(
                            code,
                            "        if let Some(v) = cmd.{entity_field} {{ model.{entity_field} = Set(Some(serde_json::to_value(v).unwrap_or(serde_json::Value::Null))); }}",
                        );
                    } else {
                        wln!(
                            code,
                            "        if let Some(v) = cmd.{entity_field} {{ model.{entity_field} = Set(serde_json::to_value(v).ok()); }}",
                        );
                    }
                } else {
                    wln!(
                        code,
                        "        if let Some(v) = cmd.{entity_field} {{ model.{entity_field} = Set(serde_json::to_value(v).unwrap_or(serde_json::Value::Null)); }}",
                    );
                }
            } else {
                let value_expr = if col.dto_rust_type.is_some() && col.is_array {
                    "v.into_iter().map(|x| x.to_string()).collect()"
                } else if col.dto_rust_type.is_some() {
                    "v.to_string()"
                } else {
                    "v"
                };
                if col.is_nullable {
                    wln!(
                        code,
                        "        if let Some(v) = cmd.{entity_field} {{ model.{entity_field} = Set(Some({value_expr})); }}",
                    );
                } else {
                    wln!(
                        code,
                        "        if let Some(v) = cmd.{entity_field} {{ model.{entity_field} = Set({value_expr}); }}",
                    );
                }
            }
        }
        wln!(code, "        match model.update(tx).await {{");
        wln!(code, "            Ok(_) => {{}}");
        wln!(code, "            Err(sea_orm::DbErr::RecordNotUpdated) => {{ /* RLS hid the row — find_by_id will return 404 */ }}");
        wln!(code, "            Err(e) => return Err(e.into()),");
        wln!(code, "        }}");

        // Replace junction rows when the update request carries the field.
        emit_junction_replace(code, &tree.junction_tables, 2);

        // Update range columns via a single raw SQL UPDATE with explicit casts.
        // All range columns are collected into one statement to avoid per-column round-trips.
        let range_cols: Vec<&TreeColumn> = tree
            .direct_columns
            .iter()
            .filter(|c| {
                !c.is_workflow_managed
                    && !c.is_composite_range
                    && !c.is_media
                    && c.pg_cast.is_some()
            })
            .collect();
        if !range_cols.is_empty() {
            // All update DTO fields are Option<T>, so we only emit the UPDATE when at
            // least one range field is present. Build the SET clause and values dynamically.
            wln!(code);
            wln!(
                code,
                "        // Range columns need explicit casts — build a single UPDATE"
            );
            wln!(code, "        {{");
            wln!(
                code,
                "            let mut set_clauses: Vec<String> = Vec::new();"
            );
            wln!(
                code,
                "            let mut values: Vec<sea_orm::Value> = Vec::new();"
            );
            for col in &range_cols {
                let cast = col.pg_cast.as_deref().unwrap();
                let dto_field = col.dto_name();
                let pg_col = q(&col.pg_column_name);
                let typed_value = typed_value_expr(&col.rust_type, "v");
                wln!(code, "            if let Some(v) = cmd.{dto_field} {{");
                let set_expr = if crate::is_geometry_cast(cast) {
                    format!("                set_clauses.push(format!(\"{pg_col} = ST_GeomFromGeoJSON(${{}})\", values.len() + 1));")
                } else {
                    format!("                set_clauses.push(format!(\"{pg_col} = ${{}}::{cast}\", values.len() + 1));")
                };
                wln!(code, "{set_expr}");
                wln!(code, "                values.push({typed_value});");
                wln!(code, "            }}");
            }
            wln!(code, "            if !set_clauses.is_empty() {{");
            wln!(
                code,
                "                let id_placeholder = format!(\"${{}}\", values.len() + 1);"
            );
            wln!(
                code,
                "                let sql = format!(\"UPDATE {schema}.{table} SET {{}} WHERE id = {{}}\", set_clauses.join(\", \"), id_placeholder);",
                schema = tree.schema_name,
                table = q(&tree.table_name),
            );
            wln!(
                code,
                "                values.push(sea_orm::Value::Uuid(Some(Box::new(id))));"
            );
            wln!(
                code,
                "                let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, values);"
            );
            wln!(code, "                tx.execute(stmt).await?;");
            wln!(code, "            }}");
            wln!(code, "        }}");
        }

        // Update child tables: delete existing + re-insert when field is present
        for child in &tree.child_tables {
            // Skip child tables with no data columns and no nested children.
            if child.columns.is_empty() && child.child_tables.is_empty() {
                continue;
            }
            let col_names: Vec<String> =
                child.columns.iter().map(|c| q(&c.pg_column_name)).collect();
            let placeholders: Vec<String> = child
                .columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let base = format!("${}", i + 3);
                    if let Some(ref cast) = col.pg_cast {
                        format!("{base}::{cast}")
                    } else {
                        base
                    }
                })
                .collect();
            let insert_sql =
                child_insert_sql(child, &col_names.join(", "), &placeholders.join(", "));
            let delete_sql = format!(
                "DELETE FROM {}.{} WHERE {} = $1",
                child.sql_schema_name,
                q(&child.sql_table_name),
                child.parent_fk_column,
            );

            wln!(code);
            if child.is_array {
                wln!(
                    code,
                    "        // Replace child rows: {}.{}",
                    child.sql_schema_name,
                    child.sql_table_name
                );
                wln!(
                    code,
                    "        if let Some(ref items) = cmd.{} {{",
                    child.field_name
                );
                wln!(
                    code,
                    "            let del = Statement::from_sql_and_values(DatabaseBackend::Postgres, \"{}\", vec![id.into()]);",
                    delete_sql
                );
                wln!(code, "            tx.execute(del).await?;");
                let item_var = if child.columns.is_empty() && child.child_tables.is_empty() {
                    "_item"
                } else {
                    "item"
                };
                wln!(code, "            for {} in items {{", item_var);
                wln!(code, "                let child_id = Uuid::new_v4();");
                wln!(
                    code,
                    "                let stmt = Statement::from_sql_and_values("
                );
                wln!(code, "                    DatabaseBackend::Postgres,");
                wln!(code, "                    \"{}\",", insert_sql);
                w!(code, "                    vec![child_id.into(), id.into()");
                for col in &child.columns {
                    emit_child_col_write_value(code, col);
                }
                wln!(code, "],");
                wln!(code, "                );");
                wln!(code, "                tx.execute(stmt).await?;");
                emit_child_inserts(code, &child.child_tables, "child_id", "item", 4);
                wln!(code, "            }}");
                wln!(code, "        }}");
            } else {
                wln!(
                    code,
                    "        // Replace optional child row: {}.{}",
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
                    "        if let Some(ref {}) = cmd.{} {{",
                    item_var,
                    child.field_name
                );
                wln!(
                    code,
                    "            let del = Statement::from_sql_and_values(DatabaseBackend::Postgres, \"{}\", vec![id.into()]);",
                    delete_sql
                );
                wln!(code, "            tx.execute(del).await?;");
                wln!(code, "            let child_id = Uuid::new_v4();");
                wln!(
                    code,
                    "            let stmt = Statement::from_sql_and_values("
                );
                wln!(code, "                DatabaseBackend::Postgres,");
                wln!(code, "                \"{}\",", insert_sql);
                w!(code, "                vec![child_id.into(), id.into()");
                for col in &child.columns {
                    emit_child_col_write_value(code, col);
                }
                wln!(code, "],");
                wln!(code, "            );");
                wln!(code, "            tx.execute(stmt).await?;");
                emit_child_inserts(code, &child.child_tables, "child_id", "item", 3);
                wln!(code, "        }}");
            }
        }
    }

    pub(crate) fn emit_delete_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(code);
        wln!(
            code,
            "    #[tracing::instrument(skip(self, tx), fields(db.operation = \"delete\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn delete(");
        wln!(code, "        &self,");
        wln!(code, "        tx: &DatabaseTransaction,");
        wln!(code, "        id: Uuid,");
        wln!(code, "    ) -> Result<(), Box<dyn std::error::Error>> {{");
        if tree.is_auditable {
            wln!(
                code,
                "        let model = crate::entity::{}::Entity::find()",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::Id.eq(id))",
                tree.entity_module
            );
            wln!(
                code,
                "            .filter(crate::entity::{}::Column::DeletedAt.is_null())",
                tree.entity_module
            );
            wln!(code, "            .one(tx)");
            wln!(code, "            .await?");
            wln!(
                code,
                "            .ok_or_else(|| Box::<dyn std::error::Error>::from(\"Entity not found or already deleted\"))?;"
            );
            wln!(
                code,
                "        let mut active: crate::entity::{}::ActiveModel = model.into();",
                tree.entity_module
            );
            wln!(
                code,
                "        active.deleted_at = sea_orm::ActiveValue::Set(Some(chrono::Utc::now().into()));"
            );
            wln!(code, "        match active.update(tx).await {{");
            wln!(code, "            Ok(_) => {{}}");
            wln!(code, "            Err(sea_orm::DbErr::RecordNotUpdated) => {{ /* RLS hid the row — find_by_id will return 404 */ }}");
            wln!(code, "            Err(e) => return Err(e.into()),");
            wln!(code, "        }}");
        } else {
            wln!(
                code,
                "        // CASCADE handles child cleanup for {}.{}",
                tree.schema_name,
                tree.table_name
            );
            wln!(
                code,
                "        crate::entity::{}::Entity::delete_by_id(id)",
                tree.entity_module
            );
            wln!(code, "            .exec(tx)");
            wln!(code, "            .await?;");
        }
        wln!(code, "        Ok(())");
        wln!(code, "    }}");
    }
}
