//! Policy-driven RLS generation from the rexlang actor policy graph
//! (issue #219, from yestechgroup/rexlang#5 phase B6).
//!
//! Reads the ingested policy (`ActorNode` / `CapabilityNode` / `GrantEdge`)
//! from the graph and emits ONE Postgres migration
//! (`020000_policy_rls.sql`, after the per-entity RLS band at 010000+)
//! containing:
//!
//! - **Role bootstrap** — human actors become Postgres group roles
//!   (snake_case via `codegraph-naming`, `NOLOGIN`); agent actors map to the
//!   existing gateway/app role `app_user` (they never hold DB credentials).
//! - **`permit` on a capability bound to class C** — one permissive RLS
//!   policy on C's table `FOR ALL TO <roles>` with the lowered `when`
//!   expression (see [`crate::db::expr_sql`]) as both `USING` and
//!   `WITH CHECK`, plus `GRANT SELECT, INSERT, UPDATE, DELETE` on the table
//!   to the same roles.
//! - **`forbid`** — `REVOKE ALL ON C's table FROM <roles>` (simpler and
//!   auditable; pinned decision). A conditional `forbid` (`when` on a
//!   forbid grant) is refused: REVOKE cannot express a row predicate.
//!
//! Precedence: policy-derived RLS is ADDITIVE with the domains.toml-driven
//! RLS emitted per entity (`db/rls.tera`). Postgres combines permissive
//! policies with OR (a row is visible when ANY permissive policy allows),
//! so these policies can only widen access, never narrow it; the
//! RESTRICTIVE org-isolation/scope/role policies from `rls.tera` still AND
//! on top. Deny-style narrowing is expressed by `forbid` → REVOKE.
//!
//! Postgres only: sqlite has no RLS — the generator is a documented no-op
//! there (both via `supported_targets` and a `has_rls()` guard).
//!
//! Gated behind the `rls_from_policy` profile feature via the `policy_rls`
//! BuildPlan capability; with the flag off the generator never runs and
//! output is byte-identical to pre-feature output.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ActorNode, CapabilityNode, GrantEdge, SchemaNode};
use serde::Serialize;

use crate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::db::expr_sql::lower_when_expr;
use crate::error::{Error, Result};
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;
use codegraph_config::DomainConfig;

/// The Postgres role that agent actors map to: the NOBYPASSRLS gateway/app
/// role created by migration `0002` and used by the serving pool.
pub const AGENT_ROLE: &str = "app_user";

/// Migration sequence for the policy RLS band (after the per-entity RLS
/// band, which starts at 10000).
pub const POLICY_RLS_MIGRATION_SEQ: usize = 20000;

pub struct PolicyRlsGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl PolicyRlsGenerator {
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

/// Resolved, render-ready policy entries grouped per table.
#[derive(Serialize)]
struct PolicyRlsContext {
    /// Human-actor DB roles to create if absent (agents reuse `app_user`).
    roles: Vec<String>,
    tables: Vec<PolicyRlsTable>,
}

#[derive(Serialize)]
struct PolicyRlsTable {
    schema: String,
    table: String,
    capabilities: Vec<PolicyRlsCapability>,
}

#[derive(Serialize)]
struct PolicyRlsCapability {
    name: String,
    class: String,
    policy_name: String,
    permit_roles: Vec<String>,
    forbid_roles: Vec<String>,
    using_sql: String,
}

#[async_trait]
impl GlobalGenerator for PolicyRlsGenerator {
    fn name(&self) -> &str {
        "policy_rls"
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
        // Documented no-op: sqlite has no RLS.
        if !self.dialect.has_rls() {
            return Ok(vec![]);
        }

        let actors = db.get_actors().await.map_err(Error::Graph)?;
        let capabilities = db.get_capabilities().await.map_err(Error::Graph)?;
        let grants = db.get_grants().await.map_err(Error::Graph)?;
        if actors.is_empty() && capabilities.is_empty() && grants.is_empty() {
            return Ok(vec![]);
        }

        let resolved = resolve_policy(db, &actors, &capabilities, &grants, config).await?;
        if resolved.tables.is_empty() {
            return Ok(vec![]);
        }

        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "policy_rls"),
            &resolved,
            project,
        )?;
        Ok(vec![GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir)
                .join(format!("{:06}_policy_rls.sql", POLICY_RLS_MIGRATION_SEQ)),
            content,
        }])
    }
}

/// Per-capability accumulated grant decisions across all actors.
#[derive(Default)]
struct CapabilityGrants {
    class: Option<String>,
    permit_roles: BTreeSet<String>,
    forbid_roles: BTreeSet<String>,
    /// Distinct lowered `when` predicates from permit grants (sorted).
    predicates: BTreeSet<String>,
    /// True when any permit grant carries no `when`.
    unconditional: bool,
}

fn role_for(actor: &ActorNode) -> String {
    match actor.kind.as_deref() {
        Some("agent") => AGENT_ROLE.to_string(),
        // Humans, and actors with no declared kind (the default audience).
        _ => codegraph_naming::to_snake_case(&actor.name),
    }
}

/// Strip the rexlang package qualifier: the class name is the last `::`
/// segment of `capability.class`.
fn unqualified_class(class: &str) -> String {
    class.rsplit("::").next().unwrap_or(class).to_string()
}

/// OR-join lowered per-grant predicates for one capability; an empty list
/// means the grant set is unconditional (`true`). Lowered predicates are
/// already fully parenthesized, so joining adds exactly one outer pair.
fn join_predicates(predicates: &[String]) -> String {
    match predicates {
        [] => "true".to_string(),
        [one] => one.clone(),
        many => format!("({})", many.join(" OR ")),
    }
}

/// Resolve the schema node for a capability's class: exact title first, then
/// the configured type suffix (`Candidate` → `CandidateType`).
async fn schema_for_class(
    db: &dyn GraphQuerier,
    class: &str,
    type_suffix: &str,
) -> Option<SchemaNode> {
    let unqualified = unqualified_class(class);
    if let Some(schema) = db.get_schema(&unqualified).await.ok().flatten() {
        return Some(schema);
    }
    let suffixed = format!("{unqualified}{type_suffix}");
    db.get_schema(&suffixed).await.ok().flatten()
}

/// Walk the policy graph and build the render context. Any `when`
/// expression outside the mappable subset, any conditional forbid, and any
/// class that does not resolve to an ingested entity schema is a HARD
/// generation error naming the capability.
async fn resolve_policy(
    db: &dyn GraphQuerier,
    actors: &[ActorNode],
    capabilities: &[CapabilityNode],
    grants: &[GrantEdge],
    config: &DomainConfig,
) -> Result<PolicyRlsContext> {
    let type_suffix = &config.defaults.type_suffix;
    let actor_roles: HashMap<&str, String> = actors
        .iter()
        .map(|actor| (actor.name.as_str(), role_for(actor)))
        .collect();

    let capability_class: HashMap<&str, &str> = capabilities
        .iter()
        .map(|cap| (cap.name.as_str(), cap.class.as_str()))
        .collect();

    let mut per_capability: BTreeMap<String, CapabilityGrants> = BTreeMap::new();
    for actor in actors {
        for permit in codegraph_core::types::resolve_effective_permits(actors, grants, &actor.name)
        {
            let entry = per_capability.entry(permit.capability.clone()).or_default();
            if entry.class.is_none() {
                entry.class = capability_class
                    .get(permit.capability.as_str())
                    .map(|c| (*c).to_string());
            }
            let role = actor_roles
                .get(actor.name.as_str())
                .cloned()
                .unwrap_or_else(|| codegraph_naming::to_snake_case(&actor.name));
            match permit.effect.as_str() {
                "forbid" => {
                    if permit.when.is_some() {
                        return Err(Error::Validation(format!(
                            "capability `{}`: `when` condition on a forbid \
                             grant cannot be lowered to a REVOKE; REVOKE \
                             cannot express a row predicate — drop the \
                             `when` or use a permit with the negation",
                            permit.capability
                        )));
                    }
                    entry.forbid_roles.insert(role);
                }
                _ => {
                    if let Some(when) = &permit.when {
                        let class = entry.class.clone().unwrap_or_default();
                        let schema =
                            schema_for_class(db, &class, type_suffix)
                                .await
                                .ok_or_else(|| {
                                    Error::Validation(class_not_mapped_error(
                                        &permit.capability,
                                        &class,
                                    ))
                                })?;
                        let fields = fields_for_schema(db, &schema).await?;
                        let predicate = lower_when_expr(when, &permit.capability, &class, &fields)
                            .map_err(Error::Validation)?;
                        entry.predicates.insert(predicate);
                    } else {
                        entry.unconditional = true;
                    }
                    entry.permit_roles.insert(role);
                }
            }
        }
    }

    // Group entries per (schema, table).
    let mut tables: BTreeMap<(String, String), Vec<PolicyRlsCapability>> = BTreeMap::new();
    let mut roles: BTreeSet<String> = BTreeSet::new();
    for (name, grants) in &per_capability {
        let class = grants.class.clone().unwrap_or_default();
        let schema = schema_for_class(db, &class, type_suffix)
            .await
            .ok_or_else(|| Error::Validation(class_not_mapped_error(name, &class)))?;
        if !schema.is_entity || schema.pg_table_name.is_empty() {
            return Err(Error::Validation(class_not_mapped_error(name, &class)));
        }
        let pg_schema = schema
            .domain
            .as_deref()
            .and_then(|domain| config.domains.get(domain))
            .map(|entry| entry.postgres_schema.clone())
            .unwrap_or_else(|| schema.domain.clone().unwrap_or_default());

        let mut ordered_predicates: Vec<String> = grants.predicates.iter().cloned().collect();
        ordered_predicates.sort();
        let using_sql = if grants.unconditional || ordered_predicates.is_empty() {
            "true".to_string()
        } else {
            join_predicates(&ordered_predicates)
        };

        let permit_roles: Vec<String> = grants.permit_roles.iter().cloned().collect();
        let forbid_roles: Vec<String> = grants.forbid_roles.iter().cloned().collect();
        roles.extend(permit_roles.iter().cloned());
        roles.extend(forbid_roles.iter().cloned());

        tables
            .entry((pg_schema, schema.pg_table_name.clone()))
            .or_default()
            .push(PolicyRlsCapability {
                policy_name: format!("policy_{}", codegraph_naming::to_snake_case(name)),
                name: name.clone(),
                class,
                permit_roles,
                forbid_roles,
                using_sql,
            });
    }

    // `app_user` already exists (migration 0002); never re-create it.
    roles.remove(AGENT_ROLE);

    Ok(PolicyRlsContext {
        roles: roles.into_iter().collect(),
        tables: tables
            .into_iter()
            .map(|((schema, table), capabilities)| PolicyRlsTable {
                schema,
                table,
                capabilities,
            })
            .collect(),
    })
}

fn class_not_mapped_error(capability: &str, class: &str) -> String {
    format!(
        "capability `{capability}`: class `{class}` does not resolve to an \
         ingested entity schema, so no RLS table can be derived"
    )
}

/// Feature name → column map for a schema: both the property name as
/// written and its snake_case form resolve to `pg_column_name`.
async fn fields_for_schema(
    db: &dyn GraphQuerier,
    schema: &SchemaNode,
) -> Result<HashMap<String, String>> {
    let properties = db
        .get_properties(&schema.title)
        .await
        .map_err(Error::Graph)?;
    let mut fields = HashMap::new();
    for property in &properties {
        fields.insert(property.name.clone(), property.pg_column_name.clone());
        fields.insert(
            codegraph_naming::to_snake_case(&property.name),
            property.pg_column_name.clone(),
        );
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(name: &str, kind: Option<&str>) -> codegraph_core::types::ActorNode {
        codegraph_core::types::ActorNode {
            name: name.to_string(),
            kind: kind.map(|k| k.to_string()),
            extends: None,
            block: Some("core".to_string()),
        }
    }

    #[test]
    fn human_actors_map_to_snake_case_db_roles() {
        assert_eq!(role_for(&actor("Recruiter", Some("human"))), "recruiter");
        assert_eq!(
            role_for(&actor("AuditManager", Some("human"))),
            "audit_manager"
        );
    }

    #[test]
    fn agent_actors_map_to_the_gateway_role() {
        assert_eq!(role_for(&actor("Helper", Some("agent"))), AGENT_ROLE);
    }

    #[test]
    fn unspecified_kind_defaults_to_human() {
        assert_eq!(role_for(&actor("Finance", None)), "finance");
    }

    #[test]
    fn class_takes_the_last_qualified_segment() {
        assert_eq!(
            unqualified_class("rex.conformance.actors::Ticket"),
            "Ticket"
        );
        assert_eq!(
            unqualified_class("recruiting::CandidateType"),
            "CandidateType"
        );
        assert_eq!(unqualified_class("CandidateType"), "CandidateType");
    }

    #[test]
    fn predicates_are_or_joined_and_parenthesized() {
        assert_eq!(
            join_predicates(&["(a > 0)".into(), "(b)".into()]),
            "((a > 0) OR (b))"
        );
        assert_eq!(join_predicates(&["(a > 0)".into()]), "(a > 0)");
    }

    #[test]
    fn empty_predicate_list_means_unconditional_access() {
        assert_eq!(join_predicates(&[]), "true");
    }
}
