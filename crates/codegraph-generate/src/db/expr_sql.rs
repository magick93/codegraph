//! Lowering of rexlang `when` expressions to Postgres RLS predicates.
//!
//! The stored `when_expr` on a [`GrantEdge`](codegraph_core::types::GrantEdge)
//! is rexlang expression SOURCE text (see `crates/codegraph/src/ifml_actor_import.rs`).
//! This module parses it with `rex-expr` (parse only — no model, no
//! typechecking) and lowers a **documented closed subset** to a Postgres
//! boolean expression usable in `USING` / `WITH CHECK`:
//!
//! | rexlang                          | SQL                              |
//! |----------------------------------|----------------------------------|
//! | `field` (own-table field ref)    | quoted column                    |
//! | `123` / `-123`                   | numeric literal                  |
//! | `"text"`                         | `'text'` (`'` doubled)           |
//! | `true` / `false`                 | `TRUE` / `FALSE`                 |
//! | `date("YYYY-MM-DD")`             | `DATE '...'` (format-validated;  |
//! |                                  | the compared field is guaranteed |
//! |                                  | date-typed by the upstream       |
//! |                                  | rex-driver typecheck)            |
//! | `==` and `=`                     | `=`                              |
//! | `!=`                             | `<>`                             |
//! | `<`, `<=`, `>`, `>=`             | `<`, `<=`, `>`, `>=`             |
//! | `&&`, `\|\|`, `!`                | `AND`, `OR`, `NOT`               |
//! | parentheses                      | preserved (fully parenthesized)  |
//! | `field == null` / `field != null`| `field IS NULL` / `IS NOT NULL`  |
//!
//! ANYTHING ELSE is refused: arithmetic, `?.`, `?:`, `let`, lambdas,
//! collection algebra, `if`, operation calls, list literals, dotted
//! (relationship) navigation, and references to fields that are not columns
//! of the capability's class. Per the rexlang Cedar-backend philosophy, what
//! cannot be mapped faithfully is a hard generation error — every refusal
//! message names the capability.

use std::collections::HashMap;

use codegraph_naming::quote_pg_column;
use rex_expr::{BinOp, Expr, ExprKind, UnOp};

/// Lower `source` (a rexlang expression) to a Postgres predicate.
///
/// `capability` and `class` appear verbatim in refusal diagnostics.
/// `fields` maps feature names as written in the expression (plus their
/// snake_case forms) to Postgres column names, as resolved from the class's
/// schema properties by the caller.
pub fn lower_when_expr(
    source: &str,
    capability: &str,
    class: &str,
    fields: &HashMap<String, String>,
) -> Result<String, String> {
    let parsed = rex_expr::parse(source);
    match parsed.ast {
        Some(ast) if parsed.errors.is_empty() => lower(&ast, capability, class, fields),
        _ => Err(format!(
            "capability `{capability}`: `when` condition is not valid \
             rexlang expression syntax: {source:?}"
        )),
    }
}

/// Renders one expression as a fully parenthesized SQL predicate, refusing
/// every construct outside the documented closed subset.
fn lower(
    expr: &Expr,
    capability: &str,
    class: &str,
    fields: &HashMap<String, String>,
) -> Result<String, String> {
    let reject = |construct: &str| {
        Err(format!(
            "capability `{capability}`: `when` condition uses {construct}, \
             which cannot be lowered to a Postgres RLS predicate; the \
             supported subset is own-table field refs, string/number/boolean/\
             date literals, comparisons, && || !, parentheses, and null \
             comparisons"
        ))
    };
    match &expr.kind {
        ExprKind::Int(value) => Ok(value.to_string()),
        ExprKind::String(text) => Ok(string_literal(text)),
        ExprKind::Bool(true) => Ok("TRUE".to_string()),
        ExprKind::Bool(false) => Ok("FALSE".to_string()),
        ExprKind::Null => reject("`null` (compare a field to null instead)"),
        ExprKind::Date { text, .. } => date_literal(text, capability),
        ExprKind::Name(name) => column_for(name, capability, class, fields),
        ExprKind::Unary { op, expr } => {
            let inner = lower(expr, capability, class, fields)?;
            match op {
                UnOp::Not => Ok(format!("(NOT {inner})")),
                UnOp::Neg => {
                    // Only a folded negative literal is in the subset; the
                    // literal arm above cannot produce it because the token
                    // grammar yields non-negative integers, so `-3` arrives
                    // here as Neg(Int). Negating anything else is arithmetic.
                    match &expr.kind {
                        ExprKind::Int(value) => Ok(format!("-{value}")),
                        _ => reject("negation of a non-literal (arithmetic)"),
                    }
                }
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            // Null comparisons: `field == null` / `field != null` (and the
            // symmetric forms) become IS NULL / IS NOT NULL.
            if matches!(op, BinOp::Eq | BinOp::Ne)
                && (matches!(lhs.kind, ExprKind::Null) || matches!(rhs.kind, ExprKind::Null))
            {
                let both_null =
                    matches!(lhs.kind, ExprKind::Null) && matches!(rhs.kind, ExprKind::Null);
                if both_null {
                    return Ok(match op {
                        BinOp::Eq => "TRUE".to_string(),
                        _ => "FALSE".to_string(),
                    });
                }
                let operand = if matches!(lhs.kind, ExprKind::Null) {
                    lower(rhs, capability, class, fields)?
                } else {
                    lower(lhs, capability, class, fields)?
                };
                return Ok(match op {
                    BinOp::Eq => format!("({operand} IS NULL)"),
                    _ => format!("({operand} IS NOT NULL)"),
                });
            }
            let sql_op = match op {
                BinOp::Eq => "=",
                BinOp::Ne => "<>",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "AND",
                BinOp::Or => "OR",
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    return reject(&format!("arithmetic (`{op}`)"));
                }
            };
            Ok(format!(
                "({} {sql_op} {})",
                lower(lhs, capability, class, fields)?,
                lower(rhs, capability, class, fields)?
            ))
        }
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => {
            if *optional_safe {
                return reject("`?.` (safe navigation)");
            }
            let receiver_text = write_path(receiver);
            reject(&format!(
                "field access `{receiver_text}.{}`, which is not an \
                 own-table column; only bare field references are supported",
                name.value
            ))
        }
        ExprKind::Coalesce { .. } => reject("`?:`"),
        ExprKind::Call { .. } => reject("an operation call"),
        ExprKind::Algebra { kind, .. } => reject(&format!("collection algebra `{kind}`")),
        ExprKind::If { .. } => reject("`if`"),
        ExprKind::Let { .. } => reject("`let`"),
        ExprKind::Lambda { .. } => reject("a lambda"),
        ExprKind::ListLiteral(_) => reject("list literals"),
    }
}

/// Resolve a bare field reference to its quoted Postgres column.
fn column_for(
    name: &str,
    capability: &str,
    class: &str,
    fields: &HashMap<String, String>,
) -> Result<String, String> {
    let column = fields
        .get(name)
        .or_else(|| fields.get(&codegraph_naming::to_snake_case(name)))
        .ok_or_else(|| {
            format!(
                "capability `{capability}`: `when` condition references \
                 `{name}`, which is not a column of `{class}`"
            )
        })?;
    Ok(quote_pg_column(column))
}

/// Render an expression path back to source-like text for diagnostics.
fn write_path(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => format!(
            "{}{}{}",
            write_path(receiver),
            if *optional_safe { "?." } else { "." },
            name.value
        ),
        _ => "<expression>".to_string(),
    }
}

/// Render `text` as a Postgres string literal, doubling single quotes.
fn string_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Lower a `date("...")` literal to a Postgres `DATE '...'` literal.
///
/// The literal text is format-validated before interpolation (rex-expr's
/// parse-only mode does not validate it — that happens in the checker, and
/// `.actor` policies reaching the graph have already been typechecked by
/// rex-driver, which guarantees the compared field is date-typed).
fn date_literal(text: &str, capability: &str) -> Result<String, String> {
    if parse_date_text(text).is_some() {
        Ok(format!("DATE '{text}'"))
    } else {
        Err(format!(
            "capability `{capability}`: `when` condition uses the date \
             literal {text:?}, which is not a calendar date `YYYY-MM-DD`"
        ))
    }
}

/// Validate `YYYY-MM-DD` (extended ISO-8601 calendar date, proleptic
/// Gregorian, optional `-` year sign) and return the components.
///
/// Mirrors rex-expr's private `parse_date_text` / `rex_runtime::Date::from_str`
/// calendar rules so both sides accept exactly the same literal text.
fn parse_date_text(text: &str) -> Option<(i32, u32, u32)> {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1i64, rest),
        None => (1, text),
    };
    let mut parts = rest.split('-');
    let (year_text, month_text, day_text) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let number = |part: &str, min_width: usize| -> Option<i64> {
        if part.len() < min_width || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse::<i64>().ok()
    };
    let year = i32::try_from(sign * number(year_text, 4)?).ok()?;
    let month = u32::try_from(number(month_text, 2)?).ok()?;
    let day = u32::try_from(number(day_text, 2)?).ok()?;
    if month == 0 || month > 12 {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let length = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > length {
        return None;
    }
    Some((year, month, day))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn lower(source: &str) -> Result<String, String> {
        lower_when_expr(
            source,
            "ReviewCandidate",
            "CandidateType",
            &fields(&[
                ("status", "status"),
                ("amount", "amount"),
                ("internal", "internal"),
                ("verified", "verified"),
                ("order", "order"),
                ("start_date", "start_date"),
            ]),
        )
    }

    fn assert_sql(source: &str, expected: &str) {
        assert_eq!(lower(source).unwrap(), expected);
    }

    fn assert_refusal(source: &str, construct: &str) {
        let err = lower(source).expect_err("expected refusal");
        assert!(
            err.contains("capability `ReviewCandidate`"),
            "refusal must name the capability, got: {err}"
        );
        assert!(
            err.contains(construct),
            "refusal must mention {construct}, got: {err}"
        );
    }

    // ── field refs ──────────────────────────────────────────────────────

    #[test]
    fn field_ref_resolves_to_column() {
        assert_sql("status", "status");
    }

    #[test]
    fn field_ref_quotes_reserved_words() {
        assert_sql("order == 1", "(\"order\" = 1)");
    }

    #[test]
    fn unknown_field_ref_is_refused_naming_capability() {
        assert_refusal("ghost == 1", "`ghost`");
    }

    // ── literals ────────────────────────────────────────────────────────

    #[test]
    fn string_literal_escapes_single_quotes() {
        // rexlang strings are double-quoted; embedded single quotes must be
        // doubled for Postgres.
        assert_sql("status == \"it's\"", "(status = 'it''s')");
    }

    #[test]
    fn double_quoted_string_literal_lowers() {
        assert_sql("status == \"active\"", "(status = 'active')");
    }

    #[test]
    fn integer_literal_lowers() {
        assert_sql("amount == 42", "(amount = 42)");
    }

    #[test]
    fn negative_integer_literal_lowers() {
        assert_sql("amount == -3", "(amount = -3)");
    }

    #[test]
    fn boolean_literals_lower_to_true_false() {
        assert_sql("internal == true", "(internal = TRUE)");
        assert_sql("verified == false", "(verified = FALSE)");
    }

    // ── comparisons ─────────────────────────────────────────────────────

    #[test]
    fn equality_operators_map_to_sql_equals() {
        assert_sql("amount == 1", "(amount = 1)");
        assert_sql("amount = 1", "(amount = 1)");
    }

    #[test]
    fn inequality_maps_to_angle_bracket() {
        assert_sql("amount != 1", "(amount <> 1)");
    }

    #[test]
    fn relational_operators_pass_through() {
        assert_sql("amount < 1", "(amount < 1)");
        assert_sql("amount <= 1000", "(amount <= 1000)");
        assert_sql("amount > 0", "(amount > 0)");
        assert_sql("amount >= 5", "(amount >= 5)");
    }

    // ── boolean algebra ─────────────────────────────────────────────────

    #[test]
    fn logical_operators_map_to_sql_keywords() {
        assert_sql("internal && verified", "(internal AND verified)");
        assert_sql("internal || verified", "(internal OR verified)");
    }

    #[test]
    fn not_maps_to_sql_not() {
        assert_sql("!internal", "(NOT internal)");
    }

    #[test]
    fn parentheses_are_preserved_and_fully_parenthesized() {
        assert_sql(
            "(internal || verified) && amount > 0",
            "((internal OR verified) AND (amount > 0))",
        );
    }

    // ── null comparisons ────────────────────────────────────────────────

    #[test]
    fn equality_with_null_becomes_is_null() {
        assert_sql("status == null", "(status IS NULL)");
    }

    #[test]
    fn inequality_with_null_becomes_is_not_null() {
        assert_sql("status != null", "(status IS NOT NULL)");
    }

    #[test]
    fn null_on_the_left_hand_side_is_handled_symmetrically() {
        assert_sql("null == status", "(status IS NULL)");
        assert_sql("null != status", "(status IS NOT NULL)");
    }

    // ── refusals: everything outside the closed subset ──────────────────

    #[test]
    fn arithmetic_is_refused() {
        assert_refusal("amount + 1 > 0", "arithmetic");
        assert_refusal("amount - 1 > 0", "arithmetic");
        assert_refusal("amount * 2 > 0", "arithmetic");
        assert_refusal("amount / 2 > 0", "arithmetic");
    }

    #[test]
    fn negation_of_a_non_literal_is_refused() {
        assert_refusal("-amount > 0", "negation");
    }

    #[test]
    fn safe_navigation_is_refused() {
        assert_refusal("assignee?.name == \"x\"", "`?.`");
    }

    #[test]
    fn dotted_navigation_is_refused() {
        assert_refusal("assignee.name == \"x\"", "assignee.name");
    }

    #[test]
    fn coalesce_is_refused() {
        assert_refusal("status ?: \"x\" == \"y\"", "`?:`");
    }

    #[test]
    fn operation_call_is_refused() {
        assert_refusal("title.trim() == \"x\"", "operation call");
    }

    #[test]
    fn collection_algebra_is_refused() {
        assert_refusal("tags.any(t => t == \"x\")", "collection algebra");
        assert_refusal("tags.size() > 1", "collection algebra");
    }

    #[test]
    fn if_expression_is_refused() {
        assert_refusal("if internal { true } else { false }", "`if`");
    }

    #[test]
    fn let_binding_is_refused() {
        assert_refusal("let x = 1; x > 0", "`let`");
    }

    #[test]
    fn lambda_is_refused() {
        assert_refusal("t => t == \"x\"", "lambda");
    }

    #[test]
    fn list_literal_is_refused() {
        assert_refusal("status == [\"a\"]", "list literal");
    }

    #[test]
    fn bare_null_is_refused() {
        assert_refusal("null", "`null`");
    }

    #[test]
    fn date_literal_lowers_to_pg_date() {
        assert_sql(
            "start_date == date(\"2026-09-17\")",
            "(start_date = DATE '2026-09-17')",
        );
    }

    #[test]
    fn date_literal_supports_ordering_comparisons() {
        assert_sql(
            "start_date >= date(\"2026-01-01\")",
            "(start_date >= DATE '2026-01-01')",
        );
        assert_sql(
            "start_date < date(\"2026-01-01\")",
            "(start_date < DATE '2026-01-01')",
        );
    }

    #[test]
    fn date_literal_combines_with_boolean_and_null_algebra() {
        assert_sql(
            "internal && start_date > date(\"2026-01-01\")",
            "(internal AND (start_date > DATE '2026-01-01'))",
        );
        assert_sql(
            "start_date == null || start_date < date(\"2020-01-01\")",
            "((start_date IS NULL) OR (start_date < DATE '2020-01-01'))",
        );
    }

    #[test]
    fn date_literal_accepts_leap_days() {
        assert_sql(
            "start_date == date(\"2028-02-29\")",
            "(start_date = DATE '2028-02-29')",
        );
    }

    #[test]
    fn malformed_date_literal_is_refused_naming_capability() {
        assert_refusal("start_date == date(\"not-a-date\")", "YYYY-MM-DD");
        assert_refusal("start_date == date(\"2026-13-01\")", "YYYY-MM-DD");
        assert_refusal("start_date == date(\"2026-02-30\")", "YYYY-MM-DD");
        assert_refusal("start_date == date(\"2026-2-3\")", "YYYY-MM-DD");
    }

    #[test]
    fn calendar_algebra_on_dates_is_still_refused() {
        // Calendar functions are method calls — outside the closed subset
        // (no clock, no computation in RLS predicates).
        let err =
            lower("start_date.plus_days(3) > date(\"2026-01-01\")").expect_err("expected refusal");
        assert!(
            err.contains("capability `ReviewCandidate`"),
            "refusal must name the capability, got: {err}"
        );
    }

    #[test]
    fn parse_failure_is_refused_naming_capability() {
        assert_refusal("status ====", "valid rexlang expression syntax");
    }
}
