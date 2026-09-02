use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyKind {
    SoftDelete(SoftDeletePolicy),
    TenantIsolation(TenantIsolationPolicy),
    RowSecurity(RowSecurityPolicy),
    Audit(AuditPolicy),
    Retention(RetentionPolicy),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyNode {
    pub name: String,
    pub kind: PolicyKind,
    pub target_schema: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoftDeletePolicy {
    pub marker: SoftDeleteMarker,
    pub visibility: SoftDeleteVisibility,
    pub cascade: DeletionPropagation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftDeleteMarker {
    Timestamp(String),
    Boolean(String),
    Status(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftDeleteVisibility {
    ExcludeByDefault,
    IncludeByDefault,
    ExplicitOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionPropagation {
    Restrict,
    Cascade,
    SoftCascade,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TenantIsolationPolicy {
    pub strategy: TenantStrategy,
    pub propagation: TenantPropagation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TenantStrategy {
    Column { property: String },
    Relationship { relationship: String },
    Schema,
    Database,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantPropagation {
    Explicit,
    Inherited,
    Derived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowOperation {
    Select,
    Insert,
    Update,
    Delete,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowSecurityPolicy {
    pub operation: RowOperation,
    pub using_expr: Option<String>,
    pub check_expr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditPolicy {
    pub track_created: bool,
    pub track_updated: bool,
    pub track_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub retention_period_days: i64,
    pub archive_strategy: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_node_serde_roundtrip() {
        let node = PolicyNode {
            name: "soft_delete_policy".to_string(),
            kind: PolicyKind::SoftDelete(SoftDeletePolicy {
                marker: SoftDeleteMarker::Timestamp("deleted_at".to_string()),
                visibility: SoftDeleteVisibility::ExcludeByDefault,
                cascade: DeletionPropagation::SoftCascade,
            }),
            target_schema: "public.users".to_string(),
            domain: Some("sales".to_string()),
        };

        let json = serde_json::to_string(&node).expect("serialize");
        let deserialized: PolicyNode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(node, deserialized);
    }

    #[test]
    fn policy_kind_soft_delete_serde() {
        let kind = PolicyKind::SoftDelete(SoftDeletePolicy {
            marker: SoftDeleteMarker::Boolean("is_deleted".to_string()),
            visibility: SoftDeleteVisibility::IncludeByDefault,
            cascade: DeletionPropagation::Restrict,
        });

        let json = serde_json::to_string(&kind).expect("serialize");
        let expected = r#"{"type":"soft_delete","marker":{"boolean":"is_deleted"},"visibility":"include_by_default","cascade":"restrict"}"#;
        assert_eq!(json, expected);

        let deserialized: PolicyKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(kind, deserialized);
    }

    #[test]
    fn policy_kind_tenant_isolation_serde() {
        let kind = PolicyKind::TenantIsolation(TenantIsolationPolicy {
            strategy: TenantStrategy::Column {
                property: "tenant_id".to_string(),
            },
            propagation: TenantPropagation::Inherited,
        });

        let json = serde_json::to_string(&kind).expect("serialize");
        let expected = r#"{"type":"tenant_isolation","strategy":{"type":"column","property":"tenant_id"},"propagation":"inherited"}"#;
        assert_eq!(json, expected);

        let deserialized: PolicyKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(kind, deserialized);
    }

    #[test]
    fn deletion_propagation_serde() {
        let variants = vec![
            ("restrict", DeletionPropagation::Restrict),
            ("cascade", DeletionPropagation::Cascade),
            ("soft_cascade", DeletionPropagation::SoftCascade),
            ("ignore", DeletionPropagation::Ignore),
        ];

        for (expected_str, variant) in variants {
            let json = serde_json::to_string(&variant).expect("serialize");
            assert_eq!(json, format!("\"{}\"", expected_str));

            let deserialized: DeletionPropagation =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(variant, deserialized);
        }
    }
}
