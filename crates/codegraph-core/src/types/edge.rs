use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    HasProperty,
    ReferencesSchema,
    ItemsOf,
    ExtendsSchema,
    DependsOn,
    HasEnumValue,
    UsesCodeList,
    ExpandsTo,
    CollapsesTo,
    ConsumesField,
    ContainsDef,
    RequiresExtension,
    InDomain,
    DomainDepends,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgeProperties {
    pub sort_order: Option<i32>,
    pub ref_path: Option<String>,
    pub resolved_classification: Option<String>,
    pub composition_type: Option<String>,
    pub dependency_type: Option<String>,
    pub render_as: Option<String>,
    pub role: Option<String>,
    pub def_name: Option<String>,
}
