use std::collections::HashMap;

use codegraph_ifml_dsl::ComponentSpec;
use serde::Serialize;

/// Generation-time authorization policy resolved from the graph's actor
/// model: per-actor effective capabilities (extends-aware, forbid-wins)
/// plus the full capability inventory. `IfmlModel.policy` is `None` when no
/// policy was ingested, and all policy-driven emission is gated on that.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PolicyContext {
    /// (actor name, effective capabilities) pairs sorted by actor name;
    /// capability lists are sorted, deduplicated, and permit-only.
    pub actors: Vec<(String, Vec<String>)>,
    /// All capability names in the policy, sorted.
    pub capabilities: Vec<String>,
}

/// Complete IFML model resolved from the graph, with dependencies
#[derive(Debug, Clone, Serialize)]
pub struct IfmlModel {
    pub view_containers: Vec<IfmlViewContainer>,
    pub actions: Vec<IfmlActionDef>,
    pub navigation_edges: Vec<NavigationEdge>,
    pub data_flows: Vec<DataFlowEdge>,
    /// Topological generation order (target views before source views)
    pub generation_order: Vec<String>,
    /// Ingested actor policy, or `None` without one.
    pub policy: Option<PolicyContext>,
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
    /// Roles allowed to view this page; empty when unrestricted
    pub roles: Vec<String>,
    /// Capabilities required to view this page; empty when unrestricted
    pub requires: Vec<String>,
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
    /// (field_name, rust_field_type) pairs resolved from the bound schema
    pub fields_with_types: Vec<(String, String)>,
    pub filter: Option<String>,
    pub properties: HashMap<String, String>,
    pub events: Vec<IfmlEvent>,
    pub parts: Vec<ComponentPart>,
    /// Typed component spec (Table/Form/Chart) parsed from the graph node's
    /// serialized spec; `None` when absent or unparseable
    pub spec: Option<ComponentSpec>,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// JS literal for the DSL default (`'home'`, `1`, `true`); `None` when
    /// the parameter has no default or the graph record lacks one.
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigationEdge {
    pub source_container: String,
    pub source_event: String,
    pub target_container: String,
    pub parameter_binding: HashMap<String, String>,
    pub conditional_expression: Option<String>,
    /// Raw component name when the event hangs off a ViewComponent;
    /// `None` when the event hangs off the container itself.
    pub source_component: Option<String>,
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
