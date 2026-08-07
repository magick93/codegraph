use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiResourceNode {
    pub name: String,
    pub schema_title: String,
    pub domain: String,
    pub label: Option<String>,
    pub path_segment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiOperationNode {
    pub name: String,
    pub kind: String,
    pub input_schema: Option<String>,
    pub output_schema: String,
    pub paging: bool,
    pub sorting: bool,
    pub filtering: bool,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionNode {
    pub transport: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpEndpointNode {
    pub method: String,
    pub path_template: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineNode {
    pub name: String,
    pub middleware: Option<Vec<String>>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorDefinitionNode {
    pub code: String,
    pub description: String,
    pub http_status: i32,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionNode {
    pub name: String,
    pub domain: Option<String>,
}
