use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeList {
    pub name: String,
    pub description: Option<String>,
    pub pg_table_name: String,
    pub render_as: String,
    pub check_expression: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumValue {
    pub value: String,
    pub display_name: Option<String>,
    pub sort_order: i32,
}
