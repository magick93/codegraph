/// Errors during field resolution — never silently degrade to serde_json::Value.
/// Note: This type is forward-looking — it is not used until Phase 5 (error hardening).
/// Created now so the crate API is complete from the start.
#[derive(Debug, thiserror::Error)]
pub enum FieldResolutionError {
    #[error("unclassified $ref '{type_name}' in field '{field_name}' of entity '{entity}'")]
    UnclassifiedRef {
        type_name: String,
        field_name: String,
        entity: String,
    },

    #[error("codelist '{table_name}' not found in codelist_enum_map for field '{field_name}'")]
    MissingCodelistEnum {
        table_name: String,
        field_name: String,
    },
}
