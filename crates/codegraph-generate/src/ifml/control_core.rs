//! Shared IFML control-inference core (issue #201).
//!
//! Pure heuristics over plain field signals (name, rust/type strings, a
//! kind-ish enum — no classifier types). Both consumers of control choice
//! build on this: the scaffold's classifier wrapper
//! (`codegraph::ifml_control_inference`) and the route generator's fallback
//! form rendering, so input-type + role decisions cannot diverge.
//!
//! The heuristic ordering is ported verbatim from the canonical
//! scaffold-side module so every decision the scaffold made before this core
//! existed resolves identically. Recognizers over rust-type spellings
//! (`bool`, numeric primitives, `Uuid`, `Date`/`Time`, `Vec<`) recover the
//! schema-level signals from the `(field_name, rust_type)` pairs the route
//! generator actually receives via `fields_with_types`.

use codegraph_config::SemanticRole;
use codegraph_ifml_dsl::InputFieldType;

/// Upper bound on dropdown value lists emitted by inference.
pub const MAX_CONTROL_VALUES: usize = 20;

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

    /// The core control for a DSL-declared input; `None` for `Custom`.
    pub fn from_dsl(input: &InputFieldType) -> Option<Self> {
        Some(match input {
            InputFieldType::Text => Self::Text,
            InputFieldType::TextArea => Self::TextArea,
            InputFieldType::Password => Self::Password,
            InputFieldType::Email => Self::Email,
            InputFieldType::Number => Self::Number,
            InputFieldType::Date => Self::Date,
            InputFieldType::Time => Self::Time,
            InputFieldType::DateTime => Self::DateTime,
            InputFieldType::Dropdown => Self::Dropdown,
            InputFieldType::RadioGroup => Self::Radio,
            InputFieldType::Checkbox => Self::Checkbox,
            InputFieldType::Toggle => Self::Toggle,
            InputFieldType::File => Self::File,
            InputFieldType::Hidden => Self::Hidden,
            InputFieldType::Custom(_) => return None,
        })
    }

    /// The HTML form-control rendering for SvelteKit pages: `<input type>`
    /// (or select/textarea variant flags), ported from the route generator's
    /// form table.
    pub fn html(self) -> HtmlInput {
        let (input_type, is_textarea, is_select, is_radio) = match self {
            ControlInputType::Text => ("text", false, false, false),
            ControlInputType::TextArea => ("textarea", true, false, false),
            ControlInputType::Password => ("password", false, false, false),
            ControlInputType::Email => ("email", false, false, false),
            ControlInputType::Number => ("number", false, false, false),
            ControlInputType::Date => ("date", false, false, false),
            ControlInputType::Time => ("time", false, false, false),
            ControlInputType::DateTime => ("datetime-local", false, false, false),
            ControlInputType::Dropdown => ("dropdown", false, true, false),
            ControlInputType::Radio => ("radio", false, false, true),
            ControlInputType::Checkbox | ControlInputType::Toggle => {
                ("checkbox", false, false, false)
            }
            ControlInputType::File => ("file", false, false, false),
            ControlInputType::Hidden => ("hidden", false, false, false),
        };
        HtmlInput {
            input_type: input_type.to_string(),
            is_textarea,
            is_select,
            is_radio,
        }
    }
}

/// HTML form-control rendering derived from a [`ControlInputType`] or a DSL
/// custom input.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlInput {
    /// `<input type>` value, or the select/textarea element discriminator.
    pub input_type: String,
    pub is_textarea: bool,
    pub is_select: bool,
    pub is_radio: bool,
}

/// Map a DSL-declared input to its HTML rendering. `Custom` inputs pass
/// through verbatim with no variant flags.
pub fn html_input_for_dsl(input: &InputFieldType) -> HtmlInput {
    match input {
        InputFieldType::Custom(custom) => HtmlInput {
            input_type: custom.clone(),
            is_textarea: false,
            is_select: false,
            is_radio: false,
        },
        standard => ControlInputType::from_dsl(standard)
            .unwrap_or(ControlInputType::Text)
            .html(),
    }
}

/// The classifier-visible distinctions that steer inference. Codelist-ish
/// kinds collapse into [`FieldKind::Codelist`]; everything else graph-side
/// is either [`FieldKind::EntityRef`] or `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Codelist,
    EntityRef,
}

/// The plain field signals inference runs over. All fields are optional
/// beyond name/rust-type so signal-poor callers (the route generator's
/// `fields_with_types` pairs) can drive the same core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSignals<'a> {
    pub name: &'a str,
    pub rust_type: &'a str,
    pub prop_type: Option<&'a str>,
    pub format: Option<&'a str>,
    pub is_array: bool,
    pub has_bounds: bool,
    pub kind: Option<FieldKind>,
    pub is_required: bool,
}

/// The control a field should render as.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlInference {
    /// DSL input type for `field <name> -> input <type>`.
    pub input: ControlInputType,
    /// Semantic slot role: [`SemanticRole::SelectionField`] for dropdowns
    /// (and radio groups); [`SemanticRole::Field`] otherwise.
    pub role: SemanticRole,
    /// Dropdown values when trivially available on the property; codelist
    /// resolution is caller-side (see [`ControlInference::with_values`]).
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

/// Infer the control for one field's signals.
pub fn infer_control(signals: &FieldSignals) -> ControlInference {
    let (input, note) = infer_input_with_note(signals);
    let role = input_role(input);
    ControlInference {
        input,
        role,
        values: Vec::new(),
        required: signals.is_required,
        note,
    }
}

/// Infer the control from the `(rust_type, field_name)` pair the route
/// generator sees in `fields_with_types`. Graph-only signals (classification
/// kind, numeric bounds, schema format) are absent here; see the conformance
/// test's divergence ledger for the classes where that matters.
pub fn control_for_field(rust_type: &str, field_name: &str) -> ControlInference {
    infer_control(&FieldSignals {
        name: field_name,
        rust_type,
        prop_type: None,
        format: None,
        is_array: false,
        has_bounds: false,
        kind: None,
        is_required: false,
    })
}

/// Per-input slot role inside a form, keyed on the rendered input type:
/// dropdowns and radio groups are selection fields.
pub fn input_field_role(input_type: &str) -> Option<SemanticRole> {
    match input_type {
        "dropdown" | "radio" => Some(SemanticRole::SelectionField),
        _ => None,
    }
}

/// The semantic slot role for an inferred control.
fn input_role(input: ControlInputType) -> SemanticRole {
    if matches!(input, ControlInputType::Dropdown | ControlInputType::Radio) {
        SemanticRole::SelectionField
    } else {
        SemanticRole::Field
    }
}

/// Rust numeric spellings the generated payloads must coerce with `Number`.
pub fn is_numeric_rust_type(rust_type: &str) -> bool {
    let t = rust_type.to_ascii_lowercase();
    [
        "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
    ]
    .iter()
    .any(|n| t.contains(n))
        || t.contains("decimal")
        || t.contains("integer")
        || t.contains("bigint")
}

/// Rust temporal spellings the generated payloads must coerce with `Date`.
pub fn is_temporal_rust_type(rust_type: &str) -> bool {
    let t = rust_type.to_ascii_lowercase();
    t.contains("datetime") || t.contains("timestamp")
}

fn infer_input_with_note(signals: &FieldSignals) -> (ControlInputType, Option<&'static str>) {
    if signals.is_array {
        return (ControlInputType::TextArea, None);
    }
    if signals.kind == Some(FieldKind::Codelist) {
        return (ControlInputType::Dropdown, None);
    }
    if signals.kind == Some(FieldKind::EntityRef) {
        return (ControlInputType::Dropdown, Some(NOTE_OPTIONS_ENDPOINT));
    }
    let lower_rust = signals.rust_type.to_ascii_lowercase();
    if lower_rust.starts_with("vec<") {
        return (ControlInputType::TextArea, None);
    }
    let name = signals.name.to_ascii_lowercase();
    if name.contains("email") {
        return (ControlInputType::Email, None);
    }
    if name.contains("password") || name.contains("secret") {
        return (ControlInputType::Password, None);
    }
    if signals.prop_type == Some("boolean") || lower_rust.contains("bool") {
        return (ControlInputType::Checkbox, None);
    }
    if signals.prop_type == Some("integer")
        || signals.prop_type == Some("number")
        || is_numeric_rust_type(&lower_rust)
        || signals.has_bounds
    {
        return (ControlInputType::Number, None);
    }
    if signals.rust_type.contains("Uuid") {
        return (ControlInputType::Hidden, None);
    }
    if let Some(fmt) = signals.format {
        if fmt.contains("date") || fmt.contains("time") {
            return (ControlInputType::DateTime, None);
        }
    }
    if signals.rust_type.contains("Date") || signals.rust_type.contains("Time") {
        return (ControlInputType::DateTime, None);
    }
    if let Some(fmt) = signals.format {
        if fmt.contains("email") {
            return (ControlInputType::Email, None);
        }
    }
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

    fn signals<'a>(name: &'a str, rust_type: &'a str) -> FieldSignals<'a> {
        FieldSignals {
            name,
            rust_type,
            prop_type: None,
            format: None,
            is_array: false,
            has_bounds: false,
            kind: None,
            is_required: false,
        }
    }

    fn infer(name: &str, rust_type: &str) -> ControlInference {
        infer_control(&signals(name, rust_type))
    }

    fn with_kind(mut s: FieldSignals<'static>, kind: FieldKind) -> FieldSignals<'static> {
        s.kind = Some(kind);
        s
    }

    #[test]
    fn arrays_map_to_textarea() {
        assert_eq!(
            infer("tags", "Vec<String>").input,
            ControlInputType::TextArea
        );
        let mut s = signals("tags", "Vec<String>");
        s.is_array = true;
        assert_eq!(infer_control(&s).input, ControlInputType::TextArea);
    }

    #[test]
    fn codelist_kinds_map_to_dropdown_selection_fields() {
        let inf = infer_control(&with_kind(signals("status", "String"), FieldKind::Codelist));
        assert_eq!(inf.input, ControlInputType::Dropdown);
        assert_eq!(inf.role, SemanticRole::SelectionField);
        assert_eq!(inf.note, None);
    }

    #[test]
    fn entity_references_map_to_dropdown_with_options_note() {
        let inf = infer_control(&with_kind(
            signals("manager", "ManagerType"),
            FieldKind::EntityRef,
        ));
        assert_eq!(inf.input, ControlInputType::Dropdown);
        assert_eq!(inf.role, SemanticRole::SelectionField);
        assert_eq!(inf.note, Some(NOTE_OPTIONS_ENDPOINT));
        assert!(inf.values.is_empty());
    }

    #[test]
    fn email_name_and_format_map_to_email() {
        assert_eq!(
            infer("contact_email", "String").input,
            ControlInputType::Email
        );
        let mut s = signals("contact", "String");
        s.format = Some("email");
        assert_eq!(infer_control(&s).input, ControlInputType::Email);
    }

    #[test]
    fn password_and_secret_names_map_to_password() {
        assert_eq!(
            infer("password_hash", "String").input,
            ControlInputType::Password
        );
        assert_eq!(
            infer("client_secret", "String").input,
            ControlInputType::Password
        );
    }

    #[test]
    fn boolean_maps_to_checkbox() {
        assert_eq!(infer("completed", "bool").input, ControlInputType::Checkbox);
        let mut s = signals("completed", "String");
        s.prop_type = Some("boolean");
        assert_eq!(infer_control(&s).input, ControlInputType::Checkbox);
    }

    #[test]
    fn numeric_types_map_to_number() {
        assert_eq!(infer("priority", "i32").input, ControlInputType::Number);
        assert_eq!(infer("rate", "f64").input, ControlInputType::Number);
        let mut s = signals("rate", "String");
        s.prop_type = Some("number");
        assert_eq!(infer_control(&s).input, ControlInputType::Number);
    }

    #[test]
    fn numeric_bounds_map_to_number() {
        let mut s = signals("amount", "String");
        s.has_bounds = true;
        assert_eq!(infer_control(&s).input, ControlInputType::Number);
    }

    #[test]
    fn uuid_rust_types_map_to_hidden() {
        assert_eq!(infer("list_id", "Uuid").input, ControlInputType::Hidden);
        assert_eq!(
            infer("list_id", "Option<Uuid>").input,
            ControlInputType::Hidden
        );
    }

    #[test]
    fn date_time_formats_map_to_datetime() {
        for fmt in ["date", "date-time", "time"] {
            let mut s = signals("window", "String");
            s.format = Some(fmt);
            assert_eq!(
                infer_control(&s).input,
                ControlInputType::DateTime,
                "format {fmt}"
            );
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
            assert_eq!(
                infer("window", rust_type).input,
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
            assert_eq!(
                infer(name, "String").input,
                ControlInputType::DateTime,
                "name {name}"
            );
        }
    }

    #[test]
    fn phone_and_url_names_stay_text() {
        assert_eq!(infer("phone", "String").input, ControlInputType::Text);
        assert_eq!(infer("website_url", "String").input, ControlInputType::Text);
    }

    #[test]
    fn recognizers_do_not_override_stronger_signals() {
        let mut s = signals("phone_verified", "bool");
        s.prop_type = Some("boolean");
        assert_eq!(
            infer_control(&s).input,
            ControlInputType::Checkbox,
            "boolean beats the phone recognizer"
        );
        assert_eq!(
            infer("updated_url", "Uuid").input,
            ControlInputType::Hidden,
            "uuid beats the url recognizer"
        );
    }

    #[test]
    fn update_words_are_not_temporal() {
        assert_eq!(infer("update", "String").input, ControlInputType::Text);
        assert_eq!(infer("latest", "String").input, ControlInputType::Text);
    }

    #[test]
    fn plain_strings_fall_through_to_text() {
        let inf = infer("title", "String");
        assert_eq!(inf.input, ControlInputType::Text);
        assert_eq!(inf.role, SemanticRole::Field);
        assert_eq!(inf.note, None);
        assert_eq!(inf.input_str(), "text");
    }

    #[test]
    fn required_flag_is_carried() {
        let mut s = signals("title", "String");
        s.is_required = true;
        assert!(infer_control(&s).required);
        assert!(!infer("notes", "String").required);
    }

    #[test]
    fn with_values_caps_at_the_limit() {
        let values: Vec<String> = (0..MAX_CONTROL_VALUES + 5)
            .map(|i| format!("v{i}"))
            .collect();
        let inf = infer_control(&with_kind(signals("status", "String"), FieldKind::Codelist))
            .with_values(values);
        assert_eq!(inf.values.len(), MAX_CONTROL_VALUES);
        assert_eq!(inf.values[0], "v0");
        assert_eq!(
            inf.values[MAX_CONTROL_VALUES - 1],
            format!("v{}", MAX_CONTROL_VALUES - 1)
        );
    }

    #[test]
    fn html_mapping_matches_the_route_generator_table() {
        let expected = [
            (ControlInputType::Text, "text"),
            (ControlInputType::TextArea, "textarea"),
            (ControlInputType::Password, "password"),
            (ControlInputType::Email, "email"),
            (ControlInputType::Number, "number"),
            (ControlInputType::Date, "date"),
            (ControlInputType::Time, "time"),
            (ControlInputType::DateTime, "datetime-local"),
            (ControlInputType::Dropdown, "dropdown"),
            (ControlInputType::Radio, "radio"),
            (ControlInputType::Checkbox, "checkbox"),
            (ControlInputType::Toggle, "checkbox"),
            (ControlInputType::File, "file"),
            (ControlInputType::Hidden, "hidden"),
        ];
        for (control, html) in expected {
            assert_eq!(control.html().input_type, html, "{control:?}");
        }
        let textarea = ControlInputType::TextArea.html();
        assert!(textarea.is_textarea);
        assert!(!textarea.is_select);
        let select = ControlInputType::Dropdown.html();
        assert!(select.is_select);
        let radio = ControlInputType::Radio.html();
        assert!(radio.is_radio);
        assert!(!radio.is_select);
    }

    #[test]
    fn html_input_for_dsl_passes_custom_through() {
        let custom = html_input_for_dsl(&InputFieldType::Custom("stars".to_string()));
        assert_eq!(custom.input_type, "stars");
        assert!(!custom.is_textarea && !custom.is_select && !custom.is_radio);
        assert_eq!(
            html_input_for_dsl(&InputFieldType::DateTime).input_type,
            "datetime-local"
        );
    }

    #[test]
    fn input_field_role_covers_selection_inputs() {
        assert_eq!(
            input_field_role("dropdown"),
            Some(SemanticRole::SelectionField)
        );
        assert_eq!(
            input_field_role("radio"),
            Some(SemanticRole::SelectionField)
        );
        assert_eq!(input_field_role("text"), None);
        assert_eq!(input_field_role("checkbox"), None);
    }
}
