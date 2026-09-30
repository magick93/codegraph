use codegraph_naming::to_snake_case;

use super::error::TranspileError;

/// The snake_case symbol at the root of a receiver/argument chain, if the
/// chain bottoms out in a plain field reference. Used for collection /
/// numeric knowledge gating: `prices filter [...] sum` roots at `prices`.
pub(super) fn root_symbol(payload: &serde_json::Value) -> Option<String> {
    let kind = payload.get("kind").and_then(|k| k.as_str())?;
    match kind {
        "SymbolReference" => payload
            .get("symbol")
            .and_then(|s| s.as_str())
            .map(to_snake_case),
        "FeatureCall" | "DeepFeatureCall" => root_symbol(payload.get("receiver")?),
        // Postfix / functional / cast operations chain onto their argument.
        "Flatten" | "Distinct" | "Reverse" | "First" | "Last" | "Sum" | "Min" | "Max"
        | "OnlyElement" | "Count" | "Filter" | "Map" | "Reduce" | "Sort" | "Then" | "WithMeta"
        | "ToString" | "ToNumber" | "ToInt" | "ToDate" | "ToDateTime" | "ToZonedDateTime"
        | "ToTime" => root_symbol(payload.get("argument")?),
        _ => None,
    }
}

/// Flatten a receiver chain into its `SymbolReference` root plus the
/// features outermost-last (`FeatureCall{recv: sym(a), feature: b}` →
/// `(sym(a), ["b"])`). `None` when the chain does not bottom out in a
/// plain `SymbolReference` (wrappers, literals, calls) — callers fall
/// back to the generic emission path.
pub(super) fn chain_parts(
    payload: &serde_json::Value,
) -> Option<(&serde_json::Value, Vec<String>)> {
    match payload.get("kind").and_then(|k| k.as_str())? {
        "SymbolReference" => Some((payload, Vec::new())),
        "FeatureCall" | "DeepFeatureCall" => {
            let feature = payload.get("feature").and_then(|f| f.as_str())?;
            let (root, mut rest) = chain_parts(payload.get("receiver")?)?;
            rest.push(feature.to_string());
            Some((root, rest))
        }
        _ => None,
    }
}

/// Kinds whose fragment must be parenthesized when composed as an operand —
/// their surface form can bind differently in Rust.
pub(super) fn wraps_as_operand(payload: &serde_json::Value) -> bool {
    matches!(
        payload.get("kind").and_then(|k| k.as_str()),
        Some("Binary" | "Conditional" | "Switch" | "Then")
    )
}

pub(super) fn wrap_operand(payload: &serde_json::Value, code: String) -> String {
    if wraps_as_operand(payload) {
        format!("({code})")
    } else {
        code
    }
}

pub(super) fn kind_for(op: &str) -> &'static str {
    match op {
        "filter" => "Filter",
        "map" => "Map",
        "min" => "Min",
        "max" => "Max",
        "sum" => "Sum",
        _ => "Reduce",
    }
}

pub(super) fn date_time_gap(kind: &str) -> String {
    format!(
        "{kind} needs a date-time library (chrono is absent from codegraph-generate by design; documented gap)"
    )
}

pub(super) fn reduce_gap() -> String {
    "reduce has no init field in the sigil serialization: the a,b[body] form needs \
     first-element-as-init (Iterator::reduce yields a non-composable Option) and the \
     [init] bracket form is stored as an empty-parameter inline function — documented gap"
        .to_string()
}

pub(super) fn optional_field_error(field: &str) -> TranspileError {
    TranspileError::new(
        "SymbolReference",
        format!("optional field '{field}' accessed bare (touch optionals via exists/absent)"),
    )
}

/// Re-wrap a bare-optional-field error raised for a feature-call receiver:
/// the optional rule is the same, but the call is the rejected node.
pub(super) fn optional_receiver_error(call: &str, err: TranspileError) -> TranspileError {
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
pub(super) fn string_literal(value: &str) -> String {
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
