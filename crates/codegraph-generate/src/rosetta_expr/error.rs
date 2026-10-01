use std::fmt;

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
    pub(super) fn new(kind: &str, detail: impl Into<String>) -> Self {
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
