use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeColumn {
    pub suffix: String,
    pub pg_type: String,
    pub rust_type: String,
    pub sea_orm_type: String,
    pub fk_target: Option<String>,
    /// When present, the DTO generator uses this type instead of `rust_type`.
    /// Used for codelist enum types (e.g. `CurrencyCodeList`) that should appear
    /// in DTOs/OpenAPI instead of plain `String`.
    #[serde(default)]
    pub dto_rust_type: Option<String>,
    /// The wrapper schema this column belongs to (e.g. "AmountType", "GeoType").
    /// Used to scope composite columns so different wrapper types with the same
    /// suffix don't collide in the graph.
    #[serde(default)]
    pub wrapper_schema: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeRange {
    pub pg_column_name: String,
    pub pg_type: String,
    pub rust_type: String,
    pub start_field: String,
    pub end_field: String,
    pub open_end: bool,
}

/// A sub-field of a StructuredWrapper type (e.g., IdentifierType).
/// Returned by `GraphQuerier::get_structured_sub_fields()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredSubField {
    /// camelCase property name as defined in the JSON schema (e.g. "schemeId")
    pub name: String,
    pub description: String,
    pub is_required: bool,
}
