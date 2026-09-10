use crate::code_writer::{wln, CodeWriter};
use crate::filter_fields::NestedFilterFieldInfo;

use super::child::emit_child_reads;
use super::dto::{emit_child_field_population, emit_entity_to_dto_field};
use super::helpers::q;
use super::junction::{emit_junction_field_population, emit_junction_reads};
use super::{EntityTree, RepositoryImplEmitter};

/// Emit type-safe value parsing for a nested filter field and return the
/// Rust expression that holds the parsed value (e.g. `"parsed"` or `"val.clone()"`).
///
/// Covers the same type surface as `typed_value_expr()` used for direct-column filters:
/// Uuid, i32, i64, f32, f64, bool, Decimal, NaiveDate, DateTime<Utc>, and String fallback.
fn emit_nested_filter_parse(code: &mut CodeWriter, nf: &NestedFilterFieldInfo) -> &'static str {
    let key = &nf.filter_key;
    // Strip Option<> wrapper for type matching — all FK columns are nullable.
    let rust_type = nf.rust_type.as_str();
    let base_type = rust_type
        .strip_prefix("Option<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(rust_type);
    match base_type {
        "Uuid" | "uuid::Uuid" => {
            wln!(
                code,
                "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        // Entity reference types (e.g. "OrganizationType") — always UUID FK columns.
        ty if ty.ends_with("Type") && ty.chars().next().is_some_and(|c| c.is_uppercase()) => {
            wln!(
                code,
                "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "i32" => {
            wln!(
                code,
                "            let parsed: i32 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i32 for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "i64" => {
            wln!(
                code,
                "            let parsed: i64 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i64 for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "f32" => {
            wln!(
                code,
                "            let parsed: f32 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid f32 for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "f64" => {
            wln!(
                code,
                "            let parsed: f64 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid f64 for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "bool" => {
            wln!(
                code,
                "            let parsed: bool = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid bool for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "Decimal" | "rust_decimal::Decimal" => {
            wln!(
                code,
                "            let parsed: rust_decimal::Decimal = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid Decimal for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "NaiveDate" | "chrono::NaiveDate" => {
            wln!(
                code,
                "            let parsed = chrono::NaiveDate::parse_from_str(val, \"%Y-%m-%d\").map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid date for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => {
            wln!(
                code,
                "            let parsed = val.parse::<chrono::DateTime<chrono::Utc>>().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid datetime for filter '{key}': {{e}}\")))?;",
            );
            "parsed"
        }
        _ => {
            // String and everything else — pass through directly.
            "val.clone()"
        }
    }
}

impl RepositoryImplEmitter {
    pub(crate) fn emit_list_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(code);
        wln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"select_list\", db.table = \"{}.{}\"))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn list(");
        wln!(code, "        &self,");
        wln!(code, "        db: &DatabaseTransaction,");
        wln!(code, "        page: u64,");
        wln!(code, "        page_size: u64,");
        wln!(
            code,
            "        filters: &std::collections::HashMap<String, String>,"
        );
        if tree.is_auditable {
            wln!(code, "        include_deleted: bool,");
        }
        wln!(
            code,
            "    ) -> Result<(Vec<{}Response>, u64), Box<dyn std::error::Error>> {{",
            tree.entity_name
        );

        // Build filter condition from JSON:API filter params.
        let has_any_filters = !tree.filter_fields.is_empty()
            || !tree.nested_filter_fields.is_empty()
            || tree.parent_ref.is_some();
        if has_any_filters {
            wln!(
                code,
                "        let mut condition = sea_orm::Condition::all();"
            );

            // --- Direct column filters ---
            for ff in &tree.filter_fields {
                // Strip r# raw identifier prefix before PascalCase conversion —
                // SeaORM Column variants use the bare name (e.g. `Type`, not `RType`).
                let bare_name = ff.field_name.strip_prefix("r#").unwrap_or(&ff.field_name);
                let pascal_col = codegraph_naming::to_pascal_case(bare_name);
                wln!(
                    code,
                    "        if let Some(val) = filters.get(\"{}\") {{",
                    ff.field_name
                );
                // Generate type-appropriate parsing. Optional (`Option<T>`) types
                // are handled via their base type — the column still stores a
                // concrete value, and filter values are never null.
                let base_type = ff
                    .rust_type
                    .trim_start_matches("Option<")
                    .trim_end_matches('>')
                    .to_string();
                match base_type.as_str() {
                    "Uuid" | "uuid::Uuid" => {
                        wln!(code, "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{}': {{e}}\", )))?;", ff.field_name);
                        wln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col);
                    }
                    "i32" => {
                        wln!(code, "            let parsed: i32 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i32 for filter '{}': {{e}}\")))?;", ff.field_name);
                        wln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col);
                    }
                    "i64" => {
                        wln!(code, "            let parsed: i64 = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid i64 for filter '{}': {{e}}\")))?;", ff.field_name);
                        wln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col);
                    }
                    "bool" => {
                        wln!(code, "            let parsed: bool = val.parse().map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid bool for filter '{}': {{e}}\")))?;", ff.field_name);
                        wln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col);
                    }
                    // Entity reference types (e.g. "ConsultationType") — FK columns
                    // are always UUIDs, even when the DTO wraps them in Option<>.
                    ty if ty.ends_with("Type")
                        && ty.chars().next().is_some_and(|c| c.is_uppercase()) =>
                    {
                        wln!(code, "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{}': {{e}}\")))?;", ff.field_name);
                        wln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col);
                    }
                    _ => {
                        // String and everything else — exact match.
                        wln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(val.clone()));", tree.entity_module, pascal_col);
                    }
                }
                wln!(code, "        }}");
            }

            // --- Parent ref filter (child entity scoped to a parent) ---
            if let Some(ref parent_ref) = tree.parent_ref {
                let pascal_col = codegraph_naming::to_pascal_case(parent_ref);
                wln!(
                    code,
                    "        if let Some(val) = filters.get(\"{parent_ref}\") {{"
                );
                wln!(code, "            let parsed = uuid::Uuid::parse_str(val).map_err(|e| Box::<dyn std::error::Error>::from(format!(\"Invalid UUID for filter '{parent_ref}': {{e}}\")))?;");
                wln!(code, "            condition = condition.add(crate::entity::{}::Column::{}.eq(parsed));", tree.entity_module, pascal_col);
                wln!(code, "        }}");
            }

            // --- Nested (child / grandchild) filters via EXISTS subqueries ---
            // All identifiers (schema, table, column) are always double-quoted in the
            // generated SQL to guard against future names that match PG reserved words.
            for nf in &tree.nested_filter_fields {
                wln!(
                    code,
                    "        if let Some(val) = filters.get(\"{}\") {{",
                    nf.filter_key
                );

                // Type-safe value parsing — same patterns as direct filters.
                let val_expr = emit_nested_filter_parse(code, nf);

                if let Some(ref ij) = nf.intermediate_join {
                    // Grandchild: nested EXISTS through an intermediate child table.
                    //   EXISTS (SELECT 1 FROM intermediate WHERE intermediate.parent_fk = parent.id
                    //     AND EXISTS (SELECT 1 FROM grandchild WHERE grandchild.child_fk = intermediate.id
                    //       AND grandchild.column = $value))
                    wln!(
                        code,
                        "            condition = condition.add(sea_orm::Condition::any().add(sea_orm::sea_query::Expr::cust_with_values("
                    );
                    wln!(
                        code,
                        "                \"EXISTS (SELECT 1 FROM \\\"{}\\\".\\\"{}\\\" _intermediate WHERE _intermediate.\\\"{}\\\" = \\\"{}\\\".\\\"{}\\\".\\\"id\\\" AND EXISTS (SELECT 1 FROM \\\"{}\\\".\\\"{}\\\" _gc WHERE _gc.\\\"{}\\\" = _intermediate.\\\"id\\\" AND _gc.\\\"{}\\\" = $1))\",",
                        ij.sql_schema,
                        ij.sql_table_name,
                        ij.parent_fk_column,
                        tree.schema_name,
                        tree.table_name,
                        nf.sql_schema,
                        nf.sql_table_name,
                        nf.parent_fk_column,
                        nf.pg_column_name,
                    );
                    wln!(
                        code,
                        "                vec![sea_orm::Value::from({val_expr})],"
                    );
                    wln!(code, "            )));");
                } else {
                    // Direct child: single EXISTS subquery.
                    //   EXISTS (SELECT 1 FROM child WHERE child.parent_fk = parent.id AND child.column = $value)
                    wln!(
                        code,
                        "            condition = condition.add(sea_orm::Condition::any().add(sea_orm::sea_query::Expr::cust_with_values("
                    );
                    wln!(
                        code,
                        "                \"EXISTS (SELECT 1 FROM \\\"{}\\\".\\\"{}\\\" _child WHERE _child.\\\"{}\\\" = \\\"{}\\\".\\\"{}\\\".\\\"id\\\" AND _child.\\\"{}\\\" = $1)\",",
                        nf.sql_schema,
                        nf.sql_table_name,
                        nf.parent_fk_column,
                        tree.schema_name,
                        tree.table_name,
                        nf.pg_column_name,
                    );
                    wln!(
                        code,
                        "                vec![sea_orm::Value::from({val_expr})],"
                    );
                    wln!(code, "            )));");
                }

                wln!(code, "        }}");
            }
        }

        wln!(
            code,
            "        let{} query = crate::entity::{}::Entity::find()",
            if tree.is_auditable { " mut" } else { "" },
            tree.entity_module
        );
        if has_any_filters {
            if tree.is_auditable {
                wln!(code, "            .filter(condition);");
            } else {
                wln!(code, "            .filter(condition)");
            }
        } else if tree.is_auditable {
            wln!(code, ";");
        }
        if tree.is_auditable {
            wln!(code, "        if !include_deleted {{");
            wln!(
                code,
                "            query = query.filter(crate::entity::{}::Column::DeletedAt.is_null());",
                tree.entity_module
            );
            wln!(code, "        }}");
            wln!(
                code,
                "        let query = query.order_by_desc(crate::entity::{}::Column::CreatedAt);",
                tree.entity_module
            );
        } else {
            wln!(
                code,
                "            .order_by_desc(crate::entity::{}::Column::CreatedAt);",
                tree.entity_module
            );
        }
        wln!(
            code,
            "        let paginator = query.paginate(db, page_size);"
        );
        wln!(code);
        wln!(code, "        let total = paginator.num_items().await?;");
        wln!(
            code,
            "        let rows = paginator.fetch_page(page).await?;"
        );
        wln!(code);
        wln!(
            code,
            "        let mut results = Vec::with_capacity(rows.len());"
        );
        wln!(code, "        for row in rows {{");

        // Query child tables for each parent row (recursively handles nested children).
        emit_child_reads(code, &tree.child_tables, "row.id", 3);
        emit_junction_reads(code, &tree.junction_tables, "row.id", 3);

        wln!(
            code,
            "            results.push({}Response {{",
            tree.entity_name
        );
        wln!(code, "                id: row.id,");
        for col in &tree.direct_columns {
            if col.is_composite_range {
                continue;
            }
            if col.field_name == "created_at" || col.field_name == "updated_at" {
                continue;
            }
            emit_entity_to_dto_field(code, col, "row", "                ");
        }
        emit_child_field_population(code, &tree.child_tables, "                ");
        emit_junction_field_population(code, &tree.junction_tables, "                ");
        if tree.has_workflow {
            wln!(code, "                workflow_state: None,");
        }
        wln!(code, "                created_at: row.created_at,");
        wln!(code, "                updated_at: row.updated_at,");
        wln!(code, "                ..Default::default()");
        wln!(code, "            }});");
        wln!(code, "        }}");
        wln!(code);
        wln!(code, "        Ok((results, total))");
        wln!(code, "    }}");
    }

    /// Emit full-text search that returns ranked IDs and total count.
    pub(crate) fn emit_search_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(code);
        wln!(
            code,
            "    #[tracing::instrument(skip(self, db), fields(db.operation = \"search_ids\", db.table = \"{}.{}\"  ))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn search_ids(");
        wln!(code, "        &self,");
        wln!(code, "        db: &DatabaseTransaction,");
        wln!(code, "        query: &str,");
        wln!(code, "        page: u64,");
        wln!(code, "        page_size: u64,");
        if tree.is_auditable {
            wln!(code, "        include_deleted: bool,");
        }
        wln!(
            code,
            "    ) -> Result<(Vec<uuid::Uuid>, u64), Box<dyn std::error::Error>> {{"
        );
        wln!(
            code,
            "        let count_stmt = Statement::from_sql_and_values("
        );
        wln!(code, "            DatabaseBackend::Postgres,");
        if tree.is_auditable {
            wln!(
                code,
                "            if include_deleted {{ \"SELECT COUNT(*) AS count FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1)\" }} else {{ \"SELECT COUNT(*) AS count FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1) AND deleted_at IS NULL\" }},",
                tree.schema_name, q(&tree.table_name), tree.fts_language,
                tree.schema_name, q(&tree.table_name), tree.fts_language,
            );
        } else {
            wln!(
                code,
                "            \"SELECT COUNT(*) AS count FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1)\",",
                tree.schema_name, q(&tree.table_name), tree.fts_language
            );
        }
        wln!(code, "            vec![query.into()],");
        wln!(code, "        );");
        wln!(
            code,
            "        let count_row = db.query_one(count_stmt).await?"
        );
        wln!(
            code,
            "            .ok_or(\"count query returned no rows\")?;"
        );
        wln!(
            code,
            "        let total: i64 = count_row.try_get(\"\", \"count\")?;"
        );
        wln!(code, "        let total = total as u64;");
        wln!(code);
        wln!(code, "        let offset = page * page_size;");
        wln!(code, "        let stmt = Statement::from_sql_and_values(");
        wln!(code, "            DatabaseBackend::Postgres,");
        if tree.is_auditable {
            wln!(
                code,
                "            if include_deleted {{ \"SELECT id FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1) ORDER BY ts_rank(search_tsv, websearch_to_tsquery('{}', $1)) DESC LIMIT $2 OFFSET $3\" }} else {{ \"SELECT id FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1) AND deleted_at IS NULL ORDER BY ts_rank(search_tsv, websearch_to_tsquery('{}', $1)) DESC LIMIT $2 OFFSET $3\" }},",
                tree.schema_name, q(&tree.table_name), tree.fts_language, tree.fts_language,
                tree.schema_name, q(&tree.table_name), tree.fts_language, tree.fts_language,
            );
        } else {
            wln!(
                code,
                "            \"SELECT id FROM {}.{} WHERE search_tsv @@ websearch_to_tsquery('{}', $1) ORDER BY ts_rank(search_tsv, websearch_to_tsquery('{}', $1)) DESC LIMIT $2 OFFSET $3\",",
                tree.schema_name, q(&tree.table_name), tree.fts_language, tree.fts_language
            );
        }
        wln!(
            code,
            "            vec![query.into(), (page_size as i64).into(), (offset as i64).into()],"
        );
        wln!(code, "        );");
        wln!(code, "        let rows = db.query_all(stmt).await?;");
        wln!(code, "        let ids: Vec<uuid::Uuid> = rows.iter()");
        wln!(
            code,
            "            .filter_map(|r| r.try_get::<uuid::Uuid>(\"\", \"id\").ok())"
        );
        wln!(code, "            .collect();");
        wln!(code);
        wln!(code, "        Ok((ids, total))");
        wln!(code, "    }}");
    }

    /// Emit semantic similarity search that returns IDs ordered by cosine similarity.
    pub(crate) fn emit_semantic_search_fn(&self, tree: &EntityTree, code: &mut CodeWriter) {
        wln!(code);
        wln!(
            code,
            "    #[tracing::instrument(skip(self, db, embedding), fields(db.operation = \"semantic_search_ids\", db.table = \"{}.{}\"  ))]",
            tree.schema_name, tree.table_name
        );
        wln!(code, "    async fn semantic_search_ids(");
        wln!(code, "        &self,");
        wln!(code, "        db: &DatabaseTransaction,");
        wln!(code, "        embedding: &[f32],");
        wln!(code, "        limit: u64,");
        if tree.is_auditable {
            wln!(code, "        include_deleted: bool,");
        }
        wln!(
            code,
            "    ) -> Result<Vec<uuid::Uuid>, Box<dyn std::error::Error>> {{"
        );
        wln!(code, "        let vec_str = format!(\"[{{}}]\", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(\",\"));");
        wln!(code, "        let stmt = Statement::from_sql_and_values(");
        wln!(code, "            DatabaseBackend::Postgres,");
        let emb_col = format!("{}_embedding", tree.table_name);
        if tree.is_auditable {
            wln!(
                code,
                "            if include_deleted {{ \"SELECT id FROM {schema}.{table} ORDER BY {col} <=> $1::vector LIMIT $2\" }} else {{ \"SELECT id FROM {schema}.{table} WHERE deleted_at IS NULL ORDER BY {col} <=> $1::vector LIMIT $2\" }},",
                schema = tree.schema_name,
                table = q(&tree.table_name),
                col = emb_col
            );
        } else {
            wln!(
                code,
                "            \"SELECT id FROM {schema}.{table} ORDER BY {col} <=> $1::vector LIMIT $2\",",
                schema = tree.schema_name,
                table = q(&tree.table_name),
                col = emb_col
            );
        }
        wln!(
            code,
            "            vec![vec_str.into(), (limit as i64).into()],"
        );
        wln!(code, "        );");
        wln!(code, "        let rows = db.query_all(stmt).await?;");
        wln!(code, "        let ids: Vec<uuid::Uuid> = rows.iter()");
        wln!(
            code,
            "            .filter_map(|r| r.try_get::<uuid::Uuid>(\"\", \"id\").ok())"
        );
        wln!(code, "            .collect();");
        wln!(code);
        wln!(code, "        Ok(ids)");
        wln!(code, "    }}");
    }
}
