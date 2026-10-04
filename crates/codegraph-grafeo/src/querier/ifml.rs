use std::collections::HashMap;

use codegraph_core::error::GraphError;
use codegraph_core::types::{
    ActionNode, DataBindingResolution, EventNode, ModuleUseRecord, NavigationFlowRecord,
    ParameterDefinitionNode, ViewComponentNode, ViewContainerNode, strip_ifml_prefix,
};

use super::query::{query_gql, query_many, query_many_params};
use crate::conversions::RowReader;
use crate::engine::GrafeoEngine;

impl GrafeoEngine {
    // ── IFML query methods ─────────────────────────────────────────────

    pub(super) async fn query_ifml_view_containers(
        &self,
    ) -> Result<Vec<ViewContainerNode>, GraphError> {
        query_many(
            self,
            "MATCH (vc:ViewContainer) RETURN \
            vc.name, vc.label, vc.is_xor, vc.is_default, \
            vc.is_landmark, vc.is_modal, vc.conditional_expression, vc.domain, \
            vc.module_uses, vc.roles, vc.requires \
            ORDER BY vc.name",
            |reader, row| view_container_from_row(reader, row, "vc"),
        )
        .await
    }

    pub(super) async fn query_ifml_container_children(
        &self,
        parent: &str,
    ) -> Result<Vec<ViewContainerNode>, GraphError> {
        let params = HashMap::from([(
            "name".to_string(),
            grafeo::Value::String(strip_ifml_prefix(parent).into()),
        )]);
        query_many_params(
            self,
            "MATCH (p:ViewContainer {name: $name})-[e:ContainsViewContainer]->(c:ViewContainer) RETURN \
             c.name, c.label, c.is_xor, c.is_default, \
             c.is_landmark, c.is_modal, c.conditional_expression, c.domain, \
             c.module_uses, c.roles, c.requires \
             ORDER BY e.sort_order, c.name",
            params,
            |reader, row| view_container_from_row(reader, row, "c"),
        )
        .await
    }

    pub(super) async fn query_ifml_view_components(
        &self,
        container_name: &str,
    ) -> Result<Vec<ViewComponentNode>, GraphError> {
        let params = HashMap::from([(
            "name".to_string(),
            grafeo::Value::String(container_name.into()),
        )]);
        query_many_params(
            self,
            "MATCH (vc:ViewContainer {name: $name})-[:ContainsViewComponent]->(comp:ViewComponent) \
             RETURN comp.name, comp.component_type, comp.mode, comp.entity, \
             comp.fields, comp.filter, comp.api_operation, comp.spec, \
             comp.conditional_expression, comp.domain ORDER BY comp.name",
            params,
            |reader, row| {
                let fields_str: Option<String> = reader.get_opt_string(row, "comp.fields")?;
                let fields: Option<Vec<String>> =
                    fields_str.and_then(|s| serde_json::from_str(&s).ok());
                Ok(ViewComponentNode {
                    name: reader.get_string(row, "comp.name")?,
                    component_type: reader.get_string(row, "comp.component_type")?,
                    mode: reader.get_opt_string(row, "comp.mode")?,
                    entity: reader.get_opt_string(row, "comp.entity")?,
                    fields,
                    filter: reader.get_opt_string(row, "comp.filter")?,
                    api_operation: reader.get_opt_string(row, "comp.api_operation")?,
                    spec: reader.get_opt_string(row, "comp.spec")?,
                    conditional_expression: reader
                        .get_opt_string(row, "comp.conditional_expression")?,
                    expr_json: None,
                    domain: reader.get_opt_string(row, "comp.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_ifml_events(
        &self,
        parent_id: &str,
    ) -> Result<Vec<EventNode>, GraphError> {
        let params = HashMap::from([(
            "name".to_string(),
            grafeo::Value::String(strip_ifml_prefix(parent_id).into()),
        )]);
        query_many_params(
            self,
            "MATCH (parent)-[:HasEvent]->(evt:Event) \
             WHERE parent.name = $name \
             RETURN evt.name, evt.event_type, evt.params, \
             evt.conditional_expression, evt.requires, evt.domain ORDER BY evt.name",
            params,
            |reader, row| {
                let params_str: Option<String> = reader.get_opt_string(row, "evt.params")?;
                let params: Option<Vec<String>> =
                    params_str.and_then(|s| serde_json::from_str(&s).ok());
                let requires: Vec<String> = reader
                    .get_opt_string(row, "evt.requires")?
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                Ok(EventNode {
                    name: reader.get_string(row, "evt.name")?,
                    event_type: reader.get_string(row, "evt.event_type")?,
                    params,
                    conditional_expression: reader
                        .get_opt_string(row, "evt.conditional_expression")?,
                    expr_json: None,
                    requires,
                    domain: reader.get_opt_string(row, "evt.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_ifml_navigation_flows(
        &self,
    ) -> Result<Vec<NavigationFlowRecord>, GraphError> {
        let gql = "MATCH (source)-[:HasEvent]->(evt:Event)-[flow:NavigationFlow]->(target:ViewContainer) \
                   RETURN source.name, evt.name, target.name, flow.target_param_binding";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut flows = Vec::new();
        for row in &result.rows {
            flows.push(NavigationFlowRecord {
                source: reader.get_string(row, "source.name")?,
                source_container: String::new(),
                event: reader.get_string(row, "evt.name")?,
                target: reader.get_string(row, "target.name")?,
                target_param_binding: reader.get_opt_string(row, "flow.target_param_binding")?,
            });
        }

        let owners = component_owner_containers(self)?;
        for flow in &mut flows {
            flow.source_container = owners
                .get(&flow.source)
                .cloned()
                .unwrap_or_else(|| flow.source.clone());
        }
        Ok(flows)
    }

    pub(super) async fn query_ifml_action_triggers(
        &self,
    ) -> Result<Vec<(String, String)>, GraphError> {
        query_many(
            self,
            "MATCH (evt:Event)-[:TriggersAction]->(a:ActionNode) \
               RETURN evt.name, a.name ORDER BY evt.name",
            |reader, row| {
                Ok((
                    reader.get_string(row, "evt.name")?,
                    reader.get_string(row, "a.name")?,
                ))
            },
        )
        .await
    }

    pub(super) async fn query_ifml_data_flows(
        &self,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, GraphError> {
        query_many(
            self,
            "MATCH (source)-[flow:DataFlow]->(target) \
               RETURN source.name, target.name, flow.source_param, flow.target_param",
            |reader, row| {
                Ok((
                    reader.get_string(row, "source.name")?,
                    reader.get_string(row, "target.name")?,
                    reader.get_opt_string(row, "flow.source_param")?,
                    reader.get_opt_string(row, "flow.target_param")?,
                ))
            },
        )
        .await
    }

    pub(super) async fn query_ifml_actions(&self) -> Result<Vec<ActionNode>, GraphError> {
        query_many(
            self,
            "MATCH (a:ActionNode) RETURN a.name, a.domain ORDER BY a.name",
            |reader, row| {
                Ok(ActionNode {
                    name: reader.get_string(row, "a.name")?,
                    domain: reader.get_opt_string(row, "a.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_ifml_parameters(
        &self,
    ) -> Result<Vec<ParameterDefinitionNode>, GraphError> {
        query_many(
            self,
            "MATCH (p:ParameterDefinition) RETURN p.name, p.direction, p.type_ref, p.domain ORDER BY p.name",
            |reader, row| {
                Ok(ParameterDefinitionNode {
                    name: reader.get_string(row, "p.name")?,
                    direction: reader.get_string(row, "p.direction")?,
                    type_ref: reader.get_string(row, "p.type_ref")?,
                    domain: reader.get_opt_string(row, "p.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_parameters_for_view(
        &self,
        container_name: &str,
    ) -> Result<Vec<ParameterDefinitionNode>, GraphError> {
        let params = HashMap::from([(
            "name".to_string(),
            grafeo::Value::String(container_name.into()),
        )]);
        query_many_params(
            self,
            "MATCH (vc:ViewContainer {name: $name})-[:HasParameter]->(p:ParameterDefinition) \
             RETURN p.name, p.direction, p.type_ref, p.domain ORDER BY p.name",
            params,
            |reader, row| {
                Ok(ParameterDefinitionNode {
                    name: reader.get_string(row, "p.name")?,
                    direction: reader.get_string(row, "p.direction")?,
                    type_ref: reader.get_string(row, "p.type_ref")?,
                    domain: reader.get_opt_string(row, "p.domain")?,
                })
            },
        )
        .await
    }

    pub(super) async fn query_data_bindings(
        &self,
    ) -> Result<Vec<DataBindingResolution>, GraphError> {
        let gql = "MATCH (c:ViewComponent)-[:HasDataBinding]->(db:DataBinding) \
                   -[:BindsToEntity]->(s:Schema) \
                   RETURN c.name, s.title, c.api_operation ORDER BY c.name";
        let result = query_gql(self, gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut bindings: Vec<DataBindingResolution> = Vec::new();
        for row in &result.rows {
            bindings.push(DataBindingResolution {
                component: reader.get_string(row, "c.name")?,
                entity_title: reader.get_string(row, "s.title")?,
                fields: Vec::new(),
                api_operation: reader.get_opt_string(row, "c.api_operation")?,
            });
        }

        let fields_gql =
            "MATCH (c:ViewComponent)-[bp:BindsToProperty]->(p:Property) RETURN c.name, p.name";
        let result = query_gql(self, fields_gql)?;
        let reader = RowReader::from_columns(&result.columns);
        let mut fields_by_component: HashMap<String, Vec<String>> = HashMap::new();
        for row in &result.rows {
            fields_by_component
                .entry(reader.get_string(row, "c.name")?)
                .or_default()
                .push(reader.get_string(row, "p.name")?);
        }
        for binding in &mut bindings {
            binding.fields = fields_by_component
                .remove(&binding.component)
                .unwrap_or_default();
        }
        Ok(bindings)
    }
}

/// Map a `ViewContainer` query row to its node. `alias` is the query's
/// column prefix (`vc` or `c`).
fn view_container_from_row(
    reader: &RowReader,
    row: &[grafeo::Value],
    alias: &str,
) -> Result<ViewContainerNode, GraphError> {
    let col = |name: &str| format!("{alias}.{name}");
    let module_uses_str: Option<String> = reader.get_opt_string(row, &col("module_uses"))?;
    let module_uses: Option<Vec<ModuleUseRecord>> =
        module_uses_str.and_then(|s| serde_json::from_str(&s).ok());
    let roles_str: Option<String> = reader.get_opt_string(row, &col("roles"))?;
    let roles: Option<Vec<String>> = roles_str.and_then(|s| serde_json::from_str(&s).ok());
    let requires_str: Option<String> = reader.get_opt_string(row, &col("requires"))?;
    let requires: Option<Vec<String>> = requires_str.and_then(|s| serde_json::from_str(&s).ok());
    Ok(ViewContainerNode {
        name: reader.get_string(row, &col("name"))?,
        label: reader.get_opt_string(row, &col("label"))?,
        is_xor: reader.get_bool(row, &col("is_xor"))?,
        is_default: reader.get_bool(row, &col("is_default"))?,
        is_landmark: reader.get_bool(row, &col("is_landmark"))?,
        is_modal: reader.get_bool(row, &col("is_modal"))?,
        conditional_expression: reader.get_opt_string(row, &col("conditional_expression"))?,
        expr_json: None,
        domain: reader.get_opt_string(row, &col("domain"))?,
        module_uses,
        roles,
        requires,
    })
}

/// Map of ViewComponent name → owning ViewContainer name, resolved from
/// ContainsViewComponent edges.
fn component_owner_containers(
    engine: &GrafeoEngine,
) -> Result<HashMap<String, String>, GraphError> {
    let gql = "MATCH (vc:ViewContainer)-[:ContainsViewComponent]->(comp:ViewComponent) \
               RETURN vc.name, comp.name";
    let result = query_gql(engine, gql)?;
    let reader = RowReader::from_columns(&result.columns);
    let mut owners = HashMap::new();
    for row in &result.rows {
        owners.insert(
            reader.get_string(row, "comp.name")?,
            reader.get_string(row, "vc.name")?,
        );
    }
    Ok(owners)
}
