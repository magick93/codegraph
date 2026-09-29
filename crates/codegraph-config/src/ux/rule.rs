use serde::{Deserialize, Serialize};

use crate::ux::dimension::Dimension;
use crate::ux::presentation::{Align, Display, FormatConfig, ToneMap};

/// Root of a ux-rules TOML document: format baseline, column/collection
/// rule arrays, and list-action settings. Unknown keys are errors
/// (`deny_unknown_fields`).
///
/// Rule resolution is **first-match-wins** per selector tier, and
/// `merge` prepends project rules ahead of the built-in pack, so a project
/// rule shadows the pack wholesale for the selectors it covers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UxRules {
    /// Locale/currency formatting baseline.
    #[serde(default)]
    pub format: FormatConfig,
    /// Column rules (TOML `[[column]]`), resolved first-match-wins.
    #[serde(default, rename = "column")]
    pub columns: Vec<ColumnRule>,
    /// Collection rules (TOML `[[collection]]`), resolved first-match-wins.
    #[serde(default, rename = "collection")]
    pub collections: Vec<CollectionRule>,
    /// List-row action settings (TOML `[actions]`).
    #[serde(default)]
    pub actions: ActionRules,
}

// Deliberate divergence from `ifml_components.rs`, which tolerates unknown
// keys: a typo'd ux-rules selector key must fail the parse instead of
// silently no-oping the rule.

/// One `[[column]]` presentation rule. Selector keys (`dimension`,
/// `classification`, `pg_type`, `name_pattern`) decide which columns a rule
/// applies to; payload keys decide how matched columns render. Rules are
/// resolved first-match-wins, and project rules sit ahead of pack rules
/// after `merge`, so an earlier match shadows everything behind it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnRule {
    /// Selector: the inferred field dimension to match.
    #[serde(default)]
    pub dimension: Option<Dimension>,
    /// Selector: the classifier classification to match (e.g. "codelist").
    #[serde(default)]
    pub classification: Option<String>,
    /// Selector: the PostgreSQL type to match (e.g. "bool", "timestamptz").
    #[serde(default)]
    pub pg_type: Option<String>,
    /// Selector: glob on the property name (`*` is the only wildcard).
    /// Empty globs and match-everything (`*`-only) globs are parse errors.
    #[serde(default)]
    pub name_pattern: Option<String>,
    /// Payload: how a matched value renders.
    #[serde(default)]
    pub display: Option<Display>,
    /// Payload: column alignment.
    #[serde(default)]
    pub align: Option<Align>,
    /// Payload: status keyword → badge variant for chip rendering.
    #[serde(default)]
    pub tone: Option<ToneMap>,
    /// Payload: whether the column is sortable in tables.
    #[serde(default)]
    pub sortable: Option<bool>,
}

impl ColumnRule {
    /// Selector identity for duplicate detection: the full selector tuple
    /// (`None` == unset, so `None == None`). Two rules with equal keys are
    /// duplicates — the first wins.
    pub(crate) fn selector_key(
        &self,
    ) -> (Option<Dimension>, Option<&str>, Option<&str>, Option<&str>) {
        (
            self.dimension,
            self.classification.as_deref(),
            self.pg_type.as_deref(),
            self.name_pattern.as_deref(),
        )
    }
}

/// One `[[collection]]` rule deciding how an entity list renders.
/// `entity_pattern` + `display` form the selector pair for duplicate
/// detection; resolution is first-match-wins with project rules ahead of
/// pack rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionRule {
    /// Selector: glob on the entity title (`*` alone is legal — collection
    /// opt-in is coarse by design).
    #[serde(default)]
    pub entity_pattern: Option<String>,
    /// Payload/selector: "table" (default) or "timeline"; "timeline"
    /// requires `order_by`.
    #[serde(default)]
    pub display: Option<String>,
    /// Payload: datetime property the timeline sorts by (required for
    /// "timeline", meaningless otherwise — a warning).
    #[serde(default)]
    pub order_by: Option<String>,
    /// Payload: entity property used as the timeline entry title.
    #[serde(default)]
    pub title_field: Option<String>,
    /// Payload: extra fields shown on each timeline entry. Field names
    /// resolve at GENERATION time (config can't see the graph); an unknown
    /// preview field is a generation-time diagnostic, not a parse error.
    #[serde(default)]
    pub preview: Vec<String>,
}

impl CollectionRule {
    /// Selector identity for duplicate detection: the `(entity_pattern,
    /// display)` pair (`None` == unset, so `None == None`).
    pub(crate) fn selector_key(&self) -> (Option<&str>, Option<&str>) {
        (self.entity_pattern.as_deref(), self.display.as_deref())
    }
}

/// `[actions]` settings for list-row actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRules {
    /// At most this many actions render inline per list row; the rest
    /// collapse into an overflow menu. Must be >= 1 (0 would hide every
    /// action — a parse error).
    #[serde(default = "default_inline_max")]
    pub inline_max: usize,
    /// Action names that require user confirmation (e.g. "delete").
    #[serde(default)]
    pub confirm: Vec<String>,
}

fn default_inline_max() -> usize {
    1
}

impl Default for ActionRules {
    fn default() -> Self {
        Self {
            inline_max: default_inline_max(),
            confirm: Vec::new(),
        }
    }
}

/// Minimal glob matcher for name/entity patterns: the only wildcard is `*`
/// (any run of characters, including none). Matching is case-sensitive;
/// every other character is literal.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star_p, mut star_t) = (usize::MAX, 0usize);
    while t < text.len() {
        if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star_p = p;
            star_t = t;
            p += 1;
        } else if star_p != usize::MAX {
            p = star_p + 1;
            star_t += 1;
            t = star_t;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_star_suffix() {
        assert!(glob_match("refund*", "refunds"));
        assert!(glob_match("refund*", "refund"));
        assert!(!glob_match("refund*", "pre_refund"));
    }

    #[test]
    fn glob_match_star_prefix() {
        assert!(glob_match("*_amount", "total_amount"));
        assert!(!glob_match("*_amount", "amount_total"));
        assert!(!glob_match("*_amount", "amount"));
    }

    #[test]
    fn glob_match_infix_and_bare_star() {
        assert!(glob_match("*amount*", "total_amount_nzd"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a*bc", "aXbc"));
    }

    #[test]
    fn glob_match_without_wildcards_is_exact() {
        assert!(glob_match("status", "status"));
        assert!(!glob_match("status", "state"));
        assert!(glob_match("", ""));
        assert!(!glob_match("", "x"));
    }

    #[test]
    fn glob_match_is_case_sensitive() {
        assert!(!glob_match("Refund*", "refunds"));
    }

    #[test]
    fn default_action_rules_keep_one_inline() {
        let actions = ActionRules::default();
        assert_eq!(actions.inline_max, 1);
        assert!(actions.confirm.is_empty());
    }

    #[test]
    fn dimension_as_str_mirrors_kebab_rename() {
        assert_eq!(Dimension::TimePoint.as_str(), "time-point");
        assert_eq!(Dimension::StatusCategory.as_str(), "status-category");
        let names = Dimension::all_kebab_names();
        assert_eq!(names.len(), 8);
        assert_eq!(names[0], "text");
        assert_eq!(names[7], "flag");
    }
}
