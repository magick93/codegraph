//! WP1.6 — `Expr::to_json` stability + expression-typing scope probes
//! (issue #254; findings in `docs/rosetta/findings/wp1.6-expr-json-typing.md`).
//!
//! Questions:
//! (a) Is `Expr::to_json` output stable enough to embed as the condition
//!     payload of a future ConditionNode/RuleNode?
//! (b) Does sigil type-check or interpret expressions? Expected: NO —
//!     parse + name resolution only; the #262 transpiler must scope that
//!     honestly.
//!
//! Characterization only: if a probe fails against the pinned behavior,
//! the surprise is the finding.

use std::collections::BTreeSet;

use sigil_diag::{Diagnostic, Severity, SourceFile};
use sigil_model::expr::Expr;
use sigil_model::{ModelFile, SemanticElement};

const FAMILIES_MODEL_NAME: &str = "families_probe.rosetta";

/// One type's conditions plus a func and two rules, collectively exercising
/// the Expr variant families. Syntax mirrors the sigil oracle fixtures
/// (`tests/oracle/expressions.rosetta`, `functions.rosetta`); the model is
/// embedded so the probe carries its own source of truth.
const FAMILIES_MODEL: &str = r#"namespace probe.families

version "1.0.0"

enum Quotation:
	Ask
	Bid displayName "Bid Price"

type Trade:
	price number (1..1)
	quantity number (1..1)
	label string (1..1)
	tags string (0..*)
	prices number (0..*)

	condition IntLiteral: 42
	condition NegativeIntLiteral: -7
	condition NumberLiteral: 1.50
	condition StringLiteral: "hello"
	condition BooleanTrue: True
	condition EmptyListLiteral: []
	condition IntListLiteral: [1, 2, 3]
	condition SymbolReference: price
	condition QualifiedSymbol: Trade.price
	condition FeatureCall: price -> label
	condition DeepFeatureCall: prices ->> label
	condition Equality: price = 1.0
	condition NotEquality: price <> 2.0
	condition Comparison: price >= 1.0
	condition Additive: price + quantity = 3.0
	condition Multiplicative: price * quantity + 1.0 = 3.0
	condition LogicalAnd: price = 1.0 and quantity = 2.0
	condition LogicalOr: price = 1.0 or quantity = 2.0
	condition Contains: prices contains 1.0
	condition DefaultOp: price default 0.0 = 0.0
	condition JoinSep: tags join ", " = "a, b"
	condition Exists: price exists
	condition SingleExists: prices single exists
	condition IsAbsent: price is absent
	condition OnlyElement: prices only-element = 1.0
	condition Count: prices count = 1
	condition Flatten: prices flatten = [1.0]
	condition Distinct: prices distinct = [1.0]
	condition Reverse: prices reverse = [1.0]
	condition First: prices first = 1.0
	condition Last: prices last = 1.0
	condition Sum: prices sum = 1.0
	condition FilterExplicit: prices filter p [p > 1.0] = [2.0]
	condition ExtractImplicit: prices extract price + 1.0
	condition ReduceExplicit: prices reduce a, b [a + b] = 3.0
	condition SortFn: prices sort a, b [a - b]
	condition MinFn: prices min p [p > 0.0]
	condition MaxFn: prices max p [p < 10.0]
	condition OneOf: label one-of
	condition ToString: price to-string = "1.0"
	condition ToNumber: label to-number = 1.0
	condition ToInt: label to-int = 1
	condition ToEnum: label to-enum Quotation = Quotation -> Ask
	condition SwitchLiteral: price switch 1 then "a", 2.5 then "b", default "c"
	condition ConditionalElse: if price > 1.0 then prices else [0.0]
	condition ConditionalNoElse: if price > 1.0 then prices
	condition OnlyExistsPaths: (price, quantity) only exists
	condition Constructor: Trade { price: 1.0, quantity: 2.0 }
	condition Disjoint: prices disjoint [1.0, 2.0]
	condition ToDate: label to-date
	condition ToTime: label to-time
	condition ToDateTime: label to-date-time
	condition ToZonedDateTime: label to-zoned-date-time
	condition WithMeta: price with-meta { scheme: "S" }
	condition AsNumber: price as number

func Quote:
	inputs:
		trade Trade (1..1)
	output:
		result number (1..1)

	alias base: trade -> price
	alias deep: trade ->> prices

	condition InputsPresent:
		trade exists and base exists

	set result:
		trade -> price

	add result:
		trade -> quantity * 2.0

	post-condition ResultPresent:
		result exists

eligibility rule IsEligible from Trade:
	price exists and quantity exists

reporting rule LabeledTags from Trade:
	tags
		filter l [l <> ""]
		then join ", "
"#;

const ILLTYPED_MODEL_NAME: &str = "illtyped_probe.rosetta";

/// Deliberately ill-typed conditions (arithmetic mixing number and string,
/// a string compared to a number) next to one unresolvable symbol head.
/// If sigil typed expressions the first three would be diagnostics; the
/// fourth is the control proving resolution is at least name-aware.
const ILLTYPED_MODEL: &str = r#"namespace probe.illtyped

type Widget:
	amount number (1..1)
	name string (1..1)

	condition MixedArithmetic: amount + name = 1.0
	condition LiteralPlusString: amount + "x" = 1
	condition CompareStringToNumber: name > 5
	condition UnknownHead: bogus + 1.0 = 1.0
"#;

fn parse_lower(source_name: &str, text: &str) -> (ModelFile, Vec<Diagnostic>) {
    let source = SourceFile::new(source_name, text);
    let (unit, diags) = sigil_syntax::parse(&source);
    let unit = unit.unwrap_or_else(|| panic!("parse produced no unit: {diags:?}"));
    (sigil_syntax::lower(source_name, &unit), diags)
}

/// Visit every Expr node (DFS pre-order) hanging off the model: Data
/// conditions, Function conditions/post-conditions/shortcuts/operations,
/// and Rule expressions (sigil-model lib.rs Condition :180, Function :324,
/// Rule :443).
fn walk_model_exprs<F: FnMut(&mut Expr)>(files: &mut [ModelFile], visit: &mut F) {
    fn recurse<F: FnMut(&mut Expr)>(expr: &mut Expr, visit: &mut F) {
        visit(expr);
        // The crate's own visitor drives the descent (sigil-model
        // expr.rs:401 for_each_child_mut).
        expr.for_each_child_mut(&mut |child| recurse(child, visit));
    }
    for file in files.iter_mut() {
        for element in file.elements.iter_mut() {
            match element {
                SemanticElement::Data(d) => {
                    for c in d.conditions.iter_mut() {
                        recurse(&mut c.expression, visit);
                    }
                }
                SemanticElement::Function(f) => {
                    for c in f.conditions.iter_mut().chain(f.post_conditions.iter_mut()) {
                        recurse(&mut c.expression, visit);
                    }
                    for s in f.shortcuts.iter_mut() {
                        recurse(&mut s.expression, visit);
                    }
                    for o in f.operations.iter_mut() {
                        recurse(&mut o.expression, visit);
                    }
                }
                SemanticElement::Rule(r) => recurse(&mut r.expression, visit),
                _ => {}
            }
        }
    }
}

/// Variant identity as recoverable from the JSON shape: the `kind` tag,
/// refined by `op` for the seven Rust variants that all collapse to
/// `"kind": "Binary"` (arithmetic/logical/equality/comparison/contains/
/// disjoint/default).
fn variant_key(json: &serde_json::Value) -> String {
    let kind = json["kind"].as_str().unwrap_or("<no kind>");
    match json["op"].as_str() {
        Some(op) => format!("{kind}({op})"),
        None => kind.to_string(),
    }
}

/// Probe 1 — parse + lower + resolve a model whose expressions collectively
/// cover the Expr variant families; assert clean resolution and collect
/// every node's `to_json()`.
#[test]
fn parse_and_resolve_exercises_expression_families() {
    let (model, syntax) = parse_lower(FAMILIES_MODEL_NAME, FAMILIES_MODEL);
    assert!(syntax.is_empty(), "syntax diagnostics: {syntax:?}");

    let mut resolution = sigil_resolve::resolve(vec![model]);
    let errors: Vec<String> = resolution
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{:?} {} {}: {}", d.severity, d.code, d.path, d.message))
        .collect();
    assert!(errors.is_empty(), "resolution errors: {errors:?}");

    let mut exprs = Vec::new();
    walk_model_exprs(&mut resolution.files, &mut |e| exprs.push(e.to_json()));
    assert!(
        exprs.len() >= 100,
        "fixture shrank: only {} expr nodes collected",
        exprs.len()
    );

    let keys: BTreeSet<String> = exprs.iter().map(variant_key).collect();
    println!("distinct variant keys ({}): {keys:?}", keys.len());
    assert!(
        keys.len() >= 8,
        "expected >= 8 distinct variant families, got {keys:?}"
    );
    let required = [
        // literals
        "Int",
        "Number",
        "String",
        "Boolean",
        "List",
        // references / feature calls
        "SymbolReference",
        "FeatureCall",
        "DeepFeatureCall",
        // arithmetic
        "Binary(+)",
        "Binary(*)",
        // equality / comparison / word operators
        "Binary(=)",
        "Binary(<>)",
        "Binary(>=)",
        "Binary(contains)",
        "Binary(default)",
        // logical
        "Binary(and)",
        "Binary(or)",
        // exists / quantifier
        "Exists",
        "Absent",
        "OnlyExists",
        // collection algebra
        "Count",
        "Flatten",
        "Distinct",
        "Reverse",
        "First",
        "Last",
        "Sum",
        "Filter",
        "Map",
        "Reduce",
        "Sort",
        "Min",
        "Max",
        // conditional / dispatch
        "Conditional",
        "Switch",
        // misc
        "Join",
        "OneOf",
        "ToString",
        "ToNumber",
        "ToInt",
        "ToEnum",
        "Constructor",
        "Then",
        "Binary(disjoint)",
        "ToDate",
        "ToDateTime",
        "ToZonedDateTime",
        "WithMeta",
        "As",
    ];
    for key in required {
        assert!(keys.contains(key), "missing family {key}; have {keys:?}");
    }
}

/// Probe 2 — the embedding contract: `to_json()` is deterministic per node
/// and its Values survive a serde_json string round-trip byte-identically.
#[test]
fn to_json_is_deterministic_and_serialization_stable() {
    let (model, syntax) = parse_lower(FAMILIES_MODEL_NAME, FAMILIES_MODEL);
    assert!(syntax.is_empty(), "syntax diagnostics: {syntax:?}");
    let mut resolution = sigil_resolve::resolve(vec![model]);

    let mut pairs = Vec::new();
    walk_model_exprs(&mut resolution.files, &mut |e| {
        pairs.push((e.to_json(), e.to_json()))
    });
    assert!(!pairs.is_empty());
    for (first, second) in &pairs {
        assert_eq!(first, second, "to_json() not deterministic for {first}");
    }

    // Serialization round-trip: Value -> string -> Value -> string must be
    // byte-stable for every collected node.
    for (v, _) in &pairs {
        let s1 = serde_json::to_string(v).unwrap();
        let back: serde_json::Value = serde_json::from_str(&s1).unwrap();
        let s2 = serde_json::to_string(&back).unwrap();
        assert_eq!(s1, s2, "serialization round-trip drifted for {v}");
    }

    // The manual Serialize impl (sigil-model expr.rs:391) delegates to
    // to_json, so serializing the Expr directly yields byte-identical
    // output to serializing its to_json() Value.
    let mut delegated = 0usize;
    for file in resolution.user_files() {
        for element in &file.elements {
            if let SemanticElement::Data(d) = element {
                for c in &d.conditions {
                    let via_expr = serde_json::to_string(&c.expression).unwrap();
                    let via_value = serde_json::to_string(&c.expression.to_json()).unwrap();
                    assert_eq!(via_expr, via_value, "Serialize/to_json drift");
                    delegated += 1;
                }
            }
        }
    }
    assert!(delegated >= 40);

    // Sample of the pinned wire shape: an arithmetic Binary and a literal
    // (note the alphabetized object keys).
    let sample = pairs
        .iter()
        .map(|(v, _)| v)
        .find(|v| variant_key(v) == "Binary(+)")
        .unwrap();
    println!(
        "example to_json (Binary(+)): {}",
        serde_json::to_string(sample).unwrap()
    );
    println!(
        "example to_json (Int): {}",
        serde_json::to_string(&pairs[0].0).unwrap()
    );
}

/// Probe 3 — resolve() does NOT type-check (or interpret) expressions.
/// Three ill-typed conditions parse and resolve silently; the only
/// diagnostic is the unknown-symbol control, proving resolution checks
/// names, not types.
#[test]
fn resolve_does_not_typecheck_expressions() {
    let (model, syntax) = parse_lower(ILLTYPED_MODEL_NAME, ILLTYPED_MODEL);
    // Characterization: the untyped grammar ACCEPTS arithmetic mixing a
    // number attribute with a string attribute/literal and a string
    // compared to an int. (If this ever fails, the parser grew syntax-level
    // type gating — update the finding.)
    assert!(
        syntax.is_empty(),
        "syntax rejected the ill-typed expressions: {syntax:?}"
    );

    let resolution = sigil_resolve::resolve(vec![model]);
    let described: Vec<String> = resolution
        .diagnostics
        .iter()
        .map(|d| format!("{:?} {} {}: {}", d.severity, d.code, d.path, d.message))
        .collect();

    // Exactly ONE diagnostic — the deliberate unknown symbol — and it is a
    // name-resolution error, not a type error. `amount + name`,
    // `amount + "x"` and `name > 5` produce nothing at all.
    assert_eq!(described.len(), 1, "diagnostics: {described:?}");
    let d = &resolution.diagnostics[0];
    assert_eq!(d.code, "E0101");
    assert_eq!(d.severity, Severity::Error);
    assert!(
        d.message.contains("unknown symbol 'bogus'"),
        "unexpected message: {}",
        d.message
    );
    assert!(
        d.path.contains("UnknownHead"),
        "diagnostic path: {}",
        d.path
    );
}

/// Probe 4 — `canonical_json` embeds expressions losslessly: each condition
/// carries the exact `Expr::to_json()` Value (byte-identical), ill-typed or
/// not, and diagnostics ride along with resolved source positions.
#[test]
fn canonical_json_embds_expressions_losslessly() {
    let (model, syntax) = parse_lower(ILLTYPED_MODEL_NAME, ILLTYPED_MODEL);
    assert!(syntax.is_empty(), "syntax diagnostics: {syntax:?}");
    let resolution = sigil_resolve::resolve(vec![model]);

    // Model-side truth: Widget's four condition expressions.
    let widget = resolution.user_files()[0]
        .elements
        .iter()
        .find_map(|e| match e {
            SemanticElement::Data(d) if d.name == "Widget" => Some(d),
            _ => None,
        })
        .unwrap();
    let model_exprs: Vec<serde_json::Value> = widget
        .conditions
        .iter()
        .map(|c| c.expression.to_json())
        .collect();
    assert_eq!(model_exprs.len(), 4);

    let sources = vec![(ILLTYPED_MODEL_NAME.to_string(), ILLTYPED_MODEL.to_string())];
    let canonical = sigil_resolve::canonical_json(&resolution, &sources);

    let mut canonical_exprs = Vec::new();
    for file in canonical["files"].as_array().unwrap() {
        for element in file["elements"].as_array().unwrap() {
            if element["name"] == "Widget" {
                for c in element["conditions"].as_array().unwrap() {
                    canonical_exprs.push(c["expression"].clone());
                }
            }
        }
    }
    assert_eq!(
        canonical_exprs.len(),
        4,
        "Widget conditions missing from canonical output"
    );

    // Lossless: the canonical payload IS Expr::to_json(), structurally and
    // byte-for-byte — including the ill-typed Binary (number + "x").
    for (i, expr) in model_exprs.iter().enumerate() {
        assert_eq!(
            canonical_exprs[i], *expr,
            "canonical condition {i} drifts from Expr::to_json"
        );
    }
    let embedded = serde_json::to_string(&canonical_exprs[1]).unwrap();
    let direct = serde_json::to_string(&model_exprs[1]).unwrap();
    assert_eq!(embedded, direct);
    assert!(embedded.contains(r#""kind":"Binary""#), "{embedded}");
    assert!(embedded.contains(r#""kind":"String""#), "{embedded}");

    // Diagnostics ride along next to the (unchecked) expressions.
    // Characterization: expression-head E0101s are pushed WITHOUT a span
    // (sigil-resolve lib.rs resolve_head), so canonical_json renders
    // "span": null / "location": null — the bridge cannot point at the
    // offending expression head from resolution output alone; only file +
    // model path identify it.
    let diags = canonical["diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "E0101");
    assert_eq!(diags[0]["severity"], "error");
    assert_eq!(diags[0]["file"], ILLTYPED_MODEL_NAME);
    assert!(
        diags[0]["path"].as_str().unwrap().contains("UnknownHead"),
        "path: {}",
        diags[0]["path"]
    );
    assert!(diags[0]["span"].is_null(), "{:?}", diags[0]["span"]);
    assert!(diags[0]["location"].is_null());
}
