use serde::{Deserialize, Serialize};

use super::policy::TenantStrategy;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityIdentityNode {
    pub name: String,
    pub subject: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MembershipNode {
    pub identity: String,
    pub tenant: String,
    pub status: MembershipStatus,
    pub roles: Vec<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    Active,
    Inactive,
    Suspended,
    Invited,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TenantNode {
    pub name: String,
    pub label: String,
    pub strategy: TenantStrategy,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub kind: ScopeKind,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Global,
    Tenant,
    OrganisationUnit,
    Project,
    Resource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_node_serde_roundtrip() {
        let node = SecurityIdentityNode {
            name: "user_123".to_string(),
            subject: "auth0|abc123".to_string(),
            domain: Some("sales".to_string()),
        };

        let json = serde_json::to_string(&node).expect("serialize");
        let deserialized: SecurityIdentityNode =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(node, deserialized);
    }

    #[test]
    fn membership_status_serde() {
        let variants = vec![
            ("active", MembershipStatus::Active),
            ("inactive", MembershipStatus::Inactive),
            ("suspended", MembershipStatus::Suspended),
            ("invited", MembershipStatus::Invited),
        ];

        for (expected_str, variant) in variants {
            let json = serde_json::to_string(&variant).expect("serialize");
            assert_eq!(json, format!("\"{}\"", expected_str));

            let deserialized: MembershipStatus =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(variant, deserialized);
        }
    }

    #[test]
    fn tenant_node_serde_roundtrip() {
        let node = TenantNode {
            name: "acme_corp".to_string(),
            label: "Acme Corporation".to_string(),
            strategy: TenantStrategy::Column {
                property: "tenant_id".to_string(),
            },
            domain: Some("sales".to_string()),
        };

        let json = serde_json::to_string(&node).expect("serialize");
        let deserialized: TenantNode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(node, deserialized);
    }
}
