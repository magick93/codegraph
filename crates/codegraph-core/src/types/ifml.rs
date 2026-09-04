use serde::{Deserialize, Serialize};

/// Strip a leading IFML node prefix (`vc:` / `comp:` / `evt:` / `param:` /
/// `action:` / `db:`) from an id, returning the id unchanged when no prefix
/// is present. Underscore-joined event names like `comp_grid_select` are
/// left untouched — only a leading `{prefix}:` is stripped.
pub fn strip_ifml_prefix(id: &str) -> &str {
    for prefix in ["vc:", "comp:", "evt:", "param:", "action:", "db:"] {
        if let Some(rest) = id.strip_prefix(prefix) {
            return rest;
        }
    }
    id
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewContainerNode {
    pub name: String,
    pub label: Option<String>,
    pub is_xor: bool,
    pub is_default: bool,
    pub is_landmark: bool,
    pub is_modal: bool,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewComponentNode {
    pub name: String,
    pub component_type: String,
    pub mode: Option<String>,
    pub entity: Option<String>,
    pub fields: Option<Vec<String>>,
    pub filter: Option<String>,
    pub api_operation: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventNode {
    pub name: String,
    pub event_type: String,
    pub params: Option<Vec<String>>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionNode {
    pub name: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterDefinitionNode {
    pub name: String,
    pub direction: String,
    pub type_ref: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataBindingNode {
    pub name: String,
    pub conditional_expression: Option<String>,
    pub expression_language: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationFlowData {
    pub target_param_binding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataFlowData {
    pub source_param: Option<String>,
    pub target_param: Option<String>,
}

/// Resolved data binding between an IFML view component and a schema entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataBindingResolution {
    pub component: String,
    pub entity_title: String,
    pub fields: Vec<String>,
    pub api_operation: Option<String>,
}
