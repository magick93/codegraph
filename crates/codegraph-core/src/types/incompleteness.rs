//! The incompleteness taxonomy (issue #279, from Morphir's
//! `Incomplete = Hole(reason) | Draft`): partial authoring is carried as a
//! structured value instead of a silently-absent node or a bare string.
//! Surfaces in mox ingest stats, `MoxState` LSP diagnostics, and doctor
//! output.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Why a definition is incomplete: an unresolvable reference naming its
/// target, a type mismatch naming both sides, or an explicit draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IncompletenessReason {
    UnresolvedReference { target: String },
    TypeMismatch { expected: String, found: String },
    Draft,
}

impl fmt::Display for IncompletenessReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IncompletenessReason::UnresolvedReference { target } => {
                write!(f, "UnresolvedReference(target: '{target}')")
            }
            IncompletenessReason::TypeMismatch { expected, found } => {
                write!(f, "TypeMismatch(expected: '{expected}', found: '{found}')")
            }
            IncompletenessReason::Draft => f.write_str("Draft"),
        }
    }
}

/// `Incompleteness { reason }` — the structured partialness carrier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incompleteness {
    pub reason: IncompletenessReason,
}

impl Incompleteness {
    pub fn unresolved_reference(target: impl Into<String>) -> Self {
        Self {
            reason: IncompletenessReason::UnresolvedReference {
                target: target.into(),
            },
        }
    }

    pub fn draft() -> Self {
        Self {
            reason: IncompletenessReason::Draft,
        }
    }

    /// The serde tag of the reason — stable vocabulary for non-Rust
    /// consumers (LSP diagnostic payloads, doctor tooling).
    pub fn kind(&self) -> &'static str {
        match &self.reason {
            IncompletenessReason::UnresolvedReference { .. } => "unresolved_reference",
            IncompletenessReason::TypeMismatch { .. } => "type_mismatch",
            IncompletenessReason::Draft => "draft",
        }
    }
}

impl fmt::Display for Incompleteness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_carries_the_vocabulary() {
        assert_eq!(
            Incompleteness::unresolved_reference("Widget").to_string(),
            "UnresolvedReference(target: 'Widget')"
        );
        assert_eq!(
            Incompleteness {
                reason: IncompletenessReason::TypeMismatch {
                    expected: "int".into(),
                    found: "string".into(),
                },
            }
            .to_string(),
            "TypeMismatch(expected: 'int', found: 'string')"
        );
        assert_eq!(Incompleteness::draft().to_string(), "Draft");
    }

    #[test]
    fn serde_tags_are_snake_case_kinds() {
        let json =
            serde_json::to_value(Incompleteness::unresolved_reference("missing.json")).unwrap();
        assert_eq!(json["reason"]["kind"], "unresolved_reference");
        assert_eq!(json["reason"]["target"], "missing.json");
    }
}
