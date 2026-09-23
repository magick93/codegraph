//! expr→Rust transpiler conformance tests (issue #262, slices 1–3).
//!
//! codegraph-generate must not depend on sigil (issue #255), so the
//! sigil-touching half of the conformance suite lives here: REAL rosetta
//! conditions are parsed (sigil parse → lower → resolve), serialized with
//! the canonical `Expr::to_json()` (exactly what the bridge stores in
//! `ConditionNode.expr_json`), and pushed through
//! `codegraph::generate::rosetta_expr::transpile`. Goldens are hand-written
//! Rust fragments per expression family.

use std::collections::HashSet;

use sigil_model::SemanticElement;

/// A trades corpus covering one transpilable condition per family plus the
/// unsupported kinds. Tabs are significant (rosetta fixtures use them);
/// optional fields are `(0..1)`.
const CORPUS: &str = r#"
namespace cond.tx
version "1.0.0"

type Party:
	partyId string (1..1)
	name string (1..1)

enum TradeStatus:
	DRAFT
	SETTLED

type LineItem:
	itemId string (1..1)
	quantity int (1..1)

type Trade:
	tradeId string (1..1)
	price number (1..1)
	quantity int (1..1)
	flagged boolean (1..1)
	memo string (0..1)
	settledOn date (0..1)
	lines LineItem (2..5)
	aliases string (1..*)
	prices number (0..*)
	counts int (0..*)
	counterparty Party (1..1)
	status TradeStatus (1..1)

	condition PositiveNotional: price * quantity > 0
	condition SettlementReady: settledOn exists and memo is absent
	condition GateChecks: price >= 1.0 and quantity <= 100
	condition Labeled: tradeId = "URGENT"
	condition Flagged: flagged = True
	condition KnownAliases: aliases = ["a", "b"]
	condition LineCount: lines count > 2
	condition PartyName: counterparty -> name exists
	condition DeepChain: counterparty -> name ->> upper exists
	condition OptionalChain: settledOn -> year exists
	condition Tagged: aliases contains "sale"
	condition Graded: quantity switch 1 then True, default False
	condition Joined: aliases join ", "
	condition FallbackPrice: price default 0.0
	condition BigLines: lines filter [quantity > 10]
	condition FilterParam: lines filter l [l -> itemId = "x"]
	condition MappedPrices: prices extract p [p]
	condition SumPrices: prices sum
	condition SumCounts: counts sum
	condition MinPrice: prices min
	condition MaxCount: counts max
	condition MinFn: prices min p [p > 0.0]
	condition SortFn: prices sort a, b [a - b]
	condition ReduceFold: prices reduce a, b [a + b]
	condition ReduceInit: prices reduce [0.0]
	condition SwitchLit: quantity switch 1 then True, 2 then False, default True
	condition SwitchRef: status switch TradeStatus then "x", default "y"
	condition JoinNoSep: aliases join = "a"
	condition DisjointAlias: aliases disjoint ["a"]
	condition FirstPrice: prices first
	condition LastPrice: prices last
	condition CastStr: tradeId to-string
	condition CastNum: tradeId to-number
	condition CastInt: tradeId to-int
	condition CastDate: tradeId to-date
	condition CastEnum: tradeId to-enum TradeStatus
	condition IfElse: if price > 1.0 then prices else counts
	condition IfNoElse: if price > 1.0 then prices
	condition OnlyOne: price only exists
	condition SingleExists: prices single exists
	condition MultipleExists: prices multiple exists
	condition ThenBind: prices extract p [p] then item
"#;

/// Parse the corpus and return `(condition name, Expr::to_json payload)`
/// for every condition on `Trade` — the exact bytes the bridge stores.
fn trade_conditions() -> Vec<(String, serde_json::Value)> {
    let source = sigil_diag::SourceFile::new("cond/tx.rosetta".to_string(), CORPUS.to_string());
    let (unit, diagnostics) = sigil_syntax::parse(&source);
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == sigil_diag::Severity::Error)
        .map(|d| format!("{:?} at {:?} msg={}", d.code, d.span, d.message))
        .collect();
    assert!(errors.is_empty(), "corpus must parse cleanly: {errors:?}");
    let unit = unit.expect("syntax tree");
    let file = sigil_syntax::lower("cond/tx.rosetta", &unit);
    let resolution = sigil_resolve::resolve(vec![file]);

    let mut out = Vec::new();
    for f in &resolution.files {
        for element in &f.elements {
            if let SemanticElement::Data(data) = element {
                if data.name != "Trade" {
                    continue;
                }
                for condition in &data.conditions {
                    out.push((
                        condition.name.clone().unwrap_or_default(),
                        condition.expression.to_json(),
                    ));
                }
            }
        }
    }
    assert!(!out.is_empty(), "Trade conditions must resolve");
    out
}

fn payload_of(conditions: &[(String, serde_json::Value)], name: &str) -> serde_json::Value {
    conditions
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("condition {name} missing"))
        .1
        .clone()
}

fn ctx<'a>(optional: &'a HashSet<String>) -> codegraph::generate::rosetta_expr::ExprContext<'a> {
    // Field knowledge mirroring what the generator derives from the Trade
    // PropertyNodes: arrays vs number/integer-typed fields.
    static COLLECTIONS: std::sync::LazyLock<HashSet<String>> = std::sync::LazyLock::new(|| {
        HashSet::from(["lines", "aliases", "prices", "counts"].map(str::to_string))
    });
    static NUMERIC: std::sync::LazyLock<HashSet<String>> = std::sync::LazyLock::new(|| {
        HashSet::from(["price", "prices", "counts"].map(str::to_string))
    });
    static INTEGER: std::sync::LazyLock<HashSet<String>> =
        std::sync::LazyLock::new(|| HashSet::from(["quantity", "counts"].map(str::to_string)));
    codegraph::generate::rosetta_expr::ExprContext {
        receiver: "dto",
        optional_fields: optional,
        collection_fields: &COLLECTIONS,
        numeric_fields: &NUMERIC,
        integer_fields: &INTEGER,
    }
}

fn transpile_named(
    conditions: &[(String, serde_json::Value)],
    name: &str,
    optional: &HashSet<String>,
) -> Result<String, codegraph::generate::rosetta_expr::TranspileError> {
    codegraph::generate::rosetta_expr::transpile(&payload_of(conditions, name), &ctx(optional))
}

#[test]
fn transpilable_conditions_produce_expected_rust() {
    let conditions = trade_conditions();
    let optional = HashSet::from(["memo".to_string(), "settled_on".to_string()]);

    // Arithmetic + comparison (nested binary parenthesized).
    assert_eq!(
        transpile_named(&conditions, "PositiveNotional", &optional).unwrap(),
        "(dto.price * dto.quantity) > 0"
    );
    // exists / absent — the sanctioned way to touch optionals.
    assert_eq!(
        transpile_named(&conditions, "SettlementReady", &optional).unwrap(),
        "dto.settled_on.is_some() && dto.memo.is_none()"
    );
    // Logical chain over comparisons (Number literals verbatim: `1.0`).
    assert_eq!(
        transpile_named(&conditions, "GateChecks", &optional).unwrap(),
        "(dto.price >= 1.0) && (dto.quantity <= 100)"
    );
    // String literal.
    assert_eq!(
        transpile_named(&conditions, "Labeled", &optional).unwrap(),
        "dto.trade_id == \"URGENT\""
    );
    // Boolean literal.
    assert_eq!(
        transpile_named(&conditions, "Flagged", &optional).unwrap(),
        "dto.flagged == true"
    );
    // List literal.
    assert_eq!(
        transpile_named(&conditions, "KnownAliases", &optional).unwrap(),
        "dto.aliases == vec![\"a\", \"b\"]"
    );
    // Count on a required array.
    assert_eq!(
        transpile_named(&conditions, "LineCount", &optional).unwrap(),
        "dto.lines.len() > 2"
    );
    // Feature-call chain on a required field, sanctioned by exists.
    assert_eq!(
        transpile_named(&conditions, "PartyName", &optional).unwrap(),
        "dto.counterparty.name.is_some()"
    );
    // Deep feature call chained onto the feature call.
    assert_eq!(
        transpile_named(&conditions, "DeepChain", &optional).unwrap(),
        "dto.counterparty.name.upper.is_some()"
    );
}

#[test]
fn optional_receiver_chain_is_refused_even_under_exists() {
    let conditions = trade_conditions();
    let optional = HashSet::from(["memo".to_string(), "settled_on".to_string()]);

    // exists sanctions only a BARE optional symbol — a deep chain through
    // `settledOn` still needs the optionality resolved first.
    let err = transpile_named(&conditions, "OptionalChain", &optional)
        .expect_err("optional receiver chain refused");
    assert_eq!(err.kind, "FeatureCall");
    assert!(
        err.detail
            .contains("optional receiver requires exists-guard"),
        "{err}"
    );
    assert!(err.detail.contains("settled_on"), "{err}");
}

#[test]
fn unsupported_conditions_carry_the_right_kind() {
    let conditions = trade_conditions();
    let optional = HashSet::from(["memo".to_string(), "settled_on".to_string()]);

    // min with an inline lambda — documented gap.
    let err = transpile_named(&conditions, "MinFn", &optional).expect_err("min fn");
    assert_eq!(err.kind, "Min");
    assert!(err.detail.contains("documented gap"), "{err}");

    // sort — no std expression form.
    let err = transpile_named(&conditions, "SortFn", &optional).expect_err("sort");
    assert_eq!(err.kind, "Sort");
    assert!(err.detail.contains("no std expression form"), "{err}");

    // reduce, both forms — no init field in the serialization.
    for name in ["ReduceFold", "ReduceInit"] {
        let err = transpile_named(&conditions, name, &optional).expect_err(name);
        assert_eq!(err.kind, "Reduce", "{name}");
        assert!(err.detail.contains("documented gap"), "{name}: {err}");
    }

    // switch with a reference guard — enum/choice knowledge needed.
    let err = transpile_named(&conditions, "SwitchRef", &optional).expect_err("switch ref");
    assert_eq!(err.kind, "Switch");
    assert!(err.detail.contains("reference guard"), "{err}");

    // to-date — no date-time crate in codegraph-generate.
    let err = transpile_named(&conditions, "CastDate", &optional).expect_err("to-date");
    assert_eq!(err.kind, "ToDate");
    assert!(err.detail.contains("date-time library"), "{err}");

    // to-enum — codelist knowledge not in the context.
    let err = transpile_named(&conditions, "CastEnum", &optional).expect_err("to-enum");
    assert_eq!(err.kind, "ToEnum");
    assert!(err.detail.contains("codelist knowledge"), "{err}");
}

// ── slice 2: word binaries, aggregates, exists modifiers ────────────────

#[test]
fn word_binaries_transpile_with_collection_gating() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    // contains over a string array field.
    assert_eq!(
        transpile_named(&conditions, "Tagged", &optional).unwrap(),
        "dto.aliases.contains(&\"sale\")"
    );
    // disjoint: negated membership for every element.
    assert_eq!(
        transpile_named(&conditions, "DisjointAlias", &optional).unwrap(),
        "dto.aliases.iter().all(|x| !vec![\"a\"].contains(x))"
    );
    // default sanctions bare optional access.
    assert_eq!(
        transpile_named(&conditions, "FallbackPrice", &optional).unwrap(),
        "dto.price.unwrap_or(0.0)"
    );
}

#[test]
fn join_uses_explicit_or_default_separator() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    assert_eq!(
        transpile_named(&conditions, "Joined", &optional).unwrap(),
        "dto.aliases.join(\", \")"
    );
    // Separator-less join emits Rust's slice-join default (the stored
    // generated "" literal is deliberately not used).
    assert_eq!(
        transpile_named(&conditions, "JoinNoSep", &optional).unwrap(),
        "dto.aliases.join(\", \") == \"a\""
    );
}

#[test]
fn aggregates_gate_on_numeric_field_knowledge() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    assert_eq!(
        transpile_named(&conditions, "SumPrices", &optional).unwrap(),
        "dto.prices.iter().sum::<f64>()"
    );
    // Integer-typed root picks i64.
    assert_eq!(
        transpile_named(&conditions, "SumCounts", &optional).unwrap(),
        "dto.counts.iter().sum::<i64>()"
    );
    assert_eq!(
        transpile_named(&conditions, "MinPrice", &optional).unwrap(),
        "dto.prices.iter().copied().fold(f64::INFINITY, f64::min)"
    );
    assert_eq!(
        transpile_named(&conditions, "MaxCount", &optional).unwrap(),
        "dto.counts.iter().copied().fold(i64::MIN, i64::max)"
    );
}

#[test]
fn exists_cardinality_checks_collection_length() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    assert_eq!(
        transpile_named(&conditions, "SingleExists", &optional).unwrap(),
        "dto.prices.len() == 1"
    );
    assert_eq!(
        transpile_named(&conditions, "MultipleExists", &optional).unwrap(),
        "dto.prices.len() >= 2"
    );
}

#[test]
fn collection_op_fragments_stay_composable() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    assert_eq!(
        transpile_named(&conditions, "FirstPrice", &optional).unwrap(),
        "dto.prices.first()"
    );
    assert_eq!(
        transpile_named(&conditions, "LastPrice", &optional).unwrap(),
        "dto.prices.last()"
    );
}

// ── slice 3: lambdas, switch, conditional, casts, then ──────────────────

#[test]
fn filter_binds_implicit_and_explicit_lambda_params() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    // Implicit form: `item` is the element binding; bare non-param symbols
    // stay receiver-field accesses.
    assert_eq!(
        transpile_named(&conditions, "BigLines", &optional).unwrap(),
        "dto.lines.iter().filter(|item| dto.quantity > 10).collect::<Vec<_>>()"
    );
    // Explicit form: the parameter binds by name.
    assert_eq!(
        transpile_named(&conditions, "FilterParam", &optional).unwrap(),
        "dto.lines.iter().filter(|l| l.item_id == \"x\").collect::<Vec<_>>()"
    );
    // extract → map, collect-terminated like every functional fragment.
    assert_eq!(
        transpile_named(&conditions, "MappedPrices", &optional).unwrap(),
        "dto.prices.iter().map(|p| p).collect::<Vec<_>>()"
    );
}

#[test]
fn switch_literal_chain_and_conditional_expressions() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    assert_eq!(
        transpile_named(&conditions, "SwitchLit", &optional).unwrap(),
        "if dto.quantity == 1 { true } else if dto.quantity == 2 { false } else { true }"
    );
    // Slice-1 corpus condition now covered by the literal-guard chain.
    assert_eq!(
        transpile_named(&conditions, "Graded", &optional).unwrap(),
        "if dto.quantity == 1 { true } else { false }"
    );
    assert_eq!(
        transpile_named(&conditions, "IfElse", &optional).unwrap(),
        "if (dto.price > 1.0) { dto.prices } else { dto.counts }"
    );
    // full == false: the generated empty-list else is emitted faithfully.
    assert_eq!(
        transpile_named(&conditions, "IfNoElse", &optional).unwrap(),
        "if (dto.price > 1.0) { dto.prices } else { vec![] }"
    );
}

#[test]
fn value_casts_append_parse_and_to_string() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    assert_eq!(
        transpile_named(&conditions, "CastStr", &optional).unwrap(),
        "dto.trade_id.to_string()"
    );
    assert_eq!(
        transpile_named(&conditions, "CastNum", &optional).unwrap(),
        "dto.trade_id.parse::<f64>().ok()"
    );
    assert_eq!(
        transpile_named(&conditions, "CastInt", &optional).unwrap(),
        "dto.trade_id.parse::<i64>().ok()"
    );
}

#[test]
fn then_binds_the_argument_as_the_implicit_variable() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    // `prices extract p [p] then item` — the then-body's implicit variable
    // carries the extract fragment.
    assert_eq!(
        transpile_named(&conditions, "ThenBind", &optional).unwrap(),
        "dto.prices.iter().map(|p| p).collect::<Vec<_>>()"
    );
}

#[test]
fn only_exists_emits_the_exists_conjunction() {
    let conditions = trade_conditions();
    let optional = HashSet::new();

    assert_eq!(
        transpile_named(&conditions, "OnlyOne", &optional).unwrap(),
        "dto.price.is_some()"
    );
}
