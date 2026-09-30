//! Public-operations RLS + route gating (issue #279), the consumption
//! half of the per-definition access flag and the deferred #169 Q3 gap.
//!
//! Reads the graph for schemas carrying `access = Public` whose entity
//! config declares `public_operations`, and emits ONE Postgres migration
//! (`021000_public_operations_rls.sql`, after `020000_policy_rls.sql`)
//! with permissive `TO PUBLIC` row-security policies plus the matching
//! `GRANT`s for the serving role. The route half lives in the router
//! generator: `route_auth_is_public` skips the permission layers for
//! entities whose ENTIRE operation set is public.
//!
//! Safety semantics mirror `policy_rls` (#219): the emitted policies are
//! PERMISSIVE, so Postgres OR-combines them with any other permissive
//! policies (only widening access); RESTRICTIVE org-isolation/scope/role
//! policies from the per-entity `rls.tera` still AND on top — a public
//! flag can never bypass a restrictive policy.
//!
//! Gated behind the `public_operations_rls` profile feature via the
//! BuildPlan capability and the `ProjectConfig` flag; both off (the
//! defaults) mean the generator never runs and output is byte-identical.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{Access, SchemaNode};
use serde::Serialize;

use crate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::error::{Error, Result};
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use crate::GenerationEntry;
use codegraph_config::DomainConfig;

/// The serving role created by migration `0002` — the only role that needs
/// explicit table grants for public reads/writes to reach clients.
pub const SERVING_ROLE: &str = "app_user";

/// Migration sequence for the public-operations RLS band (after the
/// policy RLS band at 20000).
pub const PUBLIC_OPERATIONS_RLS_MIGRATION_SEQ: usize = 21000;

pub struct PublicOperationsRlsGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl PublicOperationsRlsGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            dialect: dialect_for_target(DatabaseTarget::Postgres),
        }
    }

    pub fn with_dialect(mut self, dialect: Box<dyn SqlDialect>) -> Self {
        self.dialect = dialect;
        self
    }
}

#[derive(Serialize)]
struct PublicRlsContext {
    tables: Vec<PublicRlsTable>,
}

#[derive(Serialize)]
struct PublicRlsTable {
    schema: String,
    table: String,
    /// `read` | `create` | `update` | `delete` — the Postgres actions the
    /// public policies grant, derived from the configured operations.
    actions: Vec<String>,
    policies: Vec<PublicRlsPolicy>,
}

#[derive(Serialize)]
struct PublicRlsPolicy {
    policy_name: String,
    /// `SELECT` | `INSERT` | `UPDATE` | `DELETE`
    action: String,
    operation: String,
    /// `USING (true)` for readable/deletable rows; `WITH CHECK (true)` for
    /// inserted/updated rows (rendered inside the template).
    needs_using: bool,
    needs_check: bool,
}

/// Map a configured operation onto its Postgres policy action. Unknown
/// operations are skipped (they produce no route and no policy).
fn action_for_operation(operation: &str) -> Option<(&'static str, bool, bool)> {
    match operation {
        "list" | "read" => Some(("SELECT", true, false)),
        "create" => Some(("INSERT", false, true)),
        "update" => Some(("UPDATE", true, true)),
        "delete" => Some(("DELETE", true, false)),
        _ => None,
    }
}

#[async_trait]
impl GlobalGenerator for PublicOperationsRlsGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::PublicOperationsRls
    }

    fn supported_targets(&self) -> Option<Vec<DatabaseTarget>> {
        Some(vec![DatabaseTarget::Postgres])
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &crate::ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Documented no-ops: sqlite has no RLS; flag off = byte-identical.
        if !project.integration.public_operations_rls || !self.dialect.has_rls() {
            return Ok(vec![]);
        }

        let schemas = db.list_schemas(None).await.map_err(Error::Graph)?;
        let mut tables: BTreeMap<(String, String), PublicRlsTable> = BTreeMap::new();
        for schema in &schemas {
            if let Some(table) = public_table(schema, config) {
                tables.entry((table.0, table.1)).or_insert(table.2);
            }
        }
        if tables.is_empty() {
            return Ok(vec![]);
        }

        let context = PublicRlsContext {
            tables: tables.into_values().collect(),
        };
        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "public_operations_rls"),
            &context,
            project,
        )?;
        Ok(vec![GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir).join(format!(
                "{:06}_public_operations_rls.sql",
                PUBLIC_OPERATIONS_RLS_MIGRATION_SEQ
            )),
            content,
        }])
    }
}

/// Resolve the render-ready table entry for a schema: an entity carrying
/// `access = Public` whose domain config declares `public_operations`.
/// Returns `(pg_schema, pg_table, entry)`.
fn public_table(
    schema: &SchemaNode,
    config: &DomainConfig,
) -> Option<(String, String, PublicRlsTable)> {
    if schema.access != Some(Access::Public) || !schema.is_entity || schema.pg_table_name.is_empty()
    {
        return None;
    }
    let domain = config.domains.get(schema.domain.as_deref()?)?;
    let entity_cfg = domain.get_entity_config(&schema.title);
    let public_ops: &[String] = entity_cfg
        .and_then(|ec| ec.public_operations.as_deref())
        .unwrap_or(&[]);
    if public_ops.is_empty() {
        return None;
    }

    let mut policies = Vec::new();
    for operation in public_ops {
        let Some((action, needs_using, needs_check)) = action_for_operation(operation) else {
            continue;
        };
        policies.push(PublicRlsPolicy {
            policy_name: format!(
                "public_{}_{}",
                operation,
                codegraph_naming::to_snake_case(&schema.pg_table_name)
            ),
            action: action.to_string(),
            operation: (*operation).to_string(),
            needs_using,
            needs_check,
        });
    }
    if policies.is_empty() {
        return None;
    }

    let mut actions: Vec<String> = policies.iter().map(|p| p.action.clone()).collect();
    actions.sort();
    actions.dedup();

    Some((
        domain.postgres_schema.clone(),
        schema.pg_table_name.clone(),
        PublicRlsTable {
            schema: domain.postgres_schema.clone(),
            table: schema.pg_table_name.clone(),
            actions,
            policies,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_map_onto_postgres_actions() {
        assert_eq!(action_for_operation("list"), Some(("SELECT", true, false)));
        assert_eq!(
            action_for_operation("create"),
            Some(("INSERT", false, true))
        );
        assert_eq!(action_for_operation("update"), Some(("UPDATE", true, true)));
        assert_eq!(
            action_for_operation("delete"),
            Some(("DELETE", true, false))
        );
        assert_eq!(action_for_operation("search"), None);
    }
}
