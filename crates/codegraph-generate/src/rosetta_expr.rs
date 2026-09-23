//! Rosetta `expr_json` → Rust transpiler (issue #262, slice 1).
//!
//! The rosetta bridge stores conditions as `ConditionNode.expr_json` — the
//! canonical [`sigil_model::expr::Expr::to_json`] serialization (serde_json
//! `Value`, documented stable shape). This module turns those payloads into
//! Rust boolean expressions for the `condition_validations` generator.
//!
//! # Strategy: untyped emission with minimal local inference
//!
//! Sigil does NOT type-check expressions (resolve checks head symbols only),
//! so THIS transpiler owns semantics. It emits Rust **untyped** — the single
//! piece of inference is FIELD OPTIONALITY, provided by the caller from the
//! owning entity's PropertyNodes (`is_nullable`) via [`ExprContext`]:
//!
//! - required fields → direct access (`dto.total`);
//! - optional fields accessed bare → [`TranspileError`] (the generator keeps
//!   a `TODO(#262)` marker naming the reason);
//! - `exists` / `absent` ARE the sanctioned way to touch optional fields:
//!   `X exists` → `dto.x.is_some()`, `X absent` → `dto.x.is_none()` (only a
//!   BARE symbol argument is sanctioned — deep chains through an optional
//!   receiver are refused).
//!
//! Every bare symbol is treated as a field on the receiver (unknown-symbol
//! handling is the caller's concern).
//!
//! # Known emission caveats (documented, deliberate)
//!
//! - **Number/Int literals are emitted VERBATIM** (Rosetta keeps source
//!   text). A trailing-dot BigDecimal literal (`5.`) is valid Rosetta but
//!   not valid Rust — such conditions are rejected downstream by rustc, not
//!   silently rewritten here.
//! - **`X exists` / `X absent` on REQUIRED fields** still emit
//!   `.is_some()` / `.is_none()`, which does not compile against a
//!   non-`Option` field. Untyped policy: exists/absent are only meaningful
//!   on optionals (models write them on optionals).
//! - **`OnlyElement` emits `.first()`** — SEMANTIC GAP: Rust `first()` does
//!   not enforce Rosetta's uniqueness contract; slice 2 revisits.
//! - **`Exists` with modifier `single`/`multiple`** is
//!   [`TranspileError`]-unsupported: without types you cannot distinguish
//!   `Vec` from `Option` (`modifier "none"` → `.is_some()`; collection
//!   modifiers need collection knowledge, slice 2).
//! - Nested binary operands are parenthesized (deterministic, precedence-
//!   safe): `(a && b) || c`. Top level is not wrapped.
//!
//! codegraph-generate must NOT depend on sigil (issue #255 rule) — this
//! module operates purely on the stored `serde_json::Value`.
//!
//! Slice 2 owns: `contains` / `disjoint` / `default`, exists modifiers,
//! collection operations. Slice 3 owns: switch/join/conditional, casts,
//! functional operations (filter/extract/reduce/sort/...), constructors,
//! `OnlyExists`, `WithMeta`, `As`, `Then`.

use std::collections::HashSet;
use std::fmt;

use codegraph_naming::{escape_rust_keyword, to_snake_case};

/// Why an expression could not be transpiled.
///
/// `kind` carries the `Expr::to_json` kind tag that was rejected
/// (`"Binary"`, `"Switch"`, ...) so generators can surface it in
/// `TODO(#262)` markers; `detail` explains the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileError {
    pub kind: String,
    pub detail: String,
}

impl TranspileError {
    fn new(kind: &str, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported expression ({}): {}", self.kind, self.detail)
    }
}

impl std::error::Error for TranspileError {}

/// Transpilation context: the receiver variable name (e.g. `"dto"`) and the
/// snake_case names of fields whose DTO type is `Option<T>` (from
/// `PropertyNode.is_nullable`).
pub struct ExprContext<'a> {
    pub receiver: &'a str,
    pub optional_fields: &'a HashSet<String>,
}

/// Transpile one `Expr::to_json` payload into a Rust expression fragment.
pub fn transpile(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
) -> Result<String, TranspileError> {
    emit(payload, ctx, false)
}

/// Core emitter. `allow_optional_root` sanctions a bare optional-field
/// reference — set ONLY for `exists` / `absent` arguments (the sanctioned
/// way to touch optionals).
fn emit(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
    allow_optional_root: bool,
) -> Result<String, TranspileError> {
    let kind = payload.get("kind").and_then(|k| k.as_str());
    match kind {
        Some("Boolean") => {
            let value = payload
                .get("value")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| {
                    TranspileError::new("Boolean", "malformed payload: missing 'value'")
                })?;
            Ok(if value { "true" } else { "false" }.to_string())
        }
        Some("String") => {
            let value = payload
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    TranspileError::new("String", "malformed payload: missing 'value'")
                })?;
            Ok(string_literal(value))
        }
        // Rosetta keeps the source text of numeric literals — emit verbatim.
        // Trailing-dot BigDecimal text (`5.`) is valid Rosetta but not valid
        // Rust; see the module docs.
        Some("Number") | Some("Int") => {
            let text = payload
                .get("text")
                .and_then(|t| t.as_str())
                .ok_or_else(|| {
                    TranspileError::new(
                        kind.unwrap_or_default(),
                        "malformed payload: missing 'text'",
                    )
                })?;
            Ok(text.to_string())
        }
        Some("List") => {
            let elements = payload
                .get("elements")
                .and_then(|e| e.as_array())
                .ok_or_else(|| {
                    TranspileError::new("List", "malformed payload: missing 'elements'")
                })?;
            let mut parts = Vec::with_capacity(elements.len());
            for element in elements {
                parts.push(emit(element, ctx, false)?);
            }
            Ok(format!("vec![{}]", parts.join(", ")))
        }
        Some("SymbolReference") => emit_symbol_reference(payload, ctx, allow_optional_root),
        Some("ImplicitVariable") => Ok(ctx.receiver.to_string()),
        Some(call @ ("FeatureCall" | "DeepFeatureCall")) => {
            let Some(feature) = payload.get("feature").and_then(|f| f.as_str()) else {
                return Err(TranspileError::new(
                    call,
                    "bare '->' projection (no feature) lands in slice 3",
                ));
            };
            let receiver = payload.get("receiver").ok_or_else(|| {
                TranspileError::new(call, "malformed payload: missing 'receiver'")
            })?;
            let recv = emit(receiver, ctx, false).map_err(|e| optional_receiver_error(call, e))?;
            Ok(format!(
                "{}.{}",
                recv,
                escape_rust_keyword(&to_snake_case(feature))
            ))
        }
        Some("Binary") => emit_binary(payload, ctx),
        Some("Exists") => {
            let modifier = payload
                .get("modifier")
                .and_then(|m| m.as_str())
                .unwrap_or("none");
            if modifier != "none" {
                // Without types we cannot distinguish Vec from Option.
                return Err(TranspileError::new(
                    "Exists",
                    format!("exists modifier '{modifier}' requires collection knowledge (slice 2)"),
                ));
            }
            let argument = payload.get("argument").ok_or_else(|| {
                TranspileError::new("Exists", "malformed payload: missing 'argument'")
            })?;
            Ok(format!("{}.is_some()", emit(argument, ctx, true)?))
        }
        Some("Absent") => {
            let argument = payload.get("argument").ok_or_else(|| {
                TranspileError::new("Absent", "malformed payload: missing 'argument'")
            })?;
            Ok(format!("{}.is_none()", emit(argument, ctx, true)?))
        }
        // SEMANTIC GAP: `.first()` does not enforce Rosetta's uniqueness
        // contract (module docs). Accepted for slice 1 per #262 scope.
        Some("OnlyElement") => {
            let argument = payload.get("argument").ok_or_else(|| {
                TranspileError::new("OnlyElement", "malformed payload: missing 'argument'")
            })?;
            Ok(format!("{}.first()", emit(argument, ctx, false)?))
        }
        Some("Count") => {
            let argument = payload.get("argument").ok_or_else(|| {
                TranspileError::new("Count", "malformed payload: missing 'argument'")
            })?;
            Ok(format!("{}.len()", emit(argument, ctx, false)?))
        }
        Some(other) => Err(TranspileError::new(
            other,
            "expression family lands in slices 2-3",
        )),
        None => Err(TranspileError::new("Unknown", "payload has no 'kind' tag")),
    }
}

/// `SymbolReference` → field access on the receiver. Function-like refs
/// (`explicit == true` or non-empty `args`) are unsupported; bare access to
/// a known-optional field is refused unless sanctioned by exists/absent.
fn emit_symbol_reference(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
    allow_optional_root: bool,
) -> Result<String, TranspileError> {
    let symbol = payload
        .get("symbol")
        .and_then(|s| s.as_str())
        .ok_or_else(|| {
            TranspileError::new("SymbolReference", "malformed payload: missing 'symbol'")
        })?;
    let explicit = payload
        .get("explicit")
        .and_then(|e| e.as_bool())
        .unwrap_or(false);
    let has_args = payload
        .get("args")
        .and_then(|a| a.as_array())
        .is_some_and(|a| !a.is_empty());
    if explicit || has_args {
        return Err(TranspileError::new(
            "SymbolReference",
            format!("function-like reference '{symbol}(...)' is not a field access"),
        ));
    }
    let field = escape_rust_keyword(&to_snake_case(symbol));
    if !allow_optional_root && ctx.optional_fields.contains(&field) {
        return Err(optional_field_error(&field));
    }
    Ok(format!("{}.{}", ctx.receiver, field))
}

/// Binary operations: arithmetic, logical, equality, comparison. The
/// word-binary ops (`contains` / `disjoint` / `default`) and cardinality
/// modifiers (`any` / `all` =) are slice 2.
fn emit_binary(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
) -> Result<String, TranspileError> {
    if payload
        .get("cardMod")
        .and_then(|c| c.as_str())
        .is_some_and(|m| m != "none")
    {
        return Err(TranspileError::new(
            "Binary",
            "cardinality comparison (any/all =) requires collection knowledge (slice 2)",
        ));
    }
    let op = payload
        .get("op")
        .and_then(|o| o.as_str())
        .ok_or_else(|| TranspileError::new("Binary", "malformed payload: missing 'op'"))?;
    let rust_op = match op {
        "+" => "+",
        "-" => "-",
        "*" => "*",
        "/" => "/",
        "and" => "&&",
        "or" => "||",
        "=" => "==",
        "<>" => "!=",
        ">" => ">",
        "<" => "<",
        ">=" => ">=",
        "<=" => "<=",
        "contains" | "disjoint" | "default" => {
            return Err(TranspileError::new(
                "Binary",
                format!("operator '{op}' arrives in slice 2"),
            ));
        }
        other => {
            return Err(TranspileError::new(
                "Binary",
                format!("unknown operator '{other}'"),
            ));
        }
    };
    let left = payload
        .get("left")
        .ok_or_else(|| TranspileError::new("Binary", "malformed payload: missing 'left'"))?;
    let right = payload
        .get("right")
        .ok_or_else(|| TranspileError::new("Binary", "malformed payload: missing 'right'"))?;
    Ok(format!(
        "{} {rust_op} {}",
        wrap_nested_binary(left, ctx)?,
        wrap_nested_binary(right, ctx)?
    ))
}

/// Emit a binary operand, parenthesizing nested binary subtrees so Rust
/// precedence can never reinterpret the composition.
fn wrap_nested_binary(
    payload: &serde_json::Value,
    ctx: &ExprContext<'_>,
) -> Result<String, TranspileError> {
    let code = emit(payload, ctx, false)?;
    if payload.get("kind").and_then(|k| k.as_str()) == Some("Binary") {
        Ok(format!("({code})"))
    } else {
        Ok(code)
    }
}

fn optional_field_error(field: &str) -> TranspileError {
    TranspileError::new(
        "SymbolReference",
        format!("optional field '{field}' accessed bare (touch optionals via exists/absent)"),
    )
}

/// Re-wrap a bare-optional-field error raised for a feature-call receiver:
/// the optional rule is the same, but the call is the rejected node.
fn optional_receiver_error(call: &str, err: TranspileError) -> TranspileError {
    if err.kind == "SymbolReference" {
        if let Some(field) = err
            .detail
            .strip_prefix("optional field '")
            .and_then(|rest| rest.split('\'').next())
        {
            return TranspileError::new(
                call,
                format!("optional receiver requires exists-guard: '{field}'"),
            );
        }
    }
    err
}

/// A Rust string literal with `"` / `\` (and control-char) escaping.
fn string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx<'a>(optional: &'a HashSet<String>) -> ExprContext<'a> {
        ExprContext {
            receiver: "dto",
            optional_fields: optional,
        }
    }

    fn t(payload: serde_json::Value) -> String {
        let optional = HashSet::new();
        transpile(&payload, &ctx(&optional)).expect("transpiles")
    }

    fn e(payload: serde_json::Value) -> TranspileError {
        let optional = HashSet::new();
        transpile(&payload, &ctx(&optional)).expect_err("rejected")
    }

    fn optional_ctx<'a>(fields: &'a HashSet<String>) -> ExprContext<'a> {
        ctx(fields)
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
        assert_eq!(
            t(json!({"kind":"SymbolReference","symbol":"orderId","explicit":false,"args":[]})),
            "dto.order_id"
        );
    }

    #[test]
    fn symbol_reference_escapes_rust_keywords() {
        assert_eq!(
            t(json!({"kind":"SymbolReference","symbol":"type","explicit":false,"args":[]})),
            "dto.r#type"
        );
    }

    #[test]
    fn optional_symbol_reference_is_refused() {
        let optional = HashSet::from(["memo".to_string()]);
        let err = transpile(
            &json!({"kind":"SymbolReference","symbol":"memo","explicit":false,"args":[]}),
            &optional_ctx(&optional),
        )
        .expect_err("optional bare access refused");
        assert_eq!(err.kind, "SymbolReference");
        assert!(err.detail.contains("optional field 'memo'"), "{err}");
    }

    #[test]
    fn function_like_symbol_reference_is_unsupported() {
        let err = e(json!({"kind":"SymbolReference","symbol":"f","explicit":true,"args":[]}));
        assert_eq!(err.kind, "SymbolReference");
        assert!(err.detail.contains("function-like"), "{err}");

        let err = e(
            json!({"kind":"SymbolReference","symbol":"f","explicit":false,"args":[
                {"kind":"Int","text":"1"}
            ]}),
        );
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
                json!({"kind":"FeatureCall","receiver":
                    json!({"kind":"SymbolReference","symbol":"counterparty","explicit":false,"args":[]}),
                "feature":"name"}),
            "feature":"upper"})),
            "dto.counterparty.name.upper"
        );
    }

    #[test]
    fn deep_feature_call_nests_the_same_way() {
        assert_eq!(
            t(json!({"kind":"DeepFeatureCall","receiver":
                json!({"kind":"SymbolReference","symbol":"counterparty","explicit":false,"args":[]}),
            "feature":"name"})),
            "dto.counterparty.name"
        );
    }

    #[test]
    fn bare_projection_feature_is_unsupported() {
        let err = e(json!({"kind":"FeatureCall","receiver":
            json!({"kind":"SymbolReference","symbol":"a","explicit":false,"args":[]}),
        "feature":null}));
        assert_eq!(err.kind, "FeatureCall");
        assert!(err.detail.contains("bare '->'"), "{err}");
    }

    #[test]
    fn optional_feature_call_receiver_is_refused() {
        let optional = HashSet::from(["counterparty".to_string()]);
        let err = transpile(
            &json!({"kind":"FeatureCall","receiver":
                json!({"kind":"SymbolReference","symbol":"counterparty","explicit":false,"args":[]}),
            "feature":"name"}),
            &optional_ctx(&optional),
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
                "left":{"kind":"SymbolReference","symbol":"a","explicit":false,"args":[]},
                "right":{"kind":"Int","text":"1"}});
            assert_eq!(t(payload), format!("dto.a {rust} 1"), "op {rosetta}");
        }
    }

    #[test]
    fn nested_binaries_are_parenthesized_deterministically() {
        assert_eq!(
            t(json!({"kind":"Binary","op":"and",
                "left":{"kind":"Binary","op":">",
                    "left":{"kind":"SymbolReference","symbol":"a","explicit":false,"args":[]},
                    "right":{"kind":"Int","text":"1"}},
                "right":{"kind":"Binary","op":"<",
                    "left":{"kind":"SymbolReference","symbol":"b","explicit":false,"args":[]},
                    "right":{"kind":"Int","text":"2"}}})),
            "(dto.a > 1) && (dto.b < 2)"
        );
    }

    #[test]
    fn cardinality_modifier_comparison_is_unsupported() {
        let err = e(json!({"kind":"Binary","op":"=","cardMod":"any",
            "left":{"kind":"SymbolReference","symbol":"a","explicit":false,"args":[]},
            "right":{"kind":"Int","text":"1"}}));
        assert_eq!(err.kind, "Binary");
        assert!(err.detail.contains("cardinality comparison"), "{err}");
    }

    #[test]
    fn card_mod_none_is_fine() {
        assert_eq!(
            t(json!({"kind":"Binary","op":"=","cardMod":"none",
                "left":{"kind":"SymbolReference","symbol":"a","explicit":false,"args":[]},
                "right":{"kind":"Int","text":"1"}})),
            "dto.a == 1"
        );
    }

    #[test]
    fn word_binary_ops_are_slice_2() {
        for op in ["contains", "disjoint", "default"] {
            let err = e(json!({"kind":"Binary","op":op,
                "left":{"kind":"SymbolReference","symbol":"a","explicit":false,"args":[]},
                "right":{"kind":"Int","text":"1"}}));
            assert_eq!(err.kind, "Binary");
            assert!(err.detail.contains(op), "op {op}: {err}");
        }
    }

    // ── exists / absent ──────────────────────────────────────────────────

    #[test]
    fn exists_sanctions_optional_fields() {
        let optional = HashSet::from(["settled_on".to_string()]);
        let out = transpile(
            &json!({"kind":"Exists","modifier":"none","argument":
                json!({"kind":"SymbolReference","symbol":"settledOn","explicit":false,"args":[]})}),
            &optional_ctx(&optional),
        )
        .expect("exists transpiles");
        assert_eq!(out, "dto.settled_on.is_some()");
    }

    #[test]
    fn absent_sanctions_optional_fields() {
        let optional = HashSet::from(["memo".to_string()]);
        let out = transpile(
            &json!({"kind":"Absent","argument":
                json!({"kind":"SymbolReference","symbol":"memo","explicit":false,"args":[]})}),
            &optional_ctx(&optional),
        )
        .expect("absent transpiles");
        assert_eq!(out, "dto.memo.is_none()");
    }

    #[test]
    fn exists_with_collection_modifier_is_unsupported() {
        let err = e(json!({"kind":"Exists","modifier":"single","argument":
            json!({"kind":"SymbolReference","symbol":"lines","explicit":false,"args":[]})}));
        assert_eq!(err.kind, "Exists");
        assert!(err.detail.contains("collection knowledge"), "{err}");
    }

    // ── only-element / count ─────────────────────────────────────────────

    #[test]
    fn only_element_maps_to_first() {
        assert_eq!(
            t(json!({"kind":"OnlyElement","argument":
                json!({"kind":"SymbolReference","symbol":"lines","explicit":false,"args":[]})})),
            "dto.lines.first()"
        );
    }

    #[test]
    fn count_maps_to_len() {
        assert_eq!(
            t(json!({"kind":"Count","argument":
                json!({"kind":"SymbolReference","symbol":"lines","explicit":false,"args":[]})})),
            "dto.lines.len()"
        );
    }

    // ── slice 2-3 families carry their kind ──────────────────────────────

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
            assert_eq!(err.kind, family);
        }
    }

    #[test]
    fn missing_kind_tag_is_reported() {
        let err = e(json!({"nope":true}));
        assert_eq!(err.kind, "Unknown");
    }

    #[test]
    fn display_is_marker_friendly() {
        let err = TranspileError::new("Switch", "someday");
        assert_eq!(err.to_string(), "unsupported expression (Switch): someday");
    }
}
