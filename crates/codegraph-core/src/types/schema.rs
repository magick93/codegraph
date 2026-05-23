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
