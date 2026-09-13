//! Canonical schema-property → IFML control inference (issue #196).
//!
//! One mapping decides which of the 14 IFML DSL input types a schema property
//! renders as, plus the semantic slot role (`field` / `selection-field`,
//! mirroring [`SemanticRole`]) and optional dropdown values. The scaffold
//! consumes this today; the route generator's fallback form rendering is
//! slated to consume the same mapping so control choice is domain-driven in
//! one place.
//!
//! Heuristics are table-driven and ordered so that every decision the
//! scaffold made before this module existed still resolves identically: new
//! recognizers only fire where inference previously fell through to `text`
//! (or on classification kinds the scaffold fixtures do not carry).

use codegraph_config::SemanticRole;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;

/// Upper bound on dropdown value lists emitted by inference.
pub(crate) const MAX_CONTROL_VALUES: usize = 20;

/// Note attached to entity-reference selections whose options cannot be
/// resolved from the property alone (the options endpoint resolution is a
/// follow-up).
pub const NOTE_OPTIONS_ENDPOINT: &str =
    "options endpoint for the referenced entity is not resolved yet";

/// The 14 input types of the IFML DSL grammar, in keyword form. Mirrors
/// `codegraph_ifml_dsl::InputFieldType` (static variants only; that enum's
/// `Custom` catch-all is not inferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlInputType {
    Text,
    TextArea,
    Password,
    Email,
    Number,
    Date,
    Time,
    DateTime,
    Dropdown,
    Radio,
    Checkbox,
    Toggle,
    File,
    Hidden,
}

impl ControlInputType {
    /// The DSL keyword for this input type, as accepted by the grammar and
    /// rendered by the scaffold.
    pub fn as_str(self) -> &'static str {
        match self {
            ControlInputType::Text => "text",
            ControlInputType::TextArea => "textarea",
            ControlInputType::Password => "password",
            ControlInputType::Email => "email",
            ControlInputType::Number => "number",
            ControlInputType::Date => "date",
            ControlInputType::Time => "time",
            ControlInputType::DateTime => "datetime",
            ControlInputType::Dropdown => "dropdown",
            ControlInputType::Radio => "radio",
            ControlInputType::Checkbox => "checkbox",
            ControlInputType::Toggle => "toggle",
            ControlInputType::File => "file",
            ControlInputType::Hidden => "hidden",
        }
    }
}

/// The control a schema property should render as.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlInference {
    /// DSL input type for `field <name> -> input <type>`.
    pub input: ControlInputType,
    /// Semantic slot role: [`SemanticRole::SelectionField`] for codelists,
    /// inline enums, and entity references; [`SemanticRole::Field`] otherwise.
    pub role: SemanticRole,
    /// Dropdown values when they are trivially available on the property;
    /// codelist resolution is caller-side (see [`ControlInference::with_values`]).
    pub values: Vec<String>,
    /// Whether the underlying schema property is required.
    pub required: bool,
    /// Follow-up note for consumers that surface unresolved resolution steps.
    pub note: Option<&'static str>,
}

impl ControlInference {
    /// Attach externally resolved values, capped at [`MAX_CONTROL_VALUES`].
    pub fn with_values(mut self, values: Vec<String>) -> Self {
        self.values = values.into_iter().take(MAX_CONTROL_VALUES).collect();
        self
    }

    /// The DSL keyword for the inferred input type.
    pub fn input_str(&self) -> &'static str {
        self.input.as_str()
    }
}

/// Infer the control for one schema property.
pub fn infer_control(prop: &PropertyNode) -> ControlInference {
    let kind = prop.effective_kind();
    let (input, note) = infer_input_with_note(prop, kind.as_ref());
    let role = if input == ControlInputType::Dropdown {
        SemanticRole::SelectionField
    } else {
        SemanticRole::Field
    };
    ControlInference {
        input,
        role,
        values: Vec::new(),
        required: prop.is_required,
        note,
    }
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

fn infer_input_with_note(
    prop: &PropertyNode,
    kind: Option<&RefClassificationKind>,
) -> (ControlInputType, Option<&'static str>) {
    if prop.is_array {
        return (ControlInputType::TextArea, None);
    }
    if is_codelist_kind(kind) {
        return (ControlInputType::Dropdown, None);
    }
    if kind == Some(&RefClassificationKind::EntityReference) {
        return (ControlInputType::Dropdown, Some(NOTE_OPTIONS_ENDPOINT));
    }
    if prop.name.to_ascii_lowercase().contains("email") {
        return (ControlInputType::Email, None);
    }
    if prop.name.to_ascii_lowercase().contains("password")
        || prop.name.to_ascii_lowercase().contains("secret")
    {
        return (ControlInputType::Password, None);
    }
    match prop.prop_type.as_str() {
        "boolean" => return (ControlInputType::Checkbox, None),
        "integer" | "number" => return (ControlInputType::Number, None),
        _ => {}
    }
    if prop.minimum.is_some() || prop.maximum.is_some() {
        return (ControlInputType::Number, None);
    }
    if prop.rust_field_type.contains("Uuid") {
        return (ControlInputType::Hidden, None);
    }
    if let Some(fmt) = prop.format.as_deref() {
        if fmt.contains("date") || fmt.contains("time") {
            return (ControlInputType::DateTime, None);
        }
    }
    if prop.rust_field_type.contains("Date") || prop.rust_field_type.contains("Time") {
        return (ControlInputType::DateTime, None);
    }
    recognize_fallthrough(prop)
}

/// Recognizers for plain string properties that carry a semantic name or
/// format. They run last so they only claim what previously fell through to
/// `text`.
fn recognize_fallthrough(prop: &PropertyNode) -> (ControlInputType, Option<&'static str>) {
    if let Some(fmt) = prop.format.as_deref() {
        if fmt.contains("email") {
            return (ControlInputType::Email, None);
        }
    }
    let name = prop.name.to_ascii_lowercase();
    if TEMPORAL_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        return (ControlInputType::DateTime, None);
    }
    if CONTACT_RECOGNIZERS.iter().any(|s| name.contains(s)) {
        return (ControlInputType::Text, None);
    }
    (ControlInputType::Text, None)
}

/// Name suffixes that denote an instant/date in domain models
/// (`created_at`, `due_date`, `window_from`, ...).
const TEMPORAL_SUFFIXES: &[&str] = &["_at", "_date", "_time", "_until", "_from"];

/// Contact-ish name fragments with no dedicated DSL input type; recognized so
/// the mapping is explicit and testable rather than an accident of the
/// `text` fallback.
const CONTACT_RECOGNIZERS: &[&str] = &["phone", "url"];

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_ifml_dsl::InputFieldType;
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
