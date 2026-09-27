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

use serde_json::Value;

use crate::db::expr_sql::parse_date_text;

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
pub fn lower_when_json(expr_json: &str, owner: &str) -> Result<String, ExprTsError> {
    let payload = parse_payload(expr_json, owner)?;
    lower_when_value(&payload, owner)
}

/// Lower an IFML `conditional_expression` `expr_json` payload (the typed
/// `rex_ifml::Expression` wire JSON) to a TypeScript expression.
pub fn lower_ifml_json(expr_json: &str, owner: &str) -> Result<String, ExprTsError> {
    let payload = parse_payload(expr_json, owner)?;
    lower_ifml_value(&payload, owner)
}

fn parse_payload(expr_json: &str, owner: &str) -> Result<Value, ExprTsError> {
    serde_json::from_str(expr_json)
        .map_err(|e| ExprTsError::new(owner, format!("expr_json is not valid JSON: {e}")))
}

// ── rexlang `when` shape (kind-tagged) ─────────────────────────────────

fn lower_when_value(value: &Value, owner: &str) -> Result<String, ExprTsError> {
    let reject = |construct: &str| {
        Err(ExprTsError::new(
            owner,
            format!(
                "condition uses {construct}, which cannot be lowered to \
                 TypeScript; the supported subset is field references, \
                 string/number/boolean/date literals, comparisons, \
                 && || !, parentheses, and null comparisons"
            ),
        ))
    };
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ExprTsError::new(owner, "expr_json payload has no `kind` tag"))?;
    match kind {
        "int" => int_text(
            value
                .get("value")
                .and_then(Value::as_i64)
                .ok_or_else(|| malformed(owner, kind))?,
        ),
        "string" => Ok(string_literal(
            value
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?,
        )),
        "bool" => match value.get("value").and_then(Value::as_bool) {
            Some(true) => Ok("true".to_string()),
            Some(false) => Ok("false".to_string()),
            None => Err(malformed(owner, kind)),
        },
        "null" => reject("`null` (compare a field to null instead)"),
        "date" => {
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?;
            date_literal(text, owner)
        }
        "name" => Ok(value
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| malformed(owner, kind))?
            .to_string()),
        "feature_access" => {
            if value
                .get("optional_safe")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return reject("`?.` (safe navigation)");
            }
            let receiver = lower_when_value(
                value
                    .get("receiver")
                    .ok_or_else(|| malformed(owner, kind))?,
                owner,
            )?;
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?;
            Ok(format!("{receiver}.{name}"))
        }
        "coalesce" => reject("`?:`"),
        "call" => reject("an operation call"),
        "algebra" => reject(&format!(
            "collection algebra `{}`",
            value
                .get("op")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>")
        )),
        "binary" => {
            let op = value
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?;
            let lhs = value.get("lhs").ok_or_else(|| malformed(owner, kind))?;
            let rhs = value.get("rhs").ok_or_else(|| malformed(owner, kind))?;
            lower_when_binary(op, lhs, rhs, owner)
        }
        "unary" => {
            let op = value
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?;
            let inner_value = value.get("expr").ok_or_else(|| malformed(owner, kind))?;
            match op {
                "not" => Ok(format!("(!{})", lower_when_value(inner_value, owner)?)),
                "neg" => match inner_value.get("kind").and_then(Value::as_str) {
                    // Only a folded negative literal is in the subset,
                    // mirroring the SQL lowering.
                    Some("int") => int_text(
                        -inner_value
                            .get("value")
                            .and_then(Value::as_i64)
                            .ok_or_else(|| malformed(owner, kind))?,
                    ),
                    _ => reject("negation of a non-literal (arithmetic)"),
                },
                other => Err(malformed(owner, other)),
            }
        }
        "if" => reject("`if`"),
        "let" => reject("`let`"),
        "lambda" => reject("a lambda"),
        "list" => reject("list literals"),
        other => reject(&format!("unknown expression node `{other}`")),
    }
}

fn lower_when_binary(
    op: &str,
    lhs: &Value,
    rhs: &Value,
    owner: &str,
) -> Result<String, ExprTsError> {
    let is_null = |v: &Value| v.get("kind").and_then(Value::as_str) == Some("null");
    if matches!(op, "eq" | "ne") && (is_null(lhs) || is_null(rhs)) {
        if is_null(lhs) && is_null(rhs) {
            return Ok(match op {
                "eq" => "true".to_string(),
                _ => "false".to_string(),
            });
        }
        let operand = if is_null(lhs) {
            lower_when_value(rhs, owner)?
        } else {
            lower_when_value(lhs, owner)?
        };
        return Ok(match op {
            "eq" => format!("({operand} === null)"),
            _ => format!("({operand} !== null)"),
        });
    }
    let ts_op = match op {
        "eq" => "===",
        "ne" => "!==",
        "lt" => "<",
        "le" => "<=",
        "gt" => ">",
        "ge" => ">=",
        "and" => "&&",
        "or" => "||",
        "add" | "sub" | "mul" | "div" => {
            let symbol = match op {
                "add" => "+",
                "sub" => "-",
                "mul" => "*",
                _ => "/",
            };
            return Err(ExprTsError::new(
                owner,
                format!(
                    "condition uses arithmetic (`{symbol}`), which cannot be \
                     lowered to TypeScript; the supported subset is field \
                     references, string/number/boolean/date literals, \
                     comparisons, && || !, parentheses, and null comparisons"
                ),
            ));
        }
        other => return Err(malformed(owner, other)),
    };
    Ok(format!(
        "({} {ts_op} {})",
        lower_when_value(lhs, owner)?,
        lower_when_value(rhs, owner)?
    ))
}

// ── IFML Expression shape (type/value tagged) ──────────────────────────

fn lower_ifml_value(value: &Value, owner: &str) -> Result<String, ExprTsError> {
    let reject = |construct: &str| {
        Err(ExprTsError::new(
            owner,
            format!(
                "condition uses {construct}, which cannot be lowered to \
                 TypeScript; the supported subset is field references, \
                 string/number/boolean literals, comparisons, && || !, and \
                 parentheses"
            ),
        ))
    };
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| ExprTsError::new(owner, "expr_json payload has no `type` tag"))?;
    let inner = || value.get("value").ok_or_else(|| malformed(owner, kind));
    match kind {
        "ident" => Ok(inner()?
            .as_str()
            .ok_or_else(|| malformed(owner, kind))?
            .to_string()),
        "stringLit" => Ok(string_literal(
            inner()?.as_str().ok_or_else(|| malformed(owner, kind))?,
        )),
        "numLit" => {
            let n = inner()?.as_f64().ok_or_else(|| malformed(owner, kind))?;
            Ok(number_text(n))
        }
        "boolLit" => match inner()?.as_bool() {
            Some(true) => Ok("true".to_string()),
            Some(false) => Ok("false".to_string()),
            None => Err(malformed(owner, kind)),
        },
        "fieldExpr" => {
            let inner = inner()?;
            let object = lower_ifml_value(
                inner.get("object").ok_or_else(|| malformed(owner, kind))?,
                owner,
            )?;
            let field = inner
                .get("field")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?;
            Ok(format!("{object}.{field}"))
        }
        "binOp" => {
            let inner = inner()?;
            let op = inner
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?;
            let ts_op = match op {
                "eq" => "===",
                "ne" => "!==",
                "lt" => "<",
                "le" => "<=",
                "gt" => ">",
                "ge" => ">=",
                "and" => "&&",
                "or" => "||",
                "add" | "sub" | "mul" | "div" | "mod" => {
                    let symbol = match op {
                        "add" => "+",
                        "sub" => "-",
                        "mul" => "*",
                        "div" => "/",
                        _ => "%",
                    };
                    return reject(&format!("arithmetic (`{symbol}`)"));
                }
                "regexMatch" => return reject("a regex match (`~=`)"),
                "negRegex" => return reject("a negative regex (`!~`)"),
                other => return Err(malformed(owner, other)),
            };
            let left = lower_ifml_value(
                inner.get("left").ok_or_else(|| malformed(owner, kind))?,
                owner,
            )?;
            let right = lower_ifml_value(
                inner.get("right").ok_or_else(|| malformed(owner, kind))?,
                owner,
            )?;
            Ok(format!("({left} {ts_op} {right})"))
        }
        "unaryOp" => {
            let inner = inner()?;
            let op = inner
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| malformed(owner, kind))?;
            let operand = inner.get("operand").ok_or_else(|| malformed(owner, kind))?;
            match op {
                "not" => Ok(format!("(!{})", lower_ifml_value(operand, owner)?)),
                "neg" => match operand.get("type").and_then(Value::as_str) {
                    Some("numLit") => {
                        let n = operand
                            .get("value")
                            .and_then(Value::as_f64)
                            .ok_or_else(|| malformed(owner, kind))?;
                        Ok(number_text(-n))
                    }
                    _ => reject("negation of a non-literal (arithmetic)"),
                },
                other => Err(malformed(owner, other)),
            }
        }
        "group" => Ok(format!("({})", lower_ifml_value(inner()?, owner)?)),
        "call" => reject("an operation call"),
        other => reject(&format!("unknown expression node `{other}`")),
    }
}

// ── shared helpers ─────────────────────────────────────────────────────

fn malformed(owner: &str, kind: &str) -> ExprTsError {
    ExprTsError::new(
        owner,
        format!("expr_json payload has a malformed `{kind}` node"),
    )
}

fn int_text(value: i64) -> Result<String, ExprTsError> {
    Ok(value.to_string())
}

/// Render a number without a trailing `.0` for integral values so the
/// emitted TypeScript reads like the DSL source (`42`, `4.5`).
fn number_text(value: f64) -> String {
    if value.fract() == 0.0 && value.is_finite() {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn string_literal(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn date_literal(text: &str, owner: &str) -> Result<String, ExprTsError> {
    if parse_date_text(text).is_some() {
        Ok(format!("new Date(\"{text}\")"))
    } else {
        Err(ExprTsError::new(
            owner,
            format!(
                "condition uses the date literal {text:?}, which is not a \
                 calendar date `YYYY-MM-DD`"
            ),
        ))
    }
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
