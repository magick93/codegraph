use std::collections::HashMap;

use async_trait::async_trait;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{DataBindingResolution, EventNode, NavigationFlowRecord};
use codegraph_ifml_dsl::ComponentSpec;

use super::context::*;
use super::dependency_graph;

/// Parse a `ViewComponentNode.spec` JSON string into a typed spec.
///
/// Returns `None` when the raw value is absent or fails to deserialize —
/// a malformed spec never fails generation.
fn parse_component_spec(raw: Option<&str>) -> Option<ComponentSpec> {
    let raw = raw?;
    match serde_json::from_str::<ComponentSpec>(raw) {
        Ok(spec) => Some(spec),
        Err(err) => {
            tracing::warn!(%err, "ignoring unparseable IFML component spec");
            None
        }
    }
}

/// Parse the persisted `target_param_binding` JSON into a map.
///
/// Absent or malformed bindings yield an empty map — a malformed binding
/// never fails generation.
fn parse_param_binding(raw: Option<&str>) -> HashMap<String, String> {
    let Some(raw) = raw else {
        return HashMap::new();
    };
    match serde_json::from_str::<HashMap<String, String>>(raw) {
        Ok(map) => map,
        Err(err) => {
            tracing::warn!(%err, "ignoring unparseable IFML target_param_binding");
            HashMap::new()
        }
    }
}

/// Per-model lookup tables resolving persisted event actions and data
/// bindings without re-querying the graph per event or component.
struct ActionIndex {
    flows: Vec<NavigationFlowRecord>,
    dataflow_targets: HashMap<String, String>,
    action_triggers: HashMap<String, String>,
}

impl ActionIndex {
    fn new(
        flows: Vec<NavigationFlowRecord>,
        data_flows: Vec<(String, String, Option<String>, Option<String>)>,
        action_triggers: Vec<(String, String)>,
    ) -> Self {
        let dataflow_targets = data_flows
            .into_iter()
            .map(|(source, target, _, _)| (source, target))
            .collect();
        let action_triggers = action_triggers.into_iter().collect();
        Self {
            flows,
            dataflow_targets,
            action_triggers,
        }
    }

    fn flow_for(&self, event_name: &str) -> Option<&NavigationFlowRecord> {
        self.flows.iter().find(|f| f.event == event_name)
    }

    /// Resolve the action persisted for an event: NavigationFlow → Navigate,
    /// DataFlow → Refresh, TriggersAction → Action, else Stay.
    fn action_for(&self, event_name: &str) -> IfmlAction {
        if let Some(flow) = self.flow_for(event_name) {
            return IfmlAction::Navigate {
                target: flow.target.clone(),
                binding: parse_param_binding(flow.target_param_binding.as_deref()),
            };
        }
        if let Some(target) = self.dataflow_targets.get(event_name) {
            return IfmlAction::Refresh {
                target: target.clone(),
                binding: HashMap::new(),
            };
        }
        if let Some(name) = self.action_triggers.get(event_name) {
            return IfmlAction::Action(name.clone());
        }
        IfmlAction::Stay
    }

    fn build_event(&self, evt: EventNode) -> IfmlEvent {
        IfmlEvent {
            action: self.action_for(&evt.name),
            name: evt.name,
            event_type: evt.event_type,
            params: evt.params.unwrap_or_default(),
        }
    }

    fn navigation_edges(&self) -> Vec<NavigationEdge> {
        self.flows
            .iter()
            .map(|flow| NavigationEdge {
                source_container: flow.source_container.clone(),
                source_event: flow.event.clone(),
                target_container: flow.target.clone(),
                parameter_binding: parse_param_binding(flow.target_param_binding.as_deref()),
                conditional_expression: None,
                source_component: (flow.source != flow.source_container)
                    .then(|| flow.source.clone()),
            })
            .collect()
    }
}

/// Trait for querying the IFML model from the graph
#[async_trait]
pub trait IfmlQuerier: Send + Sync {
    async fn get_ifml_model(&self) -> Result<IfmlModel, GraphError>;
    async fn get_view_containers(&self) -> Result<Vec<IfmlViewContainer>, GraphError>;
    async fn get_view_container(&self, name: &str)
        -> Result<Option<IfmlViewContainer>, GraphError>;
    async fn get_navigation_edges(&self) -> Result<Vec<NavigationEdge>, GraphError>;
    async fn get_data_flows(&self) -> Result<Vec<DataFlowEdge>, GraphError>;
    async fn get_actions(&self) -> Result<Vec<IfmlActionDef>, GraphError>;
    async fn compute_generation_order(&self) -> Result<Vec<String>, GraphError>;
}

impl<'a> IfmlGraphQuerier<'a> {
    async fn action_index(&self) -> Result<ActionIndex, GraphError> {
        let flows = self.db.get_ifml_navigation_flows().await?;
        let data_flows = self.db.get_ifml_data_flows().await?;
        let action_triggers = self.db.get_ifml_action_triggers().await?;
        Ok(ActionIndex::new(flows, data_flows, action_triggers))
    }

    async fn get_components_for(
        &self,
        container_name: &str,
        index: &ActionIndex,
        bindings: &[DataBindingResolution],
    ) -> Result<Vec<IfmlComponent>, GraphError> {
        let raw = self.db.get_ifml_view_components(container_name).await?;
        let mut components = Vec::new();
        for comp in &raw {
            let comp_id = format!("comp:{}", comp.name);
            let raw_events = self.db.get_ifml_events(&comp_id).await?;
            let events: Vec<IfmlEvent> = raw_events
                .into_iter()
                .map(|evt| index.build_event(evt))
                .collect();

            let mut properties = HashMap::new();
            properties.insert("type".to_string(), comp.component_type.clone());
            if let Some(ref mode) = comp.mode {
                properties.insert("mode".to_string(), mode.clone());
            }

            let binding = bindings.iter().find(|b| b.component == comp.name);
            let fields_with_types = self.resolve_field_types(comp, binding).await?;

            components.push(IfmlComponent {
                name: comp.name.clone(),
                component_type: comp.component_type.clone(),
                mode: comp.mode.clone(),
                entity: comp.entity.clone(),
                fields: comp.fields.clone().unwrap_or_default(),
                fields_with_types,
                filter: comp.filter.clone(),
                properties,
                events,
                parts: Vec::new(),
                spec: parse_component_spec(comp.spec.as_deref()),
            });
        }
        Ok(components)
    }

    async fn resolve_field_types(
        &self,
        comp: &codegraph_core::types::ViewComponentNode,
        binding: Option<&DataBindingResolution>,
    ) -> Result<Vec<(String, String)>, GraphError> {
        let Some(ref entity) = comp.entity else {
            return Ok(Vec::new());
        };
        let title = match binding {
            Some(resolution) => resolution.entity_title.clone(),
            None => self.resolve_schema_title_by_name(entity).await?,
        };
        let props = self.db.get_properties(&title).await?;
        let props_by_name: HashMap<String, String> = props
            .iter()
            .map(|p| (p.name.clone(), p.rust_field_type.clone()))
            .collect();
        Ok(comp
            .fields
            .clone()
            .unwrap_or_default()
            .iter()
            .filter_map(|f| props_by_name.get(f).map(|t| (f.clone(), t.clone())))
            .collect())
    }

    async fn resolve_schema_title_by_name(&self, entity: &str) -> Result<String, GraphError> {
        if self.db.get_schema(entity).await?.is_some() {
            return Ok(entity.to_string());
        }
        let suffixed = format!("{entity}Type");
        if self.db.get_schema(&suffixed).await?.is_some() {
            return Ok(suffixed);
        }
        Ok(entity.to_string())
    }
}

/// Implementation that queries the Grafeo graph
pub struct IfmlGraphQuerier<'a> {
    db: &'a dyn GraphQuerier,
}

impl<'a> IfmlGraphQuerier<'a> {
    pub fn new(db: &'a dyn GraphQuerier) -> Self {
        Self { db }
    }
}

#[async_trait]
impl<'a> IfmlQuerier for IfmlGraphQuerier<'a> {
    async fn get_ifml_model(&self) -> Result<IfmlModel, GraphError> {
        let view_containers = self.get_view_containers().await?;
        let actions = self.get_actions().await?;
        let navigation_edges = self.get_navigation_edges().await?;
        let data_flows = self.get_data_flows().await?;
        let generation_order = self.compute_generation_order().await?;

        Ok(IfmlModel {
            view_containers,
            actions,
            navigation_edges,
            data_flows,
            generation_order,
        })
    }

    async fn get_view_containers(&self) -> Result<Vec<IfmlViewContainer>, GraphError> {
        let index = self.action_index().await?;
        let bindings = self.db.get_data_bindings().await?;
        let raw_containers = self.db.get_ifml_view_containers().await?;
        let mut containers = Vec::new();

        for vc in &raw_containers {
            let params = self
                .db
                .get_parameters_for_view(&vc.name)
                .await?
                .into_iter()
                .map(|p| ParameterDef {
                    name: p.name,
                    type_ref: p.type_ref,
                })
                .collect();
            let components = self.get_components_for(&vc.name, &index, &bindings).await?;
            let raw_events = self.db.get_ifml_events(&format!("vc:{}", vc.name)).await?;
            let events: Vec<IfmlEvent> = raw_events
                .into_iter()
                .map(|evt| index.build_event(evt))
                .collect();

            containers.push(IfmlViewContainer {
                name: vc.name.clone(),
                label: vc.label.clone(),
                is_xor: vc.is_xor,
                is_default: vc.is_default,
                is_landmark: vc.is_landmark,
                is_modal: vc.is_modal,
                params,
                components,
                events,
                containers: Vec::new(),
            });
        }

        Ok(containers)
    }

    async fn get_view_container(
        &self,
        name: &str,
    ) -> Result<Option<IfmlViewContainer>, GraphError> {
        let containers = self.get_view_containers().await?;
        Ok(containers.into_iter().find(|c| c.name == name))
    }

    async fn get_navigation_edges(&self) -> Result<Vec<NavigationEdge>, GraphError> {
        let index = self.action_index().await?;
        Ok(index.navigation_edges())
    }

    async fn get_data_flows(&self) -> Result<Vec<DataFlowEdge>, GraphError> {
        let raw = self.db.get_ifml_data_flows().await?;
        Ok(raw
            .into_iter()
            .map(
                |(source, target, source_param, target_param)| DataFlowEdge {
                    source_element: source,
                    target_element: target,
                    source_param,
                    target_param,
                },
            )
            .collect())
    }

    async fn get_actions(&self) -> Result<Vec<IfmlActionDef>, GraphError> {
        let raw = self.db.get_ifml_actions().await?;
        Ok(raw
            .into_iter()
            .map(|a| IfmlActionDef {
                name: a.name,
                properties: HashMap::new(),
                events: Vec::new(),
            })
            .collect())
    }

    async fn compute_generation_order(&self) -> Result<Vec<String>, GraphError> {
        let nav = self.get_navigation_edges().await?;
        if !nav.is_empty() {
            let pairs: Vec<(String, String)> = nav
                .iter()
                .map(|e| (e.source_container.clone(), e.target_container.clone()))
                .collect();
            return Ok(dependency_graph::compute_view_generation_order(&pairs));
        }

        let containers = self.get_view_containers().await?;
        let mut names: Vec<String> = containers.into_iter().map(|c| c.name).collect();
        names.sort();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{
        EdgeProperties, EdgeType, ParameterDefinitionNode, ViewComponentNode, ViewContainerNode,
    };
    use codegraph_ifml_dsl::{ChartKind, ColumnDef, InputFieldType};

    async fn ingest_view_container(db: &MockEngine, name: &str, landmark: bool) {
        db.ingest_view_container(&ViewContainerNode {
            name: name.to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: landmark,
            is_modal: false,
            conditional_expression: None,
            domain: None,
        })
        .await
        .unwrap();
    }

    async fn ingest_component(db: &MockEngine, parent: &str, name: &str, component_type: &str) {
        db.ingest_view_component(&ViewComponentNode {
            name: name.to_string(),
            component_type: component_type.to_string(),
            mode: None,
            entity: Some("Customer".to_string()),
            fields: Some(vec!["name".to_string()]),
            filter: None,
            api_operation: None,
            spec: None,
            conditional_expression: None,
            domain: None,
        })
        .await
        .unwrap();
        db.ingest_edge(
            &format!("vc:{parent}"),
            &format!("comp:{name}"),
            EdgeType::ContainsViewComponent,
            None,
        )
        .await
        .unwrap();
    }

    async fn ingest_event(db: &MockEngine, parent: &str, name: &str, event_type: &str) {
        db.ingest_event(&EventNode {
            name: name.to_string(),
            event_type: event_type.to_string(),
            params: None,
            conditional_expression: None,
            domain: None,
        })
        .await
        .unwrap();
        db.ingest_edge(
            &format!("comp:{parent}"),
            &format!("evt:{name}"),
            EdgeType::HasEvent,
            None,
        )
        .await
        .unwrap();
    }

    /// CustomerList(grid) --select--> CustomerDetail with a param binding.
    async fn ingest_customer_list_to_detail(db: &MockEngine) {
        ingest_view_container(db, "CustomerList", true).await;
        ingest_view_container(db, "CustomerDetail", false).await;
        ingest_component(db, "CustomerList", "grid", "list").await;
        ingest_event(db, "grid", "comp_grid_select", "select").await;
        db.ingest_edge(
            "evt:comp_grid_select",
            "vc:CustomerDetail",
            EdgeType::NavigationFlow,
            Some(&EdgeProperties {
                target_param_binding: Some(r#"{"customerId": "row.id"}"#.to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn component_event_navigate_action_is_resolved_with_binding() {
        let engine = MockEngine::new();
        ingest_customer_list_to_detail(&engine).await;

        let querier = IfmlGraphQuerier::new(&engine);
        let containers = querier.get_view_containers().await.unwrap();
        let list = containers
            .iter()
            .find(|c| c.name == "CustomerList")
            .expect("CustomerList container");
        let event = &list.components[0].events[0];
        assert_eq!(event.name, "comp_grid_select");
        match &event.action {
            IfmlAction::Navigate { target, binding } => {
                assert_eq!(target, "CustomerDetail");
                assert_eq!(
                    binding.get("customerId").map(String::as_str),
                    Some("row.id")
                );
            }
            other => panic!("expected Navigate action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn component_event_navigation_resolves_to_parent_view() {
        let engine = MockEngine::new();
        ingest_customer_list_to_detail(&engine).await;

        let querier = IfmlGraphQuerier::new(&engine);
        let edges = querier.get_navigation_edges().await.unwrap();
        assert_eq!(edges.len(), 1);
        let edge = &edges[0];
        assert_eq!(edge.source_container, "CustomerList");
        assert_eq!(edge.source_component.as_deref(), Some("grid"));
        assert_eq!(edge.source_event, "comp_grid_select");
        assert_eq!(edge.target_container, "CustomerDetail");
        assert_eq!(
            edge.parameter_binding.get("customerId").map(String::as_str),
            Some("row.id")
        );
    }

    #[tokio::test]
    async fn view_params_are_populated_from_parameter_definitions() {
        let engine = MockEngine::new();
        ingest_customer_list_to_detail(&engine).await;
        engine
            .ingest_parameter_definition(&ParameterDefinitionNode {
                name: "customerId".to_string(),
                direction: "in".to_string(),
                type_ref: "Uuid".to_string(),
                domain: None,
            })
            .await
            .unwrap();
        engine
            .ingest_edge(
                "vc:CustomerDetail",
                "param:customerId",
                EdgeType::HasParameter,
                Some(&EdgeProperties {
                    direction: Some("in".to_string()),
                    ..Default::default()
                }),
            )
            .await
            .unwrap();

        let querier = IfmlGraphQuerier::new(&engine);
        let detail = querier
            .get_view_container("CustomerDetail")
            .await
            .unwrap()
            .expect("CustomerDetail container");
        assert_eq!(detail.params.len(), 1);
        assert_eq!(detail.params[0].name, "customerId");
        assert_eq!(detail.params[0].type_ref, "Uuid");

        let list = querier
            .get_view_container("CustomerList")
            .await
            .unwrap()
            .expect("CustomerList container");
        assert!(list.params.is_empty());
    }

    #[tokio::test]
    async fn action_invocation_refresh_and_stay_actions_are_resolved() {
        let engine = MockEngine::new();
        ingest_view_container(&engine, "Dashboard", false).await;
        ingest_component(&engine, "Dashboard", "form", "form").await;
        ingest_component(&engine, "Dashboard", "chart", "chart").await;
        ingest_event(&engine, "form", "comp_form_save", "save").await;
        ingest_event(&engine, "form", "comp_form_cancel", "cancel").await;
        ingest_event(&engine, "chart", "comp_chart_load", "load").await;

        engine
            .ingest_action_node(&codegraph_core::types::ActionNode {
                name: "UpdateCustomer".to_string(),
                domain: None,
            })
            .await
            .unwrap();
        engine
            .ingest_edge(
                "evt:comp_form_save",
                "action:UpdateCustomer",
                EdgeType::TriggersAction,
                None,
            )
            .await
            .unwrap();
        engine
            .ingest_edge(
                "evt:comp_chart_load",
                "comp:chart",
                EdgeType::DataFlow,
                None,
            )
            .await
            .unwrap();

        let querier = IfmlGraphQuerier::new(&engine);
        let dashboard = querier
            .get_view_container("Dashboard")
            .await
            .unwrap()
            .expect("Dashboard container");
        let action_of = |name: &str| {
            dashboard
                .components
                .iter()
                .flat_map(|c| c.events.iter())
                .find(|e| e.name == name)
                .map(|e| e.action.clone())
                .unwrap_or_else(|| panic!("event {name} not found"))
        };
        assert_eq!(
            action_of("comp_form_save"),
            IfmlAction::Action("UpdateCustomer".to_string())
        );
        assert_eq!(action_of("comp_form_cancel"), IfmlAction::Stay);
        assert_eq!(
            action_of("comp_chart_load"),
            IfmlAction::Refresh {
                target: "chart".to_string(),
                binding: HashMap::new()
            }
        );
    }

    #[test]
    fn parse_component_spec_none_when_absent() {
        assert!(parse_component_spec(None).is_none());
        assert!(parse_component_spec(Some("")).is_none());
    }

    #[test]
    fn parse_component_spec_none_on_garbage() {
        assert!(parse_component_spec(Some("not json")).is_none());
        assert!(parse_component_spec(Some("{\"Unknown\": {}}")).is_none());
    }

    #[test]
    fn parse_component_spec_table() {
        let raw = r#"{"Table":{"columns":[
            {"Field":{"label":"Name","field":{"entity":"Customer","property":"name"}}},
            {"Lookup":{"label":"Status","field":{"entity":"Customer","property":"status"},"lookup":"status_labels"}},
            {"Expression":{"label":"Tenure","expr":{"Call":{"name":"tenure_years","args":[{"FieldExpr":{"object":{"Ident":"Customer"},"field":"hire_date"}}]}}}}
        ],"pagination":true}}"#;
        let spec = parse_component_spec(Some(raw)).expect("table spec should parse");
        let ComponentSpec::Table(table) = spec else {
            panic!("expected table spec, got {spec:?}");
        };
        assert!(table.pagination);
        assert_eq!(table.columns.len(), 3);
        assert!(matches!(table.columns[0], ColumnDef::Field { .. }));
        assert!(matches!(table.columns[1], ColumnDef::Lookup { .. }));
        assert!(matches!(table.columns[2], ColumnDef::Expression { .. }));
    }

    #[test]
    fn parse_component_spec_form_and_chart() {
        let form_raw = r#"{"Form":{"fields":[
            {"name":"email","input":"Email","required":true,"validations":[],"values":[]}
        ]}}"#;
        let spec = parse_component_spec(Some(form_raw)).expect("form spec should parse");
        let ComponentSpec::Form(form) = spec else {
            panic!("expected form spec, got {spec:?}");
        };
        assert_eq!(form.fields[0].input, InputFieldType::Email);
        assert!(form.fields[0].required);

        let chart_raw =
            r#"{"Chart":{"kind":"Bar","label_field":"region","value_fields":["revenue"]}}"#;
        let spec = parse_component_spec(Some(chart_raw)).expect("chart spec should parse");
        let ComponentSpec::Chart(chart) = spec else {
            panic!("expected chart spec, got {spec:?}");
        };
        assert_eq!(chart.kind, ChartKind::Bar);
        assert_eq!(chart.label_field.as_deref(), Some("region"));
    }
}
