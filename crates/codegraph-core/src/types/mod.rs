mod api;
mod codelist;
mod composite;
mod composition;
mod discovery;
mod edge;
mod field_def;
mod ifml;
mod persistence;
mod policy;
mod property;
mod relationship;
mod schema;
mod security;
mod stats;

pub use api::{
    ApiOperationNode, ApiResourceNode, ErrorDefinitionNode, HttpEndpointNode, InteractionNode,
    PermissionNode, PipelineNode,
};
pub use codelist::{CodeList, EnumValue};
pub use composite::{CompositeColumn, CompositeRange, StructuredSubField};
pub use composition::{ColumnInfo, CompositionNode, CompositionTree, FkDirection, FkTarget};
pub use discovery::{DetectionSource, Extension, ParentCandidate};
pub use edge::{EdgeProperties, EdgeType};
pub use field_def::{
    codelist_enum_name_from_ref, ensure_id_suffix, resolve_field, resolve_fk_column_name,
    FieldDefinition,
};
pub use ifml::{
    strip_ifml_prefix, ActionNode, DataBindingNode, DataBindingResolution, DataFlowData, EventNode,
    NavigationFlowData, ParameterDefinitionNode, ViewComponentNode, ViewContainerNode,
};
pub use persistence::{
    AuditEffect, AuditTimestampKind, AuditUserKind, PersistenceChildTable, PersistenceColumn,
    PersistenceColumnRole, PersistenceEntity, PersistenceEntityRelation, PersistencePolicies,
    RetentionEffect, RowSecurityEffect, SoftDeleteEffect, TenantIsolationEffect,
};
pub use policy::{
    AuditPolicy, DeletionPropagation, PolicyKind, PolicyNode, RetentionPolicy, RowOperation,
    RowSecurityPolicy, SoftDeleteMarker, SoftDeletePolicy, SoftDeleteVisibility,
    TenantIsolationPolicy, TenantPropagation, TenantStrategy,
};
pub use property::{inject_codelist_properties, PropertyNode};
pub use relationship::{
    Cardinality, ForeignKeySpec, Ownership, PropagationRule, PropagationTrigger, RelationshipNode,
};
pub use schema::{SchemaClassificationData, SchemaNode};
pub use security::{
    MembershipNode, MembershipStatus, Scope, ScopeKind, SecurityIdentityNode, TenantNode,
};
pub use stats::IngestStats;
