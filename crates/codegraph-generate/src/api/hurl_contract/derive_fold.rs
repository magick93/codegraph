//! Folding a projection's derive expressions over a contract's seeded
//! create body (issue #768): the expected value IS what the handler
//! computes — the same bounded arithmetic subset, integer division
//! included.
//!
/// Fold one derive expression over the seeded create-body values (issue
/// #768): the same bounded arithmetic subset the read path lowers, so the
/// contract's expected value IS the value the handler computes. `None` for
/// any leaf the body does not seed (an optional column) or any construct
/// the subset refuses — the assert is omitted, never guessed.
pub(super) fn fold_derive(source: &str, values: &serde_json::Value) -> Option<DeriveFold> {
    use phero_expr::{BinOp, ExprKind, UnOp};
    let parsed = phero_expr::parse(source);
    let ast = match parsed.errors.is_empty() {
        true => parsed.ast?,
        false => return None,
    };
    fn fold(ast: &phero_expr::Expr, values: &serde_json::Value) -> Option<DeriveFold> {
        match &ast.kind {
            ExprKind::Int(value) => Some(DeriveFold::Int(*value)),
            ExprKind::Float(value) => Some(DeriveFold::Float(*value)),
            ExprKind::Unary {
                op: UnOp::Neg,
                expr,
            } => Some(fold(expr, values)?.neg()),
            ExprKind::Name(name) => {
                let snake = codegraph_naming::to_snake_case(name);
                let value = values
                    .get(&snake)?
                    .as_i64()
                    .map(DeriveFold::Int)
                    .or_else(|| values.get(&snake)?.as_f64().map(DeriveFold::Float))?;
                Some(value)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = fold(lhs, values)?;
                let r = fold(rhs, values)?;
                match op {
                    BinOp::Add => l.add(&r),
                    BinOp::Sub => l.sub(&r),
                    BinOp::Mul => l.mul(&r),
                    BinOp::Div => l.div(&r),
                    _ => None,
                }
            }
            ExprKind::Unary { op: UnOp::Not, .. }
            | ExprKind::String(_)
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Date { .. }
            | ExprKind::FeatureAccess { .. }
            | ExprKind::Coalesce { .. }
            | ExprKind::Call { .. }
            | ExprKind::FunctionCall { .. }
            | ExprKind::Algebra { .. }
            | ExprKind::If { .. }
            | ExprKind::Let { .. }
            | ExprKind::Lambda { .. }
            | ExprKind::ListLiteral(_) => None,
        }
    }
    fold(&ast, values)
}

/// The folded value of one derive expression over the seeded body — integer
/// arithmetic stays integer (Rust `/` truncates), a float operand widens.
#[derive(Debug, Clone, Copy)]
pub(super) enum DeriveFold {
    Int(i64),
    Float(f64),
}

impl DeriveFold {
    fn neg(self) -> Self {
        match self {
            Self::Int(v) => Self::Int(-v),
            Self::Float(v) => Self::Float(-v),
        }
    }
    fn add(self, other: &Self) -> Option<Self> {
        Some(match (self, *other) {
            (Self::Int(a), Self::Int(b)) => Self::Int(a.checked_add(b)?),
            (a, b) => Self::Float(a.as_f64()? + b.as_f64()?),
        })
    }
    fn sub(self, other: &Self) -> Option<Self> {
        Some(match (self, *other) {
            (Self::Int(a), Self::Int(b)) => Self::Int(a.checked_sub(b)?),
            (a, b) => Self::Float(a.as_f64()? - b.as_f64()?),
        })
    }
    fn mul(self, other: &Self) -> Option<Self> {
        Some(match (self, *other) {
            (Self::Int(a), Self::Int(b)) => Self::Int(a.checked_mul(b)?),
            (a, b) => Self::Float(a.as_f64()? * b.as_f64()?),
        })
    }
    fn div(self, other: &Self) -> Option<Self> {
        Some(match (self, *other) {
            // Rust's `/` TRUNCATES toward zero — the lowering emits plain
            // `/`, so the fold must truncate identically (div_euclid floors
            // and would mis-assert negative seeds). The i64 overflow pair
            // (MIN / −1) panics in Rust — the handler would 500, so the
            // assert is omitted rather than pinned to a value no run
            // returns.
            (Self::Int(_), Self::Int(0)) => return None,
            (Self::Int(a), Self::Int(b)) if a == i64::MIN && b == -1 => {
                return None;
            }
            (Self::Int(a), Self::Int(b)) => Self::Int(a / b),
            (a, b) => {
                let (a, b) = (a.as_f64()?, b.as_f64()?);
                if b == 0.0 {
                    return None;
                }
                Self::Float(a / b)
            }
        })
    }
    fn as_f64(self) -> Option<f64> {
        match self {
            Self::Int(v) => Some(v as f64),
            Self::Float(v) => Some(v),
        }
    }
}

impl std::fmt::Display for DeriveFold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_division_truncates_like_the_lowering() {
        // The lowering emits plain `/` (Rust truncates toward zero); the
        // fold must agree for negative dividends (a div_euclid floor would
        // mis-assert the contract).
        let expr = phero_expr::parse("a / b").ast.expect("ast");
        let values = serde_json::json!({ "a": -42, "b": 60 });
        let folded = fold_derive("a / b", &values).expect("fold");
        let _ = expr;
        // trunc(−0.7) = 0 — a div_euclid floor would assert −1.
        assert_eq!(folded.to_string(), "0");
    }

    #[test]
    fn negative_division_truncates_toward_zero_not_floor() {
        // (-59)/2: Rust `/` = −29 (truncation); a floor would say −30 —
        // exactly the div_euclid class the fold must not regress to.
        let values = serde_json::json!({ "a": -59, "b": 2 });
        let folded = fold_derive("a / b", &values).expect("fold");
        assert_eq!(folded.to_string(), "-29");
    }

    #[test]
    fn division_by_a_seeded_zero_folds_to_no_assert() {
        let values = serde_json::json!({ "a": 1, "b": 0 });
        assert!(fold_derive("a / b", &values).is_none());
    }
}
