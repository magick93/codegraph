//! DSL validation-expression → zod check lowering for the IFML TanStack
//! form layer (issue #580).
//!
//! Typed IFML forms declare `validations: [...]` with positionally-paired
//! `messages: [...]`. This module lowers the CLOSED subset of those
//! expressions that has a zod analog into the shared
//! [`crate::forms`] IR ([`ZodCheck`]s); everything else is SKIPPED — the
//! zod schema simply does not encode it, and never renders invalid zod.
//!
//! | Expression | Lowers to |
//! |---|---|
//! | `len(x) > n` / `len(x) >= n` | `MinLength(n)` (the pin's shape: `len(title) > 2` → `z.string().min(2, …)`) |
//! | `len(x) < n` / `len(x) <= n` | `MaxLength(n)` |
//! | `x > n` / `x >= n` on a number field | `Min(n)` |
//! | `x < n` / `x <= n` on a number field | `Max(n)` |
//! | `minItems(x, n)` / `maxItems(x, n)` on an array field | `MinItems(n)` / `MaxItems(n)` |
//!
//! Strict comparisons pass the bound through as written (`>` and `>=`
//! lower identically) — the DSL's `len()`/numeric comparisons carry the
//! exact bound in practice, and the pin fixes `len(title) > 2` →
//! `min(2, …)`.
//!
//! Deliberately NOT lowered (skipped, never an error): string/boolean
//! equality (`x == "lit"`), regex matches, arithmetic, `&&`/`||`,
//! negation, dotted navigation, unknown calls. A skipped validation's
//! positionally-paired message carries forward to the nearest LOWERED
//! check unless that check already carries its own positional message —
//! so the author's prose survives whenever the schema can encode a check.
//! Presence checks (`x != null`) are unnecessary by construction: DSL
//! `required:` maps to plain zod vs `.optional()`.

use rex_ifml::{BinOp, Expression};

use crate::forms::{ZodCheck, ZodKind};

/// The `len`/`minItems`/`maxItems` call names the lowering recognizes.
const LEN_CALL: &str = "len";
const MIN_ITEMS_CALL: &str = "minItems";
const MAX_ITEMS_CALL: &str = "maxItems";

/// The `null` identifier in presence checks (`x != null`) — recognized
/// only to document the skip; required/optional carries presence.
const NULL_IDENT: &str = "null";

/// Lower one field's validations into zod checks, pairing each
/// validation's message positionally and carrying a skipped validation's
/// message to the next lowered check (unless that check has its own).
pub(crate) fn lower_field_checks(
    kind: &ZodKind,
    validations: &[Expression],
    messages: &[String],
) -> Vec<ZodCheck> {
    let mut checks: Vec<ZodCheck> = Vec::new();
    let mut carried: Option<String> = None;
    for (i, expr) in validations.iter().enumerate() {
        let positional = messages.get(i).cloned();
        match lower_one(kind, expr) {
            Some(check) => {
                let message = positional.or(carried.take());
                checks.push(with_message(check, message));
            }
            None => {
                if let Some(m) = positional {
                    carried.get_or_insert(m);
                }
            }
        }
    }
    checks
}

fn lower_one(kind: &ZodKind, expr: &Expression) -> Option<ZodCheck> {
    match expr {
        Expression::BinOp { left, op, right } => lower_comparison(kind, left, op, right)
            .or_else(|| lower_comparison(kind, right, &flip(op), left)),
        Expression::Call { name, args } => lower_call(kind, name, args),
        _ => None,
    }
}

/// `subject op NumLit` — `len(x)` comparisons on any field, numeric
/// comparisons on number fields. Flipped operand order is handled by the
/// caller via [`flip`].
fn lower_comparison(
    kind: &ZodKind,
    subject: &Expression,
    op: &BinOp,
    rhs: &Expression,
) -> Option<ZodCheck> {
    let value = num_literal(rhs)?;
    match subject {
        Expression::Call { name, args } if name == LEN_CALL && !args.is_empty() => {
            let n = usize_from(value)?;
            match op {
                BinOp::Gt | BinOp::Ge => Some(ZodCheck::MinLength {
                    value: n,
                    message: None,
                }),
                BinOp::Lt | BinOp::Le => Some(ZodCheck::MaxLength {
                    value: n,
                    message: None,
                }),
                _ => None,
            }
        }
        Expression::Ident(name) if matches!(kind, ZodKind::Number) && name != NULL_IDENT => {
            match op {
                BinOp::Gt | BinOp::Ge => Some(ZodCheck::Min {
                    value,
                    message: None,
                }),
                BinOp::Lt | BinOp::Le => Some(ZodCheck::Max {
                    value,
                    message: None,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `minItems(x, n)` / `maxItems(x, n)` — array fields only.
fn lower_call(kind: &ZodKind, name: &str, args: &[Expression]) -> Option<ZodCheck> {
    if !matches!(kind, ZodKind::Array(_)) {
        return None;
    }
    let n = usize_from(num_literal(args.get(1)?)?)?;
    match name {
        MIN_ITEMS_CALL => Some(ZodCheck::MinItems {
            value: n,
            message: None,
        }),
        MAX_ITEMS_CALL => Some(ZodCheck::MaxItems {
            value: n,
            message: None,
        }),
        _ => None,
    }
}

/// The numeric literal of a `NumLit` expression; everything else is None.
fn num_literal(expr: &Expression) -> Option<f64> {
    match expr {
        Expression::NumLit(value) => Some(value.value()),
        _ => None,
    }
}

/// The `usize` bound of a non-negative, integral literal.
fn usize_from(value: f64) -> Option<usize> {
    if value.is_sign_negative() || !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(value as usize)
}

/// Flip a comparison operator for right-hand-side subjects
/// (`2 < len(x)` ⟺ `len(x) > 2`).
fn flip(op: &BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Gt,
        BinOp::Le => BinOp::Ge,
        BinOp::Gt => BinOp::Lt,
        BinOp::Ge => BinOp::Le,
        other => other.clone(),
    }
}

/// Attach a message to a check that carries none (positional pairing);
/// checks rendered by [`crate::forms`] escape prose through
/// [`crate::forms::ts_string_literal`].
fn with_message(check: ZodCheck, message: Option<String>) -> ZodCheck {
    match (check, message) {
        (
            ZodCheck::MinLength {
                value,
                message: None,
            },
            Some(m),
        ) => ZodCheck::MinLength {
            value,
            message: Some(m),
        },
        (
            ZodCheck::MaxLength {
                value,
                message: None,
            },
            Some(m),
        ) => ZodCheck::MaxLength {
            value,
            message: Some(m),
        },
        (
            ZodCheck::Min {
                value,
                message: None,
            },
            Some(m),
        ) => ZodCheck::Min {
            value,
            message: Some(m),
        },
        (
            ZodCheck::Max {
                value,
                message: None,
            },
            Some(m),
        ) => ZodCheck::Max {
            value,
            message: Some(m),
        },
        (
            ZodCheck::MinItems {
                value,
                message: None,
            },
            Some(m),
        ) => ZodCheck::MinItems {
            value,
            message: Some(m),
        },
        (
            ZodCheck::MaxItems {
                value,
                message: None,
            },
            Some(m),
        ) => ZodCheck::MaxItems {
            value,
            message: Some(m),
        },
        (check, _) => check,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rex_ifml::Float;

    fn num(n: f64) -> Expression {
        Expression::NumLit(Float::new(n))
    }

    fn ident(name: &str) -> Expression {
        Expression::Ident(name.to_string())
    }

    fn len_gt(field: &str, n: f64) -> Expression {
        Expression::BinOp {
            left: Box::new(Expression::Call {
                name: "len".into(),
                args: vec![ident(field)],
            }),
            op: BinOp::Gt,
            right: Box::new(num(n)),
        }
    }

    fn string_kind() -> ZodKind {
        ZodKind::String
    }

    #[test]
    fn len_gt_lowers_to_min_length_with_the_paired_message() {
        let checks = lower_field_checks(
            &string_kind(),
            &[len_gt("title", 2.0)],
            &["Title too short".to_string()],
        );
        assert_eq!(
            checks,
            vec![ZodCheck::MinLength {
                value: 2,
                message: Some("Title too short".into())
            }]
        );
    }

    #[test]
    fn len_ge_ge_and_flipped_operands_lower_identically() {
        let ge = Expression::BinOp {
            left: Box::new(Expression::Call {
                name: "len".into(),
                args: vec![ident("title")],
            }),
            op: BinOp::Ge,
            right: Box::new(num(3.0)),
        };
        let flipped = Expression::BinOp {
            left: Box::new(num(3.0)),
            op: BinOp::Le,
            right: Box::new(Expression::Call {
                name: "len".into(),
                args: vec![ident("title")],
            }),
        };
        for expr in [ge, flipped] {
            let checks = lower_field_checks(&string_kind(), &[expr], &[]);
            assert_eq!(
                checks,
                vec![ZodCheck::MinLength {
                    value: 3,
                    message: None
                }]
            );
        }
    }

    #[test]
    fn len_lt_lowers_to_max_length() {
        let checks = lower_field_checks(
            &string_kind(),
            &[Expression::BinOp {
                left: Box::new(Expression::Call {
                    name: "len".into(),
                    args: vec![ident("title")],
                }),
                op: BinOp::Lt,
                right: Box::new(num(80.0)),
            }],
            &[],
        );
        assert_eq!(
            checks,
            vec![ZodCheck::MaxLength {
                value: 80,
                message: None
            }]
        );
    }

    #[test]
    fn number_bounds_lower_to_min_max() {
        let checks = lower_field_checks(
            &ZodKind::Number,
            &[
                Expression::BinOp {
                    left: Box::new(ident("amount")),
                    op: BinOp::Gt,
                    right: Box::new(num(0.0)),
                },
                Expression::BinOp {
                    left: Box::new(ident("amount")),
                    op: BinOp::Le,
                    right: Box::new(num(1_000_000.0)),
                },
            ],
            &["Must be positive".to_string()],
        );
        assert_eq!(
            checks,
            vec![
                ZodCheck::Min {
                    value: 0.0,
                    message: Some("Must be positive".into())
                },
                ZodCheck::Max {
                    value: 1_000_000.0,
                    message: None
                },
            ]
        );
    }

    #[test]
    fn numeric_comparisons_never_lower_on_string_fields() {
        let checks = lower_field_checks(
            &string_kind(),
            &[Expression::BinOp {
                left: Box::new(ident("title")),
                op: BinOp::Gt,
                right: Box::new(num(3.0)),
            }],
            &[],
        );
        assert!(checks.is_empty());
    }

    #[test]
    fn item_calls_lower_on_array_fields_only() {
        let call = |name: &str| Expression::Call {
            name: name.into(),
            args: vec![ident("tags"), num(2.0)],
        };
        let array = ZodKind::Array(Box::new(crate::forms::ZodField {
            name: String::new(),
            kind: crate::forms::ZodKind::Uuid,
            required: true,
            checks: Vec::new(),
        }));
        let checks = lower_field_checks(&array, &[call("minItems"), call("maxItems")], &[]);
        assert_eq!(
            checks,
            vec![
                ZodCheck::MinItems {
                    value: 2,
                    message: None
                },
                ZodCheck::MaxItems {
                    value: 2,
                    message: None
                },
            ]
        );
        assert!(
            lower_field_checks(&string_kind(), &[call("minItems")], &[]).is_empty(),
            "minItems on a non-array field is skipped"
        );
    }

    #[test]
    fn unsupported_operators_are_skipped_but_their_message_carries_forward() {
        // `title == ""` has no zod analog; the paired message attaches to
        // the NEXT lowered check (which carries no positional message of
        // its own).
        let eq = || Expression::BinOp {
            left: Box::new(ident("title")),
            op: BinOp::Eq,
            right: Box::new(Expression::StringLit(String::new())),
        };
        let checks = lower_field_checks(
            &string_kind(),
            &[eq(), len_gt("title", 2.0)],
            &["Required".to_string(), "Title too short".to_string()],
        );
        assert_eq!(
            checks,
            vec![ZodCheck::MinLength {
                value: 2,
                // Positional pairing wins: check 1 keeps its own message.
                message: Some("Title too short".into())
            }]
        );

        // With no positional message on the target check, the carried one
        // fills the gap.
        let checks = lower_field_checks(
            &string_kind(),
            &[eq(), len_gt("title", 2.0)],
            &["Required".to_string()],
        );
        assert_eq!(
            checks,
            vec![ZodCheck::MinLength {
                value: 2,
                message: Some("Required".into())
            }]
        );
    }

    #[test]
    fn presence_checks_and_arithmetic_lower_to_nothing() {
        let ne_null = Expression::BinOp {
            left: Box::new(ident("title")),
            op: BinOp::Ne,
            right: Box::new(ident("null")),
        };
        let arith = Expression::BinOp {
            left: Box::new(ident("amount")),
            op: BinOp::Add,
            right: Box::new(num(1.0)),
        };
        for expr in [ne_null, arith] {
            let checks = lower_field_checks(&ZodKind::Number, &[expr], &[]);
            assert!(checks.is_empty());
        }
    }

    #[test]
    fn messages_render_through_ts_string_literal_escaping() {
        let checks = lower_field_checks(
            &string_kind(),
            &[len_gt("title", 2.0)],
            &["a \"quoted\"\nline".to_string()],
        );
        let field = crate::forms::ZodField {
            name: "title".into(),
            kind: string_kind(),
            required: true,
            checks,
        };
        assert_eq!(
            field.render(""),
            "title: z.string().min(2, \"a \\\"quoted\\\"\\nline\"),"
        );
    }
}
