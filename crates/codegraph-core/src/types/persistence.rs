use serde::{Deserialize, Serialize};

/// ORM-agnostic persistence model for a single entity.
///
/// Captures the structural knowledge (columns, children, relations, policies)
/// that every persistence backend needs, without backend-specific rendering details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceEntity {
    pub title: String,
    pub table_name: String,
    pub schema_name: String,
    pub rust_type_name: String,
    pub columns: Vec<PersistenceColumn>,
    pub child_tables: Vec<PersistenceChildTable>,
    pub relations: Vec<PersistenceEntityRelation>,
    pub policies: PersistencePolicies,
}

/// A column in the persistence model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceColumn {
    pub field_name: String,
    pub column_name: String,
    pub rust_type: String,
    pub pg_type: String,
    pub is_primary_key: bool,
    pub is_nullable: bool,
    pub is_jsonb: bool,
    pub is_range: bool,
    pub pg_cast: Option<String>,
    pub role: PersistenceColumnRole,
}

/// The semantic role of a column in the persistence model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceColumnRole {
    Data,
    PrimaryKey,
    TenantScope,
    SoftDeleteMarker,
    AuditTimestamp { kind: AuditTimestampKind },
    AuditUser { kind: AuditUserKind },
    AuditFlag,
    ForeignKey { ref_entity: String },
    HierarchyParent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditTimestampKind {
    Created,
    Updated,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditUserKind {
    DeletedBy,
    UpdatedBy,
}

/// A child table derived from a ValueObject property.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceChildTable {
    pub table_name: String,
    pub struct_name: String,
    pub parent_fk: String,
    pub parent_fk_column: String,
    pub columns: Vec<PersistenceColumn>,
    pub child_tables: Vec<PersistenceChildTable>,
}

/// A relationship between two entities in the persistence model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceEntityRelation {
    pub name: String,
    pub relation_type: String,
    pub related_entity: String,
    pub from_column: String,
    pub to_column: String,
    pub is_self_ref: bool,
}

/// Policy effects extracted from the policy graph, expressed in
/// implementation-agnostic terms.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistencePolicies {
    pub soft_delete: Option<SoftDeleteEffect>,
    pub tenant_isolation: Option<TenantIsolationEffect>,
    pub row_security: Vec<RowSecurityEffect>,
    pub audit: Option<AuditEffect>,
    pub retention: Option<RetentionEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftDeleteEffect {
    pub policy_name: String,
    pub marker_column: String,
    pub marker_type: String,
    pub visibility: String,
    pub cascade: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantIsolationEffect {
    pub policy_name: String,
    pub strategy: String,
    pub column: Option<String>,
    pub propagation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowSecurityEffect {
    pub operation: String,
    pub using_expr: Option<String>,
    pub check_expr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEffect {
    pub track_created: bool,
    pub track_updated: bool,
    pub track_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionEffect {
    pub retention_period_days: i64,
    pub archive_strategy: Option<String>,
}
