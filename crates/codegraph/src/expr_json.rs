//! Canonical AST JSON (`expr_json`) for rexlang `when` expressions.
//!
//! `GrantEdge.when` stores rexlang expression SOURCE text. Issue #278 adds
//! `GrantEdge.expr_json`: the same expression as canonical, tagged JSON,
//! produced ONCE at ingest by parsing the source with `rex-expr`
//! (parse-only — no model, no typechecking, mirroring `db/expr_sql.rs`).
//!
//! This is the wire contract consumed by lowering backends
//! (`codegraph-generate::ifml::expr_ts`). Every `ExprKind` variant has a
//! lossless encoding, so canonicalization covers the full grammar; the
//! closed-subset refusals belong to the lowerings, not here.
//!
//! | kind             | payload                                            |
//! |------------------|----------------------------------------------------|
//! | `int`            | `value`                                            |
//! | `string`         | `value` (unescaped)                                |
//! | `bool`           | `value`                                            |
//! | `null`           | —                                                  |
//! | `date`           | `text` (calendar date as written)                  |
//! | `name`           | `name`                                             |
//! | `feature_access` | `receiver`, `name`, `optional_safe`                |
//! | `coalesce`       | `value`, `default`                                 |
//! | `call`           | `receiver`, `name`, `args`, `optional_safe`        |
//! | `algebra`        | `op` (first/filter/map/any/size/sum), `receiver`,  |
//! |                  | `lambda` (`{param, body}` or null), `optional_safe`|
//! | `binary`         | `op` (eq/ne/lt/le/gt/ge/add/sub/mul/div/and/or),   |
//! |                  | `lhs`, `rhs`                                       |
//! | `unary`          | `op` (`not`/`neg`), `expr`                         |
//! | `if`             | `cond`, `then`, `else`                             |
//! | `let`            | `name`, `init`, `body`                             |
//! | `lambda`         | `param`, `body`                                    |
//! | `list`           | `items`                                            |
//!
//! Ingest never fails on an expression: a source that does not parse (or a
//! payload that fails to serialize) yields `None` and the carrier keeps its
//! source string.

use rex_expr::{AlgebraKind, BinOp, Expr, ExprKind, UnOp};
use serde_json::{json, Value};

/// Canonical `expr_json` for a rexlang expression source: `None` when the
/// source is absent, does not parse, or fails to serialize.
pub fn when_expr_json(source: Option<&str>) -> Option<String> {
    let source = source?;
    let parsed = rex_expr::parse(source);
    let ast = parsed.ast.filter(|_| parsed.errors.is_empty())?;
    serde_json::to_string(&expr_value(&ast)).ok()
}

fn expr_value(expr: &Expr) -> Value {
    match &expr.kind {
        ExprKind::Int(value) => json!({"kind": "int", "value": value}),
        ExprKind::String(value) => json!({"kind": "string", "value": value}),
        ExprKind::Bool(value) => json!({"kind": "bool", "value": value}),
        ExprKind::Null => json!({"kind": "null"}),
        ExprKind::Date { text, .. } => json!({"kind": "date", "text": text}),
        ExprKind::Name(name) => json!({"kind": "name", "name": name}),
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => json!({
            "kind": "feature_access",
            "receiver": expr_value(receiver),
            "name": name.value,
            "optional_safe": optional_safe,
        }),
        ExprKind::Coalesce { value, default } => json!({
            "kind": "coalesce",
            "value": expr_value(value),
            "default": expr_value(default),
        }),
        ExprKind::Call {
            receiver,
            name,
            args,
            optional_safe,
        } => json!({
            "kind": "call",
            "receiver": expr_value(receiver),
            "name": name.value,
            "args": args.iter().map(expr_value).collect::<Vec<_>>(),
            "optional_safe": optional_safe,
        }),
        ExprKind::Algebra {
            receiver,
            kind,
            lambda,
            optional_safe,
        } => json!({
            "kind": "algebra",
            "op": kind.to_string(),
            "receiver": expr_value(receiver),
            "lambda": lambda.as_ref().map(|(param, body)| json!({
                "param": param,
                "body": expr_value(body),
            })),
            "optional_safe": optional_safe,
        }),
        ExprKind::Binary { op, lhs, rhs } => json!({
            "kind": "binary",
            "op": bin_op(op),
            "lhs": expr_value(lhs),
            "rhs": expr_value(rhs),
        }),
        ExprKind::Unary { op, expr } => json!({
            "kind": "unary",
            "op": match op {
                UnOp::Not => "not",
                UnOp::Neg => "neg",
            },
            "expr": expr_value(expr),
        }),
        ExprKind::If { cond, then, else_ } => json!({
            "kind": "if",
            "cond": expr_value(cond),
            "then": expr_value(then),
            "else": expr_value(else_),
        }),
        ExprKind::Let { name, init, body } => json!({
            "kind": "let",
            "name": name.value,
            "init": expr_value(init),
            "body": expr_value(body),
        }),
        ExprKind::Lambda { param, body } => json!({
            "kind": "lambda",
            "param": param,
            "body": expr_value(body),
        }),
        ExprKind::ListLiteral(items) => json!({
            "kind": "list",
            "items": items.iter().map(expr_value).collect::<Vec<_>>(),
        }),
    }
}

fn bin_op(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

/// The algebra op name rex-expr reports for `kind`.
#[allow(dead_code)]
fn algebra_op(kind: &AlgebraKind) -> &'static str {
    match kind {
        AlgebraKind::First => "first",
        AlgebraKind::Filter => "filter",
        AlgebraKind::Map => "map",
        AlgebraKind::Any => "any",
        AlgebraKind::Size => "size",
        AlgebraKind::Sum => "sum",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_shape_is_stable() {
        let json = when_expr_json(Some("amount > 0 && !internal")).expect("parses");
        let payload: Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(payload["kind"], "binary");
        assert_eq!(payload["op"], "and");
        assert_eq!(payload["lhs"]["op"], "gt");
        assert_eq!(
            payload["lhs"]["lhs"],
            json!({"kind": "name", "name": "amount"})
        );
        assert_eq!(payload["rhs"]["op"], "not");
    }

    #[test]
    fn date_and_null_shapes_are_stable() {
        let payload: Value =
            serde_json::from_str(&when_expr_json(Some("start_date == null")).expect("parses"))
                .expect("valid JSON");
        assert_eq!(payload["rhs"]["kind"], "null");

        let payload: Value = serde_json::from_str(
            &when_expr_json(Some("start_date >= date(\"2026-01-01\")")).expect("parses"),
        )
        .expect("valid JSON");
        assert_eq!(
            payload["lhs"],
            json!({"kind": "name", "name": "start_date"})
        );
        assert_eq!(payload["rhs"]["kind"], "date");
        assert_eq!(payload["rhs"]["text"], "2026-01-01");
    }

    #[test]
    fn parse_failure_yields_none() {
        assert_eq!(when_expr_json(Some("amount ====")), None);
        assert_eq!(when_expr_json(None), None);
    }
}
