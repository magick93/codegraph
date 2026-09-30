mod api_model;
mod atproto;
mod composition;
mod governance;
mod ifml;
mod schema;

use std::collections::{HashMap, VecDeque};

use async_trait::async_trait;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{
    ActionNode, ActorNode, ActorPolicyNode, ApiOperationNode, ApiResourceNode, CapabilityNode,
    CodeList, CollectionNode, CompositeColumn, CompositeRange, CompositionTree,
    DataBindingResolution, EnumValue, ErrorDefinitionNode, EventNode, Extension, GrantEdge,
    HttpEndpointNode, InteractionNode, LexiconNode, MembershipNode, MoxDerivedFeatureNode,
    MoxOperationNode, MoxVocabularyNode, NamespaceNode, NavigationFlowRecord,
    ParameterDefinitionNode, ParentCandidate, PermissionNode, Permit, PipelineNode, PolicyNode,
    PropertyNode, RelationshipNode, RepositoryNode, SchemaClassificationData, SchemaNode,
    SecurityIdentityNode, StructuredSubField, TenantNode, ViewComponentNode, ViewContainerNode,
};

use crate::conversions::{row_to_property_node, RowReader};
use crate::engine::GrafeoEngine;

/// The RETURN clause for all SchemaNode queries — keeps the 22 columns in one place.
pub(super) const SCHEMA_RETURN_COLS: &str = "\
    s.schema_id, s.title, s.description, \
    s.schema_type, s.classification, s.domain, s.rel_path, s.pg_type, s.rust_type, \
    s.sea_orm_type, s.rust_type_name, s.pg_table_name, s.api_path_segment, \
    s.parent_schema, s.is_entity, s.is_codelist, s.is_primitive_wrapper, \
    s.has_all_of, s.has_one_of, s.has_any_of, s.has_definitions, s.custom_annotations";

/// The RETURN clause for all PropertyNode queries — keeps the 17 columns in one place.
pub(super) const PROPERTY_RETURN_COLS: &str = "\
    p.name, p.prop_type, p.description, p.format, \
    p.is_required, p.is_nullable, p.is_array, p.pattern, \
    p.pg_column_name, p.pg_column_type, p.rust_field_name, p.rust_field_type, \
    p.sea_orm_type, p.render_strategy, p.ref_target, p.classification, \
    p.classification_kind";

/// Query result wrapper holding columns and rows from Grafeo.
pub(super) struct QResult {
    columns: Vec<String>,
    rows: Vec<Vec<grafeo::Value>>,
}

pub(super) fn query_gql(engine: &GrafeoEngine, gql: &str) -> Result<QResult, GraphError> {
    let session = engine.db().session();
    let result = session
        .execute(gql)
        .map_err(|e| GraphError::Query(format!("{e}")))?;
    let rows = result.rows().to_vec();
    Ok(QResult {
        columns: result.columns,
        rows,
    })
}

/// Execute a parameterized GQL query. Grafeo can cache query plans for
/// parameterized queries, avoiding repeated parsing of the same template.
pub(super) fn query_gql_params(
    engine: &GrafeoEngine,
    gql: &str,
    params: HashMap<String, grafeo::Value>,
) -> Result<QResult, GraphError> {
    let result = engine
        .db()
        .execute_with_params(gql, params)
        .map_err(|e| GraphError::Query(format!("{e}")))?;
    let rows = result.rows().to_vec();
    Ok(QResult {
        columns: result.columns,
        rows,
    })
}

impl GrafeoEngine {
    pub(super) async fn query_generation_order(&self) -> Result<Vec<String>, GraphError> {
        // Get all schema titles
        let all_result = query_gql(self, "MATCH (s:Schema) RETURN s.title")?;
        let reader = RowReader::from_columns(&all_result.columns);
        let all_titles: Vec<String> = all_result
            .rows
            .iter()
            .map(|row| reader.get_string(row, "s.title"))
            .collect::<Result<_, _>>()?;

        // Get DependsOn edges
        let edge_result = query_gql(
            self,
            "MATCH (a:Schema)-[:DependsOn]->(b:Schema) RETURN a.title, b.title",
        )?;
        let edge_reader = RowReader::from_columns(&edge_result.columns);

        // Build adjacency and in-degree
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

        for title in &all_titles {
            in_degree.insert(title.clone(), 0);
            adjacency.entry(title.clone()).or_default();
        }

        for row in &edge_result.rows {
            let from = edge_reader.get_string(row, "a.title")?;
            let to = edge_reader.get_string(row, "b.title")?;
            // from depends on to, so to must come first
            adjacency.entry(to.clone()).or_default().push(from.clone());
            *in_degree.entry(from).or_default() += 1;
        }

        // Kahn's algorithm
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(t, _)| t.clone())
            .collect();
        queue.make_contiguous().sort(); // deterministic ordering

        let mut order = Vec::new();
        while let Some(current) = queue.pop_front() {
            order.push(current.clone());
            if let Some(dependents) = adjacency.get(&current) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep.clone());
                        }
                    }
                }
            }
        }

        // Append any remaining (cycles)
        for title in &all_titles {
            if !order.contains(title) {
                order.push(title.clone());
            }
        }

        Ok(order)
    }

    pub(super) async fn query_all_schema_references(
        &self,
    ) -> Result<Vec<(String, String)>, GraphError> {
        let gql = "MATCH (s:Schema)-[:HasProperty]->(:Property)-[:ReferencesSchema]->(t:Schema) \
                   RETURN DISTINCT s.title, t.title";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut refs = Vec::new();
        for row in &result.rows {
            let src = reader.get_string(row, "s.title")?;
            let tgt = reader.get_string(row, "t.title")?;
            refs.push((src, tgt));
        }
        Ok(refs)
    }

    pub(super) async fn query_all_properties(
        &self,
    ) -> Result<HashMap<String, Vec<PropertyNode>>, GraphError> {
        let gql = format!(
            "MATCH (s:Schema)-[:HasProperty]->(p:Property) RETURN s.title, {PROPERTY_RETURN_COLS}"
        );
        let result = query_gql(self, &gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut map: HashMap<String, Vec<PropertyNode>> = HashMap::new();
        for row in &result.rows {
            let schema_title = reader.get_string(row, "s.title")?;
            let prop = row_to_property_node(&reader, row)?;
            map.entry(schema_title).or_default().push(prop);
        }
        Ok(map)
    }
}

#[async_trait]
impl GraphQuerier for GrafeoEngine {
    async fn get_schema(&self, title: &str) -> Result<Option<SchemaNode>, GraphError> {
        self.query_schema(title).await
    }

    async fn get_schema_by_id(&self, schema_id: &str) -> Result<Option<SchemaNode>, GraphError> {
        self.query_schema_by_id(schema_id).await
    }

    async fn get_schema_in_domain(
        &self,
        title: &str,
        domain: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        self.query_schema_in_domain(title, domain).await
    }

    async fn list_schemas(&self, domain: Option<&str>) -> Result<Vec<SchemaNode>, GraphError> {
        self.query_schemas(domain).await
    }

    async fn get_properties(&self, schema_title: &str) -> Result<Vec<PropertyNode>, GraphError> {
        self.query_properties(schema_title).await
    }

    async fn get_properties_in_domain(
        &self,
        schema_title: &str,
        domain: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        self.query_properties_in_domain(schema_title, domain).await
    }

    async fn get_child_schemas(&self, schema_title: &str) -> Result<Vec<SchemaNode>, GraphError> {
        self.query_child_schemas(schema_title).await
    }

    async fn get_classification_data(&self) -> Result<Vec<SchemaClassificationData>, GraphError> {
        self.query_classification_data().await
    }

    async fn get_entity_names(&self) -> Result<Vec<String>, GraphError> {
        self.query_entity_names().await
    }

    async fn get_entity_schema_map(&self) -> Result<HashMap<String, String>, GraphError> {
        self.query_entity_schema_map().await
    }

    async fn get_value_object_schemas(&self) -> Result<Vec<SchemaNode>, GraphError> {
        self.query_value_object_schemas().await
    }

    async fn get_parent_candidates(&self) -> Result<Vec<ParentCandidate>, GraphError> {
        self.query_parent_candidates().await
    }

    async fn get_codelist(&self, name: &str) -> Result<Option<CodeList>, GraphError> {
        self.query_codelist(name).await
    }

    async fn list_codelists(&self) -> Result<Vec<CodeList>, GraphError> {
        self.query_codelists().await
    }

    async fn get_enum_values(&self, codelist_name: &str) -> Result<Vec<EnumValue>, GraphError> {
        self.query_enum_values(codelist_name).await
    }

    async fn get_composite_columns(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Vec<CompositeColumn>, GraphError> {
        self.query_composite_columns(property_name, schema_title)
            .await
    }

    async fn get_structured_sub_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<StructuredSubField>, GraphError> {
        self.query_structured_sub_fields(schema_title).await
    }

    async fn get_composite_range(
        &self,
        schema_title: &str,
    ) -> Result<Option<CompositeRange>, GraphError> {
        self.query_composite_range(schema_title).await
    }

    async fn get_consumed_fields(
        &self,
        schema_title: &str,
    ) -> Result<Vec<(PropertyNode, String)>, GraphError> {
        self.query_consumed_fields(schema_title).await
    }

    async fn get_codelist_for_property(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<(CodeList, String)>, GraphError> {
        self.query_codelist_for_property(property_name, schema_title)
            .await
    }

    async fn get_required_extensions(
        &self,
        schema_title: &str,
    ) -> Result<Vec<Extension>, GraphError> {
        self.query_required_extensions(schema_title).await
    }

    async fn get_composition_tree(
        &self,
        schema_title: &str,
    ) -> Result<CompositionTree, GraphError> {
        self.query_composition_tree(schema_title).await
    }

    async fn get_allof_targets(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        self.query_allof_targets(schema_title).await
    }

    async fn get_schemas_that_extend(
        &self,
        parent_title: &str,
    ) -> Result<Vec<SchemaNode>, GraphError> {
        self.query_schemas_that_extend(parent_title).await
    }

    async fn get_referencing_schemas(&self, schema_title: &str) -> Result<Vec<String>, GraphError> {
        self.query_referencing_schemas(schema_title).await
    }

    async fn get_referenced_schemas(
        &self,
        schema_title: &str,
    ) -> Result<Vec<SchemaNode>, GraphError> {
        self.query_referenced_schemas(schema_title).await
    }

    async fn get_property_ref_target(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        self.query_property_ref_target(property_name, schema_title)
            .await
    }

    async fn get_property_ref_target_by_id(
        &self,
        property_name: &str,
        schema_id: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        self.query_property_ref_target_by_id(property_name, schema_id)
            .await
    }

    async fn get_properties_by_schema_id(
        &self,
        schema_id: &str,
    ) -> Result<Vec<PropertyNode>, GraphError> {
        self.query_properties_by_schema_id(schema_id).await
    }

    async fn get_array_item_schema(
        &self,
        property_name: &str,
        schema_title: &str,
    ) -> Result<Option<SchemaNode>, GraphError> {
        self.query_array_item_schema(property_name, schema_title)
            .await
    }

    async fn get_generation_order(&self) -> Result<Vec<String>, GraphError> {
        self.query_generation_order().await
    }

    async fn list_all_schema_references(&self) -> Result<Vec<(String, String)>, GraphError> {
        self.query_all_schema_references().await
    }

    async fn list_all_properties(&self) -> Result<HashMap<String, Vec<PropertyNode>>, GraphError> {
        self.query_all_properties().await
    }

    async fn get_ifml_view_containers(&self) -> Result<Vec<ViewContainerNode>, GraphError> {
        self.query_ifml_view_containers().await
    }

    async fn get_ifml_container_children(
        &self,
        parent: &str,
    ) -> Result<Vec<ViewContainerNode>, GraphError> {
        self.query_ifml_container_children(parent).await
    }

    async fn get_ifml_view_components(
        &self,
        container_name: &str,
    ) -> Result<Vec<ViewComponentNode>, GraphError> {
        self.query_ifml_view_components(container_name).await
    }

    async fn get_ifml_events(&self, parent_id: &str) -> Result<Vec<EventNode>, GraphError> {
        self.query_ifml_events(parent_id).await
    }

    async fn get_ifml_navigation_flows(&self) -> Result<Vec<NavigationFlowRecord>, GraphError> {
        self.query_ifml_navigation_flows().await
    }

    async fn get_ifml_action_triggers(&self) -> Result<Vec<(String, String)>, GraphError> {
        self.query_ifml_action_triggers().await
    }

    async fn get_ifml_data_flows(
        &self,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, GraphError> {
        self.query_ifml_data_flows().await
    }

    async fn get_ifml_actions(&self) -> Result<Vec<ActionNode>, GraphError> {
        self.query_ifml_actions().await
    }

    async fn get_ifml_parameters(&self) -> Result<Vec<ParameterDefinitionNode>, GraphError> {
        self.query_ifml_parameters().await
    }

    async fn get_parameters_for_view(
        &self,
        container_name: &str,
    ) -> Result<Vec<ParameterDefinitionNode>, GraphError> {
        self.query_parameters_for_view(container_name).await
    }

    async fn get_data_bindings(&self) -> Result<Vec<DataBindingResolution>, GraphError> {
        self.query_data_bindings().await
    }

    async fn get_namespaces(&self) -> Result<Vec<NamespaceNode>, GraphError> {
        self.query_namespaces().await
    }

    async fn get_lexicons(&self, domain: &str) -> Result<Vec<LexiconNode>, GraphError> {
        self.query_lexicons(domain).await
    }

    async fn get_lexicon_by_schema(
        &self,
        schema_title: &str,
    ) -> Result<Option<LexiconNode>, GraphError> {
        self.query_lexicon_by_schema(schema_title).await
    }

    async fn get_collections(&self, domain: &str) -> Result<Vec<CollectionNode>, GraphError> {
        self.query_collections(domain).await
    }

    async fn get_repositories(&self) -> Result<Vec<RepositoryNode>, GraphError> {
        self.query_repositories().await
    }

    async fn get_lexicon_references(&self, nsid: &str) -> Result<Vec<LexiconNode>, GraphError> {
        self.query_lexicon_references(nsid).await
    }

    async fn get_api_resources(&self) -> Result<Vec<ApiResourceNode>, GraphError> {
        self.query_api_resources().await
    }

    async fn get_api_resource(&self, name: &str) -> Result<Option<ApiResourceNode>, GraphError> {
        self.query_api_resource(name).await
    }

    async fn get_api_operations(
        &self,
        resource_name: &str,
    ) -> Result<Vec<ApiOperationNode>, GraphError> {
        self.query_api_operations(resource_name).await
    }

    async fn get_api_operation(&self, name: &str) -> Result<Option<ApiOperationNode>, GraphError> {
        self.query_api_operation(name).await
    }

    async fn get_http_endpoint_for_operation(
        &self,
        operation_name: &str,
    ) -> Result<Option<HttpEndpointNode>, GraphError> {
        self.query_http_endpoint_for_operation(operation_name).await
    }

    async fn get_interactions(
        &self,
        _operation_name: &str,
    ) -> Result<Vec<InteractionNode>, GraphError> {
        self.query_interactions(_operation_name).await
    }

    async fn get_http_endpoints(&self) -> Result<Vec<HttpEndpointNode>, GraphError> {
        self.query_http_endpoints().await
    }

    async fn get_error_definitions(&self) -> Result<Vec<ErrorDefinitionNode>, GraphError> {
        self.query_error_definitions().await
    }

    async fn get_permissions(&self) -> Result<Vec<PermissionNode>, GraphError> {
        self.query_permissions().await
    }

    async fn get_pipelines(&self) -> Result<Vec<PipelineNode>, GraphError> {
        self.query_pipelines().await
    }

    async fn get_pipeline_for_endpoint(
        &self,
        endpoint_path: &str,
    ) -> Result<Option<PipelineNode>, GraphError> {
        self.query_pipeline_for_endpoint(endpoint_path).await
    }

    async fn get_policies_for_schema(
        &self,
        schema_title: &str,
    ) -> Result<Vec<PolicyNode>, GraphError> {
        self.query_policies_for_schema(schema_title).await
    }

    async fn get_relationships_for_schema(
        &self,
        schema_title: &str,
    ) -> Result<Vec<RelationshipNode>, GraphError> {
        self.query_relationships_for_schema(schema_title).await
    }

    async fn get_relationship_by_name(
        &self,
        name: &str,
    ) -> Result<Option<RelationshipNode>, GraphError> {
        self.query_relationship_by_name(name).await
    }

    async fn list_all_policies(&self) -> Result<Vec<PolicyNode>, GraphError> {
        self.query_all_policies().await
    }

    async fn list_all_relationships(&self) -> Result<Vec<RelationshipNode>, GraphError> {
        self.query_all_relationships().await
    }

    async fn get_security_identity(
        &self,
        subject: &str,
    ) -> Result<Option<SecurityIdentityNode>, GraphError> {
        self.query_security_identity(subject).await
    }

    async fn get_memberships_for_identity(
        &self,
        identity_name: &str,
    ) -> Result<Vec<MembershipNode>, GraphError> {
        self.query_memberships_for_identity(identity_name).await
    }

    async fn get_tenant(&self, name: &str) -> Result<Option<TenantNode>, GraphError> {
        self.query_tenant(name).await
    }

    async fn list_all_tenants(&self) -> Result<Vec<TenantNode>, GraphError> {
        self.query_all_tenants().await
    }

    async fn get_actors(&self) -> Result<Vec<ActorNode>, GraphError> {
        self.query_actors().await
    }

    async fn get_capabilities(&self) -> Result<Vec<CapabilityNode>, GraphError> {
        self.query_capabilities().await
    }

    async fn get_grants(&self) -> Result<Vec<GrantEdge>, GraphError> {
        self.query_grants().await
    }

    async fn get_actor_policy(&self) -> Result<Option<ActorPolicyNode>, GraphError> {
        self.query_actor_policy().await
    }

    async fn effective_permits(&self, actor: &str) -> Result<Vec<Permit>, GraphError> {
        self.query_effective_permits(actor).await
    }

    async fn get_mox_vocabularies(&self) -> Result<Vec<MoxVocabularyNode>, GraphError> {
        self.query_mox_vocabularies().await
    }

    async fn get_mox_operations(&self) -> Result<Vec<MoxOperationNode>, GraphError> {
        self.query_mox_operations().await
    }

    async fn get_mox_derived_features(&self) -> Result<Vec<MoxDerivedFeatureNode>, GraphError> {
        self.query_mox_derived_features().await
    }

    async fn get_mox_derived_features_for_schema(
        &self,
        schema_title: &str,
    ) -> Result<Vec<MoxDerivedFeatureNode>, GraphError> {
        self.query_mox_derived_features_for_schema(schema_title)
            .await
    }
}
