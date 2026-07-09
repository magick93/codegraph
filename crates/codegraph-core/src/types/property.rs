use serde::{Deserialize, Serialize};

use codegraph_type_contracts::{DddFieldProjection, RefClassificationKind};

/// If props is empty and the entity is a codelist (enum-only JSON schema with
/// zero properties), inject synthetic PropertyNodes for the three columns that
/// the codelist DDL template creates: code, display_name, sort_order.
///
/// This ensures entity model, DTO, and repository generators produce the same
/// columns that exist in the actual database table.
pub fn inject_codelist_properties(props: &mut Vec<PropertyNode>, is_codelist: bool, domain: &str) {
    // Only inject for common-domain codelists that have _codelist.sql migrations
    // with actual code/display_name/sort_order columns. Non-common-domain codelists
    // are created by the entity DDL generator with id UUID PRIMARY KEY and no code column.
    if !props.is_empty() || !is_codelist || domain != "common" {
        return;
    }
    // Helper to create a synthetic PropertyNode for a codelist column.
    let make = |name: &str, rust_field: &str, rust_type: &str, sea_orm_type: &str,
                pg_column: &str, pg_type: &str, is_required: bool| {
        PropertyNode {
            name: name.to_string(),
            prop_type: "string".to_string(),
            description: None,
            format: None,
            is_required,
            is_nullable: !is_required,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: pg_column.to_string(),
            pg_column_type: pg_type.to_string(),
            rust_field_name: rust_field.to_string(),
            rust_field_type: rust_type.to_string(),
            sea_orm_type: sea_orm_type.to_string(),
            render_strategy: "scalar".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    };
    props.push(make("code", "code", "String", "String", "code", "TEXT", true));
    props.push(make("display_name", "display_name", "String", "String", "display_name", "TEXT", true));
    props.push(make("sort_order", "sort_order", "i32", "Integer", "sort_order", "INTEGER", false));
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyNode {
    pub name: String,
    pub prop_type: String,
    pub description: Option<String>,
    pub format: Option<String>,
    pub is_required: bool,
    pub is_nullable: bool,
    pub is_array: bool,
    pub pattern: Option<String>,
    #[serde(default)]
    pub min_length: Option<u64>,
    #[serde(default)]
    pub max_length: Option<u64>,
    #[serde(default)]
    pub minimum: Option<rust_decimal::Decimal>,
    #[serde(default)]
    pub maximum: Option<rust_decimal::Decimal>,
    pub pg_column_name: String,
    pub pg_column_type: String,
    pub rust_field_name: String,
    pub rust_field_type: String,
    pub sea_orm_type: String,
    pub render_strategy: String,
    pub ref_target: Option<String>,
    pub classification: Option<String>,

    // NEW: typed projection fields — optional for backward compat
    /// Pre-computed cross-layer projection. When Some, generators should read
    /// types from here instead of the string fields above.
    #[serde(default)]
    pub projection: Option<DddFieldProjection>,
    /// Typed classification kind. When Some, replaces render_strategy string matching.
    #[serde(default)]
    pub classification_kind: Option<RefClassificationKind>,

    // UI override fields — populated during ingestion from ui-overrides.toml
    /// UI override component for 'detail' render context (from ui-overrides.toml).
    #[serde(default)]
    pub ui_override_detail: Option<String>,
    /// UI override component for 'list-cell' render context.
    #[serde(default)]
    pub ui_override_list_cell: Option<String>,
    /// UI override component for 'form' render context.
    #[serde(default)]
    pub ui_override_form: Option<String>,
    /// UI override component for 'inline' render context.
    #[serde(default)]
    pub ui_override_inline: Option<String>,
}

impl PropertyNode {
    /// Returns the typed classification kind, falling back to parsing the
    /// `classification` string, then `render_strategy` string if neither
    /// typed field has been populated yet.
    pub fn effective_kind(&self) -> Option<RefClassificationKind> {
        // Priority 1: typed field
        if let Some(ref kind) = self.classification_kind {
            return Some(kind.clone());
        }
        // Priority 2: classification string
        if let Some(ref cls) = self.classification {
            if let Some(kind) = parse_classification_str(cls) {
                return Some(kind);
            }
        }
        // Priority 3: render_strategy string (fallback for legacy ingestion)
        parse_classification_str(&self.render_strategy)
    }
}

fn parse_classification_str(s: &str) -> Option<RefClassificationKind> {
    match s {
        "primitive_wrapper" => Some(RefClassificationKind::PrimitiveWrapper),
        "array_wrapper" => Some(RefClassificationKind::ArrayWrapper),
        "range_wrapper" => Some(RefClassificationKind::RangeWrapper),
        "codelist_reference" | "codelist" => Some(RefClassificationKind::CodelistReference),
        "codelist_check" => Some(RefClassificationKind::CodelistCheck),
        "inline_enum" => Some(RefClassificationKind::InlineEnum),
        "entity_reference" => Some(RefClassificationKind::EntityReference),
        "value_object" | "child_table" => Some(RefClassificationKind::ValueObject),
        "composite_wrapper" => Some(RefClassificationKind::CompositeWrapper),
        "structured_wrapper" => Some(RefClassificationKind::StructuredWrapper),
        "media_wrapper" => Some(RefClassificationKind::MediaWrapper),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_property() -> PropertyNode {
        PropertyNode {
            name: "test".into(),
            prop_type: "string".into(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "test".into(),
            pg_column_type: "TEXT".into(),
            rust_field_name: "test".into(),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "flat".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    #[test]
    fn effective_kind_prefers_typed_field() {
        let mut prop = base_property();
        prop.classification = Some("value_object".into());
        prop.classification_kind = Some(RefClassificationKind::EntityReference);
        // typed field takes priority over string
        assert_eq!(
            prop.effective_kind(),
            Some(RefClassificationKind::EntityReference)
        );
    }

    #[test]
    fn effective_kind_falls_back_to_classification_string() {
        let cases = vec![
            ("primitive_wrapper", RefClassificationKind::PrimitiveWrapper),
            ("array_wrapper", RefClassificationKind::ArrayWrapper),
            ("range_wrapper", RefClassificationKind::RangeWrapper),
            ("codelist", RefClassificationKind::CodelistReference),
            (
                "codelist_reference",
                RefClassificationKind::CodelistReference,
            ),
            ("codelist_check", RefClassificationKind::CodelistCheck),
            ("inline_enum", RefClassificationKind::InlineEnum),
            ("entity_reference", RefClassificationKind::EntityReference),
            ("value_object", RefClassificationKind::ValueObject),
            ("composite_wrapper", RefClassificationKind::CompositeWrapper),
        ];

        for (input, expected) in cases {
            let mut prop = base_property();
            prop.classification = Some(input.into());
            assert_eq!(
                prop.effective_kind(),
                Some(expected),
                "failed for classification string: {input}"
            );
        }
    }

    #[test]
    fn effective_kind_returns_none_when_both_absent() {
        let prop = base_property();
        assert_eq!(prop.effective_kind(), None);
    }

    #[test]
    fn effective_kind_returns_none_for_unknown_string() {
        let mut prop = base_property();
        prop.classification = Some("unknown_thing".into());
        assert_eq!(prop.effective_kind(), None);
    }

    #[test]
    fn effective_kind_falls_back_to_render_strategy() {
        let mut prop = base_property();
        prop.render_strategy = "codelist".into();
        // classification is None, render_strategy has the classification string
        assert_eq!(
            prop.effective_kind(),
            Some(RefClassificationKind::CodelistReference),
        );
    }

    #[test]
    fn effective_kind_prefers_classification_kind_over_render_strategy() {
        let mut prop = base_property();
        prop.render_strategy = "codelist".into();
        prop.classification_kind = Some(RefClassificationKind::PrimitiveWrapper);
        // typed field takes priority over render_strategy string
        assert_eq!(
            prop.effective_kind(),
            Some(RefClassificationKind::PrimitiveWrapper),
        );
    }

    #[test]
    fn property_node_has_validation_fields() {
        let mut prop = base_property();
        prop.min_length = Some(1);
        prop.max_length = Some(255);
        prop.minimum = Some(rust_decimal::Decimal::ZERO);
        prop.maximum = Some(rust_decimal::Decimal::new(999, 0));
        assert_eq!(prop.min_length, Some(1));
        assert_eq!(prop.max_length, Some(255));
        assert_eq!(prop.minimum, Some(rust_decimal::Decimal::ZERO));
        assert_eq!(prop.maximum, Some(rust_decimal::Decimal::new(999, 0)));
    }

    #[test]
    fn effective_kind_render_strategy_all_variants() {
        let cases = vec![
            ("primitive_wrapper", RefClassificationKind::PrimitiveWrapper),
            ("entity_reference", RefClassificationKind::EntityReference),
            ("value_object", RefClassificationKind::ValueObject),
            ("composite_wrapper", RefClassificationKind::CompositeWrapper),
            ("inline_enum", RefClassificationKind::InlineEnum),
        ];
        for (strategy, expected) in cases {
            let mut prop = base_property();
            prop.render_strategy = strategy.into();
            assert_eq!(
                prop.effective_kind(),
                Some(expected),
                "failed for render_strategy: {strategy}"
            );
        }
    }

    #[test]
    fn effective_kind_parses_structured_wrapper() {
        let mut prop = base_property();
        prop.classification = Some("structured_wrapper".into());
        assert_eq!(
            prop.effective_kind(),
            Some(RefClassificationKind::StructuredWrapper)
        );
    }
}
