mod api;
mod atproto;
mod authorization;
mod codelist;
mod composite;
mod composition;
mod condition;
mod discovery;
mod edge;
mod field_def;
mod function;
mod ifml;
mod mox;
mod namespace;
mod persistence;
mod policy;
mod property;
mod regulatory;
mod relationship;
mod rule;
mod schema;
mod security;
mod stats;

pub use api::{
    ApiOperationNode, ApiResourceNode, ErrorDefinitionNode, HttpEndpointNode, InteractionNode,
    PermissionNode, PipelineNode,
};
// Naming-collision resolution (issue #267): the AT-Protocol repo namespace
// node is renamed `AtprotoNamespaceNode` (grafeo label `AtprotoNamespace`,
// trait methods `ingest_atproto_namespace`/`get_atproto_namespaces`) so the
// graph-wide namespace concept (#267) can own the canonical `NamespaceNode`
// name, `:Namespace` label, and `ingest_namespace`/`list_namespaces` trait
// methods. `EdgeType::InNamespace` stays shared by both features.
pub use atproto::{AtprotoNamespaceNode, CollectionNode, LexiconNode, RepositoryNode};
pub use authorization::{
    resolve_effective_permits, ActorNode, ActorPolicyModel, ActorPolicyNode, CapabilityNode,
    DelegationRecord, GrantEdge, NeverBothGroup, Permit,
};
pub use codelist::{CodeList, EnumValue};
pub use composite::{CompositeColumn, CompositeRange, StructuredSubField};
pub use composition::{ColumnInfo, CompositionNode, CompositionTree, FkDirection, FkTarget};
pub use condition::{ConditionKind, ConditionNode};
pub use discovery::{DetectionSource, Extension, ParentCandidate};
pub use edge::{EdgeProperties, EdgeType};
pub use field_def::{
    codelist_enum_name_from_ref, ensure_id_suffix, resolve_field, resolve_fk_column_name,
    FieldDefinition,
};
pub use function::{
    FunctionAlias, FunctionDispatch, FunctionInput, FunctionNode, FunctionOperation,
    FunctionPostCondition, FunctionTransform, FunctionTransformKind,
};
pub use ifml::{
    strip_ifml_prefix, ActionNode, DataBindingNode, DataBindingResolution, DataFlowData, EventNode,
    ModuleUseRecord, NavigationFlowData, NavigationFlowRecord, ParameterDefinitionNode,
    ViewComponentNode, ViewContainerNode,
};
pub use mox::{
    MoxDerivedFeatureNode, MoxDomainModel, MoxEntry, MoxFacet, MoxOperationNode, MoxPackageNode,
    MoxParam, MoxVocabularyNode,
};
pub use namespace::{
    derive_namespace_depends, disambiguate_schema_ids, qualified_schema_id,
    topological_namespace_order, NamespaceImport, NamespaceNode,
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
pub use regulatory::{
    RegulatoryEdgeKind, RegulatoryKind, RegulatoryNode, RegulatoryOwner, RegulatoryRefRecord,
};
pub use relationship::{
    Cardinality, ForeignKeySpec, Ownership, PropagationRule, PropagationTrigger, RelationshipNode,
};
pub use rule::{RuleKind, RuleNode, RuleRefRecord};
pub use schema::{SchemaClassificationData, SchemaNode, MOX_SOURCE};
pub use security::{
    MembershipNode, MembershipStatus, Scope, ScopeKind, SecurityIdentityNode, TenantNode,
};
pub use stats::IngestStats;
