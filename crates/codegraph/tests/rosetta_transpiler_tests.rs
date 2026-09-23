//! expr→Rust transpiler conformance tests (issue #262, slice 1).
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
/// slice-1 unsupported kinds. Tabs are significant (rosetta fixtures use
/// them); optional fields are `(0..1)`.
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
    codegraph::generate::rosetta_expr::ExprContext {
        receiver: "dto",
        optional_fields: optional,
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
    let optional = HashSet::new();

    // contains → Binary (slice 2).
    let err = transpile_named(&conditions, "Tagged", &optional).expect_err("contains");
    assert_eq!(err.kind, "Binary");
    assert!(err.detail.contains("contains"), "{err}");

    // switch → Switch.
    let err = transpile_named(&conditions, "Graded", &optional).expect_err("switch");
    assert_eq!(err.kind, "Switch");

    // join → Join.
    let err = transpile_named(&conditions, "Joined", &optional).expect_err("join");
    assert_eq!(err.kind, "Join");

    // default → Binary (slice 2).
    let err = transpile_named(&conditions, "FallbackPrice", &optional).expect_err("default");
    assert_eq!(err.kind, "Binary");
    assert!(err.detail.contains("default"), "{err}");

    // filter → Filter (functional family, slice 3).
    let err = transpile_named(&conditions, "BigLines", &optional).expect_err("filter");
    assert_eq!(err.kind, "Filter");
}
