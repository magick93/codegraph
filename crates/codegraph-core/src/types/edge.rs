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

    // IFML edge types
    ContainsViewContainer,
    ContainsViewComponent,
    HasEvent,
    NavigationFlow,
    DataFlow,
    HasParameter,
    ParameterBindingGroup,
    ParameterBinding,
    HasDataBinding,
    BindsToEntity,
    BindsToProperty,
    TriggersAction,
    ActionEvent,
    HasModuleDefinition,
    HasViewComponentPart,
    HasConditionalExpr,
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
    pub target_param_binding: Option<String>,
    pub source_param: Option<String>,
    pub event_type: Option<String>,
    pub outcome: Option<String>,
    pub component_type: Option<String>,
    pub direction: Option<String>,
    pub expression: Option<String>,
}
