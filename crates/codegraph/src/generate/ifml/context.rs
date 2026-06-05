use std::collections::HashMap;

use serde::Serialize;

/// Complete IFML model resolved from the graph, with dependencies
#[derive(Debug, Clone, Serialize)]
pub struct IfmlModel {
    pub view_containers: Vec<IfmlViewContainer>,
    pub actions: Vec<IfmlActionDef>,
    pub navigation_edges: Vec<NavigationEdge>,
    pub data_flows: Vec<DataFlowEdge>,
    /// Topological generation order (target views before source views)
    pub generation_order: Vec<String>,
}

/// A view container with its full sub-graph resolved
#[derive(Debug, Clone, Serialize)]
pub struct IfmlViewContainer {
    pub name: String,
    pub label: Option<String>,
    pub is_xor: bool,
    pub is_default: bool,
    pub is_landmark: bool,
    pub is_modal: bool,
    pub params: Vec<ParameterDef>,
    pub components: Vec<IfmlComponent>,
    pub events: Vec<IfmlEvent>,
    pub containers: Vec<IfmlViewContainer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IfmlComponent {
    pub name: String,
    /// "list", "form", "details", "search", "tree", "chart"
    pub component_type: String,
    /// "view", "edit", "create"
    pub mode: Option<String>,
    /// JSON Schema entity name
    pub entity: Option<String>,
    pub fields: Vec<String>,
    pub filter: Option<String>,
    pub properties: HashMap<String, String>,
    pub events: Vec<IfmlEvent>,
    pub parts: Vec<ComponentPart>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentPart {
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IfmlEvent {
    pub name: String,
    pub event_type: String,
    pub params: Vec<String>,
    pub action: IfmlAction,
}

#[derive(Debug, Clone, Serialize)]
pub enum IfmlAction {
    Navigate {
        target: String,
        binding: HashMap<String, String>,
    },
    Refresh {
        target: String,
        binding: HashMap<String, String>,
    },
    Action(String),
    Stay,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParameterDef {
    pub name: String,
    pub type_ref: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigationEdge {
    pub source_container: String,
    pub source_event: String,
    pub target_container: String,
    pub parameter_binding: HashMap<String, String>,
    pub conditional_expression: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataFlowEdge {
    pub source_element: String,
    pub target_element: String,
    pub source_param: Option<String>,
    pub target_param: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IfmlActionDef {
    pub name: String,
    pub properties: HashMap<String, String>,
    pub events: Vec<IfmlEvent>,
}
