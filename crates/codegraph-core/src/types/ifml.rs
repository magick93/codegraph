use serde::{Deserialize, Serialize};

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
