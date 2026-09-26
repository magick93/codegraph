//! Column-slot dimension inference ("form follows data", issue #295).
//!
//! [`infer_dimension`] walks an ORDERED decision list — first hit wins. The
//! order is part of the public contract and pinned by the table-driven test
//! below, so reordering is a reviewed change:
//!
//! 1. entity reference → [`Dimension::Reference`]
//! 2. codelist / inline-enum (select) input / the entity's workflow
//!    `status_field` → [`Dimension::StatusCategory`]
//! 3. `uuid` pg type, or `name == "id"` with a string ts type →
//!    [`Dimension::Identifier`]
//! 4. boolean ts type / checkbox input → [`Dimension::Flag`]
//! 5. date / timestamp / range pg types, or date-ish inputs →
//!    [`Dimension::TimePoint`]
//! 6. numeric pg + money name heuristic → [`Dimension::Money`] (keyword
//!    inferences record a hint — see [`DimensionHints`])
//! 7. numeric pg → [`Dimension::Quantity`]
//! 8. anything else → [`Dimension::Text`]
//!
//! Documented edge semantics:
//!
//! - **Arrays** infer the ELEMENT dimension (the `[]` suffix is stripped
//!   before pg matching); the caller renders join-aware — pass 1 only
//!   labels the element.
//! - **Range types** (`DATERANGE`, `TSTZRANGE`, …) infer TimePoint; range
//!   rendering is a v1 limitation — table cells format the lower bound.
//! - **StructuredWrapper** columns are single JSONB slots and fall through
//!   to Text at the column level; their sub-fields infer independently via
//!   [`infer_sub_field_dimensions`].
//! - **Synthetic columns** (`prop == None`, e.g. injected codelist form
//!   fields) infer from the [`UiField`] alone.
//! - Name matching is case-insensitive over `_`-split words (a keyword
//!   matches as a whole word anywhere in the name), which covers exact-name
//!   and `*_keyword` suffix shapes without real stemming.

use codegraph_config::ux::Dimension;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;

use crate::ui::page::{UiField, UiSubField};

/// Money-name keywords, matched case-insensitively as whole `_`-split words
/// (name or `*_keyword` suffix shapes; contains-as-word is the same check).
const MONEY_KEYWORDS: [&str; 9] = [
    "amount", "total", "subtotal", "price", "cost", "fee", "balance", "salary", "rate",
];

/// Hints collected while inferring one column's dimension.
///
/// Currently only keyword-derived Money inferences produce hints (the
/// heuristic is honest about guessing); a later diagnostics sub-task
/// surfaces them so authors can pin intent with an explicit
/// `[[column]] dimension = ...` rule.
#[derive(Debug, Default, Clone)]
pub struct DimensionHints(Vec<String>);

impl DimensionHints {
    /// True when no hint was recorded.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of recorded hints.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Consume the collector into its hint strings.
    pub fn into_vec(self) -> Vec<String> {
        self.0
    }

    fn push(&mut self, hint: String) {
        self.0.push(hint);
    }
}

impl std::ops::Deref for DimensionHints {
    type Target = [String];

    fn deref(&self) -> &[String] {
        &self.0
    }
}

/// One StructuredWrapper sub-field's independently inferred dimension.
#[derive(Debug, Clone)]
pub struct SubFieldDimension {
    /// The sub-field's snake_case name (as rendered inside the wrapper).
    pub name: String,
    /// The inferred dimension.
    pub dimension: Dimension,
}

/// Infer the dimension of one column slot.
///
/// See the module docs for the ordered decision list and edge semantics.
/// `prop` is the graph property backing the field (`None` for synthetic
/// columns); it currently contributes the inline-enum signal only, while
/// the collected [`UiField`] carries the rest.
pub fn infer_dimension(
    prop: Option<&PropertyNode>,
    field: &UiField,
    workflow_status_field: Option<&str>,
) -> Dimension {
    infer_dimension_with_hints(prop, field, workflow_status_field).0
}

/// [`infer_dimension`] plus the hints collected on the way.
///
/// Pass this variant when building generation-time diagnostics; the plain
/// wrapper stays ergonomic for rule resolution.
pub fn infer_dimension_with_hints(
    prop: Option<&PropertyNode>,
    field: &UiField,
    workflow_status_field: Option<&str>,
) -> (Dimension, DimensionHints) {
    let mut hints = DimensionHints::default();
    let dimension = infer_dimension_inner(prop, field, workflow_status_field, &mut hints);
    (dimension, hints)
}

fn infer_dimension_inner(
    prop: Option<&PropertyNode>,
    field: &UiField,
    workflow_status_field: Option<&str>,
    hints: &mut DimensionHints,
) -> Dimension {
    let name = field.name.to_lowercase();
    let ts_type = field.ts_type.to_lowercase();
    let pg_upper = element_pg_type(&field.pg_type).to_uppercase();

    // 1. Entity reference → Reference.
    if field.is_entity_ref {
        return Dimension::Reference;
    }

    // 2. Codelist, inline-enum select, or the workflow status field →
    //    StatusCategory.
    if field.is_codelist
        || field.input_type == "select"
        || is_inline_enum(prop)
        || workflow_status_field == Some(field.name.as_str())
    {
        return Dimension::StatusCategory;
    }

    // 3. UUID pg, or a bare `id` string column → Identifier.
    if pg_upper == "UUID" || (name == "id" && ts_type == "string") {
        return Dimension::Identifier;
    }

    // 4. Boolean / checkbox → Flag.
    if ts_type == "boolean" || field.input_type == "checkbox" {
        return Dimension::Flag;
    }

    // 5. Date / timestamp / range pg, or date-ish input → TimePoint.
    if is_time_pg(&pg_upper)
        || matches!(
            field.input_type.as_str(),
            "date" | "datetime-local" | "date-range"
        )
    {
        return Dimension::TimePoint;
    }

    // 6-7. Numeric pg: money keywords → Money (with a hint — the inference
    //      is keyword-based, never graph-backed), otherwise Quantity.
    if is_numeric_pg(&pg_upper) {
        if let Some(keyword) = money_keyword(&name) {
            hints.push(format!(
                "column `{}` inferred Money from its name (\"{keyword}\"); \
                 pin intent with a [[column]] rule: dimension = \"money\" (or \"quantity\")",
                field.name
            ));
            return Dimension::Money;
        }
        return Dimension::Quantity;
    }

    // 8. Everything else → Text.
    Dimension::Text
}

/// Infer dimensions for a StructuredWrapper's sub-fields independently.
///
/// Sub-fields have no pg type of their own (they render inside the
/// wrapper's JSONB column), so only the type-free branches apply: the
/// workflow status field match → StatusCategory, a bare `id` name →
/// Identifier, otherwise Text. Money heuristics need a numeric signal and
/// never fire here, so no hints can be produced.
pub fn infer_sub_field_dimensions(
    field: &UiField,
    workflow_status_field: Option<&str>,
) -> Vec<SubFieldDimension> {
    field
        .structured_sub_fields
        .iter()
        .map(|sub| SubFieldDimension {
            dimension: infer_sub_field_dimension(sub, workflow_status_field),
            name: sub.snake_name.clone(),
        })
        .collect()
}

fn infer_sub_field_dimension(sub: &UiSubField, workflow_status_field: Option<&str>) -> Dimension {
    if workflow_status_field == Some(sub.snake_name.as_str()) {
        Dimension::StatusCategory
    } else if sub.snake_name.eq_ignore_ascii_case("id") {
        Dimension::Identifier
    } else {
        Dimension::Text
    }
}

/// The element pg type of a column: arrays (`TEXT[]`, `NUMERIC(10,2)[]`)
/// infer their element dimension; the caller renders join-aware.
fn element_pg_type(pg_type: &str) -> &str {
    pg_type.strip_suffix("[]").unwrap_or(pg_type)
}

fn is_inline_enum(prop: Option<&PropertyNode>) -> bool {
    matches!(
        prop.and_then(|p| p.classification_kind.as_ref()),
        Some(RefClassificationKind::InlineEnum)
    )
}

fn is_time_pg(pg_upper: &str) -> bool {
    pg_upper == "DATE"
        || pg_upper.starts_with("TIMESTAMP")
        // Range types (DATERANGE, TSTZRANGE, INT4RANGE, INT8RANGE) render
        // as TimePoint — v1 formats the lower bound in table cells.
        || pg_upper.contains("RANGE")
}

fn is_numeric_pg(pg_upper: &str) -> bool {
    matches!(
        pg_upper,
        "SMALLINT"
            | "INTEGER"
            | "INT"
            | "BIGINT"
            | "REAL"
            | "FLOAT"
            | "DOUBLE"
            | "DOUBLE PRECISION"
    ) || pg_upper.starts_with("NUMERIC")
        || pg_upper.starts_with("DECIMAL")
}

/// The money keyword matched as a whole `_`-split word (`amount`, `total`,
/// `*_amount`, …), or the `*_cents` integer convention. `name` must already
/// be lowercased.
fn money_keyword(name: &str) -> Option<&'static str> {
    let words: Vec<&str> = name.split('_').collect();
    if words.contains(&"cents") {
        return Some("cents");
    }
    MONEY_KEYWORDS.iter().copied().find(|k| words.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, pg_type: &str, ts_type: &str, input_type: &str) -> UiField {
        UiField {
            name: name.to_string(),
            label: String::new(),
            ts_type: ts_type.to_string(),
            input_type: input_type.to_string(),
            is_required: false,
            is_array: false,
            is_entity_ref: false,
            is_immutable: false,
            is_codelist: false,
            is_range: false,
            codelist_values: vec![],
            description: String::new(),
            pg_type: pg_type.to_string(),
            open_end: false,
            ref_api_path: None,
            structured_sub_fields: vec![],
            nested_type_name: None,
        }
    }

    fn sub_field(snake_name: &str) -> UiSubField {
        UiSubField {
            name: snake_name.to_string(),
            snake_name: snake_name.to_string(),
            label: snake_name.to_string(),
            is_required: false,
            description: String::new(),
            show_by_default: true,
        }
    }

    fn prop(pg_type: &str, kind: Option<RefClassificationKind>) -> PropertyNode {
        PropertyNode {
            name: "prop".into(),
            prop_type: "string".into(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "prop".into(),
            pg_column_type: pg_type.into(),
            rust_field_name: "prop".into(),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "scalar".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: kind,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    struct Row {
        case: &'static str,
        field: UiField,
        prop: Option<PropertyNode>,
        workflow: Option<&'static str>,
        expect: Dimension,
        expect_hints: usize,
    }

    fn rows() -> Vec<Row> {
        vec![
            // 1. Entity reference → Reference.
            Row {
                case: "entity_ref_infers_reference",
                field: {
                    let mut f = field("assignee", "UUID", "string", "text");
                    f.is_entity_ref = true;
                    f
                },
                prop: None,
                workflow: None,
                expect: Dimension::Reference,
                expect_hints: 0,
            },
            // 2a. Codelist → StatusCategory.
            Row {
                case: "codelist_infers_status_category",
                field: {
                    let mut f = field("status", "TEXT", "string", "select");
                    f.is_codelist = true;
                    f.codelist_values = vec!["draft".into(), "approved".into()];
                    f
                },
                prop: None,
                workflow: None,
                expect: Dimension::StatusCategory,
                expect_hints: 0,
            },
            // 2b. Inline-enum select input (graph signal) → StatusCategory.
            Row {
                case: "inline_enum_infers_status_category",
                field: field("priority", "TEXT", "string", "select"),
                prop: Some(prop("TEXT", Some(RefClassificationKind::InlineEnum))),
                workflow: None,
                expect: Dimension::StatusCategory,
                expect_hints: 0,
            },
            // 2c. The workflow status field infers StatusCategory even with
            //     no codelist signal — the workflow alone suffices.
            Row {
                case: "workflow_status_field_infers_status_category",
                field: field("document_status_code", "TEXT", "string", "text"),
                prop: None,
                workflow: Some("document_status_code"),
                expect: Dimension::StatusCategory,
                expect_hints: 0,
            },
            // 2 beats 6: a codelist whose name carries a money keyword is a
            // status category, not Money.
            Row {
                case: "codelist_beats_money_name",
                field: {
                    let mut f = field("fee_status", "TEXT", "string", "select");
                    f.is_codelist = true;
                    f
                },
                prop: None,
                workflow: None,
                expect: Dimension::StatusCategory,
                expect_hints: 0,
            },
            // 2 beats 6: the workflow status field match beats the money
            // name heuristic too.
            Row {
                case: "workflow_status_beats_money_name",
                field: field("rate", "INTEGER", "number", "number"),
                prop: None,
                workflow: Some("rate"),
                expect: Dimension::StatusCategory,
                expect_hints: 0,
            },
            // 3a. UUID pg → Identifier.
            Row {
                case: "uuid_infers_identifier",
                field: field("id", "UUID", "string", "text"),
                prop: None,
                workflow: None,
                expect: Dimension::Identifier,
                expect_hints: 0,
            },
            // 3b. Bare `id` with a string ts type (synthetic, no prop) →
            //     Identifier.
            Row {
                case: "id_name_with_string_ts_infers_identifier",
                field: field("id", "TEXT", "string", "text"),
                prop: None,
                workflow: None,
                expect: Dimension::Identifier,
                expect_hints: 0,
            },
            // 3 negative: an `_id` suffix alone is NOT an Identifier.
            Row {
                case: "id_suffix_text_stays_text",
                field: field("worker_id", "TEXT", "string", "text"),
                prop: None,
                workflow: None,
                expect: Dimension::Text,
                expect_hints: 0,
            },
            // 4. Boolean / checkbox → Flag.
            Row {
                case: "boolean_infers_flag",
                field: field("active", "BOOLEAN", "boolean", "checkbox"),
                prop: None,
                workflow: None,
                expect: Dimension::Flag,
                expect_hints: 0,
            },
            // 5a. Date → TimePoint.
            Row {
                case: "date_infers_time_point",
                field: field("due_date", "DATE", "string", "date"),
                prop: None,
                workflow: None,
                expect: Dimension::TimePoint,
                expect_hints: 0,
            },
            // 5b. Timestamp → TimePoint.
            Row {
                case: "timestamp_infers_time_point",
                field: field("created_at", "TIMESTAMPTZ", "string", "datetime-local"),
                prop: None,
                workflow: None,
                expect: Dimension::TimePoint,
                expect_hints: 0,
            },
            // 5c. Range types → TimePoint (v1 formats the lower bound).
            Row {
                case: "tstzrange_infers_time_point",
                field: {
                    let mut f = field("validity_period", "TSTZRANGE", "string", "date-range");
                    f.is_range = true;
                    f
                },
                prop: None,
                workflow: None,
                expect: Dimension::TimePoint,
                expect_hints: 0,
            },
            Row {
                case: "daterange_infers_time_point",
                field: field("enrollment_window", "DATERANGE", "string", "date-range"),
                prop: None,
                workflow: None,
                expect: Dimension::TimePoint,
                expect_hints: 0,
            },
            // 6a. Money keyword + numeric pg → Money, with a hint.
            Row {
                case: "total_amount_numeric_infers_money",
                field: field("total_amount", "NUMERIC(10,2)", "string", "number"),
                prop: None,
                workflow: None,
                expect: Dimension::Money,
                expect_hints: 1,
            },
            // 6b. `*_cents` integer convention → Money, with a hint.
            Row {
                case: "line_total_cents_infers_money",
                field: field("line_total_cents", "BIGINT", "number", "number"),
                prop: None,
                workflow: None,
                expect: Dimension::Money,
                expect_hints: 1,
            },
            // 6 negative: money keywords need a numeric signal.
            Row {
                case: "salary_band_text_stays_text",
                field: field("salary_band", "TEXT", "string", "text"),
                prop: None,
                workflow: None,
                expect: Dimension::Text,
                expect_hints: 0,
            },
            // 7. Numeric pg without a money name → Quantity.
            Row {
                case: "quantity_numeric_infers_quantity",
                field: field("quantity", "INTEGER", "number", "number"),
                prop: None,
                workflow: None,
                expect: Dimension::Quantity,
                expect_hints: 0,
            },
            // Money keyword matching is case-insensitive.
            Row {
                case: "money_keyword_is_case_insensitive",
                field: field("FEE", "INTEGER", "number", "number"),
                prop: None,
                workflow: None,
                expect: Dimension::Money,
                expect_hints: 1,
            },
            // Arrays infer the ELEMENT dimension: decimal array → Quantity.
            Row {
                case: "decimal_array_infers_element_quantity",
                field: {
                    let mut f = field("scores", "NUMERIC(10,2)[]", "Array<string>", "array");
                    f.is_array = true;
                    f
                },
                prop: None,
                workflow: None,
                expect: Dimension::Quantity,
                expect_hints: 0,
            },
            // Junction array-of-entity-ref → Reference.
            Row {
                case: "entity_ref_array_infers_reference",
                field: {
                    let mut f = field("related_workers", "TEXT[]", "Array<string>", "array");
                    f.is_array = true;
                    f.is_entity_ref = true;
                    f
                },
                prop: None,
                workflow: None,
                expect: Dimension::Reference,
                expect_hints: 0,
            },
            // 8. Fallthrough → Text.
            Row {
                case: "plain_text_infers_text",
                field: field("description", "TEXT", "string", "text"),
                prop: None,
                workflow: None,
                expect: Dimension::Text,
                expect_hints: 0,
            },
        ]
    }

    #[test]
    fn decision_list_table_pins_every_branch() {
        for row in rows() {
            let (dimension, hints) =
                infer_dimension_with_hints(row.prop.as_ref(), &row.field, row.workflow);
            assert_eq!(
                dimension, row.expect,
                "case '{}': expected {:?}, got {:?}",
                row.case, row.expect, dimension
            );
            assert_eq!(
                hints.len(),
                row.expect_hints,
                "case '{}': expected {} hints, got {hints:?}",
                row.case,
                row.expect_hints
            );
        }
    }

    #[test]
    fn infer_dimension_wrapper_drops_hints() {
        let f = field("total_amount", "NUMERIC(10,2)", "string", "number");
        assert_eq!(
            infer_dimension(None, &f, None),
            Dimension::Money,
            "the ergonomic wrapper returns the same dimension"
        );
    }

    #[test]
    fn money_hints_name_the_column_and_the_pin() {
        let f = field("total_amount", "NUMERIC(10,2)", "string", "number");
        let (_, hints) = infer_dimension_with_hints(None, &f, None);
        assert_eq!(hints.len(), 1);
        let hint = &hints[0];
        assert!(hint.contains("total_amount"), "{hint}");
        assert!(hint.contains("inferred Money"), "{hint}");
        assert!(hint.contains("[[column]]"), "{hint}");
        assert!(hint.contains("money"), "{hint}");
        assert!(hint.contains("quantity"), "{hint}");
    }

    #[test]
    fn non_money_inferences_emit_no_hints() {
        let quantity = field("headcount", "INTEGER", "number", "number");
        let (_, hints) = infer_dimension_with_hints(None, &quantity, None);
        assert!(hints.is_empty());

        let mut status = field("status", "TEXT", "string", "select");
        status.is_codelist = true;
        let (_, hints) = infer_dimension_with_hints(None, &status, None);
        assert!(hints.is_empty());
    }

    #[test]
    fn structured_wrapper_column_infers_text_but_sub_fields_recurse() {
        let mut wrapper = field("identifier", "JSONB", "string", "text");
        wrapper.structured_sub_fields =
            vec![sub_field("scheme_id"), sub_field("status"), sub_field("id")];
        wrapper.nested_type_name = Some("IdentifierType".to_string());

        // Column level: the JSONB slot falls through to Text.
        assert_eq!(infer_dimension(None, &wrapper, None), Dimension::Text);

        // Sub-fields infer independently — the workflow signal flows in,
        // money heuristics cannot (no numeric signal exists).
        let subs = infer_sub_field_dimensions(&wrapper, Some("status"));
        let by_name: Vec<(&str, Dimension)> = subs
            .iter()
            .map(|s| (s.name.as_str(), s.dimension))
            .collect();
        assert_eq!(
            by_name,
            vec![
                ("scheme_id", Dimension::Text),
                ("status", Dimension::StatusCategory),
                ("id", Dimension::Identifier),
            ]
        );
    }

    #[test]
    fn sub_fields_without_signals_infer_text() {
        let mut wrapper = field("person_legal", "JSONB", "string", "text");
        wrapper.structured_sub_fields = vec![sub_field("city"), sub_field("country_code")];
        let subs = infer_sub_field_dimensions(&wrapper, None);
        assert!(
            subs.iter().all(|s| s.dimension == Dimension::Text),
            "{subs:?}"
        );
    }
}
