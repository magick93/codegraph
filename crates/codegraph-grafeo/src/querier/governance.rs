use std::collections::HashMap;

use codegraph_core::error::GraphError;
use codegraph_core::types::{
    resolve_effective_permits, ActorNode, ActorPolicyNode, CapabilityNode, ConditionNode,
    DelegationRecord, ErrorDefinitionNode, FunctionNode, GrantEdge, MembershipNode,
    MoxDerivedFeatureNode, MoxOperationNode, MoxVocabularyNode, NeverBothGroup, PermissionNode,
    Permit, PipelineNode, PolicyNode, RegulatoryEdgeKind, RegulatoryNode, RegulatoryRefRecord,
    RelationshipNode, RuleNode, RuleRefRecord, SecurityIdentityNode, TenantNode,
};

use super::{query_gql, query_gql_params};
use crate::conversions::{
    row_to_condition_node, row_to_function_node, row_to_membership_node, row_to_policy_node,
    row_to_regulatory_node, row_to_regulatory_ref_record, row_to_relationship_node,
    row_to_rule_node, row_to_security_identity_node, row_to_tenant_node, RowReader,
};
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    pub(super) async fn query_error_definitions(&self) -> Result<Vec<ErrorDefinitionNode>, GraphError> {
        let gql = "MATCH (ed:ErrorDefinition) RETURN ed.code, ed.description, ed.http_status, ed.domain ORDER BY ed.code";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(ErrorDefinitionNode {
                code: reader.get_string(row, "ed.code")?,
                description: reader.get_string(row, "ed.description")?,
                http_status: reader.get_i32(row, "ed.http_status")?,
                domain: reader.get_opt_string(row, "ed.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_permissions(&self) -> Result<Vec<PermissionNode>, GraphError> {
        let gql = "MATCH (pm:Permission) RETURN pm.name, pm.domain ORDER BY pm.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(PermissionNode {
                name: reader.get_string(row, "pm.name")?,
                domain: reader.get_opt_string(row, "pm.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_pipelines(&self) -> Result<Vec<PipelineNode>, GraphError> {
        let gql = "MATCH (pl:Pipeline) RETURN pl.name, pl.middleware, pl.domain ORDER BY pl.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            let middleware_str: Option<String> = reader.get_opt_string(row, "pl.middleware")?;
            let middleware: Option<Vec<String>> =
                middleware_str.and_then(|s| serde_json::from_str(&s).ok());
            nodes.push(PipelineNode {
                name: reader.get_string(row, "pl.name")?,
                middleware,
                domain: reader.get_opt_string(row, "pl.domain")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_pipeline_for_endpoint(
        &self,
        endpoint_path: &str,
    ) -> Result<Option<PipelineNode>, GraphError> {
        let params = HashMap::from([(
            "path".to_string(),
            grafeo::Value::String(endpoint_path.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (he:HttpEndpoint {path_template: $path})-[:UsesPipeline]->(pl:Pipeline) RETURN pl.name, pl.middleware, pl.domain",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        let row = &result.rows[0];
        let middleware_str: Option<String> = reader.get_opt_string(row, "pl.middleware")?;
        let middleware: Option<Vec<String>> =
            middleware_str.and_then(|s| serde_json::from_str(&s).ok());
        Ok(Some(PipelineNode {
            name: reader.get_string(row, "pl.name")?,
            middleware,
            domain: reader.get_opt_string(row, "pl.domain")?,
        }))
    }

    // ── Persistence Metamodel queries ─────────────────────────────────

    pub(super) async fn query_policies_for_schema(
        &self,
        schema_title: &str,
    ) -> Result<Vec<PolicyNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (p:Policy {target_schema: $title}) RETURN p.name AS name, \
             p.kind_json AS kind_json, p.target_schema AS target_schema, p.domain AS domain",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_policy_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_relationships_for_schema(
        &self,
        schema_title: &str,
    ) -> Result<Vec<RelationshipNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (r:Relationship) \
             WHERE r.source_schema = $title OR r.target_schema = $title \
             RETURN r.name AS name, r.source_schema AS source_schema, \
             r.target_schema AS target_schema, r.cardinality AS cardinality, \
             r.ownership AS ownership, r.fk_json AS fk_json, \
             r.propagation_json AS propagation_json, r.domain AS domain",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_relationship_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_relationship_by_name(
        &self,
        name: &str,
    ) -> Result<Option<RelationshipNode>, GraphError> {
        let params = HashMap::from([("name".to_string(), grafeo::Value::String(name.into()))]);
        let result = query_gql_params(
            self,
            "MATCH (r:Relationship {name: $name}) \
             RETURN r.name AS name, r.source_schema AS source_schema, \
             r.target_schema AS target_schema, r.cardinality AS cardinality, \
             r.ownership AS ownership, r.fk_json AS fk_json, \
             r.propagation_json AS propagation_json, r.domain AS domain",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_relationship_node(&reader, &result.rows[0])?))
    }

    pub(super) async fn query_all_policies(&self) -> Result<Vec<PolicyNode>, GraphError> {
        let gql = "MATCH (p:Policy) RETURN p.name AS name, p.kind_json AS kind_json, \
                   p.target_schema AS target_schema, p.domain AS domain";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_policy_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_all_relationships(&self) -> Result<Vec<RelationshipNode>, GraphError> {
        let gql = "MATCH (r:Relationship) \
                   RETURN r.name AS name, r.source_schema AS source_schema, \
                   r.target_schema AS target_schema, r.cardinality AS cardinality, \
                   r.ownership AS ownership, r.fk_json AS fk_json, \
                   r.propagation_json AS propagation_json, r.domain AS domain";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_relationship_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_security_identity(
        &self,
        subject: &str,
    ) -> Result<Option<SecurityIdentityNode>, GraphError> {
        let params =
            HashMap::from([("subject".to_string(), grafeo::Value::String(subject.into()))]);
        let result = query_gql_params(
            self,
            "MATCH (s:SecurityIdentity {subject: $subject}) \
             RETURN s.name AS name, s.subject AS subject, s.domain AS domain",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_security_identity_node(
            &reader,
            &result.rows[0],
        )?))
    }

    pub(super) async fn query_memberships_for_identity(
        &self,
        identity_name: &str,
    ) -> Result<Vec<MembershipNode>, GraphError> {
        let params = HashMap::from([(
            "identity".to_string(),
            grafeo::Value::String(identity_name.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (m:Membership {identity: $identity}) \
             RETURN m.identity AS identity, m.tenant AS tenant, m.status AS status, \
             m.roles_json AS roles_json, m.valid_from AS valid_from, \
             m.valid_until AS valid_until",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_membership_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_tenant(&self, name: &str) -> Result<Option<TenantNode>, GraphError> {
        let params = HashMap::from([("name".to_string(), grafeo::Value::String(name.into()))]);
        let result = query_gql_params(
            self,
            "MATCH (t:Tenant {name: $name}) \
             RETURN t.name AS name, t.label AS label, t.strategy_json AS strategy_json, \
             t.domain AS domain",
            params,
        )?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        Ok(Some(row_to_tenant_node(&reader, &result.rows[0])?))
    }

    pub(super) async fn query_all_tenants(&self) -> Result<Vec<TenantNode>, GraphError> {
        let gql = "MATCH (t:Tenant) RETURN t.name AS name, t.label AS label, \
                   t.strategy_json AS strategy_json, t.domain AS domain";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_tenant_node(&reader, row))
            .collect()
    }

    // ── Authorization metamodel queries ───────────────────────────────

    pub(super) async fn query_actors(&self) -> Result<Vec<ActorNode>, GraphError> {
        let gql = "MATCH (a:Actor) RETURN a.name, a.kind, a.extends, a.block ORDER BY a.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(ActorNode {
                name: reader.get_string(row, "a.name")?,
                kind: reader.get_opt_string(row, "a.kind")?,
                extends: reader.get_opt_string(row, "a.extends")?,
                block: reader.get_opt_string(row, "a.block")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_capabilities(&self) -> Result<Vec<CapabilityNode>, GraphError> {
        let gql = "MATCH (c:Capability) RETURN c.name, c.class, c.block ORDER BY c.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            nodes.push(CapabilityNode {
                name: reader.get_string(row, "c.name")?,
                class: reader.get_string(row, "c.class")?,
                block: reader.get_opt_string(row, "c.block")?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_grants(&self) -> Result<Vec<GrantEdge>, GraphError> {
        let gql = "MATCH (a:Actor)-[g:Grant]->(c:Capability) \
                   RETURN a.name, c.name, g.effect, g.when_expr, g.obligations \
                   ORDER BY a.name, c.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut grants = Vec::new();
        for row in &result.rows {
            let obligations_json = reader.get_opt_string(row, "g.obligations")?;
            let obligations: Vec<String> = obligations_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            grants.push(GrantEdge {
                actor: reader.get_string(row, "a.name")?,
                capability: reader.get_string(row, "c.name")?,
                effect: reader.get_string(row, "g.effect")?,
                when: reader.get_opt_string(row, "g.when_expr")?,
                expr_json: None,
                obligations,
            });
        }
        Ok(grants)
    }

    pub(super) async fn query_actor_policy(&self) -> Result<Option<ActorPolicyNode>, GraphError> {
        let gql = "MATCH (p:ActorPolicy) RETURN p.blocks, p.never_both, p.purposes, p.delegations LIMIT 1";
        let result = query_gql(self, gql)?;
        if result.rows.is_empty() {
            return Ok(None);
        }
        let reader = RowReader::from_columns(&result.columns);
        let row = &result.rows[0];
        let blocks: Vec<String> = reader
            .get_opt_string(row, "p.blocks")?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let never_both: Vec<NeverBothGroup> = reader
            .get_opt_string(row, "p.never_both")?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let purposes: Vec<String> = reader
            .get_opt_string(row, "p.purposes")?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let delegations: Vec<DelegationRecord> = reader
            .get_opt_string(row, "p.delegations")?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Ok(Some(ActorPolicyNode {
            blocks,
            never_both,
            purposes,
            delegations,
        }))
    }

    pub(super) async fn query_effective_permits(&self, actor: &str) -> Result<Vec<Permit>, GraphError> {
        let actors = self.query_actors().await?;
        let grants = self.query_grants().await?;
        Ok(resolve_effective_permits(&actors, &grants, actor))
    }

    // ── mox domain metamodel queries ──────────────────────────────────

    pub(super) async fn query_mox_vocabularies(&self) -> Result<Vec<MoxVocabularyNode>, GraphError> {
        let gql = "MATCH (v:Vocabulary) RETURN v.name, v.package, v.source, v.version, \
                   v.key_facet, v.facets_json, v.entries_json ORDER BY v.package, v.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            let facets_json = reader.get_string(row, "v.facets_json")?;
            let entries_json = reader.get_string(row, "v.entries_json")?;
            nodes.push(MoxVocabularyNode {
                name: reader.get_string(row, "v.name")?,
                package: reader.get_string(row, "v.package")?,
                source: reader.get_string(row, "v.source")?,
                version: reader.get_opt_string(row, "v.version")?,
                key_facet: reader.get_string(row, "v.key_facet")?,
                facets: serde_json::from_str(&facets_json)
                    .map_err(|e| GraphError::Query(format!("invalid vocabulary facets: {e}")))?,
                entries: serde_json::from_str(&entries_json)
                    .map_err(|e| GraphError::Query(format!("invalid vocabulary entries: {e}")))?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_mox_operations(&self) -> Result<Vec<MoxOperationNode>, GraphError> {
        let gql = "MATCH (o:Operation) RETURN o.name, o.class, o.package, o.description, \
                   o.return_type, o.params_json, o.bodies_json ORDER BY o.class, o.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut nodes = Vec::new();
        for row in &result.rows {
            let params_json = reader.get_string(row, "o.params_json")?;
            let bodies_json = reader.get_string(row, "o.bodies_json")?;
            nodes.push(MoxOperationNode {
                name: reader.get_string(row, "o.name")?,
                class: reader.get_string(row, "o.class")?,
                package: reader.get_string(row, "o.package")?,
                description: reader.get_opt_string(row, "o.description")?,
                return_type: reader.get_string(row, "o.return_type")?,
                params: serde_json::from_str(&params_json)
                    .map_err(|e| GraphError::Query(format!("invalid operation params: {e}")))?,
                bodies: serde_json::from_str(&bodies_json)
                    .map_err(|e| GraphError::Query(format!("invalid operation bodies: {e}")))?,
            });
        }
        Ok(nodes)
    }

    pub(super) async fn query_mox_derived_features(&self) -> Result<Vec<MoxDerivedFeatureNode>, GraphError> {
        let gql = "MATCH (d:DerivedFeature) RETURN d.name, d.class, d.package, d.type_ref, d.expr \
                   ORDER BY d.class, d.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| mox_derived_feature_from_row(&reader, row, "d"))
            .collect()
    }

    pub(super) async fn query_mox_derived_features_for_schema(
        &self,
        schema_title: &str,
    ) -> Result<Vec<MoxDerivedFeatureNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (d:DerivedFeature)-[:BelongsToClass]->(s:Schema {title: $title}) \
             RETURN d.name, d.class, d.package, d.type_ref, d.expr ORDER BY d.name",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| mox_derived_feature_from_row(&reader, row, "d"))
            .collect()
    }

    // ── Constraint plane queries (issue #261) ─────────────────────────

    pub(super) async fn query_conditions_for_schema(
        &self,
        schema_title: &str,
    ) -> Result<Vec<ConditionNode>, GraphError> {
        let params = HashMap::from([(
            "title".to_string(),
            grafeo::Value::String(schema_title.into()),
        )]);
        let result = query_gql_params(
            self,
            "MATCH (c:Condition) WHERE c.owner_title = $title \
             RETURN c.name AS name, c.owner_title AS owner_title, c.kind AS kind, \
             c.expr_json AS expr_json, c.options AS options, \
             c.definition AS definition, c.domain AS domain \
             ORDER BY c.name",
            params,
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_condition_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_conditions(&self) -> Result<Vec<ConditionNode>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (c:Condition) \
             RETURN c.name AS name, c.owner_title AS owner_title, c.kind AS kind, \
             c.expr_json AS expr_json, c.options AS options, \
             c.definition AS definition, c.domain AS domain \
             ORDER BY c.owner_title, c.name",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_condition_node(&reader, row))
            .collect()
    }

    // ── Regulatory reference plane queries (issue #265) ───────────────

    pub(super) async fn query_regulatory(&self) -> Result<Vec<RegulatoryNode>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (r:Regulatory) \
             RETURN r.name AS name, r.kind AS kind, r.label AS label, \
             r.definition AS definition, r.domain AS domain, \
             r.properties_json AS properties_json \
             ORDER BY r.kind, r.name",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_regulatory_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_regulatory_references(&self) -> Result<Vec<RegulatoryRefRecord>, GraphError> {
        // One MATCH per (edge family × owner label) combination — GQL has
        // no union edge pattern, and the owner label is part of the read
        // back record. Owners use their natural keys (Schema → title,
        // Condition/Regulatory → name).
        let mut records = Vec::new();
        for (label, edge_kind) in [
            ("RegulatoryReference", RegulatoryEdgeKind::Reference),
            ("HasRuleSource", RegulatoryEdgeKind::RuleSource),
            ("CorpusInBody", RegulatoryEdgeKind::CorpusInBody),
            ("DerivesFrom", RegulatoryEdgeKind::DerivesFrom),
        ] {
            // Only RegulatoryReference carries the ref_path property —
            // selecting it on the other families would read undeclared
            // properties.
            let ref_select = match edge_kind {
                RegulatoryEdgeKind::Reference => "e.ref_path AS ref_path",
                _ => "NULL AS ref_path",
            };
            for (owner_label, owner_key) in [
                ("Schema", "title"),
                ("Condition", "name"),
                ("Regulatory", "name"),
                ("Function", "name"),
                ("Rule", "name"),
            ] {
                let gql = format!(
                    "MATCH (a:{owner_label})-[e:{label}]->(b:Regulatory) \
                     RETURN a.{owner_key} AS owner, b.name AS target, \
                     b.kind AS target_kind, {ref_select} \
                     ORDER BY owner, target"
                );
                let result = query_gql(self, &gql)?;
                let reader = RowReader::from_columns(&result.columns);
                for row in &result.rows {
                    let mut record = row_to_regulatory_ref_record(&reader, row, edge_kind)?;
                    record.owner_label = owner_label.to_string();
                    records.push(record);
                }
            }
        }
        records.sort_by(|a, b| (&a.owner, &a.target).cmp(&(&b.owner, &b.target)));
        Ok(records)
    }

    // ── Computation plane queries (issue #263) ─────────────────────────

    pub(super) async fn query_functions(&self) -> Result<Vec<FunctionNode>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (f:Function) \
             RETURN f.name AS name, f.domain AS domain, f.definition AS definition, \
             f.extends_function AS extends_function, f.payload_json AS payload_json \
             ORDER BY f.domain, f.name",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_function_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_function_extends(&self) -> Result<Vec<(String, String)>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (a:Function)-[:FunctionExtends]->(b:Function) \
             RETURN a.name AS child, b.name AS parent ORDER BY child",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| {
                Ok((
                    reader.get_string(row, "child")?,
                    reader.get_string(row, "parent")?,
                ))
            })
            .collect()
    }

    // ── Rule plane queries (issue #264) ────────────────────────────────

    pub(super) async fn query_rules(&self) -> Result<Vec<RuleNode>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (r:Rule) \
             RETURN r.name AS name, r.domain AS domain, r.definition AS definition, \
             r.kind AS kind, r.input_type AS input_type, r.payload_json AS payload_json \
             ORDER BY r.domain, r.name",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| row_to_rule_node(&reader, row))
            .collect()
    }

    pub(super) async fn query_rule_applies_to(&self) -> Result<Vec<(String, String)>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (a:Rule)-[:RuleAppliesTo]->(b:Schema) \
             RETURN a.name AS rule, b.title AS schema_title ORDER BY rule",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| {
                Ok((
                    reader.get_string(row, "rule")?,
                    reader.get_string(row, "schema_title")?,
                ))
            })
            .collect()
    }

    pub(super) async fn query_rule_references(&self) -> Result<Vec<RuleRefRecord>, GraphError> {
        let result = query_gql(
            self,
            "MATCH (a:Schema)-[e:RuleReference]->(b:Rule) \
             RETURN a.title AS schema_title, e.ref_path AS attribute, \
             b.name AS rule, e.rule_source AS rule_source \
             ORDER BY rule_source, schema_title, attribute",
        )?;
        let reader = RowReader::from_columns(&result.columns);
        result
            .rows
            .iter()
            .map(|row| {
                Ok(RuleRefRecord {
                    schema_title: reader.get_string(row, "schema_title")?,
                    attribute: reader.get_string(row, "attribute")?,
                    rule: reader.get_string(row, "rule")?,
                    rule_source: reader.get_string(row, "rule_source")?,
                })
            })
            .collect()
    }
}

/// Map a `DerivedFeature` query row to its node. `alias` is the query's
/// column prefix.
fn mox_derived_feature_from_row(
    reader: &RowReader,
    row: &[grafeo::Value],
    alias: &str,
) -> Result<MoxDerivedFeatureNode, GraphError> {
    let col = |name: &str| format!("{alias}.{name}");
    Ok(MoxDerivedFeatureNode {
        name: reader.get_string(row, &col("name"))?,
        class: reader.get_string(row, &col("class"))?,
        package: reader.get_string(row, &col("package"))?,
        type_ref: reader.get_string(row, &col("type_ref"))?,
        expr: reader.get_opt_string(row, &col("expr"))?,
    })
}
