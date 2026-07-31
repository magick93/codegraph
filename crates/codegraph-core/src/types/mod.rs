mod atproto;
mod codelist;
mod composite;
mod composition;
mod discovery;
mod edge;
mod field_def;
mod ifml;
mod property;
mod schema;
mod stats;

pub use codelist::{CodeList, EnumValue};
pub use composite::{CompositeColumn, CompositeRange, StructuredSubField};
pub use composition::{ColumnInfo, CompositionNode, CompositionTree, FkDirection, FkTarget};
pub use discovery::{DetectionSource, Extension, ParentCandidate};
pub use edge::{EdgeProperties, EdgeType};
pub use field_def::{codelist_enum_name_from_ref, ensure_id_suffix, resolve_fk_column_name, resolve_field, FieldDefinition};
pub use ifml::{
    ActionNode, DataBindingNode, DataFlowData, EventNode, NavigationFlowData,
    ParameterDefinitionNode, ViewComponentNode, ViewContainerNode,
};
pub use property::{inject_codelist_properties, PropertyNode};
pub use schema::{SchemaClassificationData, SchemaNode};
pub use atproto::{CollectionNode, LexiconNode, NamespaceNode, RepositoryNode};
pub use stats::IngestStats;
