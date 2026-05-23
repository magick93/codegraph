//! Name-based field role detection and type-based scalar classification.

use std::collections::HashSet;

/// Orthogonal name-based role for a schema field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FieldRole {
    /// Primary key (`id`)
    Identity,
    /// Timestamp fields (`created_at`, `updated_at`)
    Timestamp,
    /// Tenant identifier (`tenant_id`)
    TenantId,
    /// Foreign key to parent entity
    ParentFk,
    /// Regular data field
    Data,
}

impl FieldRole {
    /// Detect the role of a field from its snake_case name and known FK set.
    pub fn detect(field_name: &str, known_fks: &HashSet<String>) -> Self {
        if field_name == "id" {
            return Self::Identity;
        }
        if field_name == "created_at" || field_name == "updated_at" {
            return Self::Timestamp;
        }
        if field_name == "tenant_id" {
            return Self::TenantId;
        }
        if known_fks.contains(field_name) {
            return Self::ParentFk;
        }
        Self::Data
    }

    /// Whether this field is system-managed (excluded from create/update DTOs).
    pub fn is_system_managed(&self) -> bool {
        !matches!(self, Self::Data)
    }
}

/// Type-based classification for unclassified scalar fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScalarKind {
    String,
    Integer,
    Number,
    Boolean,
    Date,
    DateTime,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn detect_identity() {
        assert_eq!(
            FieldRole::detect("id", &HashSet::new()),
            FieldRole::Identity
        );
    }

    #[test]
    fn detect_timestamp() {
        assert_eq!(
            FieldRole::detect("created_at", &HashSet::new()),
            FieldRole::Timestamp
        );
        assert_eq!(
            FieldRole::detect("updated_at", &HashSet::new()),
            FieldRole::Timestamp
        );
    }

    #[test]
    fn detect_tenant_id() {
        assert_eq!(
            FieldRole::detect("tenant_id", &HashSet::new()),
            FieldRole::TenantId
        );
    }

    #[test]
    fn detect_parent_fk() {
        let mut fks = HashSet::new();
        fks.insert("person_id".to_string());
        assert_eq!(FieldRole::detect("person_id", &fks), FieldRole::ParentFk);
    }

    #[test]
    fn detect_data() {
        assert_eq!(
            FieldRole::detect("given_name", &HashSet::new()),
            FieldRole::Data
        );
    }

    #[test]
    fn system_managed_flags() {
        assert!(FieldRole::Identity.is_system_managed());
        assert!(FieldRole::Timestamp.is_system_managed());
        assert!(FieldRole::TenantId.is_system_managed());
        assert!(FieldRole::ParentFk.is_system_managed());
        assert!(!FieldRole::Data.is_system_managed());
    }
}
