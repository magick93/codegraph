use serde::{Deserialize, Serialize};

/// Closed vocabulary of field dimensions a `[[column]]` rule can target.
/// This is the vocabulary Pass 1 infers against and rules select on;
/// unknown dimension strings in TOML are a parse error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dimension {
    /// Free text (names, descriptions) — default left-aligned rendering.
    Text,
    /// Countable quantity — right-aligned with tabular figures.
    Quantity,
    /// Monetary amount — right-aligned, formatted with `[format].currency`.
    Money,
    /// A point in time — rendered localized (template-side).
    TimePoint,
    /// Status/lifecycle category — renders as tone-mapped chips.
    StatusCategory,
    /// An identifier — renders as copy-to-clipboard chips.
    Identifier,
    /// A reference to another entity — renders as links.
    Reference,
    /// A boolean flag — renders as chips.
    Flag,
}

impl Dimension {
    /// The kebab-case TOML spelling of this dimension (mirrors the
    /// `kebab-case` serde renaming).
    pub fn as_str(self) -> &'static str {
        match self {
            Dimension::Text => "text",
            Dimension::Quantity => "quantity",
            Dimension::Money => "money",
            Dimension::TimePoint => "time-point",
            Dimension::StatusCategory => "status-category",
            Dimension::Identifier => "identifier",
            Dimension::Reference => "reference",
            Dimension::Flag => "flag",
        }
    }

    /// Every valid kebab-case spelling, in enum order (used by error
    /// messages that enumerate the accepted values).
    pub fn all_kebab_names() -> Vec<&'static str> {
        vec![
            Dimension::Text,
            Dimension::Quantity,
            Dimension::Money,
            Dimension::TimePoint,
            Dimension::StatusCategory,
            Dimension::Identifier,
            Dimension::Reference,
            Dimension::Flag,
        ]
        .into_iter()
        .map(Dimension::as_str)
        .collect()
    }
}
