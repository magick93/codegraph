use serde::{Deserialize, Serialize};

use crate::field_role::ScalarKind;
use crate::{DddFieldProjection, FieldRole};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedField {
    pub json_name: String,
    pub rust_name: String,
    pub db_column_name: String,
    pub is_required: bool,
    pub is_array: bool,
    pub role: FieldRole,
    pub classification: FieldClassification,
    pub projection: DddFieldProjection,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldClassification {
    Classified(RefClassificationKind),
    DirectPrimitive(ScalarKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefClassificationKind {
    PrimitiveWrapper,
    ArrayWrapper,
    RangeWrapper,
    CodelistReference,
    /// Codelist rendered as CHECK constraint instead of FK lookup.
    /// Determined by `codelist_as_check.schemas` in classifier.toml.
    CodelistCheck,
    InlineEnum,
    EntityReference,
    ValueObject,
    CompositeWrapper,
    /// Structured JSONB wrapper — a typed Rust struct stored as JSONB.
    /// Configured via `[structured_wrappers]` in classifier.toml.
    StructuredWrapper,
    /// Media reference stored as URL + MIME type columns.
    /// Configured via `[media_wrappers]` in classifier.toml.
    MediaWrapper,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_wrapper_round_trips_through_serde() {
        let kind = RefClassificationKind::MediaWrapper;
        let json = serde_json::to_string(&kind).unwrap();
        let back: RefClassificationKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, RefClassificationKind::MediaWrapper);
    }
}
