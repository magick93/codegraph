//! The design-invariant verdict collector for the hurl contracts (DD5,
//! issue #666 slice 4e). Split from `hurl_contract.rs` by the #751 file-
//! length contract: the THREE-VERDICT evaluator (parse → decide → flip)
//! is one closed unit beside the fact builder the contract consumes.

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::SchemaNode;

use super::ContractSection;
use crate::error::Result;

// ── Design-invariant contract verdicts (DD5, issue #666 slice 4e) ──────
//
// The FreshFixtureGuard THREE-VERDICT model (the #663 precedent — never
// guess a status, a mis-pinned leg fails the suite as hard in either
// direction):
//
// - Denied — a literal comparison over the contract's OWN create/update
//   values provably fails (plain `&&` from the root, decided at
//   generation time) → the violating leg pins 409 + INVARIANT_VIOLATION.
// - Satisfied — every decided leaf holds and nothing is undecidable → no
//   leg (the create succeeds).
// - Unverifiable — any comparison the collector cannot decide (an absent
//   optional field, arithmetic, collections, `||` over an undecided
//   sibling) → the leg is OMITTED with the documented omission comment.
//   MIXED verdicts never mis-fire: one provably failing conjunct decides
//   the conjunction (Denied) even beside undecided siblings.

/// One leaf's decision against a concrete JSON value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LeafDecision {
    Holds,
    Violates,
    Undecided,
}

/// Fold one plan's authored expression over the contract's body values.
/// Returns the decision plus, when a violating leg is constructible, the
/// `(wire_key, value)` replacement that turns a satisfied leaf into a
/// violating one.
fn decide_invariant_plan(
    expression: &str,
    body: &serde_json::Value,
    fields: &std::collections::HashMap<String, crate::ddd::service_ops::FieldInfo>,
) -> (LeafDecision, Option<(String, serde_json::Value)>) {
    let parsed = phero_expr::parse(expression);
    let Some(expr) = parsed.ast.filter(|_| parsed.errors.is_empty()) else {
        return (LeafDecision::Undecided, None);
    };
    decide_expr(&expr, body, fields, false)
}

/// The recursive collector. `negated` flips a leaf's polarity (the
/// violating replacement follows: a NOT'd comparison that violates on the
/// body holds on the value that violates the bare comparison).
#[allow(clippy::only_used_in_recursion)]
fn decide_expr(
    expr: &phero_expr::Expr,
    body: &serde_json::Value,
    fields: &std::collections::HashMap<String, crate::ddd::service_ops::FieldInfo>,
    negated: bool,
) -> (LeafDecision, Option<(String, serde_json::Value)>) {
    use phero_expr::{BinOp, ExprKind, UnOp};
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => match op {
            BinOp::And => {
                let (l, mut flip) = decide_expr(lhs, body, fields, negated);
                let (r, rflip) = decide_expr(rhs, body, fields, negated);
                if flip.is_none() {
                    flip = rflip;
                }
                let decision = match (l, r) {
                    (LeafDecision::Violates, _) | (_, LeafDecision::Violates) => {
                        LeafDecision::Violates
                    }
                    (LeafDecision::Undecided, _) | (_, LeafDecision::Undecided) => {
                        LeafDecision::Undecided
                    }
                    (LeafDecision::Holds, LeafDecision::Holds) => LeafDecision::Holds,
                };
                (decision, flip)
            }
            BinOp::Or => {
                let (l, lflip) = decide_expr(lhs, body, fields, negated);
                let (r, rflip) = decide_expr(rhs, body, fields, negated);
                let decision = match (l, r) {
                    (LeafDecision::Holds, _) | (_, LeafDecision::Holds) => LeafDecision::Holds,
                    (LeafDecision::Undecided, _) | (_, LeafDecision::Undecided) => {
                        LeafDecision::Undecided
                    }
                    (LeafDecision::Violates, LeafDecision::Violates) => LeafDecision::Violates,
                };
                let flip = if lflip.is_some() { lflip } else { rflip };
                (decision, flip)
            }
            comparison
            @ (BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge) => {
                // A leaf: `field <op> literal` (either operand order).
                let (field_name, lit_expr, swapped) = match (&lhs.kind, &rhs.kind) {
                    (ExprKind::Name(n), other) if is_literal(other) => (n, other, false),
                    (other, ExprKind::Name(n)) if is_literal(other) => (n, other, true),
                    _ => return (LeafDecision::Undecided, None),
                };
                let Some(info) = fields.get(field_name) else {
                    return (LeafDecision::Undecided, None);
                };
                let wire_key = &info.rust_name;
                let Some(value) = body.get(wire_key) else {
                    // The contract seeds REQUIRED fields only — an absent
                    // field's fresh value is not decidable here.
                    return (LeafDecision::Undecided, None);
                };
                let Some(literal) = literal_value(lit_expr) else {
                    return (LeafDecision::Undecided, None);
                };
                let (cmp_holds, flipped_op) = compare(*comparison, value, &literal, swapped);
                let decision = if cmp_holds {
                    if negated {
                        LeafDecision::Violates
                    } else {
                        LeafDecision::Holds
                    }
                } else if negated {
                    LeafDecision::Holds
                } else {
                    LeafDecision::Violates
                };
                // The violating replacement (only constructed for a leaf
                // that currently HOLDS — the leg flips it): the value where
                // the EFFECTIVE comparison (after swap/negation) is false.
                let flip = if decision == LeafDecision::Holds {
                    Some((wire_key.clone(), violating_value(flipped_op, &literal)))
                } else {
                    None
                };
                (decision, flip)
            }
            _ => (LeafDecision::Undecided, None),
        },
        ExprKind::Unary { op, expr: inner } if *op == UnOp::Not => {
            decide_expr(inner, body, fields, !negated)
        }
        _ => (LeafDecision::Undecided, None),
    }
}

/// `true` for the literal kinds the collector compares.
fn is_literal(expr: &phero_expr::ExprKind) -> bool {
    use phero_expr::ExprKind;
    matches!(
        expr,
        ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_)
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Date { .. }
    )
}

/// The literal's JSON value (dates compare as their ISO strings).
fn literal_value(expr: &phero_expr::ExprKind) -> Option<serde_json::Value> {
    use phero_expr::ExprKind;
    Some(match expr {
        ExprKind::Int(v) => serde_json::Value::from(*v),
        ExprKind::Float(v) => serde_json::Value::from(*v),
        ExprKind::String(s) => serde_json::Value::from(s.clone()),
        ExprKind::Bool(b) => serde_json::Value::from(*b),
        ExprKind::Null => serde_json::Value::Null,
        ExprKind::Date { text, .. } => serde_json::Value::from(text.clone()),
        _ => return None,
    })
}

/// Whether `value <op> literal` holds, with the SWAPPED-and-NEGATED-
/// corrected operator returned for the violating-value computation.
/// `swapped` means the source read `literal <op> field` — the effective
/// comparison over (value, literal) mirrors the operator.
fn compare(
    op: phero_expr::BinOp,
    value: &serde_json::Value,
    literal: &serde_json::Value,
    swapped: bool,
) -> (bool, phero_expr::BinOp) {
    use phero_expr::BinOp;
    let effective = match (op, swapped) {
        (BinOp::Lt, true) => BinOp::Gt,
        (BinOp::Le, true) => BinOp::Ge,
        (BinOp::Gt, true) => BinOp::Lt,
        (BinOp::Ge, true) => BinOp::Le,
        (other, _) => other,
    };
    let ord = partial_cmp_json(value, literal);
    let holds = match effective {
        BinOp::Eq => matches!(ord, Some(std::cmp::Ordering::Equal)),
        BinOp::Ne => !matches!(ord, Some(std::cmp::Ordering::Equal)),
        BinOp::Lt => matches!(ord, Some(std::cmp::Ordering::Less)),
        BinOp::Le => !matches!(ord, Some(std::cmp::Ordering::Greater)),
        BinOp::Gt => matches!(ord, Some(std::cmp::Ordering::Greater)),
        BinOp::Ge => !matches!(ord, Some(std::cmp::Ordering::Less)),
        _ => false,
    };
    (holds, effective)
}

/// Numeric (Int/Float cross), string, and boolean partial ordering; dates
/// ride their ISO strings. `None` = incomparable (Undecided semantics via
/// `false` + the collector refusing to flip an incomparable leaf — a
/// flip-less Undecided never constructs a leg).
fn partial_cmp_json(a: &serde_json::Value, b: &serde_json::Value) -> Option<std::cmp::Ordering> {
    let num = |v: &serde_json::Value| -> Option<f64> {
        v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))
    };
    if let (Some(x), Some(y)) = (num(a), num(b)) {
        return x.partial_cmp(&y);
    }
    if let (Some(x), Some(y)) = (a.as_str(), b.as_str()) {
        return Some(x.cmp(y));
    }
    if let (Some(x), Some(y)) = (a.as_bool(), b.as_bool()) {
        return Some(x.cmp(&y));
    }
    if a.is_null() && b.is_null() {
        return Some(std::cmp::Ordering::Equal);
    }
    None
}

/// A value where `value <op> literal` is FALSE — the violating seed for a
/// satisfied leaf's contract leg.
fn violating_value(op: phero_expr::BinOp, literal: &serde_json::Value) -> serde_json::Value {
    use phero_expr::BinOp;
    let bump = |delta: f64| -> serde_json::Value {
        if let Some(i) = literal.as_i64() {
            serde_json::Value::from(i + delta as i64)
        } else if let Some(f) = literal.as_f64() {
            serde_json::Value::from(f + delta)
        } else {
            literal.clone()
        }
    };
    match op {
        // x > lit fails at lit (and below): step down.
        BinOp::Gt | BinOp::Ge => bump(-1.0),
        // x < lit fails at lit (and above): step up.
        BinOp::Lt | BinOp::Le => bump(1.0),
        BinOp::Eq => {
            if let Some(b) = literal.as_bool() {
                serde_json::Value::from(!b)
            } else if let Some(s) = literal.as_str() {
                serde_json::Value::from(format!("{s}-x"))
            } else {
                bump(1.0)
            }
        }
        // x != lit fails at lit itself.
        BinOp::Ne => literal.clone(),
        _ => literal.clone(),
    }
}

/// One entity's invariant contract facts (issue #666 slice 4e): the
/// violating-create sections (leg + omission comments, rendered after the
/// CREATE entry), the update-roundtrip denial verdict, and its comment.
pub(super) struct InvariantContractFacts {
    pub(super) create_sections: Vec<ContractSection>,
    pub(super) update_denied: bool,
    pub(super) update_note: Option<ContractSection>,
}

/// Resolve the invariant contract facts for one entity: evaluate every
/// plan against the contract's own create values (and the update values
/// over the create candidate), the THREE-VERDICT model deciding each leg.
/// Silent (empty facts) for entities without invariants. The violating
/// leg posts the contract's OWN create URL (nested routes included —
/// `hurl == router == utoipa`).
pub(super) async fn invariant_contract_facts(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema: &SchemaNode,
    create_url: &str,
    create_body: &str,
    update_body: &str,
) -> Result<InvariantContractFacts> {
    let mut facts = InvariantContractFacts {
        create_sections: Vec::new(),
        update_denied: false,
        update_note: None,
    };
    let enforcement =
        crate::ddd::invariants::enforcement_for(db, config, domain, &schema.title).await;
    if enforcement.is_empty() {
        return Ok(facts);
    }
    let Ok(create_json) = serde_json::from_str::<serde_json::Value>(create_body) else {
        return Ok(facts);
    };
    let props = db.get_properties(&schema.title).await?;
    let fields = crate::ddd::service_ops::fields_for(&props);

    // CREATE verdict: one violating leg names the FIRST plan whose
    // decision is provable (a decided conjunct decides the conjunction);
    // a plan the collector cannot decide records the documented omission.
    let mut leg_body: Option<String> = None;
    let mut leg_note: Option<String> = None;
    let mut undecided: Vec<String> = Vec::new();
    for plan in &enforcement.plans {
        let (decision, flip) = decide_invariant_plan(&plan.expression, &create_json, &fields);
        match decision {
            LeafDecision::Violates => {
                // The contract's own values already violate — the leg
                // posts them verbatim.
                leg_body = Some(create_body.to_string());
                leg_note = Some(format!(
                    "invariant {} of `{}` (`{}`) fails on the contract's own create values",
                    plan.index, enforcement.class, plan.expression
                ));
                break;
            }
            LeafDecision::Holds => {
                if let Some((key, value)) = flip
                    && leg_body.is_none()
                {
                    // Flip ONE satisfied leaf into its violating seed.
                    let mut candidate = create_json.clone();
                    if let Some(obj) = candidate.as_object_mut()
                        && obj.insert(key.clone(), value.clone()).is_some()
                    {
                        leg_body =
                            Some(serde_json::to_string_pretty(&candidate).unwrap_or_default());
                        leg_note = Some(format!(
                            "invariant {} of `{}` (`{}`): `{}` seeded to a value the \
                             comparison provably rejects",
                            plan.index, enforcement.class, plan.expression, key
                        ));
                    }
                }
            }
            LeafDecision::Undecided => {
                undecided.push(format!(
                    "invariant {} of `{}` (`{}`) is not provable from the contract's \
                     own create body (an optional field is absent, or the expression \
                     uses a construct outside the collector's decided subset) — no \
                     violating leg is emitted",
                    plan.index, enforcement.class, plan.expression
                ));
            }
        }
    }
    if let Some(body) = leg_body {
        facts.create_sections.push(ContractSection {
            body: format!(
                "# Design invariant denial (DD5, issue #666 slice 4e) — {note}.\n\
                 # Three-verdict model: this leg is emitted ONLY because the denial\n\
                 # is decidable from the contract's own values (the #663 posture).\n\
                 POST {create_url}\n\
                 Authorization: Bearer {{{{api_key}}}}\n\
                 {body}\n\
                 \n\
                 HTTP 409\n\
                 [Asserts]\n\
                 jsonpath \"$.error.code\" == \"INVARIANT_VIOLATION\"",
                note = leg_note.unwrap_or_default(),
                body = body,
                create_url = create_url,
            ),
        });
    }
    for note in undecided {
        facts.create_sections.push(ContractSection {
            body: format!(
                "# Design invariant omission (DD5, issue #666 slice 4e): {note}\n\
                 # (the `Body {{ may_refuse }}` precedent — never a guessed status)."
            ),
        });
    }

    // UPDATE verdict: the candidate is the create values with the update
    // values applied (the PUT roundtrip replaces every seeded field). A
    // provable violation flips the roundtrip's expectation to the 409 the
    // handler answers pre-write.
    if let Ok(update_json) = serde_json::from_str::<serde_json::Value>(update_body) {
        let mut candidate = create_json.clone();
        if let (Some(obj), Some(patch)) = (candidate.as_object_mut(), update_json.as_object()) {
            for (k, v) in patch {
                obj.insert(k.clone(), v.clone());
            }
        }
        for plan in &enforcement.plans {
            let (decision, _) = decide_invariant_plan(&plan.expression, &candidate, &fields);
            if decision == LeafDecision::Violates {
                facts.update_denied = true;
                facts.update_note = Some(ContractSection {
                    body: format!(
                        "# Design invariant denial (DD5, issue #666 slice 4e): the update\n\
                         # roundtrip's own values violate invariant {} of `{}` (`{}`) — the\n\
                         # PUT below pins the 409 the handler answers BEFORE the write.",
                        plan.index, enforcement.class, plan.expression
                    ),
                });
                break;
            }
        }
    }
    Ok(facts)
}
