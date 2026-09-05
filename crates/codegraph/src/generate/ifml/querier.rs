use std::collections::HashMap;

use async_trait::async_trait;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
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
    async fn get_components_for(
        &self,
        container_name: &str,
    ) -> Result<Vec<IfmlComponent>, GraphError> {
        let raw = self.db.get_ifml_view_components(container_name).await?;
        let mut components = Vec::new();
        for comp in &raw {
            let comp_id = format!("comp:{}", comp.name);
            let events = self.get_events_for(&comp_id).await?;

            let mut properties = HashMap::new();
            properties.insert("type".to_string(), comp.component_type.clone());
            if let Some(ref mode) = comp.mode {
                properties.insert("mode".to_string(), mode.clone());
            }

            let fields_with_types = self.resolve_field_types(comp).await?;

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
    ) -> Result<Vec<(String, String)>, GraphError> {
        let Some(ref entity) = comp.entity else {
            return Ok(Vec::new());
        };
        let title = if self.db.get_schema(entity).await?.is_some() {
            entity.clone()
        } else {
            let suffixed = format!("{entity}Type");
            if self.db.get_schema(&suffixed).await?.is_some() {
                suffixed
            } else {
                entity.clone()
            }
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

    async fn get_events_for(&self, parent_id: &str) -> Result<Vec<IfmlEvent>, GraphError> {
        let raw = self.db.get_ifml_events(parent_id).await?;
        Ok(raw
            .into_iter()
            .map(|evt| IfmlEvent {
                name: evt.name,
                event_type: evt.event_type,
                params: evt.params.unwrap_or_default(),
                action: IfmlAction::Stay,
            })
            .collect())
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
        let raw_containers = self.db.get_ifml_view_containers().await?;
        let mut containers = Vec::new();

        for vc in &raw_containers {
            let components = self.get_components_for(&vc.name).await?;
            let events = self.get_events_for(&format!("vc:{}", vc.name)).await?;

            containers.push(IfmlViewContainer {
                name: vc.name.clone(),
                label: vc.label.clone(),
                is_xor: vc.is_xor,
                is_default: vc.is_default,
                is_landmark: vc.is_landmark,
                is_modal: vc.is_modal,
                params: Vec::new(),
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
        let raw = self.db.get_ifml_navigation_flows().await?;
        Ok(raw
            .into_iter()
            .map(|(source, event, target)| NavigationEdge {
                source_container: source,
                source_event: event,
                target_container: target,
                parameter_binding: HashMap::new(),
                conditional_expression: None,
            })
            .collect())
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
    use codegraph_ifml_dsl::{ChartKind, ColumnDef, InputFieldType};

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
