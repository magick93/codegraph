use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PersistenceColumnRole, SoftDeleteEffect};

use crate::error::Result;
use crate::generate::persistence::build_persistence_entity;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

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
    fn name(&self) -> &str {
        "cornucopia_queries"
    }

    fn supported_targets(&self) -> Option<Vec<crate::generate::db::dialect::DatabaseTarget>> {
        None
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        _tera: &tera::Tera,
        _project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let entity = build_persistence_entity(
            db,
            schema_title,
            domain,
            config,
            &self.parent_candidates,
        )
        .await?;

        let mut sql = String::with_capacity(4096);

        let table = format!("\"{}\".\"{}\"", entity.schema_name, entity.table_name);
        let pk = "id";
        let tenant_col = entity
            .columns
            .iter()
            .find(|c| c.role == PersistenceColumnRole::TenantScope);
        let tenant_field = tenant_col.map(|c| c.field_name.as_str());

        let sd_effect: Option<&SoftDeleteEffect> = entity.policies.soft_delete.as_ref();
        let soft_delete_col = sd_effect.map(|e| e.marker_column.as_str());

        let data_cols: Vec<_> = entity
            .columns
            .iter()
            .filter(|c| matches!(c.role, PersistenceColumnRole::Data | PersistenceColumnRole::ForeignKey { .. }))
            .collect();
        let return_cols: Vec<_> = entity
            .columns
            .iter()
            .filter(|c| {
                matches!(
                    c.role,
                    PersistenceColumnRole::Data
                        | PersistenceColumnRole::PrimaryKey
                        | PersistenceColumnRole::ForeignKey { .. }
                        | PersistenceColumnRole::TenantScope
                        | PersistenceColumnRole::SoftDeleteMarker
                        | PersistenceColumnRole::AuditTimestamp { .. }
                        | PersistenceColumnRole::AuditFlag
                        | PersistenceColumnRole::HierarchyParent
                )
            })
            .collect();

        let entity_name = codegraph_naming::to_snake_case(&codegraph_naming::strip_suffix(
            &entity.rust_type_name,
            &config.defaults.type_suffix,
        ));
        let module = format!("{}_{}", domain, entity.table_name);

        // ── File header ──────────────────────────────────────────────
        write_sql_header(&mut sql, schema_title, &entity, domain);

        // ── List active (non-deleted) ─────────────────────────────────
        write_list_query(
            &mut sql,
            &table,
            &entity_name,
            &entity,
            tenant_field,
            soft_delete_col,
            &return_cols,
        );

        // ── Get by ID ─────────────────────────────────────────────────
        write_get_by_id_query(
            &mut sql,
            &table,
            &entity_name,
            tenant_field,
            soft_delete_col,
            &return_cols,
        );

        // ── Create ────────────────────────────────────────────────────
        write_create_query(
            &mut sql,
            &table,
            &entity_name,
            &entity,
            &data_cols,
            tenant_field,
            &return_cols,
        );

        // ── Update ────────────────────────────────────────────────────
        write_update_query(
            &mut sql,
            &table,
            &entity_name,
            &entity,
            &data_cols,
            tenant_field,
            soft_delete_col,
        );

        // ── Soft-delete ───────────────────────────────────────────────
        if soft_delete_col.is_some() {
            write_soft_delete_query(
                &mut sql,
                &table,
                &entity_name,
                tenant_field,
                soft_delete_col,
            );
        } else {
            write_hard_delete_query(&mut sql, &table, &entity_name, tenant_field);
        }

        let rel_path = format!("queries/{}/{}.sql", domain, entity.table_name);
        Ok(vec![GeneratedFile {
            path: self.output_dir.join(&rel_path),
            content: sql,
        }])
    }
}

fn write_sql_header(
    sql: &mut String,
    schema_title: &str,
    entity: &codegraph_core::types::PersistenceEntity,
    domain: &str,
) {
    let _ = format!(
        "-- Cornucopia queries for {} (domain: {})\n\
         -- Generated by codegraph — persistence_provider = \"cornucopia\"\n\
         -- Schema: {}\n\n",
        schema_title, domain, entity.table_name,
    );
}

fn write_list_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    entity: &codegraph_core::types::PersistenceEntity,
    tenant_field: Option<&str>,
    soft_delete_col: Option<&str>,
    return_cols: &[&codegraph_core::types::PersistenceColumn],
) {
    let col_list = return_cols
        .iter()
        .map(|c| format!("\"{}\"", c.column_name))
        .collect::<Vec<_>>()
        .join(", ");

    let nullable_hints = return_cols
        .iter()
        .map(|c| if c.is_nullable { format!("{}?", c.field_name) } else { c.field_name.clone() })
        .collect::<Vec<_>>()
        .join(", ");

    let mut params = String::new();
    let mut where_clauses = Vec::new();
    if let Some(tf) = tenant_field {
        params.push_str(&format!("{}", tf));
        where_clauses.push(format!("\"{}\" = :{}", tf, tf));
    }
    if let Some(sd) = soft_delete_col {
        where_clauses.push(format!("\"{}\" IS NULL", sd));
    }

    let where_clause = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("\n  WHERE {}", where_clauses.join("\n    AND "))
    };

    let param_list = if params.is_empty() {
        String::new()
    } else {
        format!(" ({})", params)
    };

    sql.push_str(&format!(
        "--! list_{0}{1} : ({2})\n\
         --- List all active {0} records.\n\
         SELECT {3}\n\
         FROM {4}{5}\n\
         ORDER BY \"created_at\" DESC;\n\n",
        entity_name, param_list, nullable_hints, col_list, table, where_clause,
    ));
}

fn write_get_by_id_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    tenant_field: Option<&str>,
    soft_delete_col: Option<&str>,
    return_cols: &[&codegraph_core::types::PersistenceColumn],
) {
    let col_list = return_cols
        .iter()
        .map(|c| format!("\"{}\"", c.column_name))
        .collect::<Vec<_>>()
        .join(", ");

    let nullable_hints = return_cols
        .iter()
        .map(|c| if c.is_nullable { format!("{}?", c.field_name) } else { c.field_name.clone() })
        .collect::<Vec<_>>()
        .join(", ");

    let mut params = vec!["id".to_string()];
    let mut where_clauses = vec!["\"id\" = :id".to_string()];
    if let Some(tf) = tenant_field {
        params.push(tf.to_string());
        where_clauses.push(format!("\"{}\" = :{}", tf, tf));
    }
    if let Some(sd) = soft_delete_col {
        where_clauses.push(format!("\"{}\" IS NULL", sd));
    }

    let param_list = params.join(", ");
    let where_clause = where_clauses.join("\n    AND ");

    sql.push_str(&format!(
        "--! get_{0} ({1}) : ({2})\n\
         --- Get a single {0} by ID.\n\
         SELECT {3}\n\
         FROM {4}\n\
         WHERE {5};\n\n",
        entity_name, param_list, nullable_hints, col_list, table, where_clause,
    ));
}

fn write_create_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    entity: &codegraph_core::types::PersistenceEntity,
    data_cols: &[&codegraph_core::types::PersistenceColumn],
    tenant_field: Option<&str>,
    return_cols: &[&codegraph_core::types::PersistenceColumn],
) {
    let mut insert_cols = Vec::new();
    let mut insert_params = Vec::new();
    let mut param_defs = Vec::new();

    if let Some(tf) = tenant_field {
        insert_cols.push(format!("\"{}\"", tf));
        insert_params.push(format!(":{}", tf));
        param_defs.push(tf.to_string());
    }

    for col in data_cols {
        let is_create_col = !matches!(
            col.role,
            PersistenceColumnRole::AuditTimestamp { .. }
                | PersistenceColumnRole::AuditUser { .. }
                | PersistenceColumnRole::AuditFlag
                | PersistenceColumnRole::SoftDeleteMarker
                | PersistenceColumnRole::PrimaryKey
                | PersistenceColumnRole::TenantScope
        );
        if is_create_col {
            insert_cols.push(format!("\"{}\"", col.column_name));
            insert_params.push(format!(":{}", col.field_name));
            param_defs.push(col.field_name.clone());
        }
    }

    if insert_cols.is_empty() {
        sql.push_str(&format!(
            "-- No writable data columns for entity: {}\n\n",
            entity_name
        ));
        return;
    }

    let return_col_list = return_cols
        .iter()
        .map(|c| format!("\"{}\"", c.column_name))
        .collect::<Vec<_>>()
        .join(", ");

    let return_hints = return_cols
        .iter()
        .map(|c| if c.is_nullable { format!("{}?", c.field_name) } else { c.field_name.clone() })
        .collect::<Vec<_>>()
        .join(", ");

    sql.push_str(&format!(
        "--! create_{0} ({1}) : ({2})\n\
         --- Create a new {0}.\n\
         INSERT INTO {3} ({4})\n\
         VALUES ({5})\n\
         RETURNING {6};\n\n",
        entity_name,
        param_defs.join(", "),
        return_hints,
        table,
        insert_cols.join(", "),
        insert_params.join(", "),
        return_col_list,
    ));
}

fn write_update_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    entity: &codegraph_core::types::PersistenceEntity,
    data_cols: &[&codegraph_core::types::PersistenceColumn],
    tenant_field: Option<&str>,
    soft_delete_col: Option<&str>,
) {
    let updatable: Vec<_> = data_cols
        .iter()
        .filter(|c| {
            !matches!(
                c.role,
                PersistenceColumnRole::AuditTimestamp { .. }
                    | PersistenceColumnRole::AuditUser { .. }
                    | PersistenceColumnRole::AuditFlag
                    | PersistenceColumnRole::SoftDeleteMarker
                    | PersistenceColumnRole::PrimaryKey
                    | PersistenceColumnRole::TenantScope
            )
        })
        .collect();

    if updatable.is_empty() {
        sql.push_str(&format!(
            "-- No updatable data columns for entity: {}\n\n",
            entity_name
        ));
        return;
    }

    let set_clauses = updatable
        .iter()
        .map(|c| format!("\"{}\" = :{}", c.column_name, c.field_name))
        .collect::<Vec<_>>()
        .join(",\n    ");

    let mut params = vec!["id".to_string()];
    for c in &updatable {
        params.push(c.field_name.clone());
    }
    if let Some(tf) = tenant_field {
        params.push(tf.to_string());
    }

    let mut where_clauses = vec!["\"id\" = :id".to_string()];
    if let Some(tf) = tenant_field {
        where_clauses.push(format!("\"{}\" = :{}", tf, tf));
    }
    if let Some(sd) = soft_delete_col {
        where_clauses.push(format!("\"{}\" IS NULL", sd));
    }

    sql.push_str(&format!(
        "--! update_{0} ({1})\n\
         --- Update an existing {0}.\n\
         UPDATE {2}\n\
         SET {3}\n\
         WHERE {4};\n\n",
        entity_name,
        params.join(", "),
        table,
        set_clauses,
        where_clauses.join("\n  AND "),
    ));
}

fn write_soft_delete_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    tenant_field: Option<&str>,
    soft_delete_col: Option<&str>,
) {
    let sd_col = soft_delete_col.unwrap_or("deleted_at");
    let mut params = vec!["id".to_string()];
    let mut where_clauses = vec!["\"id\" = :id".to_string()];
    if let Some(tf) = tenant_field {
        params.push(tf.to_string());
        where_clauses.push(format!("\"{}\" = :{}", tf, tf));
    }
    where_clauses.push(format!("\"{}\" IS NULL", sd_col));

    sql.push_str(&format!(
        "--! delete_{0} ({1})\n\
         --- Soft-delete a {0} by setting the deletion marker.\n\
         UPDATE {2}\n\
         SET \"{3}\" = NOW()\n\
         WHERE {4};\n\n",
        entity_name,
        params.join(", "),
        table,
        sd_col,
        where_clauses.join("\n  AND "),
    ));
}

fn write_hard_delete_query(
    sql: &mut String,
    table: &str,
    entity_name: &str,
    tenant_field: Option<&str>,
) {
    let mut params = vec!["id".to_string()];
    let mut where_clauses = vec!["\"id\" = :id".to_string()];
    if let Some(tf) = tenant_field {
        params.push(tf.to_string());
        where_clauses.push(format!("\"{}\" = :{}", tf, tf));
    }

    sql.push_str(&format!(
        "--! delete_{0} ({1})\n\
         --- Hard-delete a {0}.\n\
         DELETE FROM {2}\n\
         WHERE {3};\n\n",
        entity_name,
        params.join(", "),
        table,
        where_clauses.join("\n  AND "),
    ));
}
