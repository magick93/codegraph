use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaNode {
    pub schema_id: String,
    pub title: String,
    pub description: Option<String>,
    pub schema_type: String,
    pub classification: String,
    pub domain: Option<String>,
    pub rel_path: String,
    pub pg_type: String,
    pub rust_type: String,
    pub sea_orm_type: String,
    pub rust_type_name: String,
    pub pg_table_name: String,
    pub api_path_segment: String,
    pub parent_schema: Option<String>,
    pub is_entity: bool,
    pub is_codelist: bool,
    pub is_primitive_wrapper: bool,
    pub has_all_of: bool,
    pub has_one_of: bool,
    pub has_any_of: bool,
    pub has_definitions: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaClassificationData {
    pub title: String,
    pub domain: Option<String>,
    pub rel_path: String,
    pub schema_type: String,
    pub is_codelist: bool,
    pub is_primitive_wrapper: bool,
    pub has_all_of: bool,
    pub composes_noun_type: bool,
    pub field_count: usize,
    pub required_field_count: usize,
    pub ref_count: usize,
    pub in_degree: usize,
    pub is_enum: bool,
    pub is_string_type: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_schema() -> SchemaNode {
        SchemaNode {
            schema_id: "recruiting/candidate_type.json".into(),
            title: "CandidateType".into(),
            description: None,
            schema_type: "object".into(),
            classification: "entity".into(),
            domain: Some("recruiting".into()),
            rel_path: "recruiting/candidate_type.json".into(),
            pg_type: "JSONB".into(),
            rust_type: "candidate::Model".into(),
            sea_orm_type: "candidate".into(),
            rust_type_name: "CandidateType".into(),
            pg_table_name: "candidate".into(),
            api_path_segment: "candidates".into(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
        }
    }

    #[test]
    fn schema_node_defaults_for_non_entity() {
        let s = SchemaNode {
            schema_id: "common/some_type.json".into(),
            title: "SomeType".into(),
            description: None,
            schema_type: "object".into(),
            classification: String::new(),
            domain: None,
            rel_path: "common/some_type.json".into(),
            pg_type: String::new(),
            rust_type: String::new(),
            sea_orm_type: String::new(),
            rust_type_name: String::new(),
            pg_table_name: String::new(),
            api_path_segment: String::new(),
            parent_schema: None,
            is_entity: false,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
        };
        assert!(!s.is_entity);
        assert!(!s.is_codelist);
        assert!(s.api_path_segment.is_empty());
        assert!(s.domain.is_none());
    }

    #[test]
    fn schema_node_api_path_segment_independent_of_title() {
        let s = SchemaNode {
            api_path_segment: "people".into(),
            title: "PersonType".into(),
            ..base_schema()
        };
        assert_eq!(s.api_path_segment, "people");
        assert_eq!(s.title, "PersonType");
    }

    #[test]
    fn schema_node_is_codelist_independent_of_is_entity() {
        let s = SchemaNode {
            is_entity: false,
            is_codelist: true,
            classification: "codelist".into(),
            ..base_schema()
        };
        assert!(!s.is_entity);
        assert!(s.is_codelist);
    }

    #[test]
    fn schema_node_serde_roundtrip() {
        let s = SchemaNode {
            description: Some("A candidate for a job position".into()),
            parent_schema: Some("recruiting/super_type.json".into()),
            has_all_of: true,
            ..base_schema()
        };
        let json = serde_json::to_string(&s).unwrap();
        let s2: SchemaNode = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn schema_classification_data_serde_roundtrip() {
        let d = SchemaClassificationData {
            title: "CandidateType".into(),
            domain: Some("recruiting".into()),
            rel_path: "recruiting/candidate_type.json".into(),
            schema_type: "object".into(),
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: true,
            composes_noun_type: true,
            field_count: 12,
            required_field_count: 4,
            ref_count: 3,
            in_degree: 1,
            is_enum: false,
            is_string_type: false,
        };
        let json = serde_json::to_string(&d).unwrap();
        let d2: SchemaClassificationData = serde_json::from_str(&json).unwrap();
        assert_eq!(d, d2);
    }
}
