use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How a column value renders in tables and details views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Display {
    Raw,
    Chip,
    CopyChip,
    Link,
}

/// Column alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    Left,
    Right,
}

/// Number/date formatting settings. Defaults to the codegraph locale
/// baseline; packs and project configs override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormatConfig {
    /// BCP-47 locale for number/date rendering (pack default: "en-NZ").
    #[serde(default = "default_locale")]
    pub locale: String,
    /// ISO-4217 currency code for money formatting; `None` = no currency
    /// formatting.
    #[serde(default)]
    pub currency: Option<String>,
}

fn default_locale() -> String {
    "en-NZ".to_string()
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            locale: default_locale(),
            currency: None,
        }
    }
}

/// Badge variants a `tone` map value may take, mirroring the workflow
/// panel's variant set. Chips render badge variants, not free CSS —
/// unknown values are a parse error naming this set.
pub const VALID_TONE_VALUES: [&str; 4] = ["default", "secondary", "destructive", "outline"];

/// Badge variant used when a status value has no tone keyword, mirroring the
/// fallback of `variantFor()` in `workflow_panel.tera`.
pub const WORKFLOW_TONE_FALLBACK: &str = "outline";

/// Keyword → badge-variant tone mapping for status values.
///
/// `ToneMap::default()` mirrors the keyword table of `variantFor()` in
/// `templates/ui/scaffold/workflow_panel.tera` (`active`/`approved` →
/// `default`, `pending`/`draft` → `secondary`) so chips and workflow badges
/// agree by construction. The template's terminal → `destructive` branch is
/// driven by the generator's is-terminal flag, not a keyword, so no keyword
/// maps to `destructive` by default.
///
/// Values must be one of [`VALID_TONE_VALUES`] (parse-validated — chips
/// render badge variants, not free CSS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToneMap(pub BTreeMap<String, String>);

impl ToneMap {
    /// Variant for a status value: the configured keyword mapping, else
    /// [`WORKFLOW_TONE_FALLBACK`].
    pub fn lookup(&self, value: &str) -> &str {
        self.0
            .get(value)
            .map(String::as_str)
            .unwrap_or(WORKFLOW_TONE_FALLBACK)
    }

    /// True when no keyword is mapped (serialize-skip signal: templates
    /// treat an absent map as "no tone entries").
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for ToneMap {
    fn default() -> Self {
        Self(BTreeMap::from([
            ("active".to_string(), "default".to_string()),
            ("approved".to_string(), "default".to_string()),
            ("pending".to_string(), "secondary".to_string()),
            ("draft".to_string(), "secondary".to_string()),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_map_default_mirrors_workflow_panel_variant_for() {
        let tone = ToneMap::default();
        assert_eq!(tone.lookup("active"), "default");
        assert_eq!(tone.lookup("approved"), "default");
        assert_eq!(tone.lookup("pending"), "secondary");
        assert_eq!(tone.lookup("draft"), "secondary");
        assert_eq!(tone.lookup("anything-else"), "outline");
        assert_eq!(WORKFLOW_TONE_FALLBACK, "outline");
        assert_eq!(tone.0.len(), 4);
    }

    #[test]
    fn tone_map_parses_transparent_table() {
        let tone: ToneMap =
            toml::from_str("failed = \"destructive\"\ninactive = \"destructive\"\n").unwrap();
        assert_eq!(tone.lookup("failed"), "destructive");
        assert_eq!(tone.lookup("inactive"), "destructive");
        assert_eq!(tone.lookup("unknown"), "outline");
    }

    #[test]
    fn display_and_align_parse_kebab_case() {
        assert_eq!(
            serde_json::from_str::<Display>("\"raw\"").unwrap(),
            Display::Raw
        );
        assert_eq!(
            serde_json::from_str::<Display>("\"copy-chip\"").unwrap(),
            Display::CopyChip
        );
        assert_eq!(
            serde_json::from_str::<Align>("\"right\"").unwrap(),
            Align::Right
        );
    }

    #[test]
    fn unknown_display_variant_fails_naming_valid_variants() {
        let err =
            serde_json::from_str::<Display>("\"shimmer\"").expect_err("unknown display must fail");
        assert!(err.to_string().contains("unknown variant"), "{err}");
        assert!(err.to_string().contains("copy-chip"), "{err}");
    }

    #[test]
    fn format_config_rejects_unknown_keys() {
        let err = toml::from_str::<FormatConfig>("locale = \"en-NZ\"\nlocael = \"x\"")
            .expect_err("unknown format key must fail");
        assert!(err.to_string().contains("unknown field"), "{err}");
    }

    #[test]
    fn format_config_defaults() {
        let format: FormatConfig = toml::from_str("").unwrap();
        assert_eq!(format, FormatConfig::default());
        assert_eq!(format.locale, "en-NZ");
        assert!(format.currency.is_none());
    }
}
