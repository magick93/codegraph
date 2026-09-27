//! Lowering of canonical expression AST JSON to TypeScript (issue #278).
//!
//! Mirrors `crate::db::expr_sql` (the rexlang `when` → Postgres lowering)
//! with the same **closed subset** philosophy: what cannot be mapped
//! faithfully to TypeScript is a hard, named error — never a silent
//! passthrough. Two carriers are supported:
//!
//! - [`lower_when_json`]: the canonical rex-expr AST JSON persisted on
//!   `GrantEdge.expr_json` (produced by `codegraph::expr_json`).
//! - [`lower_ifml_json`]: the typed IFML `Expression` wire JSON persisted
//!   on the four IFML conditional carriers (`expr_json`).
//!
//! | rexlang / IFML              | TypeScript                     |
//! |-----------------------------|--------------------------------|
//! | `==` and `=`                | `===`                          |
//! | `!=`                        | `!==`                          |
//! | `<`, `<=`, `>`, `>=`        | `<`, `<=`, `>`, `>=`           |
//! | `&&`, `\|\|`, `!`           | `&&`, `||`, `!`                |
//! | `field == null` / `!= null` | `field === null` / `!== null`  |
//! | `date("YYYY-MM-DD")`        | `new Date("YYYY-MM-DD")`       |
//! | string/number/bool literals | literals                       |
//! | field refs / dotted paths   | identifiers / property access  |
//!
//! Refused: arithmetic, `?.`, `?:`, `let`, lambdas, collection algebra,
//! `if`, operation calls, list literals, and the IFML regex operators
//! (`~=` / `!~`).

/// A lowering refusal: `owner` names the carrier (view, component, event,
/// or capability) and `message` names the unsupported construct.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{owner}: {message}")]
pub struct ExprTsError {
    pub owner: String,
    pub message: String,
}

impl ExprTsError {
    fn new(owner: &str, message: impl Into<String>) -> Self {
        Self {
            owner: owner.to_string(),
            message: message.into(),
        }
    }
}

/// Lower a `GrantEdge.expr_json` payload (canonical rex-expr AST JSON) to
/// a TypeScript expression.
pub fn lower_when_json(_expr_json: &str, owner: &str) -> Result<String, ExprTsError> {
    Err(ExprTsError::new(owner, "lowering not implemented"))
}

/// Lower an IFML `conditional_expression` `expr_json` payload (the typed
/// `rex_ifml::Expression` wire JSON) to a TypeScript expression.
pub fn lower_ifml_json(_expr_json: &str, owner: &str) -> Result<String, ExprTsError> {
    Err(ExprTsError::new(owner, "lowering not implemented"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn when_json(value: Value) -> String {
        value.to_string()
    }

    fn ifml_json(value: Value) -> String {
        value.to_string()
    }

    // ── rexlang `when` goldens ──────────────────────────────────────────

    #[test]
    fn comparisons_and_bool_ops_lower_to_ts() {
        let lower = |src: String| lower_when_json(&src, "ReviewCandidate").unwrap();

        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"eq","lhs":{"kind":"name","name":"status"},"rhs":{"kind":"string","value":"active"}})
            )),
            "(status === \"active\")"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"ne","lhs":{"kind":"name","name":"amount"},"rhs":{"kind":"int","value":1}})
            )),
            "(amount !== 1)"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"lt","lhs":{"kind":"name","name":"amount"},"rhs":{"kind":"int","value":1}})
            )),
            "(amount < 1)"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"le","lhs":{"kind":"name","name":"amount"},"rhs":{"kind":"int","value":1}})
            )),
            "(amount <= 1)"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"gt","lhs":{"kind":"name","name":"amount"},"rhs":{"kind":"int","value":0}})
            )),
            "(amount > 0)"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"ge","lhs":{"kind":"name","name":"amount"},"rhs":{"kind":"int","value":5}})
            )),
            "(amount >= 5)"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"and","lhs":{"kind":"name","name":"internal"},"rhs":{"kind":"name","name":"verified"}})
            )),
            "(internal && verified)"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"binary","op":"or","lhs":{"kind":"name","name":"internal"},"rhs":{"kind":"name","name":"verified"}})
            )),
            "(internal || verified)"
        );
        assert_eq!(
            lower(when_json(
                json!({"kind":"unary","op":"not","expr":{"kind":"name","name":"internal"}})
            )),
            "(!internal)"
        );
    }

    #[test]
    fn null_checks_lower_to_strict_equality() {
        assert_eq!(
            lower_when_json(&when_json(json!({"kind":"binary","op":"eq","lhs":{"kind":"name","name":"status"},"rhs":{"kind":"null"}})), "X").unwrap(),
            "(status === null)"
        );
        assert_eq!(
            lower_when_json(&when_json(json!({"kind":"binary","op":"ne","lhs":{"kind":"name","name":"status"},"rhs":{"kind":"null"}})), "X").unwrap(),
            "(status !== null)"
        );
        assert_eq!(
            lower_when_json(&when_json(json!({"kind":"binary","op":"eq","lhs":{"kind":"null"},"rhs":{"kind":"name","name":"status"}})), "X").unwrap(),
            "(status === null)"
        );
    }

    #[test]
    fn date_literal_lowers_to_new_date() {
        assert_eq!(
            lower_when_json(&when_json(json!({"kind":"binary","op":"eq","lhs":{"kind":"name","name":"start_date"},"rhs":{"kind":"date","text":"2026-01-01"}})), "X").unwrap(),
            "(start_date === new Date(\"2026-01-01\"))"
        );
    }

    #[test]
    fn literals_lower_to_ts_literals() {
        assert_eq!(
            lower_when_json(&when_json(json!({"kind":"int","value":42})), "X").unwrap(),
            "42"
        );
        assert_eq!(
            lower_when_json(&when_json(json!({"kind":"bool","value":true})), "X").unwrap(),
            "true"
        );
        assert_eq!(
            lower_when_json(&when_json(json!({"kind":"string","value":"it\"s"})), "X").unwrap(),
            "\"it\\\"s\""
        );
    }

    // ── IFML conditional goldens ────────────────────────────────────────

    #[test]
    fn ifml_conditionals_lower_to_ts() {
        assert_eq!(
            lower_ifml_json(&ifml_json(json!({"type":"binOp","value":{"left":{"type":"fieldExpr","value":{"object":{"type":"ident","value":"row"},"field":"active"}},"op":"eq","right":{"type":"boolLit","value":true}}})), "grid").unwrap(),
            "(row.active === true)"
        );
        assert_eq!(
            lower_ifml_json(&ifml_json(json!({"type":"binOp","value":{"left":{"type":"ident","value":"promo"},"op":"and","right":{"type":"group","value":{"type":"ident","value":"ready"}}}})), "grid").unwrap(),
            "(promo && (ready))"
        );
        assert_eq!(
            lower_ifml_json(&ifml_json(json!({"type":"unaryOp","value":{"op":"not","operand":{"type":"ident","value":"open"}}})), "grid").unwrap(),
            "(!open)"
        );
    }

    // ── refusals: closed subset, named errors ───────────────────────────

    #[test]
    fn unsupported_node_is_a_named_error() {
        let err = lower_when_json(
            &when_json(json!({"kind":"binary","op":"add","lhs":{"kind":"name","name":"amount"},"rhs":{"kind":"int","value":1}})),
            "ReviewCandidate",
        )
        .expect_err("arithmetic must be refused");
        assert!(err.to_string().contains("ReviewCandidate"), "{err}");
        assert!(err.to_string().contains("arithmetic"), "{err}");

        let err = lower_ifml_json(
            &ifml_json(json!({"type":"call","value":{"name":"len","args":[{"type":"ident","value":"title"}]}})),
            "editor",
        )
        .expect_err("calls must be refused");
        assert!(err.to_string().contains("editor"), "{err}");

        let err = lower_ifml_json(
            &ifml_json(json!({"type":"binOp","value":{"left":{"type":"ident","value":"name"},"op":"regexMatch","right":{"type":"stringLit","value":"^A"}}})),
            "grid",
        )
        .expect_err("regex match must be refused");
        assert!(err.to_string().contains("regex"), "{err}");
    }
}
