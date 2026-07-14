use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NamespaceNode {
    pub authority: String,
    pub segment: String,
    pub domain: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LexiconNode {
    pub nsid: String,
    pub lex_type: String,
    pub key_strategy: String,
    pub revision: Option<i64>,
    pub description: Option<String>,
    pub domain: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CollectionNode {
    pub nsid: String,
    pub key_strategy: String,
    pub domain: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RepositoryNode {
    pub did: String,
    pub handle: Option<String>,
    pub pds_endpoint: String,
    pub org_name: String,
    pub tenancy_mode: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_node_serialization_roundtrip() {
        let node = NamespaceNode {
            authority: "nz.gravy".to_string(),
            segment: "grants".to_string(),
            domain: "atproto".to_string(),
        };
        let json = serde_json::to_string(&node).unwrap();
        let roundtripped: NamespaceNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, roundtripped);
    }

    #[test]
    fn lexicon_node_serialization_roundtrip() {
        let node = LexiconNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            lex_type: "record".to_string(),
            key_strategy: "tid".to_string(),
            revision: Some(1),
            description: Some("A grant record".to_string()),
            domain: "atproto".to_string(),
        };
        let json = serde_json::to_string(&node).unwrap();
        let roundtripped: LexiconNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, roundtripped);
    }

    #[test]
    fn collection_node_serialization_roundtrip() {
        let node = CollectionNode {
            nsid: "nz.gravy.grants.grant".to_string(),
            key_strategy: "tid".to_string(),
            domain: "atproto".to_string(),
        };
        let json = serde_json::to_string(&node).unwrap();
        let roundtripped: CollectionNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, roundtripped);
    }

    #[test]
    fn repository_node_serialization_roundtrip() {
        let node = RepositoryNode {
            did: "did:plc:abc123".to_string(),
            handle: Some("gravy.nz".to_string()),
            pds_endpoint: "https://pds.gravy.nz".to_string(),
            org_name: "Gravy".to_string(),
            tenancy_mode: "shared_pds".to_string(),
        };
        let json = serde_json::to_string(&node).unwrap();
        let roundtripped: RepositoryNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, roundtripped);
    }

    #[test]
    fn namespace_node_default_has_empty_fields() {
        let node = NamespaceNode::default();
        assert_eq!(node.authority, "");
        assert_eq!(node.segment, "");
        assert_eq!(node.domain, "");
    }

    #[test]
    fn lexicon_node_default_has_sane_values() {
        let node = LexiconNode::default();
        assert_eq!(node.nsid, "");
        assert_eq!(node.lex_type, "");
        assert_eq!(node.key_strategy, "");
        assert_eq!(node.revision, None);
        assert_eq!(node.description, None);
        assert_eq!(node.domain, "");
    }

    #[test]
    fn collection_node_default_has_empty_fields() {
        let node = CollectionNode::default();
        assert_eq!(node.nsid, "");
        assert_eq!(node.key_strategy, "");
        assert_eq!(node.domain, "");
    }

    #[test]
    fn repository_node_default_has_sane_values() {
        let node = RepositoryNode::default();
        assert_eq!(node.did, "");
        assert_eq!(node.handle, None);
        assert_eq!(node.pds_endpoint, "");
        assert_eq!(node.org_name, "");
        assert_eq!(node.tenancy_mode, "");
    }

    #[test]
    fn edge_types_can_be_constructed_and_matched() {
        use crate::types::edge::EdgeType;

        let edges = vec![
            EdgeType::InNamespace,
            EdgeType::ProjectsToLexicon,
            EdgeType::DefinesCollection,
            EdgeType::LexiconReferences,
            EdgeType::StoredInRepository,
        ];

        for edge in &edges {
            match edge {
                EdgeType::InNamespace => assert!(true),
                EdgeType::ProjectsToLexicon => assert!(true),
                EdgeType::DefinesCollection => assert!(true),
                EdgeType::LexiconReferences => assert!(true),
                EdgeType::StoredInRepository => assert!(true),
                _ => panic!("unexpected edge variant"),
            }
        }

        assert_eq!(edges.len(), 5);
    }

    #[test]
    fn lexicon_node_json_structure() {
        let node = LexiconNode {
            nsid: "com.example.lexicon".to_string(),
            lex_type: "query".to_string(),
            key_strategy: "did".to_string(),
            revision: None,
            description: None,
            domain: "example".to_string(),
        };
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["nsid"], "com.example.lexicon");
        assert_eq!(json["lex_type"], "query");
        assert_eq!(json["key_strategy"], "did");
        assert_eq!(json["revision"], serde_json::Value::Null);
        assert_eq!(json["description"], serde_json::Value::Null);
        assert_eq!(json["domain"], "example");
    }
}
