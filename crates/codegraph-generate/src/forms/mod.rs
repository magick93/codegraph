//! Shared zod validation lowering (issue #580).
//!
//! ONE zod vocabulary for BOTH generated planes: the entity plane lowers
//! DTO required/optionality + #261 constraint bounds into [`ZodObject`]s;
//! the IFML plane lowers DSL `validations: [...]` expressions +
//! positionally-paired `messages` the same way. The planes own their
//! input-to-IR mapping; this module owns the zod SHAPE — the emitted
//! schema source, the escaping, and the pinned npm versions.
//!
//! Zod is consumed by TanStack Form directly as a Standard Schema (passed
//! straight to `validators`), so the rendered source must parse under the
//! pinned zod major (`ZOD_VERSION`). The spellings here are zod-v4:
//! `z.email()`/`z.url()`/`z.uuid()` top-level and `z.iso.datetime()`.

/// Pinned `@tanstack/svelte-form` version for generated `package.json`s.
pub const TANSTACK_SVELTE_FORM_VERSION: &str = "^1.33.5";
/// Pinned `zod` version for generated `package.json`s.
pub const ZOD_VERSION: &str = "^4.6.7";

/// One zod object schema: a named TS `const` rendering as
/// `z.object({ ... })`.
#[derive(Debug, Clone, PartialEq)]
pub struct ZodObject {
    /// TS identifier for the exported schema const (e.g. `candidateSchema`).
    pub name: String,
    pub fields: Vec<ZodField>,
}

/// One field of a [`ZodObject`].
#[derive(Debug, Clone, PartialEq)]
pub struct ZodField {
    /// The DTO/DSL field name as written in the form payload (already in
    /// the plane's wire casing).
    pub name: String,
    pub kind: ZodKind,
    /// Plain feature = required; `T[0..1]` = optional.
    pub required: bool,
    pub checks: Vec<ZodCheck>,
}

/// The zod type of a field.
#[derive(Debug, Clone, PartialEq)]
pub enum ZodKind {
    String,
    Number,
    Boolean,
    Email,
    Url,
    Uuid,
    /// Date-only input (`type="date"`).
    Date,
    /// Datetime input (`type="datetime-local"`).
    DateTime,
    /// Codelist/inline enum: the closed value set.
    Enum(Vec<String>),
    /// Junction/array feature: `z.array(<element>)`.
    Array(Box<ZodField>),
}

/// One constraint check. `message` is the DSL's positionally-paired
/// message when present (rendered as the zod check's error message).
#[derive(Debug, Clone, PartialEq)]
pub enum ZodCheck {
    MinLength {
        value: usize,
        message: Option<String>,
    },
    MaxLength {
        value: usize,
        message: Option<String>,
    },
    Min {
        value: f64,
        message: Option<String>,
    },
    Max {
        value: f64,
        message: Option<String>,
    },
    MinItems {
        value: usize,
        message: Option<String>,
    },
    MaxItems {
        value: usize,
        message: Option<String>,
    },
}

/// Escape a Rust string into a double-quoted TS string literal. Escapes
/// backslash, double quote, and control characters — the `js_template_literal`
/// lesson (#560) applied to zod sources: prose from the model must never
/// reach emitted TS unquoted.
pub fn ts_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:04x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl ZodField {
    /// Render the field as `name: <zod expr>,` (one indented line).
    pub fn render(&self, indent: &str) -> String {
        format!("{}{}: {},", indent, self.name, self.render_expr())
    }

    /// Render the field's zod expression WITHOUT the `name:` prefix and
    /// trailing comma — used for array element types.
    pub fn render_expr(&self) -> String {
        let mut expr = match &self.kind {
            ZodKind::String => "z.string()".to_string(),
            ZodKind::Number => "z.number()".to_string(),
            ZodKind::Boolean => "z.boolean()".to_string(),
            ZodKind::Email => "z.email()".to_string(),
            ZodKind::Url => "z.url()".to_string(),
            ZodKind::Uuid => "z.uuid()".to_string(),
            ZodKind::Date => "z.iso.date()".to_string(),
            ZodKind::DateTime => "z.iso.datetime()".to_string(),
            ZodKind::Enum(values) => {
                let items: Vec<String> = values.iter().map(|v| ts_string_literal(v)).collect();
                format!("z.enum([{}])", items.join(", "))
            }
            ZodKind::Array(elem) => {
                let inner = elem.render_expr();
                format!("z.array({inner})")
            }
        };
        for check in &self.checks {
            expr.push_str(&check.render());
        }
        if !self.required {
            expr.push_str(".optional()");
        }
        expr
    }
}

impl ZodCheck {
    fn render(&self) -> String {
        let (call, message) = match self {
            ZodCheck::MinLength { value, message } => (format!(".min({value}"), message),
            ZodCheck::MaxLength { value, message } => (format!(".max({value}"), message),
            ZodCheck::Min { value, message } => (format!(".min({value}"), message),
            ZodCheck::Max { value, message } => (format!(".max({value}"), message),
            ZodCheck::MinItems { value, message } => (format!(".min({value}"), message),
            ZodCheck::MaxItems { value, message } => (format!(".max({value}"), message),
        };
        match message {
            Some(m) => format!("{call}, {})", ts_string_literal(m)),
            None => format!("{call})"),
        }
    }
}

impl ZodObject {
    /// Render the full TS source: `export const <name> = z.object({...});`
    /// with one indented field per line.
    pub fn render_schema(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("export const {} = z.object({{\n", self.name));
        for field in &self.fields {
            out.push_str(&field.render("  "));
            out.push('\n');
        }
        out.push_str("});\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_string_field_renders_plain_zod_string() {
        let field = ZodField {
            name: "title".into(),
            kind: ZodKind::String,
            required: true,
            checks: vec![],
        };
        assert_eq!(field.render("  "), "  title: z.string(),");
    }

    #[test]
    fn optional_field_appends_optional() {
        let field = ZodField {
            name: "notes".into(),
            kind: ZodKind::String,
            required: false,
            checks: vec![],
        };
        assert_eq!(field.render(""), "notes: z.string().optional(),");
    }

    #[test]
    fn length_checks_carry_dsl_messages() {
        let field = ZodField {
            name: "title".into(),
            kind: ZodKind::String,
            required: true,
            checks: vec![
                ZodCheck::MinLength {
                    value: 2,
                    message: Some("Title too short".into()),
                },
                ZodCheck::MaxLength {
                    value: 80,
                    message: None,
                },
            ],
        };
        assert_eq!(
            field.render(""),
            "title: z.string().min(2, \"Title too short\").max(80),"
        );
    }

    #[test]
    fn number_bounds_render_min_max() {
        let field = ZodField {
            name: "amount".into(),
            kind: ZodKind::Number,
            required: true,
            checks: vec![
                ZodCheck::Min {
                    value: 0.0,
                    message: Some("Must be positive".into()),
                },
                ZodCheck::Max {
                    value: 1_000_000.0,
                    message: None,
                },
            ],
        };
        assert_eq!(
            field.render(""),
            "amount: z.number().min(0, \"Must be positive\").max(1000000),"
        );
    }

    #[test]
    fn enum_kind_renders_closed_value_set_with_escaping() {
        let field = ZodField {
            name: "status".into(),
            kind: ZodKind::Enum(vec!["draft".into(), "has \"quotes\"".into()]),
            required: true,
            checks: vec![],
        };
        assert_eq!(
            field.render(""),
            "status: z.enum([\"draft\", \"has \\\"quotes\\\"\"]),"
        );
    }

    #[test]
    fn array_kind_renders_nested_element_with_item_bounds() {
        let field = ZodField {
            name: "assigneeIds".into(),
            kind: ZodKind::Array(Box::new(ZodField {
                name: String::new(),
                kind: ZodKind::Uuid,
                required: true,
                checks: vec![],
            })),
            required: true,
            checks: vec![
                ZodCheck::MinItems {
                    value: 1,
                    message: Some("Pick at least one".into()),
                },
                ZodCheck::MaxItems {
                    value: 10,
                    message: None,
                },
            ],
        };
        assert_eq!(
            field.render(""),
            "assigneeIds: z.array(z.uuid()).min(1, \"Pick at least one\").max(10),"
        );
    }

    #[test]
    fn control_kinds_map_to_zod_v4_top_level_spells() {
        let cases = [
            (ZodKind::Email, "email: z.email(),"),
            (ZodKind::Url, "url: z.url(),"),
            (ZodKind::Uuid, "id: z.uuid(),"),
            (ZodKind::Date, "dob: z.iso.date(),"),
            (ZodKind::DateTime, "at: z.iso.datetime(),"),
            (ZodKind::Boolean, "active: z.boolean(),"),
            (ZodKind::Number, "n: z.number(),"),
        ];
        for (kind, expected) in cases {
            let field = ZodField {
                name: expected.split(':').next().unwrap().to_string(),
                kind,
                required: true,
                checks: vec![],
            };
            assert_eq!(field.render(""), expected);
        }
    }

    #[test]
    fn object_schema_renders_exported_const() {
        let obj = ZodObject {
            name: "candidateSchema".into(),
            fields: vec![
                ZodField {
                    name: "title".into(),
                    kind: ZodKind::String,
                    required: true,
                    checks: vec![ZodCheck::MinLength {
                        value: 2,
                        message: Some("Title too short".into()),
                    }],
                },
                ZodField {
                    name: "notes".into(),
                    kind: ZodKind::String,
                    required: false,
                    checks: vec![],
                },
            ],
        };
        assert_eq!(
            obj.render_schema(),
            "export const candidateSchema = z.object({\n  \
             title: z.string().min(2, \"Title too short\"),\n  \
             notes: z.string().optional(),\n});\n"
        );
    }

    #[test]
    fn ts_string_literal_escapes_prose_metacharacters() {
        assert_eq!(ts_string_literal("plain"), "\"plain\"");
        assert_eq!(ts_string_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(ts_string_literal("a\\b"), "\"a\\\\b\"");
        assert_eq!(ts_string_literal("line\nbreak"), "\"line\\nbreak\"");
        assert_eq!(ts_string_literal("tab\there"), "\"tab\\there\"");
    }

    #[test]
    fn versions_are_pinned_caret_majors() {
        assert!(TANSTACK_SVELTE_FORM_VERSION.starts_with('^'));
        assert!(ZOD_VERSION.starts_with('^'));
    }
}
