use serde::{Deserialize, Serialize};

use super::policy::DeletionPropagation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipNode {
    pub name: String,
    pub source_schema: String,
    pub target_schema: String,
    pub cardinality: Cardinality,
    pub ownership: Ownership,
    pub foreign_key: Option<ForeignKeySpec>,
    pub propagation: Vec<PropagationRule>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Owns,
    References,
    BelongsTo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignKeySpec {
    pub source_column: String,
    pub target_schema: String,
    pub target_table: String,
    pub target_column: String,
    pub on_delete: String,
    pub on_update: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropagationRule {
    pub trigger: PropagationTrigger,
    pub behavior: DeletionPropagation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropagationTrigger {
    OnDelete,
    OnCreate,
    OnUpdate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationship_node_serde_roundtrip() {
        let node = RelationshipNode {
            name: "order_customer".to_string(),
            source_schema: "public.orders".to_string(),
            target_schema: "public.customers".to_string(),
            cardinality: Cardinality::ManyToOne,
            ownership: Ownership::References,
            foreign_key: Some(ForeignKeySpec {
                source_column: "customer_id".to_string(),
                target_schema: "public".to_string(),
                target_table: "customers".to_string(),
                target_column: "id".to_string(),
                on_delete: "RESTRICT".to_string(),
                on_update: "CASCADE".to_string(),
            }),
            propagation: vec![PropagationRule {
                trigger: PropagationTrigger::OnDelete,
                behavior: DeletionPropagation::Restrict,
            }],
            domain: Some("sales".to_string()),
        };

        let json = serde_json::to_string(&node).expect("serialize");
        let deserialized: RelationshipNode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(node, deserialized);
    }

    #[test]
    fn cardinality_serde() {
        let variants = vec![
            ("one_to_one", Cardinality::OneToOne),
            ("one_to_many", Cardinality::OneToMany),
            ("many_to_one", Cardinality::ManyToOne),
            ("many_to_many", Cardinality::ManyToMany),
        ];

        for (expected_str, variant) in variants {
            let json = serde_json::to_string(&variant).expect("serialize");
            assert_eq!(json, format!("\"{}\"", expected_str));

            let deserialized: Cardinality = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(variant, deserialized);
        }
    }

    #[test]
    fn ownership_serde() {
        let variants = vec![
            ("owns", Ownership::Owns),
            ("references", Ownership::References),
            ("belongs_to", Ownership::BelongsTo),
        ];

        for (expected_str, variant) in variants {
            let json = serde_json::to_string(&variant).expect("serialize");
            assert_eq!(json, format!("\"{}\"", expected_str));

            let deserialized: Ownership = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(variant, deserialized);
        }
    }
}
