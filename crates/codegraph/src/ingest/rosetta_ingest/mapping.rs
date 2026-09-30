//! Rosetta builtin/alias type mappings for the rosetta bridge (split out
//! of the `rosetta_ingest` module, #371).

use std::collections::HashMap;

use codegraph_type_contracts::PgType;

/// The Rosetta builtin simple types (issue #256) and their JSON-path
/// primitive mappings. `time` has no `PgType` variant and maps to TEXT
/// carrying an ISO-8601 time string (format hint `time`), mirroring how
/// the JSON path treats unmapped string formats.
pub(super) fn builtin_mapping(name: &str) -> Option<(PgType, Option<&'static str>, &'static str)> {
    match name {
        "int" => Some((PgType::Integer, None, "integer")),
        "number" => Some((PgType::DoublePrecision, None, "number")),
        "string" => Some((PgType::Text, None, "string")),
        "boolean" => Some((PgType::Boolean, None, "boolean")),
        "date" => Some((PgType::Date, Some("date"), "string")),
        "dateTime" => Some((PgType::Timestamptz, Some("date-time"), "string")),
        "zonedDateTime" => Some((PgType::Timestamptz, Some("date-time"), "string")),
        "time" => Some((PgType::Text, Some("time"), "string")),
        _ => None,
    }
}

/// Parameter-aware alias lowering: resolve a user `typeAlias` to the builtin
/// primitive it names, honoring its arguments. `number(digits, fractionalDigits)`
/// becomes `NUMERIC(p,s)` when scaled, `BIGINT`/`INTEGER` for unscaled digits,
/// `DOUBLE PRECISION` when unparameterized; a `string(pattern: ...)` shaped like
/// the canonical UUID form becomes a real `UUID` column. Alias chains are
/// followed a few hops; aliases landing on data types/enum return `None` so
/// the caller's enum/entity-reference arms take over.
pub(super) fn alias_builtin_mapping(
    name: &str,
    aliases: &HashMap<String, sigil_model::TypeRef>,
    depth: usize,
) -> Option<(PgType, Option<&'static str>, &'static str)> {
    if depth > 4 {
        return None;
    }
    let type_ref = aliases.get(name)?;
    let base = type_ref.name.rsplit('.').next().unwrap_or(&type_ref.name);
    if builtin_mapping(base).is_none() {
        // Alias-to-alias chains follow; alias-to-data-type stops here.
        return if aliases.contains_key(base) {
            alias_builtin_mapping(base, aliases, depth + 1)
        } else {
            None
        };
    }
    let int_arg = |param: &str| {
        type_ref.arguments.iter().find_map(|a| {
            if a.parameter != param {
                return None;
            }
            match &a.argument_value {
                sigil_model::ArgumentValue::Int(v) => i64::try_from(*v).ok(),
                _ => None,
            }
        })
    };
    let str_arg = |param: &str| {
        type_ref.arguments.iter().find_map(|a| {
            if a.parameter != param {
                return None;
            }
            match &a.argument_value {
                sigil_model::ArgumentValue::Str(s) => Some(s.clone()),
                _ => None,
            }
        })
    };
    Some(match base {
        "number" => {
            let scale = int_arg("fractionalDigits").unwrap_or(0);
            let digits = int_arg("digits");
            if scale > 0 {
                (
                    PgType::Numeric {
                        precision: digits.unwrap_or(18).clamp(1, 65) as u8,
                        scale: scale.clamp(0, 30) as u8,
                    },
                    None,
                    "number",
                )
            } else {
                match digits {
                    Some(d) if d > 9 => (PgType::BigInt, None, "integer"),
                    Some(_) => (PgType::Integer, None, "integer"),
                    None => (PgType::DoublePrecision, None, "number"),
                }
            }
        }
        "string" => {
            let uuid_shaped = str_arg("pattern")
                .is_some_and(|p| p.contains("{8}") && p.contains("{12}") && p.contains('-'));
            if uuid_shaped {
                (PgType::Uuid, None, "string")
            } else {
                (PgType::Text, None, "string")
            }
        }
        other => builtin_mapping(other)?,
    })
}
