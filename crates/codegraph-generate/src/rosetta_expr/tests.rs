use super::*;
use serde_json::json;

/// Caller-derived field knowledge for tests, mirroring what the
/// generator builds from PropertyNodes.
struct Sets {
    optional: HashSet<String>,
    collections: HashSet<String>,
    numeric: HashSet<String>,
    integer: HashSet<String>,
    enums: HashSet<String>,
}

impl Sets {
    fn empty() -> Self {
        Self {
            optional: HashSet::new(),
            collections: HashSet::new(),
            numeric: HashSet::new(),
            integer: HashSet::new(),
            enums: HashSet::new(),
        }
    }
    fn optional_fields(fields: &[&str]) -> Self {
        Self {
            optional: fields.iter().map(|f| f.to_string()).collect(),
            ..Self::empty()
        }
    }
    fn collections(fields: &[&str]) -> Self {
        Self {
            collections: fields.iter().map(|f| f.to_string()).collect(),
            ..Self::empty()
        }
    }
    fn numeric(numeric: &[&str], integer: &[&str]) -> Self {
        Self {
            numeric: numeric.iter().map(|f| f.to_string()).collect(),
            integer: integer.iter().map(|f| f.to_string()).collect(),
            ..Self::empty()
        }
    }
    fn enums(names: &[&str]) -> Self {
        Self {
            enums: names.iter().map(|n| n.to_string()).collect(),
            ..Self::empty()
        }
    }
    fn ctx(&self) -> ExprContext<'_> {
        ExprContext {
            receiver: "dto",
            optional_fields: &self.optional,
            collection_fields: &self.collections,
            numeric_fields: &self.numeric,
            integer_fields: &self.integer,
            enum_types: &self.enums,
        }
    }
}

fn t(payload: serde_json::Value) -> String {
    t_in(payload, &Sets::empty())
}

fn t_in(payload: serde_json::Value, sets: &Sets) -> String {
    transpile(&payload, &sets.ctx()).expect("transpiles")
}

fn e(payload: serde_json::Value) -> TranspileError {
    e_in(payload, &Sets::empty())
}

fn e_in(payload: serde_json::Value, sets: &Sets) -> TranspileError {
    transpile(&payload, &sets.ctx()).expect_err("rejected")
}

fn sym(s: &str) -> serde_json::Value {
    json!({"kind":"SymbolReference","symbol":s,"explicit":false,"args":[]})
}

fn int(n: &str) -> serde_json::Value {
    json!({"kind":"Int","text":n})
}

// ── Literals ─────────────────────────────────────────────────────────

#[test]
fn boolean_literals() {
    assert_eq!(t(json!({"kind":"Boolean","value":true})), "true");
    assert_eq!(t(json!({"kind":"Boolean","value":false})), "false");
}

#[test]
fn string_literal_escapes_quote_and_backslash() {
    assert_eq!(t(json!({"kind":"String","value":"plain"})), "\"plain\"");
    assert_eq!(
        t(json!({"kind":"String","value":"he said \"hi\" \\ ok"})),
        "\"he said \\\"hi\\\" \\\\ ok\""
    );
}

#[test]
fn number_and_int_literals_are_verbatim() {
    assert_eq!(t(json!({"kind":"Number","text":"12.5"})), "12.5");
    assert_eq!(t(json!({"kind":"Number","text":"0.0"})), "0.0");
    assert_eq!(t(json!({"kind":"Int","text":"42"})), "42");
    assert_eq!(t(json!({"kind":"Int","text":"-3"})), "-3");
}

#[test]
fn list_literal_becomes_vec() {
    assert_eq!(
        t(json!({"kind":"List","elements":[
            {"kind":"String","value":"a"},
            {"kind":"Int","text":"1"}
        ]})),
        "vec![\"a\", 1]"
    );
    assert_eq!(t(json!({"kind":"List","elements":[]})), "vec![]");
}

// ── Symbol references / implicit variable ────────────────────────────

#[test]
fn symbol_reference_becomes_field_access() {
    assert_eq!(t(sym("orderId")), "dto.order_id");
}

#[test]
fn symbol_reference_escapes_rust_keywords() {
    assert_eq!(t(sym("type")), "dto.r#type");
}

#[test]
fn optional_symbol_reference_is_refused() {
    let sets = Sets::optional_fields(&["memo"]);
    let err = e_in(sym("memo"), &sets);
    assert_eq!(err.kind, "SymbolReference");
    assert!(err.detail.contains("optional field 'memo'"), "{err}");
}

#[test]
fn function_like_symbol_reference_is_unsupported() {
    let err = e(json!({"kind":"SymbolReference","symbol":"f","explicit":true,"args":[]}));
    assert_eq!(err.kind, "SymbolReference");
    assert!(err.detail.contains("function-like"), "{err}");

    let err = e(json!({"kind":"SymbolReference","symbol":"f","explicit":false,"args":[int("1")]}));
    assert_eq!(err.kind, "SymbolReference");
}

#[test]
fn implicit_variable_is_the_receiver() {
    assert_eq!(t(json!({"kind":"ImplicitVariable"})), "dto");
}

// ── Feature calls ────────────────────────────────────────────────────

#[test]
fn feature_call_chain_on_required_fields() {
    assert_eq!(
        t(json!({"kind":"FeatureCall","receiver":
            json!({"kind":"FeatureCall","receiver": sym("counterparty"),
        "feature":"name"}),
        "feature":"upper"})),
        "dto.counterparty.name.upper"
    );
}

#[test]
fn deep_feature_call_nests_the_same_way() {
    assert_eq!(
        t(json!({"kind":"DeepFeatureCall","receiver": sym("counterparty"),"feature":"name"})),
        "dto.counterparty.name"
    );
}

#[test]
fn bare_projection_feature_is_unsupported() {
    let err = e(json!({"kind":"FeatureCall","receiver": sym("a"),"feature":null}));
    assert_eq!(err.kind, "FeatureCall");
    assert!(err.detail.contains("bare '->'"), "{err}");
}

#[test]
fn optional_feature_call_receiver_is_refused() {
    let sets = Sets::optional_fields(&["counterparty"]);
    let err = transpile(
        &json!({"kind":"FeatureCall","receiver": sym("counterparty"),"feature":"name"}),
        &sets.ctx(),
    )
    .expect_err("optional receiver refused");
    assert_eq!(err.kind, "FeatureCall");
    assert!(
        err.detail
            .contains("optional receiver requires exists-guard"),
        "{err}"
    );
}

// ── Binary operations ────────────────────────────────────────────────

#[test]
fn arithmetic_comparison_logical_ops_map() {
    let ops = [
        ("+", "+"),
        ("-", "-"),
        ("*", "*"),
        ("/", "/"),
        ("and", "&&"),
        ("or", "||"),
        ("=", "=="),
        ("<>", "!="),
        (">", ">"),
        ("<", "<"),
        (">=", ">="),
        ("<=", "<="),
    ];
    for (rosetta, rust) in ops {
        let payload = json!({"kind":"Binary","op":rosetta,
            "left":sym("a"),"right":int("1")});
        assert_eq!(t(payload), format!("dto.a {rust} 1"), "op {rosetta}");
    }
}

#[test]
fn nested_binaries_are_parenthesized_deterministically() {
    assert_eq!(
        t(json!({"kind":"Binary","op":"and",
            "left":{"kind":"Binary","op":">","left":sym("a"),"right":int("1")},
            "right":{"kind":"Binary","op":"<","left":sym("b"),"right":int("2")}})),
        "(dto.a > 1) && (dto.b < 2)"
    );
}

#[test]
fn cardinality_modifier_comparison_is_unsupported() {
    let err = e(json!({"kind":"Binary","op":"=","cardMod":"any",
        "left":sym("a"),"right":int("1")}));
    assert_eq!(err.kind, "Binary");
    assert!(err.detail.contains("cardinality comparison"), "{err}");
}

#[test]
fn card_mod_none_is_fine() {
    assert_eq!(
        t(json!({"kind":"Binary","op":"=","cardMod":"none",
            "left":sym("a"),"right":int("1")})),
        "dto.a == 1"
    );
}

// ── contains / disjoint / default (slice 2) ──────────────────────────

#[test]
fn contains_on_collection_field() {
    let sets = Sets::collections(&["aliases"]);
    assert_eq!(
        t_in(
            json!({"kind":"Binary","op":"contains","left":sym("aliases"),
                "right":{"kind":"String","value":"sale"}}),
            &sets
        ),
        "dto.aliases.contains(&\"sale\")"
    );
}

#[test]
fn contains_wraps_complex_right_operands() {
    let sets = Sets::collections(&["aliases"]);
    let payload = json!({"kind":"Binary","op":"contains","left":sym("aliases"),
        "right":{"kind":"Binary","op":"+","left":sym("a"),"right":sym("b")}});
    assert_eq!(
        t_in(payload, &sets),
        "dto.aliases.contains(&(dto.a + dto.b))"
    );
}

#[test]
fn contains_on_non_collection_is_refused() {
    let err = e(json!({"kind":"Binary","op":"contains","left":sym("name"),
        "right":{"kind":"String","value":"x"}}));
    assert_eq!(err.kind, "Binary");
    assert!(err.detail.contains("collection receiver"), "{err}");
}

#[test]
fn disjoint_on_collection_field() {
    let sets = Sets::collections(&["tags"]);
    assert_eq!(
        t_in(
            json!({"kind":"Binary","op":"disjoint","left":sym("tags"),
                "right":{"kind":"List","elements":[{"kind":"String","value":"a"}]}}),
            &sets
        ),
        "dto.tags.iter().all(|x| !vec![\"a\"].contains(x))"
    );
}

#[test]
fn default_sanctions_bare_optional_left() {
    let sets = Sets::optional_fields(&["memo"]);
    assert_eq!(
        t_in(
            json!({"kind":"Binary","op":"default","left":sym("memo"),
                "right":{"kind":"String","value":"x"}}),
            &sets
        ),
        "dto.memo.unwrap_or(\"x\")"
    );
}

// ── exists / absent ──────────────────────────────────────────────────

#[test]
fn exists_sanctions_optional_fields() {
    let sets = Sets::optional_fields(&["settled_on"]);
    let out = t_in(
        json!({"kind":"Exists","modifier":"none","argument":sym("settledOn")}),
        &sets,
    );
    assert_eq!(out, "dto.settled_on.is_some()");
}

#[test]
fn absent_sanctions_optional_fields() {
    let sets = Sets::optional_fields(&["memo"]);
    assert_eq!(
        t_in(json!({"kind":"Absent","argument":sym("memo")}), &sets),
        "dto.memo.is_none()"
    );
}

#[test]
fn exists_single_and_multiple_check_collection_length() {
    let sets = Sets::collections(&["lines"]);
    assert_eq!(
        t_in(
            json!({"kind":"Exists","modifier":"single","argument":sym("lines")}),
            &sets
        ),
        "dto.lines.len() == 1"
    );
    assert_eq!(
        t_in(
            json!({"kind":"Exists","modifier":"multiple","argument":sym("lines")}),
            &sets
        ),
        "dto.lines.len() >= 2"
    );
}

#[test]
fn exists_modifiers_on_non_collections_are_refused() {
    for modifier in ["single", "multiple"] {
        let err = e(json!({"kind":"Exists","modifier":modifier,"argument":sym("price")}));
        assert_eq!(err.kind, "Exists");
        assert!(err.detail.contains("collection field"), "{err}");
    }
}

// ── join ─────────────────────────────────────────────────────────────

#[test]
fn join_with_explicit_separator() {
    let sets = Sets::collections(&["tags"]);
    assert_eq!(
        t_in(
            json!({"kind":"Join","left":sym("tags"),
                "right":{"kind":"String","value":", "},"explicitSeparator":true}),
            &sets
        ),
        "dto.tags.join(\", \")"
    );
}

#[test]
fn join_without_separator_defaults_to_comma_space() {
    let sets = Sets::collections(&["tags"]);
    assert_eq!(
        t_in(
            json!({"kind":"Join","left":sym("tags"),
                "right":{"kind":"String","value":""},"explicitSeparator":false}),
            &sets
        ),
        "dto.tags.join(\", \")"
    );
}

#[test]
fn join_on_non_collection_is_refused() {
    let err = e(json!({"kind":"Join","left":sym("name"),
        "right":{"kind":"String","value":", "},"explicitSeparator":true}));
    assert_eq!(err.kind, "Join");
    assert!(err.detail.contains("collection field"), "{err}");
}

// ── collection ops (fragment-style contract) ─────────────────────────

#[test]
fn flatten_distinct_reverse_first_last_chain_fragments() {
    assert_eq!(
        t(json!({"kind":"Flatten","argument":sym("rows")})),
        "dto.rows.iter().flatten().collect::<Vec<_>>()"
    );
    assert_eq!(
        t(json!({"kind":"Distinct","argument":sym("prices")})),
        "dto.prices.iter().collect::<std::collections::BTreeSet<_>>()"
    );
    assert_eq!(
        t(json!({"kind":"Reverse","argument":sym("prices")})),
        "dto.prices.iter().rev().collect::<Vec<_>>()"
    );
    assert_eq!(
        t(json!({"kind":"First","argument":sym("prices")})),
        "dto.prices.first()"
    );
    assert_eq!(
        t(json!({"kind":"Last","argument":sym("prices")})),
        "dto.prices.last()"
    );
}

#[test]
fn collection_ops_compose_as_postfix_chain() {
    // prices distinct flatten reverse first last sum
    let mut payload = sym("prices");
    for kind in ["Distinct", "Flatten", "Reverse", "First", "Last"] {
        payload = json!({"kind":kind,"argument":payload});
    }
    let sets = Sets::numeric(&["prices"], &[]);
    assert_eq!(
        t_in(json!({"kind":"Sum","argument":payload}), &sets),
        "dto.prices.iter().collect::<std::collections::BTreeSet<_>>()\
         .iter().flatten().collect::<Vec<_>>()\
         .iter().rev().collect::<Vec<_>>().first().last().iter().sum::<f64>()"
    );
}

// ── aggregates ───────────────────────────────────────────────────────

#[test]
fn sum_picks_element_type_from_field_knowledge() {
    let float = Sets::numeric(&["prices"], &[]);
    assert_eq!(
        t_in(json!({"kind":"Sum","argument":sym("prices")}), &float),
        "dto.prices.iter().sum::<f64>()"
    );
    let integer = Sets::numeric(&["counts"], &["counts"]);
    assert_eq!(
        t_in(json!({"kind":"Sum","argument":sym("counts")}), &integer),
        "dto.counts.iter().sum::<i64>()"
    );
}

#[test]
fn sum_on_non_numeric_is_refused() {
    let err = e(json!({"kind":"Sum","argument":sym("lines")}));
    assert_eq!(err.kind, "Sum");
    assert!(err.detail.contains("numeric element type"), "{err}");
}

#[test]
fn min_max_fold_with_identity() {
    let float = Sets::numeric(&["prices"], &[]);
    assert_eq!(
        t_in(json!({"kind":"Min","argument":sym("prices")}), &float),
        "dto.prices.iter().copied().fold(f64::INFINITY, f64::min)"
    );
    assert_eq!(
        t_in(json!({"kind":"Max","argument":sym("prices")}), &float),
        "dto.prices.iter().copied().fold(f64::NEG_INFINITY, f64::max)"
    );
    let integer = Sets::numeric(&["counts"], &["counts"]);
    assert_eq!(
        t_in(json!({"kind":"Min","argument":sym("counts")}), &integer),
        "dto.counts.iter().copied().fold(i64::MAX, i64::min)"
    );
}

#[test]
fn min_with_lambda_is_a_documented_gap() {
    let sets = Sets::numeric(&["prices"], &[]);
    let err = e_in(
        json!({"kind":"Min","argument":sym("prices"),"function":{
            "parameters":["p"],
            "body":{"kind":"Binary","op":">","left":sym("p"),"right":{"kind":"Number","text":"0.0"}}}}),
        &sets,
    );
    assert_eq!(err.kind, "Min");
    assert!(err.detail.contains("documented gap"), "{err}");
}

#[test]
fn count_requires_a_collection_field() {
    let sets = Sets::collections(&["lines"]);
    assert_eq!(
        t_in(json!({"kind":"Count","argument":sym("lines")}), &sets),
        "dto.lines.len()"
    );
    let err = e(json!({"kind":"Count","argument":sym("price")}));
    assert_eq!(err.kind, "Count");
    assert!(err.detail.contains("collection field"), "{err}");
}

// ── filter / extract (lambda frames) ─────────────────────────────────

#[test]
fn filter_implicit_body_binds_item() {
    let sets = Sets::collections(&["lines"]);
    assert_eq!(
        t_in(
            json!({"kind":"Filter","argument":sym("lines"),"function":{
                "parameters":[],
                "body":{"kind":"Binary","op":"<","left":sym("quantity"),"right":int("10")}}}),
            &sets
        ),
        "dto.lines.iter().filter(|item| dto.quantity < 10).collect::<Vec<_>>()"
    );
}

#[test]
fn filter_explicit_param_binds_by_name() {
    let sets = Sets::collections(&["lines"]);
    assert_eq!(
        t_in(
            json!({"kind":"Filter","argument":sym("lines"),"function":{
                "parameters":["l"],
                "body":{"kind":"Binary","op":"=","left":
                    json!({"kind":"FeatureCall","receiver":sym("l"),"feature":"itemId"}),
                "right":{"kind":"String","value":"x"}}}}),
            &sets
        ),
        "dto.lines.iter().filter(|l| l.item_id == \"x\").collect::<Vec<_>>()"
    );
}

#[test]
fn filter_param_shadows_receiver_field() {
    // A parameter named like a field binds the parameter (innermost
    // frame first), even when the field exists on the receiver.
    let sets = Sets::collections(&["lines"]);
    assert_eq!(
        t_in(
            json!({"kind":"Filter","argument":sym("lines"),"function":{
                "parameters":["price"],
                "body":{"kind":"Binary","op":">","left":sym("price"),"right":{"kind":"Number","text":"1.0"}}}}),
            &sets
        ),
        "dto.lines.iter().filter(|price| price > 1.0).collect::<Vec<_>>()"
    );
}

#[test]
fn extract_maps_with_the_lambda_body() {
    let sets = Sets::collections(&["prices"]);
    assert_eq!(
        t_in(
            json!({"kind":"Map","argument":sym("prices"),"function":{
                "parameters":["p"],"body":sym("p")}}),
            &sets
        ),
        "dto.prices.iter().map(|p| p).collect::<Vec<_>>()"
    );
}

#[test]
fn filter_without_a_function_is_refused() {
    let sets = Sets::collections(&["lines"]);
    let err = e_in(
        json!({"kind":"Filter","argument":sym("lines"),"function":null}),
        &sets,
    );
    assert_eq!(err.kind, "Filter");
    assert!(err.detail.contains("no body"), "{err}");
}

#[test]
fn multi_parameter_filter_is_refused() {
    let sets = Sets::collections(&["lines"]);
    let err = e_in(
        json!({"kind":"Filter","argument":sym("lines"),"function":{
            "parameters":["a","b"],"body":sym("a")}}),
        &sets,
    );
    assert_eq!(err.kind, "Filter");
    assert!(err.detail.contains("multi-parameter"), "{err}");
}

#[test]
fn implicit_variable_is_refused_under_explicit_params() {
    let sets = Sets::collections(&["lines"]);
    let err = e_in(
        json!({"kind":"Filter","argument":sym("lines"),"function":{
            "parameters":["l"],"body":json!({"kind":"ImplicitVariable"})}}),
        &sets,
    );
    assert_eq!(err.kind, "ImplicitVariable");
}

// ── reduce / sort (documented gaps) ──────────────────────────────────

#[test]
fn reduce_is_a_documented_gap_in_both_forms() {
    let explicit = e(json!({"kind":"Reduce","argument":sym("prices"),"function":{
        "parameters":["a","b"],
        "body":{"kind":"Binary","op":"+","left":sym("a"),"right":sym("b")}}}));
    assert_eq!(explicit.kind, "Reduce");
    assert!(explicit.detail.contains("documented gap"), "{explicit}");

    let init = e(json!({"kind":"Reduce","argument":sym("prices"),"function":{
        "parameters":[],"body":{"kind":"List","elements":[{"kind":"Number","text":"0.0"}]}}}));
    assert_eq!(init.kind, "Reduce");
}

#[test]
fn sort_is_a_documented_gap() {
    let err = e(json!({"kind":"Sort","argument":sym("prices"),"function":{
        "parameters":["a","b"],
        "body":{"kind":"Binary","op":"-","left":sym("a"),"right":sym("b")}}}));
    assert_eq!(err.kind, "Sort");
    assert!(err.detail.contains("no std expression form"), "{err}");
}

// ── switch / conditional ─────────────────────────────────────────────

fn switch_case(guard: serde_json::Value, expr: serde_json::Value) -> serde_json::Value {
    json!({"guard":guard,"expression":expr})
}

#[test]
fn switch_literal_guards_become_if_else_chain() {
    let payload = json!({"kind":"Switch","argument":sym("price"),"cases":[
        switch_case(json!({"kind":"Literal","value":int("1")}), json!({"kind":"String","value":"a"})),
        switch_case(json!({"kind":"Literal","value":{"kind":"Number","text":"2.5"}}),
            json!({"kind":"String","value":"b"})),
        json!({"default":true,"expression":{"kind":"String","value":"c"}}),
    ]});
    assert_eq!(
        t(payload),
        "if dto.price == 1 { \"a\" } else if dto.price == 2.5 { \"b\" } else { \"c\" }"
    );
}

#[test]
fn switch_default_only_emits_the_default_expression() {
    // Regression: a default-only switch used to emit bare `else { .. }`,
    // which is not valid Rust.
    let payload = json!({"kind":"Switch","argument":sym("price"),"cases":[
        json!({"default":true,"expression":{"kind":"String","value":"c"}}),
    ]});
    assert_eq!(t(payload), "\"c\"");
}

#[test]
fn switch_without_default_is_refused() {
    let payload = json!({"kind":"Switch","argument":sym("price"),"cases":[
        switch_case(json!({"kind":"Literal","value":int("1")}), json!({"kind":"Boolean","value":true})),
    ]});
    let err = e(payload);
    assert_eq!(err.kind, "Switch");
    assert!(err.detail.contains("exactly one default"), "{err}");
}

#[test]
fn switch_reference_guard_is_refused() {
    let payload = json!({"kind":"Switch","argument":sym("status"),"cases":[
        switch_case(json!({"kind":"Reference","target":"TradeStatus"}),
            json!({"kind":"String","value":"x"})),
        json!({"default":true,"expression":{"kind":"String","value":"y"}}),
    ]});
    let err = e(payload);
    assert_eq!(err.kind, "Switch");
    assert!(err.detail.contains("reference guard"), "{err}");
}

#[test]
fn switch_used_as_operand_is_parenthesized() {
    let switch = json!({"kind":"Switch","argument":sym("quantity"),"cases":[
        switch_case(json!({"kind":"Literal","value":int("1")}), json!({"kind":"Boolean","value":true})),
        json!({"default":true,"expression":{"kind":"Boolean","value":false}}),
    ]});
    let payload = json!({"kind":"Binary","op":"=","left":switch,
        "right":{"kind":"Boolean","value":true}});
    assert_eq!(
        t(payload),
        "(if dto.quantity == 1 { true } else { false }) == true"
    );
}

#[test]
fn conditional_becomes_if_else_expression() {
    let payload = json!({"kind":"Conditional",
        "if":{"kind":"Binary","op":">","left":sym("price"),"right":{"kind":"Number","text":"1.0"}},
        "then":sym("prices"),
        "else":{"kind":"List","elements":[{"kind":"Number","text":"0.0"}]},
        "full":true});
    // `transpile` roots at BOOL position (issue #283): ANY List
    // else-arm — a real authored list included — lowers to `false`
    // there, because no list is bool.
    assert_eq!(
        t(payload.clone()),
        "if (dto.price > 1.0) { dto.prices } else { false }"
    );
    // Value position keeps the faithful list rendering (byte-identity).
    let sets = Sets::empty();
    let out = transpile_scoped(&payload, &sets.ctx(), &[]).expect("transpiles");
    assert_eq!(
        out,
        "if (dto.price > 1.0) { dto.prices } else { vec![0.0] }"
    );
}

#[test]
fn conditional_without_else_in_bool_position_emits_false() {
    // Issue #283: `if COND then X` carries the GENERATED `full:false`
    // empty-list else. At a condition root (bool position) `vec![]` is
    // not bool and the generated crate cannot compile — the List else
    // lowers to `false`. Value position still emits `vec![]` (the
    // AveragingMethodologyExists value-position pin below).
    let payload = json!({"kind":"Conditional",
        "if":{"kind":"Binary","op":">","left":sym("price"),"right":{"kind":"Number","text":"1.0"}},
        "then":sym("prices"),
        "else":{"kind":"List","elements":[]},
        "full":false});
    assert_eq!(
        t(payload),
        "if (dto.price > 1.0) { dto.prices } else { false }"
    );
}

#[test]
fn conditional_as_operand_is_parenthesized() {
    let conditional = json!({"kind":"Conditional",
        "if":{"kind":"Boolean","value":true},"then":int("1"),"else":int("2"),
        "full":true});
    let payload = json!({"kind":"Binary","op":"+","left":conditional,"right":int("3")});
    assert_eq!(t(payload), "(if true { 1 } else { 2 }) + 3");
}

// ── bool-position rule (issue #283) ──────────────────────────────────

/// The verbatim `Reset.AveragingMethodologyExists` payload shape
/// (docs/trade-state-spike.md §2.3): `if observations->count > 1 then
/// averagingMethodology exists` — a `full:false` Conditional whose
/// else-arm is the generated empty list.
fn averaging_methodology_exists() -> serde_json::Value {
    json!({
        "kind":"Conditional","full":false,
        "if":{"kind":"Binary","op":">",
            "left":{"kind":"Count","argument":sym("observations")},
            "right":int("1")},
        "then":{"kind":"Exists","modifier":"none",
            "argument":sym("averagingMethodology")},
        "else":{"kind":"List","elements":[]}
    })
}

/// Field knowledge for the AveragingMethodologyExists payload:
/// `observations` is an array, `averagingMethodology` an optional.
fn averaging_sets() -> Sets {
    Sets {
        collections: HashSet::from(["observations".to_string()]),
        optional: HashSet::from(["averaging_methodology".to_string()]),
        ..Sets::empty()
    }
}

/// Byte-exact mirror of the per-condition function emission in
/// `ddd/validations.rs::emit_transpiled_conditions` (the condition
/// root is wrapped as `if !(…) {`), so transpiler goldens pin the
/// full integration shape.
fn validations_condition_body(name: &str, entity: &str, fn_name: &str, expr: &str) -> String {
    format!(
        "/// Condition '{name}' for {entity} (create path).\n\
         pub fn validate_{fn_name}(dto: &Create{entity}Request) -> Result<(), String> {{\n    \
         if !({expr}) {{\n        \
         return Err(\"{name} failed\".to_string());\n    }}\n    \
         Ok(())\n}}"
    )
}

#[test]
fn averaging_methodology_lowers_list_else_to_false_in_bool_position() {
    // The defect (issue #283): the root emitted
    // `… else { vec![] }` inside `if !(…) {` — not bool, no compile.
    let sets = averaging_sets();
    assert_eq!(
        transpile(&averaging_methodology_exists(), &sets.ctx()).expect("transpiles"),
        "if (dto.observations.len() > 1) { dto.averaging_methodology.is_some() } else { false }"
    );
}

#[test]
fn averaging_methodology_exists_full_validations_fn_body_golden() {
    // Regression golden: the FULL emitted fn body, exactly as
    // validations.rs renders the transpiled condition root.
    let sets = averaging_sets();
    let expr = transpile(&averaging_methodology_exists(), &sets.ctx()).expect("transpiles");
    assert_eq!(
        validations_condition_body(
            "AveragingMethodologyExists",
            "Reset",
            "averaging_methodology_exists",
            &expr
        ),
        "/// Condition 'AveragingMethodologyExists' for Reset (create path).\n\
         pub fn validate_averaging_methodology_exists(dto: &CreateResetRequest) -> Result<(), String> {\n    \
         if !(if (dto.observations.len() > 1) { dto.averaging_methodology.is_some() } else { false }) {\n        \
         return Err(\"AveragingMethodologyExists failed\".to_string());\n    }\n    \
         Ok(())\n}"
    );
}

#[test]
fn averaging_methodology_exists_value_position_keeps_vec_empty() {
    // Byte-identity pin (issue #283): the SAME payload in VALUE
    // position — function aliases/operations transpile through
    // `transpile_scoped` — still emits the faithful generated
    // empty-list else (the documented slice-2 decision).
    let sets = averaging_sets();
    let out =
        transpile_scoped(&averaging_methodology_exists(), &sets.ctx(), &[]).expect("transpiles");
    assert_eq!(
        out,
        "if (dto.observations.len() > 1) { dto.averaging_methodology.is_some() } else { vec![] }"
    );
}

#[test]
fn bool_position_conditional_with_boolean_else_emits_it() {
    // The rule only rewrites LIST else-arms; a real Boolean else is
    // emitted as-is in bool position.
    let payload = json!({"kind":"Conditional","full":true,
        "if":{"kind":"Binary","op":">","left":sym("price"),"right":{"kind":"Number","text":"1.0"}},
        "then":{"kind":"Boolean","value":true},
        "else":{"kind":"Boolean","value":false}});
    assert_eq!(t(payload), "if (dto.price > 1.0) { true } else { false }");
}

#[test]
fn switch_in_bool_position_lowers_a_list_default_to_false() {
    // Finding (issue #283): switches REQUIRE an authored default
    // (emit_switch enforces exactly one), so the `full:false`
    // generated-else shape cannot arise there — but an authored
    // empty-list default is the same non-bool situation and follows
    // the same `false` rule in bool position.
    let payload = json!({"kind":"Switch","argument":sym("quantity"),"cases":[
        switch_case(json!({"kind":"Literal","value":int("1")}), json!({"kind":"Boolean","value":true})),
        json!({"default":true,"expression":{"kind":"List","elements":[]}}),
    ]});
    assert_eq!(t(payload), "if dto.quantity == 1 { true } else { false }");

    // Default-only switch: the List default becomes `false` directly.
    let default_only = json!({"kind":"Switch","argument":sym("quantity"),"cases":[
        json!({"default":true,"expression":{"kind":"List","elements":[]}}),
    ]});
    assert_eq!(t(default_only.clone()), "false");

    // Value position keeps the faithful `vec![]` default.
    let sets = Sets::empty();
    let out = transpile_scoped(&default_only, &sets.ctx(), &[]).expect("transpiles");
    assert_eq!(out, "vec![]");
}

// ── casts ────────────────────────────────────────────────────────────

#[test]
fn value_casts_append_their_suffix() {
    assert_eq!(
        t(json!({"kind":"ToString","argument":sym("tradeId")})),
        "dto.trade_id.to_string()"
    );
    assert_eq!(
        t(json!({"kind":"ToNumber","argument":sym("tradeId")})),
        "dto.trade_id.parse::<f64>().ok()"
    );
    assert_eq!(
        t(json!({"kind":"ToInt","argument":sym("tradeId")})),
        "dto.trade_id.parse::<i64>().ok()"
    );
}

#[test]
fn date_time_casts_are_documented_gaps() {
    for kind in ["ToDate", "ToDateTime", "ToZonedDateTime", "ToTime"] {
        let err = e(json!({"kind":kind,"argument":sym("tradeId")}));
        assert_eq!(err.kind, kind);
        assert!(err.detail.contains("date-time library"), "{err}");
    }
}

#[test]
fn to_enum_is_a_documented_gap() {
    let err = e(json!({"kind":"ToEnum","enumeration":"TradeStatus","argument":sym("tradeId")}));
    assert_eq!(err.kind, "ToEnum");
    assert!(err.detail.contains("codelist knowledge"), "{err}");
}

// ── then ─────────────────────────────────────────────────────────────

#[test]
fn then_with_null_function_passes_through() {
    assert_eq!(
        t(json!({"kind":"Then","argument":sym("prices"),"function":null})),
        "dto.prices"
    );
}

#[test]
fn then_named_ref_function_becomes_a_call() {
    assert_eq!(
        t(json!({"kind":"Then","argument":sym("price"),"function":{
            "parameters":[],
            "body":{"kind":"SymbolReference","symbol":"round","explicit":true,"args":[]}}})),
        "round(dto.price)"
    );
}

#[test]
fn then_implicit_body_binds_the_argument_fragment() {
    // prices extract p [p] then item — the then-body's implicit variable
    // binds the extract fragment.
    let extract = json!({"kind":"Map","argument":sym("prices"),"function":{
        "parameters":["p"],"body":sym("p")}});
    let sets = Sets::collections(&["prices"]);
    assert_eq!(
        t_in(
            json!({"kind":"Then","argument":extract,"function":{
                "parameters":[],"body":json!({"kind":"ImplicitVariable"})}}),
            &sets
        ),
        "dto.prices.iter().map(|p| p).collect::<Vec<_>>()"
    );
}

#[test]
fn then_with_parameters_is_refused() {
    let err = e(json!({"kind":"Then","argument":sym("price"),"function":{
        "parameters":["p"],"body":sym("p")}}));
    assert_eq!(err.kind, "Then");
    assert!(err.detail.contains("parameterized"), "{err}");
}

// ── slice 3 stubs ────────────────────────────────────────────────────

#[test]
fn with_meta_passes_through_with_a_dropped_marker() {
    assert_eq!(
        t(json!({"kind":"WithMeta","argument":sym("price"),"entries":[
            {"key":"scheme","value":{"kind":"String","value":"x"}}]})),
        "dto.price /* meta dropped */"
    );
}

#[test]
fn only_exists_is_a_conjunction_of_exists() {
    let sets = Sets::optional_fields(&["price", "quantity"]);
    assert_eq!(
        t_in(
            json!({"kind":"OnlyExists","args":[sym("price"), sym("quantity")],
                "parentheses":true}),
            &sets
        ),
        "dto.price.is_some() && dto.quantity.is_some()"
    );
}

#[test]
fn only_exists_without_arguments_is_refused() {
    let err = e(json!({"kind":"OnlyExists","args":[],"parentheses":false}));
    assert_eq!(err.kind, "OnlyExists");
}

#[test]
fn type_system_stubs_carry_their_kind() {
    for (family, payload) in [
        ("As", json!({"kind":"As","type":"Foo","argument":sym("a")})),
        ("AsKey", json!({"kind":"AsKey","argument":sym("a")})),
        ("OneOf", json!({"kind":"OneOf","argument":sym("a")})),
        (
            "Choice",
            json!({"kind":"Choice","necessity":"optional","attributes":["x"],"argument":sym("a")}),
        ),
        (
            "Constructor",
            json!({"kind":"Constructor","type":{"name":"Foo","arguments":[]},
            "values":[],"implicitEmpty":false}),
        ),
    ] {
        let err = e(payload);
        assert_eq!(err.kind, family, "{family}");
    }
}

// ── malformed payloads carry their family kind ────────────────────────

#[test]
fn unsupported_families_carry_the_kind_string() {
    let families = [
        "Join",
        "Conditional",
        "Switch",
        "WithMeta",
        "As",
        "Then",
        "Filter",
        "Map",
        "Reduce",
        "Sort",
        "Min",
        "Max",
        "Flatten",
        "Distinct",
        "Reverse",
        "First",
        "Last",
        "Sum",
        "AsKey",
        "OneOf",
        "Choice",
        "Constructor",
        "OnlyExists",
        "ToString",
        "ToDate",
    ];
    for family in families {
        let err = e(json!({"kind":family}));
        assert_eq!(err.kind, family, "{family}");
    }
}

#[test]
fn missing_kind_tag_is_reported() {
    let err = e(json!({"nope":true}));
    assert_eq!(err.kind, "Unknown");
}

#[test]
fn unknown_kind_is_reported() {
    let err = e(json!({"kind":"Mystery","argument":sym("a")}));
    assert_eq!(err.kind, "Mystery");
}

#[test]
fn display_is_marker_friendly() {
    let err = TranspileError::new("Switch", "someday");
    assert_eq!(err.to_string(), "unsupported expression (Switch): someday");
}

// ── enum literals + enum equality (issue #283 slice b) ───────────────

fn enum_call(enum_name: &str, feature: &str) -> serde_json::Value {
    json!({"kind":"FeatureCall","receiver":sym(enum_name),"feature":feature})
}

#[test]
fn enum_qualified_feature_call_becomes_variant_literal() {
    let sets = Sets::enums(&["PositionStatusEnum"]);
    assert_eq!(
        t_in(enum_call("PositionStatusEnum", "Closed"), &sets),
        "PositionStatusEnum::Closed"
    );
}

#[test]
fn enum_variant_feature_is_pascal_cased() {
    let sets = Sets::enums(&["EventIntentEnum"]);
    assert_eq!(
        t_in(
            enum_call("EventIntentEnum", "corporateActionAdjustment"),
            &sets
        ),
        "EventIntentEnum::CorporateActionAdjustment"
    );
}

#[test]
fn deep_feature_call_enum_receiver_lowers_the_same() {
    let sets = Sets::enums(&["E"]);
    assert_eq!(
        t_in(
            json!({"kind":"DeepFeatureCall","receiver":sym("E"),"feature":"V"}),
            &sets
        ),
        "E::V"
    );
}

#[test]
fn receiver_not_in_enum_types_stays_field_access() {
    // The generic receiver-field policy: unknown symbols are receiver
    // fields (the documented untyped policy — no enum knowledge).
    assert_eq!(
        t(enum_call("PositionStatusEnum", "Closed")),
        "dto.position_status_enum.closed"
    );
}

#[test]
fn field_wins_tie_over_enum_name() {
    // `Status` is BOTH an enum_types member and a known field — the
    // field wins (resolution order tier 2 beats tier 3).
    let sets = Sets {
        collections: HashSet::from(["status".to_string()]),
        ..Sets::enums(&["Status"])
    };
    assert_eq!(
        t_in(enum_call("Status", "Closed"), &sets),
        "dto.status.closed"
    );
}

#[test]
fn bare_enum_namespace_symbol_is_refused() {
    // A type name is not a value: bare `PositionStatusEnum` outside a
    // qualified `Enum -> Variant` literal is refused.
    let sets = Sets::enums(&["PositionStatusEnum"]);
    let err = e_in(sym("PositionStatusEnum"), &sets);
    assert_eq!(err.kind, "SymbolReference");
    assert!(err.detail.contains("qualified 'Enum -> Variant'"), "{err}");
}

#[test]
fn local_beats_enum_name() {
    // Resolution order tier 1: a lambda local named like an enum type
    // binds the local.
    let sets = Sets::enums(&["Status"]);
    let payload = sym("Status");
    let locals = vec!["status".to_string()];
    let out = transpile_scoped(&payload, &sets.ctx(), &locals).expect("transpiles");
    assert_eq!(out, "status");
}

#[test]
fn optional_enum_field_equality_lowers_to_as_ref_some() {
    // ClosedStateExists if-arm: `positionState = PositionStatusEnum ->
    // Closed` over an optional codelist-reference field.
    let sets = Sets {
        optional: HashSet::from(["position_state".to_string()]),
        ..Sets::enums(&["PositionStatusEnum"])
    };
    let payload = json!({"kind":"Binary","op":"=","cardMod":"none",
        "left":sym("positionState"),"right":enum_call("PositionStatusEnum","Closed")});
    assert_eq!(
        t_in(payload, &sets),
        "dto.position_state.as_ref() == Some(&PositionStatusEnum::Closed)"
    );
}

#[test]
fn optional_enum_field_inequality_lowers_to_as_ref_ne() {
    let sets = Sets {
        optional: HashSet::from(["position_state".to_string()]),
        ..Sets::enums(&["PositionStatusEnum"])
    };
    let payload = json!({"kind":"Binary","op":"<>","cardMod":"none",
        "left":sym("positionState"),"right":enum_call("PositionStatusEnum","Closed")});
    assert_eq!(
        t_in(payload, &sets),
        "dto.position_state.as_ref() != Some(&PositionStatusEnum::Closed)"
    );
}

#[test]
fn required_enum_field_equality_is_plain() {
    // A required enum field renders without the Option wrapper in the
    // DTO — plain `==`.
    let sets = Sets::enums(&["EventIntentEnum"]);
    let payload = json!({"kind":"Binary","op":"=","cardMod":"none",
        "left":sym("intent"),"right":enum_call("EventIntentEnum","Novation")});
    assert_eq!(
        t_in(payload, &sets),
        "dto.intent == EventIntentEnum::Novation"
    );
}

#[test]
fn enum_literal_on_the_left_matches_too() {
    let sets = Sets {
        optional: HashSet::from(["status".to_string()]),
        ..Sets::enums(&["TradeStatus"])
    };
    let payload = json!({"kind":"Binary","op":"=","cardMod":"none",
        "left":enum_call("TradeStatus","Settled"),"right":sym("status")});
    assert_eq!(
        t_in(payload, &sets),
        "dto.status.as_ref() == Some(&TradeStatus::Settled)"
    );
}

#[test]
fn enum_field_vs_field_equality_keeps_the_optional_refusal() {
    // Neither side is a literal: the generic operand path applies and
    // the bare optional field keeps its refusal (field-vs-field
    // optional comparison stays unsupported — documented).
    let sets = Sets {
        optional: HashSet::from(["a".to_string()]),
        ..Sets::enums(&["E"])
    };
    let payload = json!({"kind":"Binary","op":"=","cardMod":"none",
        "left":sym("a"),"right":sym("b")});
    let err = e_in(payload, &sets);
    assert_eq!(err.kind, "SymbolReference");
    assert!(err.detail.contains("optional field 'a'"), "{err}");
}

#[test]
fn enum_equality_field_side_respects_lambda_locals() {
    // The field side resolving as a lambda local is not a receiver
    // field: generic path emits the bare local against the literal.
    let sets = Sets::enums(&["EventIntentEnum"]);
    let payload = json!({"kind":"Binary","op":"=","cardMod":"none",
        "left":sym("intent"),"right":enum_call("EventIntentEnum","Novation")});
    let locals = vec!["intent".to_string()];
    let out = transpile_scoped(&payload, &sets.ctx(), &locals).expect("transpiles");
    assert_eq!(out, "intent == EventIntentEnum::Novation");
}

#[test]
fn exists_over_enum_chain_is_refused() {
    let sets = Sets::enums(&["E"]);
    let err = e_in(
        json!({"kind":"Exists","modifier":"none","argument":enum_call("E","V")}),
        &sets,
    );
    assert_eq!(err.kind, "Exists");
    assert!(err.detail.contains("cannot be exists-checked"), "{err}");
}

// ── guarded-optional chains (issue #283 slice b) ─────────────────────

fn chain(root: &str, feature: &str) -> serde_json::Value {
    json!({"kind":"FeatureCall","receiver":sym(root),"feature":feature})
}

#[test]
fn chain_exists_through_optional_root_uses_the_map_shape() {
    let sets = Sets::optional_fields(&["primitive_instruction"]);
    let payload = json!({"kind":"Exists","modifier":"none","argument":
        chain("primitiveInstruction", "execution")});
    assert_eq!(
        t_in(payload, &sets),
        "dto.primitive_instruction.as_ref().map(|v| v.execution.is_some()).unwrap_or(false)"
    );
}

#[test]
fn chain_absent_through_optional_root_unwraps_to_true() {
    // A `None` root means the whole path is absent.
    let sets = Sets::optional_fields(&["a"]);
    let payload = json!({"kind":"Absent","argument":chain("a", "b")});
    assert_eq!(
        t_in(payload, &sets),
        "dto.a.as_ref().map(|v| v.b.is_none()).unwrap_or(true)"
    );
}

#[test]
fn chain_exists_through_required_root_stays_plain() {
    let sets = Sets::empty();
    let payload = json!({"kind":"Exists","modifier":"none","argument":chain("a", "b")});
    assert_eq!(t_in(payload, &sets), "dto.a.b.is_some()");
}

#[test]
fn chain_exists_multi_feature_is_plain_beyond_the_root() {
    // Only the ROOT's optionality is known — deeper hops emit plain
    // access (documented caveat class).
    let sets = Sets::optional_fields(&["a"]);
    let payload = json!({"kind":"Exists","modifier":"none","argument":
        json!({"kind":"FeatureCall","receiver":chain("a","b"),"feature":"c"})});
    assert_eq!(
        t_in(payload, &sets),
        "dto.a.as_ref().map(|v| v.b.c.is_some()).unwrap_or(false)"
    );
}

#[test]
fn chain_features_snake_case_and_escape_keywords() {
    let sets = Sets::optional_fields(&["a"]);
    let payload = json!({"kind":"Exists","modifier":"none","argument":
        json!({"kind":"FeatureCall","receiver":chain("a","tradeId"),"feature":"type"})});
    assert_eq!(
        t_in(payload, &sets),
        "dto.a.as_ref().map(|v| v.trade_id.r#type.is_some()).unwrap_or(false)"
    );
}

#[test]
fn only_exists_lowers_chain_arguments() {
    // ExclusiveSplitPrimitive then-arm: `primitiveInstruction -> split
    // only exists` over an optional choice-typed receiver.
    let sets = Sets::optional_fields(&["primitive_instruction"]);
    let payload = json!({"kind":"OnlyExists","args":[chain("primitiveInstruction","split")],
        "parentheses":false});
    assert_eq!(
        t_in(payload, &sets),
        "dto.primitive_instruction.as_ref().map(|v| v.split.is_some()).unwrap_or(false)"
    );
}

#[test]
fn only_exists_conjoins_mixed_bare_and_chain_arguments() {
    let sets = Sets::optional_fields(&["price", "a"]);
    let payload = json!({"kind":"OnlyExists","args":[sym("price"), chain("a","b")],
        "parentheses":true});
    assert_eq!(
        t_in(payload, &sets),
        "dto.price.is_some() && dto.a.as_ref().map(|v| v.b.is_some()).unwrap_or(false)"
    );
}

#[test]
fn chain_through_lambda_local_root_is_plain() {
    let sets = Sets::optional_fields(&["a"]);
    let payload = json!({"kind":"Exists","modifier":"none","argument":chain("x","f")});
    let locals = vec!["x".to_string()];
    let out = transpile_scoped(&payload, &sets.ctx(), &locals).expect("transpiles");
    assert_eq!(out, "x.f.is_some()");
}

#[test]
fn value_position_chain_through_optional_root_still_refused() {
    // Slice-1 rule unchanged: exists/absent is the sanction — a value
    // position (function aliases/operations) keeps the refusal.
    let sets = Sets::optional_fields(&["a"]);
    let err = transpile_scoped(&chain("a", "b"), &sets.ctx(), &[])
        .expect_err("value-position chain refused");
    assert_eq!(err.kind, "FeatureCall");
    assert!(
        err.detail
            .contains("optional receiver requires exists-guard"),
        "{err}"
    );
}

// ── transpile_scoped (issue #263 slice-4 addition) ──────────────────

fn t_scoped(payload: serde_json::Value, locals: &[&str]) -> String {
    let sets = Sets::empty();
    let locals: Vec<String> = locals.iter().map(|s| s.to_string()).collect();
    transpile_scoped(&payload, &sets.ctx(), &locals).expect("transpiles")
}

#[test]
fn scoped_locals_emit_bare() {
    // `base + trade -> quantity` with `base` a local (an alias) and
    // `trade` an input local: both emit bare, the feature as a field.
    let payload = json!({
        "kind":"Binary","op":"+",
        "left": sym("base"),
        "right": {"kind":"FeatureCall","receiver":sym("trade"),"feature":"quantity"},
    });
    assert_eq!(
        t_scoped(payload, &["base", "trade"]),
        "base + trade.quantity"
    );
}

#[test]
fn scoped_unknown_symbols_stay_receiver_fields() {
    // Not-a-local: unchanged transpile semantics (receiver prefix).
    assert_eq!(t_scoped(sym("base"), &[]), "dto.base");
}

#[test]
fn scoped_local_beats_receiver_even_when_named_field() {
    let payload = json!({
        "kind":"FeatureCall","receiver":sym("result"),"feature":"price",
    });
    assert_eq!(t_scoped(payload, &["result"]), "result.price");
}

#[test]
fn scoped_exists_on_local_keeps_is_some_shape() {
    let payload = json!({"kind":"Exists","modifier":"none","argument":sym("result")});
    assert_eq!(t_scoped(payload, &["result"]), "result.is_some()");
}

#[test]
fn scoped_snake_cases_and_keyword_escapes_locals() {
    // Matching is on the EMITTED (snake_cased, keyword-escaped) name —
    // the generator registers locals in exactly that form.
    let payload = json!({"kind":"FeatureCall","receiver":sym("Order"),"feature":"type"});
    assert_eq!(t_scoped(payload, &["order", "r#type"]), "order.r#type");
}
