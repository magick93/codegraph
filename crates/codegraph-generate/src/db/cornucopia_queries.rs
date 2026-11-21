use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::ProjectConfig;
use crate::error::Result;
use crate::persistence::build_persistence_entity;
use crate::traits::{EntityGenerator, EntityGeneratorKind, GeneratedFile};
use codegraph_config::DomainConfig;

use crate::ddd::repository_emitter::{
    ChildTableInfo, EntityTree, JunctionTableInfo, RepositoryImplEmitter, TreeColumn,
};

/// Generates annotated Cornucopia SQL files for an entity's CRUD surface.
///
/// One file per entity at `queries/{domain}/{table}.sql`. Each annotated query
/// block (`--! name (params) : (returns)`) becomes a typed Rust function in the
/// generated `cornucopia-queries` crate.
///
/// Tenant isolation is NOT expressed in the SQL: the DDL trigger fills the
/// tenant column from `app.organization_id` on INSERT and Postgres RLS
/// (`SET LOCAL ROLE app_user` in the query/command layers) filters reads —
/// exactly like the SeaORM backend.
pub struct CornucopiaQueryGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<codegraph_core::types::ParentCandidate>,
}

impl CornucopiaQueryGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
        }
    }

    pub fn with_parent_candidates(
        mut self,
        candidates: Vec<codegraph_core::types::ParentCandidate>,
    ) -> Self {
        self.parent_candidates = candidates;
        self
    }
}

#[async_trait]
impl EntityGenerator for CornucopiaQueryGenerator {
    fn kind(&self) -> EntityGeneratorKind {
        EntityGeneratorKind::CornucopiaQueries
    }

    fn supported_targets(&self) -> Option<Vec<crate::db::dialect::DatabaseTarget>> {
        None
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        _tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if !project.is_cornucopia() {
            return Ok(Vec::new());
        }

        let parent_ref = crate::resolve_parent_fk_column_same_domain(
            schema_title,
            &self.parent_candidates,
            config
                .domains
                .get(domain)
                .and_then(|d| d.get_entity_config(schema_title)),
            domain,
            config,
            db,
        )
        .await;

        let tree = RepositoryImplEmitter
            .query_entity_tree(db, schema_title, domain, config, parent_ref.as_deref())
            .await?;

        if tree.table_name.is_empty() {
            return Ok(Vec::new());
        }

        // Column → PostgreSQL type name, from the persistence IR (used to
        // derive SQL casts for columns whose rust_type doesn't reveal the DB
        // type, e.g. geometry/geography columns stored as String).
        let entity_ir =
            build_persistence_entity(db, schema_title, domain, config, &self.parent_candidates)
                .await?;
        let pg_types: std::collections::HashMap<String, String> = entity_ir
            .columns
            .iter()
            .map(|c| (c.column_name.clone(), c.pg_type.clone()))
            .collect();

        let mut sql = render_entity_sql(schema_title, domain, config, &tree, &pg_types);

        // `?include=` hydration queries (issue #781 P1 preview feature).
        // Only the shapes the plane implements reach here — every other
        // modelled path stays un-hydratable (the handler answers its guarded
        // 501 naming the shape), so no query is emitted for it.
        if project.codegen.cornucopia_include_hydration {
            sql.push_str(
                &render_include_queries(
                    db,
                    config,
                    project,
                    domain,
                    schema_title,
                    &tree,
                    &self.parent_candidates,
                )
                .await,
            );
        }

        // Deployed-worker query for tree_include entities: the resolution
        // needs the repository emitter's graph walk (the persistence tree
        // leaves tree_include empty), so it happens here in the async caller.
        if tree.hierarchy_field.is_some() {
            let resolved = RepositoryImplEmitter
                .query_entity_tree(db, schema_title, domain, config, None)
                .await?;
            if !resolved.tree_include.is_empty() {
                write_tree_workers_query(&mut sql, &resolved);
            }
        }

        let rel_path = format!("queries/{}/{}.sql", domain, tree.table_name);
        Ok(vec![GeneratedFile {
            path: self.output_dir.join(&rel_path),
            content: sql,
        }])
    }
}

/// The Postgres cast target for a column — explicit range casts win; otherwise
/// derive from the DTO rust type. Scalar params bind as text (Option<String>
/// binds NULL cleanly) and cast in SQL.
fn pg_cast_for(col: &TreeColumn, pg_types: &std::collections::HashMap<String, String>) -> String {
    if let Some(ref cast) = col.pg_cast {
        return cast.clone();
    }
    if let Some(pg) = pg_types.get(&col.pg_column_name) {
        let pg_upper = pg.to_uppercase();
        if pg_upper.contains("GEOMETRY") {
            return "geometry".to_string();
        }
        if pg_upper.contains("GEOGRAPHY") {
            return "geography".to_string();
        }
        if pg_upper.contains("VECTOR") {
            return "vector".to_string();
        }
        if pg_upper.contains("TSTZRANGE") {
            return "tstzrange".to_string();
        }
        if pg_upper.contains("DATERANGE") {
            return "daterange".to_string();
        }
    }
    match col.rust_type.as_str() {
        "Uuid" | "uuid::Uuid" => "uuid",
        "i32" => "int4",
        "i64" => "int8",
        "f32" => "float4",
        "f64" => "float8",
        "bool" => "bool",
        "Decimal" | "rust_decimal::Decimal" => "numeric",
        "NaiveDate" | "chrono::NaiveDate" => "date",
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => "timestamptz",
        "serde_json::Value" | "Vec<serde_json::Value>" => "jsonb",
        _ => "text",
    }
    .to_string()
}

/// Postgres cast target for a child-table column.
fn child_pg_cast_for(col: &crate::ddd::repository_emitter::ChildColumn) -> String {
    if let Some(ref cast) = col.pg_cast {
        return cast.clone();
    }
    match col.rust_type.as_str() {
        "Uuid" | "uuid::Uuid" => "uuid",
        "i32" => "int4",
        "i64" => "int8",
        "f32" => "float4",
        "f64" => "float8",
        "bool" => "bool",
        "Decimal" | "rust_decimal::Decimal" => "numeric",
        "NaiveDate" | "chrono::NaiveDate" => "date",
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => "timestamptz",
        "serde_json::Value" | "Vec<serde_json::Value>" => "jsonb",
        _ => "text",
    }
    .to_string()
}

/// Whether the column is bound typed (arrays) rather than as text.
fn is_array_col(col: &TreeColumn) -> bool {
    col.is_array || col.rust_type.starts_with("Vec<")
}

/// The snake_case entity name used to prefix query function names.
fn snake_name(tree: &EntityTree) -> String {
    codegraph_naming::to_snake_case(&tree.entity_name)
}

/// Render the full SQL file for one entity.
fn render_entity_sql(
    schema_title: &str,
    domain: &str,
    config: &DomainConfig,
    tree: &EntityTree,
    pg_types: &std::collections::HashMap<String, String>,
) -> String {
    let mut sql = String::with_capacity(16 * 1024);

    let table = format!("\"{}\".\"{}\"", tree.schema_name, tree.table_name);
    let entity_name = snake_name(tree);
    let soft_delete_col = tree.soft_delete_column.as_deref();

    // Direct columns participating in row returns (excludes composite ranges,
    // which do not exist as physical columns).
    let row_cols: Vec<&TreeColumn> = tree
        .direct_columns
        .iter()
        .filter(|c| !c.is_composite_range)
        .collect();

    sql.push_str(&format!(
        "-- Cornucopia queries for {} (domain: {})\n\
         -- Generated by codegraph — persistence_provider = \"cornucopia\"\n\n",
        schema_title, domain,
    ));

    // ── list (+ count) ─────────────────────────────────────────────────
    write_list_queries(
        &mut sql,
        &table,
        &entity_name,
        soft_delete_col,
        &row_cols,
        pg_types,
        tree.append_only,
    );

    // ── get by id ──────────────────────────────────────────────────────
    write_get_queries(
        &mut sql,
        &table,
        &entity_name,
        soft_delete_col,
        &row_cols,
        pg_types,
        tree.append_only,
    );
    if let Some(ref parent_fk) = tree.parent_ref {
        write_get_scoped_queries(
            &mut sql,
            &table,
            &entity_name,
            soft_delete_col,
            &row_cols,
            parent_fk,
            pg_types,
            tree.append_only,
        );
    }

    // ── create ─────────────────────────────────────────────────────────
    write_create_query(&mut sql, &table, &entity_name, tree, pg_types);

    // ── update (COALESCE so missing Option fields keep existing values) ─
    write_update_query(
        &mut sql,
        &table,
        &entity_name,
        tree,
        soft_delete_col,
        pg_types,
    );

    // ── delete ─────────────────────────────────────────────────────────
    write_delete_query(&mut sql, &table, &entity_name, soft_delete_col);

    // ── child tables (VO tables) ───────────────────────────────────────
    for child in &tree.child_tables {
        write_child_queries(&mut sql, &tree.schema_name, &entity_name, child);
    }

    // ── junction tables (array-of-entity-ref `refers`, issue #539) ─────
    for j in &tree.junction_tables {
        write_junction_queries(&mut sql, tree, j);
    }

    // ── nested filter helpers (child/grandchild EXISTS equivalents) ─────
    write_nested_filter_queries(&mut sql, tree);

    // ── FTS / embeddings / tree ────────────────────────────────────────
    if tree.has_fts {
        write_search_queries(
            &mut sql,
            &table,
            &entity_name,
            soft_delete_col,
            &tree.fts_language,
        );
    }
    if tree.has_embeddings
        && let Some(ec) = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(schema_title))
        && let Some(first) = ec.search.embedding_columns.first()
    {
        let emb_col = format!("{}_embedding", first);
        write_embedding_queries(&mut sql, &table, &entity_name, soft_delete_col, &emb_col);
    }
    if let Some(ref hf) = tree.hierarchy_field {
        write_tree_query(
            &mut sql,
            &table,
            &entity_name,
            hf,
            &row_cols,
            pg_types,
            tree.append_only,
        );
    }

    // ── declared .ddd design finders (issue #449) ──────────────────────
    write_design_finder_queries(
        &mut sql,
        &table,
        &entity_name,
        tree,
        soft_delete_col,
        &row_cols,
        pg_types,
    );

    sql
}

/// One annotated equality query per declared design finder. Parameter
/// columns resolve exactly like the SeaORM emitter's (the same
/// `finder_columns` contract), so both providers emit the same finders.
///
/// Binding: `String`/`bool`/`i64`/`f64`/`Uuid` params bind their natural
/// rust type (tokio-postgres infers the column type from the prepared
/// comparison, exactly like `get_{entity} (id)`); `Decimal`/`NaiveDate`
/// params bind as text with an explicit `::text` cast on the column to
/// avoid depending on the generated queries crate's numeric/chrono
/// features.
fn write_design_finder_queries(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    tree: &EntityTree,
    soft_delete_col: Option<&str>,
    row_cols: &[&TreeColumn],
    pg_types: &std::collections::HashMap<String, String>,
) {
    if tree.design_finders.is_empty() {
        return;
    }
    let cols = row_col_list(row_cols, pg_types, tree.append_only);
    let hints = row_hints(row_cols, tree.append_only);

    for finder in &tree.design_finders {
        // DD9 typed queries (issue #659) carry their own resolved plan and
        // render a parameterized predicate + ORDER BY + page window.
        if finder.kind == crate::ddd::design::FinderKind::Query {
            if let Some(plan) = &finder.query {
                write_design_query_sql(
                    sql,
                    table,
                    entity_name,
                    finder,
                    plan,
                    soft_delete_col,
                    &cols,
                    &hints,
                );
            }
            continue;
        }
        let Ok(bindings) = crate::ddd::repository_emitter::finder_columns(
            finder,
            &tree.direct_columns,
            &tree.child_tables,
        ) else {
            continue;
        };
        // `findByExample`'s Option params use the nullability idiom:
        // `(:p IS NULL OR col = :p)` — a None param filters its column out.
        // The FIRST reference needs an explicit cast: Postgres cannot infer
        // a bare parameter's type from `IS NULL` alone.
        let example = finder.kind == crate::ddd::design::FinderKind::FindByExample;
        let mut clauses = Vec::with_capacity(bindings.len() + 1);
        // Contained-value params (issue #523) group per child table into
        // one EXISTS — same-row semantics, matching the SeaORM emitter.
        let mut exists_groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, binding) in bindings.iter().enumerate() {
            if let crate::ddd::repository_emitter::FinderBinding::Child(child) = binding {
                match exists_groups.iter_mut().find(|(g, _)| g == &child.group) {
                    Some((_, idxs)) => idxs.push(i),
                    None => exists_groups.push((child.group.clone(), vec![i])),
                }
            }
        }
        for (param, binding) in finder.params.iter().zip(bindings.iter()) {
            let Some(col) = binding.direct() else {
                // Contained-value params ride their EXISTS clause below.
                continue;
            };
            let pg_col = format!("\"{}\"", col.pg_column_name);
            if example {
                // `:p IS NULL` stays bare — cornucopia's derive detects the
                // nullability idiom textually and binds Option; the cast
                // rides the EQUALITY so Postgres can type the parameter
                // (a bare param used only in IS NULL has no inferable type).
                // Fully cast on BOTH references: Postgres must type the
                // parameter without relying on cross-reference inference,
                // and the header's `?` (below) is what makes cornucopia
                // bind Option — never the SQL text.
                if crate::ddd::repository_emitter::finder_param_binds_as_text(&param.rust_type) {
                    clauses.push(format!(
                        "(:{p}::text IS NULL OR {pg_col}::text = :{p}::text)",
                        p = param.name
                    ));
                } else {
                    let pg_type = pg_types
                        .get(&col.pg_column_name)
                        .map(String::as_str)
                        .unwrap_or("text");
                    clauses.push(format!(
                        "(:{p}::{t} IS NULL OR {pg_col} = :{p}::{t})",
                        p = param.name,
                        t = pg_type
                    ));
                }
            } else if crate::ddd::repository_emitter::finder_param_binds_as_text(&param.rust_type) {
                clauses.push(format!("{pg_col}::text = :{}", param.name));
            } else {
                clauses.push(format!("{pg_col} = :{}", param.name));
            }
        }
        for (_, idxs) in &exists_groups {
            let crate::ddd::repository_emitter::FinderBinding::Child(child) = &bindings[idxs[0]]
            else {
                continue;
            };
            let alias = if child.intermediate.is_some() {
                "_gc"
            } else {
                "_child"
            };
            let preds = idxs
                .iter()
                .map(|i| {
                    let param = &finder.params[*i];
                    let crate::ddd::repository_emitter::FinderBinding::Child(c) = &bindings[*i]
                    else {
                        return String::new();
                    };
                    if crate::ddd::repository_emitter::finder_param_binds_as_text(&param.rust_type)
                    {
                        format!(
                            "{alias}.\"{}\"::text = :{}::text",
                            c.pg_column_name, param.name
                        )
                    } else {
                        format!("{alias}.\"{}\" = :{}", c.pg_column_name, param.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(" AND ");
            clauses.push(match &child.intermediate {
                None => format!(
                    "EXISTS (SELECT 1 FROM \"{}\".\"{}\" _child WHERE _child.\"{}\" = {table}.\"id\" AND {preds})",
                    child.sql_schema,
                    child.sql_table_name,
                    child.parent_fk_column,
                    table = table,
                    preds = preds
                ),
                Some(ij) => format!(
                    "EXISTS (SELECT 1 FROM \"{}\".\"{}\" _intermediate WHERE _intermediate.\"{}\" = {table}.\"id\" AND EXISTS (SELECT 1 FROM \"{}\".\"{}\" _gc WHERE _gc.\"{}\" = _intermediate.\"id\" AND {preds}))",
                    ij.sql_schema,
                    ij.sql_table_name,
                    ij.parent_fk_column,
                    child.sql_schema,
                    child.sql_table_name,
                    child.parent_fk_column,
                    table = table,
                    preds = preds
                ),
            });
        }
        // The declared signature carries no include_deleted parameter —
        // soft-deleted rows always stay invisible to design finders.
        if let Some(sd) = soft_delete_col {
            clauses.push(format!("\"{sd}\" IS NULL"));
        }
        sql.push_str(&format!(
            "--! {fn_name} ({params}) : ({hints})\n\
             --- Design finder `{author}` (declared in the .ddd design): equality query.\n\
             SELECT {cols}\n\
             FROM {table}\n\
             WHERE {};\n\n",
            clauses.join("\n    AND "),
            fn_name = design_finder_query_name(finder, entity_name),
            params = finder
                .params
                .iter()
                .map(|p| {
                    // `?` marks the header param nullable — cornucopia binds
                    // Option<T> for it. Every findByExample parameter binds
                    // as Option (the None-filters idiom); other finders bind
                    // their declared type directly.
                    if example || p.rust_type.starts_with("Option<") {
                        format!("{}?", p.name)
                    } else {
                        p.name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            author = finder.name,
        ));
    }
}

/// One annotated query per DD9 `query`-carrying design operation (issue
/// #659): the predicate renders parameterized (named binds, in first-
/// occurrence order), the sort keys become the ORDER BY (with the identity
/// tiebreaker), and a declared `page` appends `LIMIT`/`OFFSET` binds.
///
/// The header parameter list is the bind order the adapter passes to
/// `bind(…)`: predicate binds first, then `limit`/`offset`.
#[allow(clippy::too_many_arguments)]
fn write_design_query_sql(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    finder: &crate::ddd::design::DesignFinder,
    plan: &crate::ddd::repository_query::QueryPlan,
    soft_delete_col: Option<&str>,
    cols: &str,
    hints: &str,
) {
    let rendered = crate::ddd::repository_query::render_sql(plan, &finder.params);
    let mut clauses = vec![rendered.predicate.clone()];
    if let Some(sd) = soft_delete_col {
        clauses.push(format!("\"{sd}\" IS NULL"));
    }
    let mut header_params: Vec<String> = rendered.args.iter().map(|arg| arg.name.clone()).collect();
    if plan.has_page() {
        header_params.push(crate::ddd::repository_query::PAGE_SIZE_BIND.to_string());
        header_params.push(crate::ddd::repository_query::PAGE_OFFSET_BIND.to_string());
    }
    let mut statement = format!(
        "--! {fn_name} ({params}) : ({hints})\n\
         --- Design query `{author}` (declared in the .ddd design): typed predicate, \
         parameterized.\n\
         SELECT {cols}\n\
         FROM {table}\n\
         WHERE {where_clause}\n\
         ORDER BY {order_by}\n",
        fn_name = design_finder_query_name(finder, entity_name),
        params = header_params.join(", "),
        author = finder.name,
        where_clause = clauses.join("\n    AND "),
        order_by = rendered.order_by,
    );
    if plan.has_page() {
        statement.push_str(&format!(
            "LIMIT :{} OFFSET :{}\n",
            crate::ddd::repository_query::PAGE_SIZE_BIND,
            crate::ddd::repository_query::PAGE_OFFSET_BIND
        ));
    }
    statement.push_str(";\n\n");
    sql.push_str(&statement);
}

/// The cornucopia query function name for a design finder:
/// `{method_name}_{entity_snake}` (e.g. `find_by_title_book`).
fn design_finder_query_name(
    finder: &crate::ddd::design::DesignFinder,
    entity_name: &str,
) -> String {
    format!("{}_{}", finder.method_name, entity_name)
}

fn row_col_list(
    cols: &[&TreeColumn],
    pg_types: &std::collections::HashMap<String, String>,
    append_only: bool,
) -> String {
    let mut names: Vec<String> = vec!["\"id\"".to_string()];
    for c in cols {
        names.push(select_col(
            c.field_name.trim_start_matches("r#"),
            c,
            pg_types,
        ));
    }
    names.push("\"created_at\"".to_string());
    // Append-only entities (#284) carry no updated_at column.
    if !append_only {
        names.push("\"updated_at\"".to_string());
    }
    names.join(", ")
}

/// One SELECT column, aliased to the Rust field name the row hints (and
/// the repository emitter's `row.<field>` accessors) use: physical columns
/// renamed at the DDL (codelist-ref `reward` → `reward_code`) must still
/// surface under their field names, and cornucopia validates the hints
/// against the RETURNED column names.
fn select_col(
    field_name: &str,
    c: &TreeColumn,
    pg_types: &std::collections::HashMap<String, String>,
) -> String {
    let expr = if needs_text_cast(c, pg_types) {
        format!("\"{}\"::text", c.pg_column_name)
    } else {
        format!("\"{}\"", c.pg_column_name)
    };
    if c.pg_column_name == field_name {
        expr
    } else {
        format!("{expr} AS \"{field_name}\"")
    }
}

/// Whether a column's DB type needs an explicit `::text` cast in SELECT lists
/// because its mapped Rust type (String) cannot be read directly by
/// tokio-postgres (ranges, geometry/geography, vector, numeric — `String`'s
/// `FromSql` rejects all of these, even for NULL values, because
/// `Row::try_get` type-checks `accepts()` before checking for NULL).
fn needs_text_cast(col: &TreeColumn, pg_types: &std::collections::HashMap<String, String>) -> bool {
    if col.pg_cast.is_some() {
        return true;
    }
    let Some(pg) = pg_types.get(&col.pg_column_name) else {
        return false;
    };
    let pg_upper = pg.to_uppercase();
    pg_upper.contains("GEOMETRY")
        || pg_upper.contains("GEOGRAPHY")
        || pg_upper.contains("VECTOR")
        || pg_upper.contains("RANGE")
        || pg_upper.contains("NUMERIC")
}

fn row_hints(cols: &[&TreeColumn], append_only: bool) -> String {
    let mut hints: Vec<String> = vec!["id".to_string()];
    for c in cols {
        // Hints name the cornucopia-generated struct fields — the Rust
        // field names the repository emitter reads (`row.reward`). The
        // SELECT list aliases each physical column to its field name (see
        // `select_col`), so hint == returned column == struct field.
        let name = c.field_name.trim_start_matches("r#");
        hints.push(if c.is_nullable {
            format!("{}?", name)
        } else {
            name.to_string()
        });
    }
    hints.push("created_at".to_string());
    // Append-only entities (#284) carry no updated_at column.
    if !append_only {
        hints.push("updated_at".to_string());
    }
    hints.join(", ")
}

fn write_list_queries(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    soft_delete_col: Option<&str>,
    row_cols: &[&TreeColumn],
    pg_types: &std::collections::HashMap<String, String>,
    append_only: bool,
) {
    let cols = row_col_list(row_cols, pg_types, append_only);
    let hints = row_hints(row_cols, append_only);

    for (suffix, include_deleted) in [("", false), ("_including_deleted", true)] {
        let where_clause = if !include_deleted {
            soft_delete_col
                .map(|sd| format!("\n  WHERE \"{sd}\" IS NULL"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        sql.push_str(&format!(
            "--! list_{entity_name}{suffix} (offset, page_size) : ({hints})\n\
             --- List {entity_name} records.\n\
             SELECT {cols}\n\
             FROM {table}{where_clause}\n\
             ORDER BY \"created_at\" DESC\n\
             LIMIT :page_size OFFSET :offset;\n\n",
        ));
        let count_where_clause = if include_deleted {
            String::new()
        } else {
            soft_delete_col
                .map(|sd| format!("\n  WHERE \"{sd}\" IS NULL"))
                .unwrap_or_default()
        };
        sql.push_str(&format!(
            "--! count_{entity_name}{suffix} : (count)\n\
             --- Count of {entity_name} records.\n\
             SELECT COUNT(*) AS \"count\"\n\
             FROM {table}{count_where_clause};\n\n",
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn write_get_scoped_queries(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    soft_delete_col: Option<&str>,
    row_cols: &[&TreeColumn],
    parent_fk: &str,
    pg_types: &std::collections::HashMap<String, String>,
    append_only: bool,
) {
    let cols = row_col_list(row_cols, pg_types, append_only);
    let hints = row_hints(row_cols, append_only);

    for (suffix, include_deleted) in [("", false), ("_including_deleted", true)] {
        let mut clauses = vec![
            "\"id\" = :id".to_string(),
            format!("\"{parent_fk}\" = :parent_id"),
        ];
        if !include_deleted && let Some(sd) = soft_delete_col {
            clauses.push(format!("\"{}\" IS NULL", sd));
        }
        sql.push_str(&format!(
            "--! get_{entity_name}_scoped{suffix} (id, parent_id) : ({hints})\n\
             --- Get a single {entity_name} by ID, scoped to its parent.\n\
             SELECT {cols}\n\
             FROM {table}\n\
             WHERE {};\n\n",
            clauses.join("\n    AND "),
        ));
    }
}

fn write_get_queries(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    soft_delete_col: Option<&str>,
    row_cols: &[&TreeColumn],
    pg_types: &std::collections::HashMap<String, String>,
    append_only: bool,
) {
    let cols = row_col_list(row_cols, pg_types, append_only);
    let hints = row_hints(row_cols, append_only);

    for (suffix, include_deleted) in [("", false), ("_including_deleted", true)] {
        let mut clauses = vec!["\"id\" = :id".to_string()];
        if !include_deleted && let Some(sd) = soft_delete_col {
            clauses.push(format!("\"{}\" IS NULL", sd));
        }
        sql.push_str(&format!(
            "--! get_{entity_name}{suffix} (id) : ({hints})\n\
             --- Get a single {entity_name} by ID.\n\
             SELECT {cols}\n\
             FROM {table}\n\
             WHERE {};\n\n",
            clauses.join("\n    AND "),
        ));
    }
}

// ── `?include=` hydration queries (issue #781 P1) ─────────────────────

/// Generation-time facts for one cornucopia include query.
///
/// Plain data (no tree walk) so the SQL shape is unit-testable without a
/// graph: the emitters build it from the resolved include path + the target's
/// `EntityTree`, and the KATs build it by hand.
struct IncludeQueryFacts<'a> {
    /// The dot-notation alias exactly as the request spells it (the query
    /// function name is derived from it).
    alias: &'a str,
    /// The shape the SQL/adapter/query planes implement for this path (the
    /// SAME classifier the handler reads).
    shape: crate::api::include_plan::CornucopiaIncludeShape,
    /// FK column ON THE SOURCE table referencing the target (`person_id`).
    /// Read only by the scalar shape.
    fk_column: &'a str,
    /// FK column ON THE TARGET table referencing the source (`customer_id`).
    /// Read only by the array shape — it is the predicate of both queries and
    /// the selected group key of the batch query.
    reverse_fk_column: &'a str,
    /// Whether the reverse FK is a REQUIRED genuine `EntityReference`. It
    /// decides the batch key's nullability hint: required ⇒ `source_id`
    /// (a plain `Uuid` row field), optional ⇒ `source_id?` (`Option<Uuid>`),
    /// which is what the adapter's group-key arm matches.
    reverse_fk_is_required: bool,
    /// The target entity's tree (columns, table, soft delete).
    target: &'a EntityTree,
    /// Column → PG type for the target's casts.
    target_pg_types: &'a std::collections::HashMap<String, String>,
}

/// Append one `include_*` query pair per implemented include path.
///
/// Resolution goes through the SAME gate the handler uses
/// ([`crate::api::include_path::resolve_include_paths_for_entity`]) and the
/// SAME per-path verdict ([`crate::api::include_plan::IncludeSupport`]), so the
/// emitted SQL surface cannot advertise a path the handler refuses.
async fn render_include_queries(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    project: &ProjectConfig,
    domain: &str,
    schema_title: &str,
    tree: &EntityTree,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
) -> String {
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(schema_title));
    let paths = crate::api::include_path::resolve_include_paths_for_entity(
        db,
        config,
        domain,
        schema_title,
        entity_cfg,
        project.is_workers_topology(),
    )
    .await
    .unwrap_or_default();
    let support = crate::api::include_plan::include_support_for(project);

    let mut sql = String::new();
    for path in support.supported_paths(&paths) {
        let Some(seg) = path.segments.first() else {
            continue;
        };
        // The SAME classifier the handler reads: only an implemented shape
        // emits SQL (a refusal here is defensive — `supported_paths` already
        // filtered on it).
        let Ok(shape) = crate::api::include_plan::CornucopiaIncludeShape::classify(path) else {
            continue;
        };
        // A shape the plane implements always resolves its target tree; a
        // miss is a graph inconsistency, not a generation decision — skip it
        // (the handler's per-path verdict stays authoritative).
        let Ok(target) = RepositoryImplEmitter
            .query_entity_tree(db, &seg.schema_title, &seg.domain, config, None)
            .await
        else {
            continue;
        };
        let Ok(entity_ir) = build_persistence_entity(
            db,
            &seg.schema_title,
            &seg.domain,
            config,
            parent_candidates,
        )
        .await
        else {
            continue;
        };
        let target_pg_types: std::collections::HashMap<String, String> = entity_ir
            .columns
            .iter()
            .map(|c| (c.column_name.clone(), c.pg_type.clone()))
            .collect();
        let facts = IncludeQueryFacts {
            alias: &path.alias,
            shape,
            fk_column: &seg.fk_column,
            reverse_fk_column: &seg.reverse_fk_column,
            reverse_fk_is_required: seg.reverse_fk_is_required,
            target: &target,
            target_pg_types: &target_pg_types,
        };
        sql.push_str(&render_include_sql(tree, &facts));
    }
    sql
}

/// Render the two annotated queries for one implemented include path.
///
/// **Single scalar** (`include_{alias}_for_{source} (source_id)`): the target
/// row addressed through the SOURCE's own FK column. The FK is read by a
/// correlated subquery rather than a JOIN so the SELECT list stays the plain
/// unqualified `row_col_list` every other query uses (a JOIN makes `"id"`
/// ambiguous), and so RLS applies to the source row as well: a tenant who
/// cannot see the source sees no include.
///
/// **Batch scalar** (`include_{alias}_batch_for_{source} (source_ids)`): one
/// row per (source, target) pair, keyed by `source_id`, so the adapter can
/// build the provider-identical `HashMap<Uuid, Option<{Target}Response>>` the
/// LIST envelope serializes. The inner derived table carries the source id +
/// FK under distinct names, so the target column list is unambiguous.
///
/// **Single array** (`include_{alias}_for_{source}` same signature): every
/// TARGET row whose reverse FK points at the source — the one-to-many shape.
/// No JOIN: the target table alone carries both the predicate column and the
/// selected columns, so the plain `row_col_list` renderer applies verbatim.
///
/// **Batch array** (`…_batch_for_…`): the same rows for many sources in one
/// query, with the reverse FK selected FIRST under the fixed alias
/// `"source_id"` so the adapter groups without depending on the target's
/// column list carrying the FK under its DTO field name (a config-declared
/// child's `parent_ref` column need not appear in the child's schema
/// properties). The annotation's `source_id`/`source_id?` hint is the row
/// type's nullability source — it mirrors `reverse_fk_is_required`, which is
/// exactly the split the adapter's group-key arm matches. Rows whose FK is
/// NULL never match `= ANY(...)`, so the optional arm's `None` is
/// unreachable-by-construction (but required by the type).
///
/// Soft-delete: the TARGET's marker column filters in both queries (matching
/// the SeaORM include fetch, which runs under the default scope — there is no
/// per-include `include_deleted` request parameter).
fn render_include_sql(tree: &EntityTree, facts: &IncludeQueryFacts<'_>) -> String {
    let src_table = format!("\"{}\".\"{}\"", tree.schema_name, tree.table_name);
    let target = facts.target;
    let target_table = format!("\"{}\".\"{}\"", target.schema_name, target.table_name);
    let source = &tree.table_name;
    let alias = facts.alias;
    let alias_snake = facts.alias.replace('.', "_");

    let row_cols: Vec<&TreeColumn> = target
        .direct_columns
        .iter()
        .filter(|c| !c.is_composite_range)
        .collect();
    let cols = row_col_list(&row_cols, facts.target_pg_types, target.append_only);
    let hints = row_hints(&row_cols, target.append_only);
    let soft_delete = target.soft_delete_column.as_deref();

    match facts.shape {
        crate::api::include_plan::CornucopiaIncludeShape::SingleArray => {
            let reverse_fk = facts.reverse_fk_column;
            // `AND` joins the reverse-FK predicate on both queries.
            let single_soft = soft_delete
                .map(|sd| format!("\n  AND \"{sd}\" IS NULL"))
                .unwrap_or_default();
            let batch_soft = soft_delete
                .map(|sd| format!("\n  AND \"{sd}\" IS NULL"))
                .unwrap_or_default();
            let mut sql = String::new();
            sql.push_str(&format!(
                "--! include_{alias_snake}_for_{source} (source_id) : ({hints})\n\
                 --- Eager-load the `{alias}` collection for one {source} row through the \
                 target's reverse FK (issue #781 P1).\n\
                 SELECT {cols}\n\
                 FROM {target_table}\n\
                 WHERE \"{reverse_fk}\" = :source_id{single_soft};\n\n",
            ));
            // The hint is the row type's nullability source: a required FK is a
            // plain `Uuid`, a nullable one an `Option<Uuid>` — the same split
            // the adapter's group-key arm consumes.
            let key_hint = if facts.reverse_fk_is_required {
                "source_id"
            } else {
                "source_id?"
            };
            sql.push_str(&format!(
                "--! include_{alias_snake}_batch_for_{source} (source_ids) : ({key_hint}, {hints})\n\
                 --- Eager-load the `{alias}` collection for many {source} rows, grouped by the \
                 target's reverse FK (issue #781 P1).\n\
                 SELECT \"{reverse_fk}\" AS \"source_id\", {cols}\n\
                 FROM {target_table}\n\
                 WHERE \"{reverse_fk}\" = ANY(:source_ids::uuid[]){batch_soft};\n\n",
            ));
            return sql;
        }
        crate::api::include_plan::CornucopiaIncludeShape::SingleScalar => {}
    }

    let fk = facts.fk_column;
    let batch_cols =
        prefixed_row_col_list("t", &row_cols, facts.target_pg_types, target.append_only);
    let single_soft = soft_delete
        .map(|sd| format!("\n  AND \"{sd}\" IS NULL"))
        .unwrap_or_default();
    let batch_soft = soft_delete
        .map(|sd| format!("\nWHERE \"t\".\"{sd}\" IS NULL"))
        .unwrap_or_default();

    let mut sql = String::new();
    sql.push_str(&format!(
        "--! include_{alias_snake}_for_{source} (source_id) : ({hints})\n\
         --- Eager-load `{alias}` for one {source} row through its own FK (issue #781 P1).\n\
         SELECT {cols}\n\
         FROM {target_table}\n\
         WHERE \"id\" = (SELECT \"{fk}\" FROM {src_table} WHERE \"id\" = :source_id){single_soft}\n\
         LIMIT 1;\n\n",
    ));
    sql.push_str(&format!(
        "--! include_{alias_snake}_batch_for_{source} (source_ids) : (source_id, {hints})\n\
         --- Eager-load `{alias}` for many {source} rows, keyed by source id (issue #781 P1).\n\
         SELECT \"src\".\"source_id\" AS \"source_id\", {batch_cols}\n\
         FROM (\n\
         \x20   SELECT \"id\" AS \"source_id\", \"{fk}\" AS \"target_id\"\n\
         \x20   FROM {src_table}\n\
         \x20   WHERE \"id\" = ANY(:source_ids::uuid[])\n\
         ) AS \"src\"\n\
         JOIN {target_table} AS \"t\" ON \"t\".\"id\" = \"src\".\"target_id\"{batch_soft};\n\n",
    ));
    sql
}

/// `row_col_list` with every column qualified by the JOIN alias — needed when
/// the FROM clause carries more than one table (the batch include query).
fn prefixed_row_col_list(
    prefix: &str,
    cols: &[&TreeColumn],
    pg_types: &std::collections::HashMap<String, String>,
    append_only: bool,
) -> String {
    let mut names: Vec<String> = vec![format!("\"{prefix}\".\"id\"")];
    for c in cols {
        names.push(select_col_prefixed(
            prefix,
            c.field_name.trim_start_matches("r#"),
            c,
            pg_types,
        ));
    }
    names.push(format!("\"{prefix}\".\"created_at\""));
    if !append_only {
        names.push(format!("\"{prefix}\".\"updated_at\""));
    }
    names.join(", ")
}

/// [`select_col`] qualified by a table alias.
fn select_col_prefixed(
    prefix: &str,
    field_name: &str,
    c: &TreeColumn,
    pg_types: &std::collections::HashMap<String, String>,
) -> String {
    let expr = if needs_text_cast(c, pg_types) {
        format!("\"{prefix}\".\"{}\"::text", c.pg_column_name)
    } else {
        format!("\"{prefix}\".\"{}\"", c.pg_column_name)
    };
    if c.pg_column_name == field_name {
        expr
    } else {
        format!("{expr} AS \"{field_name}\"")
    }
}

fn is_writable_col(col: &TreeColumn) -> bool {
    !col.is_workflow_managed && !col.is_composite_range && !col.is_media
}
fn param_name(col: &TreeColumn) -> String {
    col.field_name.trim_start_matches("r#").to_string()
}

/// Annotation param name with the cornucopia `?` nullable marker.
fn param_sig(col: &TreeColumn) -> String {
    if col.is_nullable {
        format!("{}?", param_name(col))
    } else {
        param_name(col)
    }
}

fn write_create_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    tree: &EntityTree,
    pg_types: &std::collections::HashMap<String, String>,
) {
    // The parent FK column (when present in direct_columns) is bound from the
    // path parameter as a trailing :parent_id — keep it out of the DTO-driven
    // column list.
    let writable: Vec<&TreeColumn> = tree
        .direct_columns
        .iter()
        .filter(|c| is_writable_col(c))
        .filter(|c| {
            !tree
                .parent_ref
                .as_ref()
                .is_some_and(|pr| c.pg_column_name.eq_ignore_ascii_case(pr))
        })
        .collect();

    if writable.is_empty() {
        // Entities whose data lives entirely in child tables (or system-managed
        // columns) still need a create query — Postgres fills the defaults.
        sql.push_str(&format!(
            "--! create_{entity_name} : (id)\n\
             --- Create a new {entity_name} (defaults only).\n\
             INSERT INTO {table} DEFAULT VALUES\n\
             RETURNING \"id\";\n\n",
        ));
        return;
    }

    let mut insert_cols: Vec<String> = writable
        .iter()
        .map(|c| format!("\"{}\"", c.pg_column_name))
        .collect();
    let mut insert_params: Vec<String> = writable
        .iter()
        .map(|c| {
            if c.is_structured_wrapper || is_array_col(c) {
                format!(":{}", param_name(c))
            } else {
                format!(":{}::text::{}", param_name(c), pg_cast_for(c, pg_types))
            }
        })
        .collect();
    let mut param_defs: Vec<String> = writable.iter().map(|c| param_sig(c)).collect();
    // Child entities: the parent FK column is not part of the DTO — bind it
    // from the path parameter via a trailing :parent_id param.
    if let Some(ref parent_fk) = tree.parent_ref {
        insert_cols.push(format!("\"{parent_fk}\""));
        insert_params.push(":parent_id".to_string());
        param_defs.push("parent_id".to_string());
    }

    sql.push_str(&format!(
        "--! create_{entity_name} ({}) : (id)\n\
         --- Create a new {entity_name}.\n\
         INSERT INTO {table} ({})\n\
         VALUES ({})\n\
         RETURNING \"id\";\n\n",
        param_defs.join(", "),
        insert_cols.join(", "),
        insert_params.join(", "),
    ));
}

fn write_update_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    tree: &EntityTree,
    soft_delete_col: Option<&str>,
    pg_types: &std::collections::HashMap<String, String>,
) {
    let updatable: Vec<&TreeColumn> = tree
        .direct_columns
        .iter()
        .filter(|c| is_writable_col(c))
        .collect();

    // Append-only entities (#284) have no UPDATE path — the repository never
    // calls update for them, so no query is emitted (the table carries no
    // updated_at column to no-op-touch either).
    if tree.append_only {
        return;
    }

    if updatable.is_empty() {
        sql.push_str(&format!(
            "--! update_{entity_name} (id)\n\
             --- No-op update for {entity_name}.\n\
             UPDATE {table}\n\
             SET \"updated_at\" = \"updated_at\"\n\
             WHERE \"id\" = :id;\n\n",
        ));
        return;
    }

    let set_clauses = updatable
        .iter()
        .map(|c| {
            if c.is_structured_wrapper || is_array_col(c) {
                format!(
                    "\"{}\" = COALESCE(:{}, \"{}\")",
                    c.pg_column_name,
                    param_name(c),
                    c.pg_column_name
                )
            } else {
                format!(
                    "\"{}\" = COALESCE(:{}::text::{}, \"{}\")",
                    c.pg_column_name,
                    param_name(c),
                    pg_cast_for(c, pg_types),
                    c.pg_column_name
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",\n    ");

    let mut params = vec!["id".to_string()];
    for c in &updatable {
        // Update DTO fields are all Option<T> — mark every param nullable so
        // Option binds compile and NULL keeps existing values via COALESCE.
        params.push(format!("{}?", param_name(c)));
    }

    let mut where_clauses = vec!["\"id\" = :id".to_string()];
    if let Some(sd) = soft_delete_col {
        where_clauses.push(format!("\"{}\" IS NULL", sd));
    }

    sql.push_str(&format!(
        "--! update_{entity_name} ({})\n\
         --- Update an existing {entity_name}.\n\
         UPDATE {table}\n\
         SET {set_clauses}\n\
         WHERE {};\n\n",
        params.join(", "),
        where_clauses.join("\n  AND "),
    ));
}

fn write_delete_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    soft_delete_col: Option<&str>,
) {
    let mut where_clauses = vec!["\"id\" = :id".to_string()];
    if let Some(sd) = soft_delete_col {
        where_clauses.push(format!("\"{}\" IS NULL", sd));
    }
    match soft_delete_col {
        Some(sd) => {
            sql.push_str(&format!(
                "--! delete_{entity_name} (id)\n\
                 --- Soft-delete a {entity_name} by setting the deletion marker.\n\
                 UPDATE {table}\n\
                 SET \"{sd}\" = NOW()\n\
                 WHERE {};\n\n",
                where_clauses.join("\n  AND "),
            ));
        }
        None => {
            sql.push_str(&format!(
                "--! delete_{entity_name} (id)\n\
                 --- Hard-delete a {entity_name}.\n\
                 DELETE FROM {table}\n\
                 WHERE {};\n\n",
                where_clauses.join("\n  AND "),
            ));
        }
    }
}

/// Emit list/insert/delete queries for a child (value-object) table.
fn write_child_queries(
    sql: &mut String,
    schema_name: &str,
    entity_name: &str,
    child: &ChildTableInfo,
) {
    let child_table = format!("\"{}\".\"{}\"", schema_name, child.sql_table_name);
    let fk = &child.parent_fk_column;
    // All child columns participate in the row return (the FK is a real
    // property on the child DTO); only the INSERT excludes it.
    let data_cols: Vec<_> = child.columns.iter().collect();
    // Aliased to the child DTO field names — same contract as `select_col`
    // (hints name struct fields; the returned columns must match them).
    let col_names: Vec<String> = data_cols
        .iter()
        .map(|c| {
            let field = c.field_name.trim_start_matches("r#");
            let expr = if c.pg_cast.is_some()
                || matches!(
                    c.rust_type.as_str(),
                    "String" | "Decimal" | "rust_decimal::Decimal"
                ) {
                // Range/geometry/numeric columns: the DTO holds strings — cast
                // to text so tokio-postgres can deserialize them.
                format!("\"{}\"::text", c.pg_column_name)
            } else {
                format!("\"{}\"", c.pg_column_name)
            };
            if c.pg_column_name == field {
                expr
            } else {
                format!("{expr} AS \"{field}\"")
            }
        })
        .collect();

    if col_names.is_empty() {
        sql.push_str(&format!(
            "--! list_{entity_name}_{child} ({fk}) : (id)\n\
             --- Child rows for {entity_name}.\n\
             SELECT \"id\"\n\
             FROM {table}\n\
             WHERE \"{fk}\" = :{fk};\n\n",
            child = child.sql_table_name,
            table = child_table,
        ));
    } else {
        let hints: Vec<String> = data_cols
            .iter()
            .map(|c| {
                let name = c.field_name.trim_start_matches("r#");
                if c.is_nullable {
                    format!("{}?", name)
                } else {
                    name.to_string()
                }
            })
            .collect();
        sql.push_str(&format!(
            "--! list_{entity_name}_{child} ({fk}) : (id, {hints})\n\
             --- Child rows for {entity_name}.\n\
             SELECT \"id\", {cols}\n\
             FROM {table}\n\
             WHERE \"{fk}\" = :{fk};\n\n",
            child = child.sql_table_name,
            hints = hints.join(", "),
            cols = col_names.join(", "),
            table = child_table,
        ));
    }

    // Insert a child row (id generated by the adapter).
    //
    // Issue #707: a MEDIA reference is written only by the dedicated upload
    // handler — the request DTOs omit it, so the adapter never calls these.
    // Emitting them would ship dead SQL that a future caller could reach for.
    if child.is_media {
        return;
    }
    let mut insert_cols = vec!["\"id\"".to_string(), format!("\"{}\"", fk)];
    let mut insert_vals = vec![":id".to_string(), format!(":{}", fk)];
    let mut param_defs = vec!["id".to_string(), fk.clone()];
    for c in &data_cols {
        if c.pg_column_name == *fk {
            continue;
        }
        insert_cols.push(format!("\"{}\"", c.pg_column_name));
        let pname = c.field_name.trim_start_matches("r#");
        if c.rust_type.starts_with("Vec<") {
            insert_vals.push(format!(":{}", pname));
        } else {
            insert_vals.push(format!(":{}::text::{}", pname, child_pg_cast_for(c)));
        }
        param_defs.push(if c.is_nullable {
            format!("{}?", pname)
        } else {
            pname.to_string()
        });
    }
    sql.push_str(&format!(
        "--! insert_{entity_name}_{child} ({})\n\
         --- Insert a child row for {entity_name}.\n\
         INSERT INTO {table} ({})\n\
         VALUES ({});\n\n",
        param_defs.join(", "),
        insert_cols.join(", "),
        insert_vals.join(", "),
        child = child.sql_table_name,
        table = child_table,
    ));

    // Delete child rows by parent FK (used when replacing children on update).
    sql.push_str(&format!(
        "--! delete_{entity_name}_{child} ({fk})\n\
         --- Delete child rows for {entity_name}.\n\
         DELETE FROM {table}\n\
         WHERE \"{fk}\" = :{fk};\n\n",
        child = child.sql_table_name,
        table = child_table,
    ));

    // Nested grandchildren.
    for grandchild in &child.child_tables {
        write_child_queries(sql, schema_name, entity_name, grandchild);
    }
}

/// Emit list/link/unlink queries for a junction (array-of-entity-ref `refers`)
/// table (issue #539). The cornucopia path previously had NO junction writer:
/// the DDL created the table, the DTO accepted the ids, and every create/update
/// silently dropped them. The adapter calls these on create (link per id),
/// update (unlink + re-link), and response construction (list).
fn write_junction_queries(sql: &mut String, tree: &EntityTree, j: &JunctionTableInfo) {
    let entity_name = snake_name(tree);
    let table = format!("\"{}\".\"{}\"", j.sql_schema_name, j.sql_table_name);
    let pfk = &j.parent_fk_column;
    let cfk = &j.child_fk_column;
    sql.push_str(&format!(
        "--! list_{entity}_{junction} ({pfk}) : ({cfk})\n\
         --- Linked ids for {entity}.{field}.\n\
         SELECT \"{cfk}\"\n\
         FROM {table}\n\
         WHERE \"{pfk}\" = :{pfk}\n\
         ORDER BY \"created_at\";\n\n\
         --! link_{entity}_{junction} ({pfk}, {cfk})\n\
         --- Link one {cfk} to a {entity} row.\n\
         INSERT INTO {table} (\"{pfk}\", \"{cfk}\")\n\
         VALUES (:{pfk}, :{cfk});\n\n\
         --! unlink_{entity}_{junction} ({pfk})\n\
         --- Unlink all ids from a {entity} row (update-replace).\n\
         DELETE FROM {table}\n\
         WHERE \"{pfk}\" = :{pfk};\n\n",
        entity = entity_name,
        junction = j.sql_table_name,
        field = j.field_name,
        table = table,
        pfk = pfk,
        cfk = cfk,
    ));
}

/// Emit one helper query per nested (dot-notation) filter. The adapter uses
/// these to filter rows in memory by parent-id membership — the equivalent of
/// the SeaORM EXISTS subqueries.
/// Collect all (recursive) child-table SQL names for an entity.
fn collect_child_table_names(
    children: &[ChildTableInfo],
    out: &mut std::collections::HashSet<String>,
) {
    for child in children {
        out.insert(child.sql_table_name.clone());
        collect_child_table_names(&child.child_tables, out);
    }
}

fn write_nested_filter_queries(sql: &mut String, tree: &EntityTree) {
    // The nested-filter resolver over-generates VO child tables (the DDL may
    // flatten a VO into a scalar column instead). Only emit helper queries for
    // tables that actually exist:
    //   - VO children the DDL materialized (present in tree.child_tables), or
    //   - child-entity filters (parent FK is not the plain {parent_table}_id).
    let vo_pattern = format!("{}_id", tree.table_name);
    let real_vo_tables: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        collect_child_table_names(&tree.child_tables, &mut set);
        set
    };
    for nf in &tree.nested_filter_fields {
        let is_vo_style = nf.parent_fk_column == vo_pattern;
        let is_child_entity = nf.intermediate_join.is_none() && !is_vo_style;
        let is_real_table = real_vo_tables.contains(&nf.sql_table_name);
        if !is_real_table && !is_child_entity {
            continue;
        }
        let qname = nf.filter_key.replace('.', "_");
        let leaf_cast = nested_filter_cast(&nf.rust_type);
        let val_param = if leaf_cast.is_empty() {
            ":val".to_string()
        } else {
            format!(":val::text::{leaf_cast}")
        };
        if let Some(ref ij) = nf.intermediate_join {
            sql.push_str(&format!(
                "--! filter_{qname} (val) : ({fk})\n\
                 --- Nested filter helper: parent ids with a matching grandchild.\n\
                 SELECT _i.\"{fk}\"\n\
                 FROM \"{ij_schema}\".\"{ij_table}\" _i\n\
                 WHERE EXISTS (SELECT 1 FROM \"{schema}\".\"{table}\" _gc\n\
                 \x20       WHERE _gc.\"{gc_fk}\" = _i.\"id\" AND _gc.\"{col}\" = {val_param});\n\n",
                fk = ij.parent_fk_column,
                ij_schema = ij.sql_schema,
                ij_table = ij.sql_table_name,
                schema = nf.sql_schema,
                table = nf.sql_table_name,
                gc_fk = nf.parent_fk_column,
                col = nf.pg_column_name,
            ));
        } else {
            sql.push_str(&format!(
                "--! filter_{qname} (val) : ({fk})\n\
                 --- Nested filter helper: parent ids with a matching child.\n\
                 SELECT \"{fk}\"\n\
                 FROM \"{schema}\".\"{table}\"\n\
                 WHERE \"{col}\" = {val_param};\n\n",
                fk = nf.parent_fk_column,
                schema = nf.sql_schema,
                table = nf.sql_table_name,
                col = nf.pg_column_name,
            ));
        }
    }
}

/// Postgres cast target for a nested-filter value ("" = plain text compare).
fn nested_filter_cast(rust_type: &str) -> &'static str {
    let base = rust_type
        .strip_prefix("Option<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(rust_type);
    match base {
        "Uuid" | "uuid::Uuid" => "uuid",
        // Entity reference types (e.g. "OrganizationType") are always UUID FK columns.
        ty if ty.ends_with("Type") && ty.chars().next().is_some_and(|c| c.is_uppercase()) => "uuid",
        "i32" => "int4",
        "i64" => "int8",
        "f32" => "float4",
        "f64" => "float8",
        "bool" => "bool",
        "Decimal" | "rust_decimal::Decimal" => "numeric",
        "NaiveDate" | "chrono::NaiveDate" => "date",
        "DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => "timestamptz",
        _ => "",
    }
}

fn write_search_queries(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    soft_delete_col: Option<&str>,
    fts_language: &str,
) {
    for (suffix, include_deleted) in [("", false), ("_including_deleted", true)] {
        let extra = if !include_deleted {
            soft_delete_col
                .map(|sd| format!(" AND \"{sd}\" IS NULL"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        sql.push_str(&format!(
            "--! search_{entity_name}{suffix} (q, offset, page_size) : (id)\n\
             --- Full-text search over {entity_name}.\n\
             SELECT \"id\"\n\
             FROM {table}\n\
             WHERE search_tsv @@ websearch_to_tsquery('{fts_language}', :q){extra}\n\
             ORDER BY ts_rank(search_tsv, websearch_to_tsquery('{fts_language}', :q)) DESC\n\
             LIMIT :page_size OFFSET :offset;\n\n",
        ));
        sql.push_str(&format!(
            "--! search_count_{entity_name}{suffix} (q) : (count)\n\
             --- Count of full-text search matches for {entity_name}.\n\
             SELECT COUNT(*) AS \"count\"\n\
             FROM {table}\n\
             WHERE search_tsv @@ websearch_to_tsquery('{fts_language}', :q){extra};\n\n",
        ));
    }
}

fn write_embedding_queries(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    soft_delete_col: Option<&str>,
    emb_col: &str,
) {
    for (suffix, include_deleted) in [("", false), ("_including_deleted", true)] {
        let extra = if !include_deleted {
            soft_delete_col
                .map(|sd| format!(" AND \"{sd}\" IS NULL"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        sql.push_str(&format!(
            "--! semantic_{entity_name}{suffix} (embedding, limit) : (id)\n\
             --- Semantic similarity search over {entity_name}.\n\
             SELECT \"id\"\n\
             FROM {table}\n\
             WHERE \"{emb_col}\" IS NOT NULL{extra}\n\
             ORDER BY \"{emb_col}\" <=> :embedding::text::vector\n\
             LIMIT :limit;\n\n",
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn write_tree_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    hierarchy_field: &str,
    row_cols: &[&TreeColumn],
    pg_types: &std::collections::HashMap<String, String>,
    append_only: bool,
) {
    let cols = row_col_list(row_cols, pg_types, append_only);
    let prefixed: Vec<String> = std::iter::once("c.\"id\"".to_string())
        .chain(row_cols.iter().map(|c| {
            if needs_text_cast(c, pg_types) {
                format!("c.\"{}\"::text", c.pg_column_name)
            } else {
                format!("c.\"{}\"", c.pg_column_name)
            }
        }))
        .chain(std::iter::once("c.\"created_at\"".to_string()))
        .chain(std::iter::once("c.\"updated_at\"".to_string()).filter(|_| !append_only))
        .collect();
    let hints = row_hints(row_cols, append_only);
    sql.push_str(&format!(
        "--! tree_{entity_name} (root_id, max_depth?) : ({hints})\n\
         --- Recursive subtree rooted at a {entity_name}.\n\
         WITH RECURSIVE tree AS (\n\
         \x20 SELECT {cols}, 0 AS _tree_depth FROM {table} WHERE \"id\" = :root_id\n\
         \x20 UNION ALL\n\
         \x20 SELECT {prefixed}, t._tree_depth + 1 AS _tree_depth FROM {table} c JOIN tree t ON c.\"{hierarchy_field}\" = t.\"id\"\n\
         \x20 WHERE (:max_depth::int4 IS NULL OR t._tree_depth < :max_depth::int4)\n\
         )\n\
         SELECT {cols} FROM tree ORDER BY _tree_depth, \"created_at\";\n\n",
        prefixed = prefixed.join(", "),
    ));
}

/// Worker-map query for `tree_include` hydration: deployments ⨝ workers ⨝
/// person ⨝ person-name, keyed by position id — the cornucopia counterpart
/// of the SeaORM find_tree's `deployed_worker` fetch, so both persistence
/// providers return identical tree rows.
fn write_tree_workers_query(sql: &mut String, tree: &EntityTree) {
    let Some(inc) = tree.tree_include.first() else {
        return;
    };
    let mut from_clause = format!(
        "{} d JOIN {} w ON w.\"id\" = d.\"{}\" AND w.deleted_at IS NULL",
        inc.via_table, inc.parent_table, inc.parent_ref_column
    );
    for (i, (table, fk_col, parent_alias)) in inc.worker_detail_joins.iter().enumerate() {
        let alias = if i == 0 { "wp" } else { "wpn" };
        from_clause.push_str(&format!(
            " JOIN {table} {alias} ON {alias}.\"{fk_col}\" = {parent_alias}.\"id\""
        ));
    }
    let worker_json = if inc.worker_detail_joins.is_empty() {
        "jsonb_build_object('id', w.\"id\")".to_string()
    } else {
        "jsonb_build_object('id', w.\"id\", 'given_name', wpn.given, 'family_name', wpn.family, 'avatar_url', wp.avatar_url)".to_string()
    };
    sql.push_str(&format!(
        "--! tree_{snake}_workers (position_ids) : (position_id, deployed_worker)\n\
         --- Deployed-worker map for tree rows, keyed by position id.\n\
         SELECT d.\"{fk}\" AS \"position_id\", {worker_json} AS \"deployed_worker\"\n\
         FROM {from_clause}\n\
         WHERE d.\"{fk}\" = ANY(:position_ids::uuid[]) AND d.deleted_at IS NULL;\n\n",
        snake = tree.table_name,
        fk = inc.via_fk_column,
    ));
}

#[cfg(test)]
mod include_query_tests {
    use super::*;

    fn col(field: &str, pg_column: &str, rust_type: &str, is_nullable: bool) -> TreeColumn {
        TreeColumn {
            field_name: field.to_string(),
            pg_column_name: pg_column.to_string(),
            dto_field_name: None,
            rust_type: rust_type.to_string(),
            is_nullable,
            is_entity_ref: false,
            dto_rust_type: None,
            is_workflow_managed: false,
            is_array: false,
            pg_cast: None,
            is_composite_range: false,
            is_structured_wrapper: false,
            is_media: false,
        }
    }

    fn tree(
        entity: &str,
        table: &str,
        columns: Vec<TreeColumn>,
        soft_delete: Option<&str>,
        append_only: bool,
    ) -> EntityTree {
        EntityTree {
            entity_name: entity.to_string(),
            module_name: table.to_string(),
            schema_name: "common".to_string(),
            table_name: table.to_string(),
            entity_module: format!("common_{table}"),
            namespace: None,
            direct_columns: columns,
            child_tables: vec![],
            junction_tables: vec![],
            has_create: true,
            has_read: true,
            has_update: false,
            has_delete: false,
            has_list: true,
            has_workflow: false,
            has_fts: false,
            has_embeddings: false,
            fts_language: "english".to_string(),
            is_auditable: soft_delete.is_some(),
            soft_delete_visibility: "exclude_by_default".to_string(),
            soft_delete_column: soft_delete.map(str::to_string),
            soft_delete_cascade: "restrict".to_string(),
            track_updated_user: false,
            track_deleted_user: false,
            append_only,
            filter_fields: vec![],
            nested_filter_fields: vec![],
            parent_ref: None,
            hierarchy_field: None,
            tree_include: vec![],
            design_finders: vec![],
        }
    }

    fn facts<'a>(
        alias: &'a str,
        fk: &'a str,
        target: &'a EntityTree,
        pg_types: &'a std::collections::HashMap<String, String>,
    ) -> IncludeQueryFacts<'a> {
        IncludeQueryFacts {
            alias,
            shape: crate::api::include_plan::CornucopiaIncludeShape::SingleScalar,
            fk_column: fk,
            reverse_fk_column: fk,
            reverse_fk_is_required: false,
            target,
            target_pg_types: pg_types,
        }
    }

    fn array_facts<'a>(
        alias: &'a str,
        reverse_fk: &'a str,
        reverse_fk_is_required: bool,
        target: &'a EntityTree,
        pg_types: &'a std::collections::HashMap<String, String>,
    ) -> IncludeQueryFacts<'a> {
        IncludeQueryFacts {
            alias,
            shape: crate::api::include_plan::CornucopiaIncludeShape::SingleArray,
            fk_column: "unused_for_the_array_shape",
            reverse_fk_column: reverse_fk,
            reverse_fk_is_required,
            target,
            target_pg_types: pg_types,
        }
    }

    fn worker() -> EntityTree {
        tree("Worker", "worker", vec![], Some("deleted_at"), false)
    }

    /// The single-fetch query addresses the FK through a correlated subquery
    /// (unambiguous columns, RLS on the source) and filters the TARGET's
    /// soft-delete marker.
    #[test]
    fn single_scalar_include_sql_shape() {
        let target = tree(
            "Person",
            "person",
            vec![
                col("given_name", "given_name", "String", false),
                col("nickname", "nickname", "String", true),
            ],
            Some("deleted_at"),
            false,
        );
        let pg = std::collections::HashMap::new();
        let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

        assert!(
            sql.contains(
                "--! include_person_for_worker (source_id) : (id, given_name, nickname?, created_at, updated_at)"
            ),
            "{sql}"
        );
        assert!(
            sql.contains(
                "FROM \"common\".\"person\"\nWHERE \"id\" = (SELECT \"person_id\" FROM \"common\".\"worker\" WHERE \"id\" = :source_id)\n  AND \"deleted_at\" IS NULL\nLIMIT 1;"
            ),
            "{sql}"
        );
    }

    /// The batch query keys each target row by the source id and qualifies
    /// every target column (the JOIN makes unqualified names ambiguous).
    #[test]
    fn batch_scalar_include_sql_is_keyed_and_qualified() {
        let target = tree(
            "Person",
            "person",
            vec![col("given_name", "given_name", "String", false)],
            None,
            false,
        );
        let pg = std::collections::HashMap::new();
        let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

        assert!(
            sql.contains("--! include_person_batch_for_worker (source_ids) : (source_id, id, given_name, created_at, updated_at)"),
            "{sql}"
        );
        assert!(
            sql.contains("SELECT \"src\".\"source_id\" AS \"source_id\", \"t\".\"id\", \"t\".\"given_name\", \"t\".\"created_at\", \"t\".\"updated_at\""),
            "{sql}"
        );
        assert!(
            sql.contains("WHERE \"id\" = ANY(:source_ids::uuid[])"),
            "{sql}"
        );
        assert!(
            sql.contains(
                "JOIN \"common\".\"person\" AS \"t\" ON \"t\".\"id\" = \"src\".\"target_id\";"
            ),
            "{sql}"
        );
        // No soft-delete filter when the target has no marker.
        assert!(!sql.contains("IS NULL"), "{sql}");
    }

    /// Append-only targets carry no `updated_at` (issue #284), and a
    /// renamed physical column keeps its field-name alias.
    #[test]
    fn append_only_target_omits_updated_at_and_keeps_aliases() {
        let target = tree(
            "Person",
            "person",
            vec![col("gender", "gender_code", "String", false)],
            Some("deleted_at"),
            true,
        );
        let pg = std::collections::HashMap::new();
        let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

        assert!(
            sql.contains("AS \"gender\""),
            "physical column keeps its field alias:\n{sql}"
        );
        assert!(
            sql.contains("(id, gender, created_at)"),
            "append-only target has no updated_at hint:\n{sql}"
        );
        assert!(!sql.contains("updated_at"), "{sql}");
        assert!(
            sql.contains("WHERE \"t\".\"deleted_at\" IS NULL;"),
            "batch filters the target's soft delete:\n{sql}"
        );
    }

    /// Columns whose Postgres type maps to `String` in Rust (numeric, ranges,
    /// geometry) cast to text in both the plain and the prefixed list.
    #[test]
    fn text_cast_applies_to_both_select_forms() {
        let target = tree(
            "Person",
            "person",
            vec![col("salary", "salary", "String", false)],
            None,
            false,
        );
        let mut pg = std::collections::HashMap::new();
        pg.insert("salary".to_string(), "NUMERIC".to_string());
        let sql = render_include_sql(&worker(), &facts("person", "person_id", &target, &pg));

        assert!(sql.contains("\"salary\"::text"), "{sql}");
        assert!(sql.contains("\"t\".\"salary\"::text"), "{sql}");
    }

    /// A reverse-FK target: the child rows live on the target table, and the
    /// predicate is the target's own FK column — no source subquery, no JOIN.
    fn deployment() -> EntityTree {
        tree(
            "Deployment",
            "deployment",
            vec![
                col("reference", "reference", "String", false),
                col("customer_id", "customer_id", "Uuid", false),
            ],
            Some("deleted_at"),
            false,
        )
    }

    /// Array single: the target's reverse FK is the predicate, the target's
    /// soft-delete marker ANDs on, and there is no scalar `LIMIT 1`.
    #[test]
    fn single_array_include_sql_scans_the_reverse_fk() {
        let pg = std::collections::HashMap::new();
        let sql = render_include_sql(
            &tree("Customer", "customer", vec![], None, false),
            &array_facts("deployment", "customer_id", false, &deployment(), &pg),
        );

        assert!(
            sql.contains(
                "--! include_deployment_for_customer (source_id) : (id, reference, customer_id, created_at, updated_at)"
            ),
            "{sql}"
        );
        assert!(
            sql.contains(
                "SELECT \"id\", \"reference\", \"customer_id\", \"created_at\", \"updated_at\"\nFROM \"common\".\"deployment\"\nWHERE \"customer_id\" = :source_id\n  AND \"deleted_at\" IS NULL;"
            ),
            "{sql}"
        );
        assert!(!sql.contains("LIMIT 1"), "{sql}");
        assert!(
            !sql.contains("SELECT \"customer_id\" FROM \"common\".\"customer\""),
            "no source subquery for the reverse shape:\n{sql}"
        );
    }

    /// Array batch: one query for many sources, the reverse FK aliased to the
    /// fixed `source_id` key the adapter groups by, the `uuid[]` cast the
    /// other array-parameter query pins, and the target's soft-delete marker.
    /// An OPTIONAL reverse FK carries the `source_id?` hint (`Option<Uuid>`),
    /// matching the adapter's `None => continue` arm.
    #[test]
    fn batch_array_include_sql_keys_the_reverse_fk() {
        let pg = std::collections::HashMap::new();
        let sql = render_include_sql(
            &tree("Customer", "customer", vec![], None, false),
            &array_facts("deployment", "customer_id", false, &deployment(), &pg),
        );

        assert!(
            sql.contains(
                "--! include_deployment_batch_for_customer (source_ids) : (source_id?, id, reference, customer_id, created_at, updated_at)"
            ),
            "{sql}"
        );
        assert!(
            sql.contains(
                "SELECT \"customer_id\" AS \"source_id\", \"id\", \"reference\", \"customer_id\", \"created_at\", \"updated_at\"\nFROM \"common\".\"deployment\"\nWHERE \"customer_id\" = ANY(:source_ids::uuid[])\n  AND \"deleted_at\" IS NULL;"
            ),
            "{sql}"
        );
        // Single-table shape — no JOIN derived table needed.
        assert!(!sql.contains("JOIN"), "{sql}");
        assert!(!sql.contains("target_id"), "{sql}");
    }

    /// A REQUIRED reverse FK hints `source_id` (plain `Uuid`), the row type the
    /// adapter's `let key = row.source_id;` arm matches.
    #[test]
    fn required_reverse_fk_batch_hint_is_non_nullable() {
        let pg = std::collections::HashMap::new();
        let sql = render_include_sql(
            &tree("Customer", "customer", vec![], None, false),
            &array_facts("invoice", "customer_id", true, &deployment(), &pg),
        );
        assert!(
            sql.contains(
                "--! include_invoice_batch_for_customer (source_ids) : (source_id, id, reference, customer_id, created_at, updated_at)"
            ),
            "{sql}"
        );
        assert!(!sql.contains("source_id?"), "{sql}");
    }

    /// The array shape keeps the append-only (`updated_at` suppression) and
    /// marker-free arms identical to the scalar shape: no invented filter.
    #[test]
    fn array_include_omits_updated_at_and_invents_no_marker() {
        let target = tree(
            "Deployment",
            "deployment",
            vec![col("customer_id", "customer_id", "Uuid", false)],
            None,
            true,
        );
        let pg = std::collections::HashMap::new();
        let sql = render_include_sql(
            &tree("Customer", "customer", vec![], None, false),
            &array_facts("deployment", "customer_id", false, &target, &pg),
        );

        assert!(
            sql.contains("(source_id) : (id, customer_id, created_at)"),
            "append-only target has no updated_at hint:\n{sql}"
        );
        assert!(!sql.contains("updated_at"), "{sql}");
        assert!(!sql.contains("IS NULL"), "no marker is invented:\n{sql}");
    }
}
