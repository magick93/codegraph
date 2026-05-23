use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DetectionSource {
    ScalarRef,  // Required scalar $ref to entity
    ArrayItems, // Parent has array property with ItemsOf edge to entity
    Manual,     // Configured in domains.toml
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParentCandidate {
    pub child_title: String,
    pub parent_title: String,
    pub field_name: String,
    pub source: DetectionSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extension {
    pub name: String,
}
