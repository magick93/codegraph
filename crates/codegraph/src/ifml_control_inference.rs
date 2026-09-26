//! Schema-property → IFML control inference (issues #196, #201).
//!
//! Thin classifier-context wrapper over the shared pure core in
//! [`codegraph_generate::ifml::control_core`]: `PropertyNode` signals are
//! projected onto the core's `FieldSignals` (kind → `FieldKind`, bounds →
//! `has_bounds`), and the core's ordered heuristics produce the decision.
//! Dropdown value resolution and the entity-reference options note stay
//! graph-side; the route generator consumes the same core through
//! `control_for_field`, so input-type + role decisions cannot diverge.

use codegraph_core::types::PropertyNode;
use codegraph_generate::ifml::control_core::infer_control as core_infer_control;
use codegraph_type_contracts::RefClassificationKind;

pub use codegraph_generate::ifml::control_core::MAX_CONTROL_VALUES;
pub use codegraph_generate::ifml::control_core::{
    control_for_field, ControlInference, ControlInputType, FieldKind, FieldSignals, HtmlInput,
    NOTE_OPTIONS_ENDPOINT,
};

/// Infer the control for one schema property.
pub fn infer_control(prop: &PropertyNode) -> ControlInference {
    core_infer_control(&signals_for(prop))
}

/// Infer just the DSL input-type keyword for one schema property.
pub fn infer_input_type(prop: &PropertyNode) -> &'static str {
    infer_control(prop).input.as_str()
}

/// Codelist/enum classifications whose value sets live in the graph.
pub fn is_codelist_kind(kind: Option<&RefClassificationKind>) -> bool {
    matches!(
        kind,
        Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::CodelistCheck)
            | Some(RefClassificationKind::InlineEnum)
    )
}

fn signals_for(prop: &PropertyNode) -> FieldSignals<'_> {
    let kind = prop.effective_kind();
    FieldSignals {
        name: &prop.name,
        rust_type: &prop.rust_field_type,
        prop_type: Some(prop.prop_type.as_str()),
        format: prop.format.as_deref(),
        is_array: prop.is_array,
        has_bounds: prop.minimum.is_some() || prop.maximum.is_some(),
        kind: field_kind(kind.as_ref()),
        is_required: prop.is_required,
    }
}

fn field_kind(kind: Option<&RefClassificationKind>) -> Option<FieldKind> {
    if is_codelist_kind(kind) {
        Some(FieldKind::Codelist)
    } else if kind == Some(&RefClassificationKind::EntityReference) {
        Some(FieldKind::EntityRef)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::SemanticRole;
    use rex_ifml::InputFieldType;
    use rust_decimal::Decimal;

    fn prop(name: &str, prop_type: &str) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: prop_type.to_string(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: name.to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: name.to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "String".to_string(),
            render_strategy: "scalar".to_string(),
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

    fn required(mut p: PropertyNode) -> PropertyNode {
        p.is_required = true;
        p
    }

    fn with_kind(mut p: PropertyNode, kind: RefClassificationKind) -> PropertyNode {
        p.classification_kind = Some(kind);
        p
    }

    fn with_format(mut p: PropertyNode, format: &str) -> PropertyNode {
        p.format = Some(format.to_string());
        p
    }

    fn with_rust_type(mut p: PropertyNode, rust_type: &str) -> PropertyNode {
        p.rust_field_type = rust_type.to_string();
        p
    }

    fn as_array(mut p: PropertyNode) -> PropertyNode {
        p.is_array = true;
        p
    }

    fn with_bounds(mut p: PropertyNode, min: i64, max: i64) -> PropertyNode {
        p.minimum = Some(Decimal::from(min));
        p.maximum = Some(Decimal::from(max));
        p
    }

    #[test]
    fn input_types_round_trip_through_the_dsl() {
        let expected = [
            (ControlInputType::Text, InputFieldType::Text),
            (ControlInputType::TextArea, InputFieldType::TextArea),
            (ControlInputType::Password, InputFieldType::Password),
            (ControlInputType::Email, InputFieldType::Email),
            (ControlInputType::Number, InputFieldType::Number),
            (ControlInputType::Date, InputFieldType::Date),
            (ControlInputType::Time, InputFieldType::Time),
            (ControlInputType::DateTime, InputFieldType::DateTime),
            (ControlInputType::Dropdown, InputFieldType::Dropdown),
            (ControlInputType::Radio, InputFieldType::RadioGroup),
            (ControlInputType::Checkbox, InputFieldType::Checkbox),
            (ControlInputType::Toggle, InputFieldType::Toggle),
            (ControlInputType::File, InputFieldType::File),
            (ControlInputType::Hidden, InputFieldType::Hidden),
        ];
        for (ours, dsl) in expected {
            assert_eq!(
                InputFieldType::from(ours.as_str()),
                dsl,
                "{ours:?} does not round-trip through the DSL grammar"
            );
        }
    }

    #[test]
    fn arrays_map_to_textarea() {
        assert_eq!(
            infer_control(&as_array(prop("tags", "string"))).input,
            ControlInputType::TextArea
        );
    }

    #[test]
    fn codelist_kinds_map_to_dropdown_selection_fields() {
        for kind in [
            RefClassificationKind::CodelistReference,
            RefClassificationKind::CodelistCheck,
            RefClassificationKind::InlineEnum,
        ] {
            let inf = infer_control(&with_kind(prop("status", "string"), kind.clone()));
            assert_eq!(inf.input, ControlInputType::Dropdown, "{kind:?}");
            assert_eq!(inf.role, SemanticRole::SelectionField, "{kind:?}");
            assert_eq!(inf.note, None, "{kind:?}");
        }
    }

    #[test]
    fn entity_references_map_to_dropdown_with_options_note() {
        let inf = infer_control(&with_kind(
            prop("manager_id", "string"),
            RefClassificationKind::EntityReference,
        ));
        assert_eq!(inf.input, ControlInputType::Dropdown);
        assert_eq!(inf.role, SemanticRole::SelectionField);
        assert_eq!(inf.note, Some(NOTE_OPTIONS_ENDPOINT));
        assert!(inf.values.is_empty());
    }

    #[test]
    fn email_name_and_format_map_to_email() {
        let inf = infer_control(&prop("contact_email", "string"));
        assert_eq!(inf.input, ControlInputType::Email);
        assert_eq!(inf.role, SemanticRole::Field);

        let inf = infer_control(&with_format(prop("contact", "string"), "email"));
        assert_eq!(inf.input, ControlInputType::Email);
    }

    #[test]
    fn password_and_secret_names_map_to_password() {
        let inf = infer_control(&prop("password_hash", "string"));
        assert_eq!(inf.input, ControlInputType::Password);
        assert_eq!(
            infer_control(&prop("client_secret", "string")).input,
            ControlInputType::Password
        );
    }

    #[test]
    fn boolean_maps_to_checkbox() {
        let inf = infer_control(&prop("completed", "boolean"));
        assert_eq!(inf.input, ControlInputType::Checkbox);
        assert_eq!(inf.role, SemanticRole::Field);
    }

    #[test]
    fn numeric_types_map_to_number() {
        assert_eq!(
            infer_control(&prop("priority", "integer")).input,
            ControlInputType::Number
        );
        assert_eq!(
            infer_control(&prop("rate", "number")).input,
            ControlInputType::Number
        );
    }

    #[test]
    fn numeric_bounds_map_to_number() {
        let inf = infer_control(&with_bounds(prop("amount", "string"), 0, 100));
        assert_eq!(inf.input, ControlInputType::Number);
    }

    #[test]
    fn uuid_rust_types_map_to_hidden() {
        let inf = infer_control(&with_rust_type(prop("list_id", "string"), "Uuid"));
        assert_eq!(inf.input, ControlInputType::Hidden);
        assert_eq!(
            infer_control(&with_rust_type(prop("list_id", "string"), "Option<Uuid>")).input,
            ControlInputType::Hidden
        );
    }

    #[test]
    fn date_time_formats_map_to_datetime() {
        for fmt in ["date", "date-time", "time"] {
            let inf = infer_control(&with_format(prop("window", "string"), fmt));
            assert_eq!(inf.input, ControlInputType::DateTime, "format {fmt}");
        }
    }

    #[test]
    fn rust_date_time_types_map_to_datetime() {
        for rust_type in [
            "NaiveDateTime",
            "DateTime<Utc>",
            "NaiveDate",
            "chrono::NaiveTime",
        ] {
            let inf = infer_control(&with_rust_type(prop("window", "string"), rust_type));
            assert_eq!(
                inf.input,
                ControlInputType::DateTime,
                "rust type {rust_type}"
            );
        }
    }

    #[test]
    fn temporal_name_suffixes_map_to_datetime() {
        for name in [
            "created_at",
            "due_date",
            "start_time",
            "open_until",
            "window_from",
        ] {
            let inf = infer_control(&prop(name, "string"));
            assert_eq!(inf.input, ControlInputType::DateTime, "name {name}");
        }
    }

    #[test]
    fn phone_and_url_names_stay_text() {
        assert_eq!(
            infer_control(&prop("phone", "string")).input,
            ControlInputType::Text
        );
        assert_eq!(
            infer_control(&prop("website_url", "string")).input,
            ControlInputType::Text
        );
    }

    #[test]
    fn recognizers_do_not_override_stronger_signals() {
        assert_eq!(
            infer_control(&prop("phone_verified", "boolean")).input,
            ControlInputType::Checkbox,
            "boolean beats the phone recognizer"
        );
        assert_eq!(
            infer_control(&with_rust_type(prop("updated_url", "string"), "Uuid")).input,
            ControlInputType::Hidden,
            "uuid beats the url recognizer"
        );
    }

    #[test]
    fn update_words_are_not_temporal() {
        assert_eq!(
            infer_control(&prop("update", "string")).input,
            ControlInputType::Text
        );
        assert_eq!(
            infer_control(&prop("latest", "string")).input,
            ControlInputType::Text
        );
    }

    #[test]
    fn plain_strings_fall_through_to_text() {
        let inf = infer_control(&prop("title", "string"));
        assert_eq!(inf.input, ControlInputType::Text);
        assert_eq!(inf.role, SemanticRole::Field);
        assert_eq!(inf.note, None);
        assert_eq!(inf.input_str(), "text");
    }

    #[test]
    fn required_flag_is_carried() {
        assert!(infer_control(&required(prop("title", "string"))).required);
        assert!(!infer_control(&prop("notes", "string")).required);
    }

    #[test]
    fn with_values_caps_at_the_limit() {
        let values: Vec<String> = (0..MAX_CONTROL_VALUES + 5)
            .map(|i| format!("v{i}"))
            .collect();
        let inf = infer_control(&with_kind(
            prop("status", "string"),
            RefClassificationKind::CodelistReference,
        ))
        .with_values(values);
        assert_eq!(inf.values.len(), MAX_CONTROL_VALUES);
        assert_eq!(inf.values[0], "v0");
        assert_eq!(
            inf.values[MAX_CONTROL_VALUES - 1],
            format!("v{}", MAX_CONTROL_VALUES - 1)
        );
    }

    #[test]
    fn infer_input_type_matches_control_inference() {
        let cases = [
            prop("title", "string"),
            with_kind(
                prop("status", "string"),
                RefClassificationKind::CodelistReference,
            ),
            with_rust_type(prop("list_id", "string"), "Uuid"),
            as_array(prop("tags", "string")),
            prop("completed", "boolean"),
        ];
        for case in &cases {
            assert_eq!(infer_input_type(case), infer_control(case).input.as_str());
        }
    }
}
