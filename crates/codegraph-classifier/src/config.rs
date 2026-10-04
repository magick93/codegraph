use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ClassifierConfig {
    #[serde(default = "default_threshold")]
    pub inline_enum_threshold: usize,
    #[serde(default)]
    pub required_extensions: Vec<String>,
    #[serde(default)]
    pub primitive_wrappers: HashMap<String, TypeMapping>,
    #[serde(default)]
    pub array_wrappers: HashMap<String, TypeMapping>,
    #[serde(default)]
    pub range_wrappers: HashMap<String, RangeMapping>,
    #[serde(default)]
    pub composite_wrappers: Vec<CompositeWrapper>,
    #[serde(default)]
    pub composite_ranges: Vec<CompositeRange>,
    #[serde(default)]
    pub structured_wrappers: HashMap<String, TypeMapping>,
    #[serde(default)]
    pub media_wrappers: HashMap<String, MediaWrapper>,
    #[serde(default)]
    pub codelist_as_check: CodelistAsCheck,
    #[serde(default)]
    pub naming_rules: HashMap<String, NamingRule>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct NamingRule {
    pub score: i32,
    #[serde(rename = "type")]
    pub rule_type: NamingRuleType,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub enum NamingRuleType {
    #[serde(rename = "hard")]
    Hard,
    #[serde(rename = "soft")]
    Soft,
}

fn default_threshold() -> usize {
    20
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TypeMapping {
    pub postgres: String,
    pub rust: String,
    #[serde(default)]
    pub sea_orm: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RangeMapping {
    pub postgres: String,
    pub rust: String,
    #[serde(default)]
    pub open_end: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompositeWrapper {
    pub schema: String,
    pub columns: Vec<CompositeWrapperColumn>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompositeWrapperColumn {
    pub suffix: String,
    pub postgres: String,
    pub rust: String,
    #[serde(default)]
    pub sea_orm: String,
    #[serde(default)]
    pub fk_table: String,
    /// Optional Rust type override for DTO generation (e.g. `CurrencyCodeList`).
    /// When present, the DTO generator uses this instead of `rust`.
    #[serde(default)]
    pub dto_rust_type: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MediaWrapper {
    pub columns: Vec<CompositeWrapperColumn>,
    #[serde(default)]
    pub accept: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompositeRange {
    pub schema: String,
    pub start: String,
    pub end: String,
    pub column: String,
    pub postgres: String,
    pub rust: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CodelistAsCheck {
    #[serde(default)]
    pub schemas: Vec<String>,
}

pub fn parse_classifier_config(path: &Path) -> Result<ClassifierConfig, Box<dyn Error>> {
    let content = std::fs::read_to_string(path)?;
    parse_classifier_config_str(&content)
}

pub fn parse_classifier_config_str(content: &str) -> Result<ClassifierConfig, Box<dyn Error>> {
    let config: ClassifierConfig = toml::from_str(content)?;
    Ok(config)
}

/// Deterministically select the naming rule matching `title` (issue #332).
///
/// A title may contain several rule patterns (e.g. "WorkerCompensationReportType"
/// contains both "Compensation" and "Report"). Picking the first match in
/// `HashMap` iteration order makes the winner depend on the per-process hash
/// seed — which flips the soft/hard decision and the vo_score delta, pushing
/// borderline schemas across the net_score ≥ 4 entity boundary from run to
/// run ("Auto-classified N entities" flipping 0/1).
///
/// The total order here is specificity: the LONGEST contained pattern wins
/// (longest match is the standard specificity rule); equal-length patterns
/// tie-break lexicographically. Single-match titles — the common case — are
/// unaffected.
pub fn select_naming_rule<'a>(
    title: &str,
    rules: &'a HashMap<String, NamingRule>,
) -> Option<(&'a String, &'a NamingRule)> {
    rules
        .iter()
        .filter(|(pattern, _)| title.contains(pattern.as_str()))
        .min_by(|(a, _), (b, _)| b.len().cmp(&a.len()).then_with(|| a.cmp(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_media_wrappers_table() {
        let toml = r#"
            [media_wrappers.MediaReferenceType]
            accept = ["image/*", "application/pdf"]

            [[media_wrappers.MediaReferenceType.columns]]
            suffix = "_url"
            postgres = "TEXT"
            rust = "Option<String>"
            sea_orm = "Text"

            [[media_wrappers.MediaReferenceType.columns]]
            suffix = "_mime_type"
            postgres = "TEXT"
            rust = "Option<String>"
            sea_orm = "Text"
        "#;
        let config = parse_classifier_config_str(toml).unwrap();
        let mw = config.media_wrappers.get("MediaReferenceType").unwrap();
        assert_eq!(mw.columns.len(), 2);
        assert_eq!(mw.columns[0].suffix, "_url");
        assert_eq!(mw.accept, vec!["image/*", "application/pdf"]);
    }

    #[test]
    fn parses_structured_wrappers_table() {
        let toml = r#"
            [structured_wrappers]
            "IdentifierType" = { postgres = "JSONB", rust = "IdentifierType", sea_orm = "Json" }
        "#;
        let config = parse_classifier_config_str(toml).unwrap();
        let sw = config.structured_wrappers.get("IdentifierType").unwrap();
        assert_eq!(sw.postgres, "JSONB");
        assert_eq!(sw.rust, "IdentifierType");
        assert_eq!(sw.sea_orm, "Json");
    }

    fn rule(score: i32, rule_type: NamingRuleType) -> NamingRule {
        NamingRule { score, rule_type }
    }

    fn sample_rules() -> HashMap<String, NamingRule> {
        HashMap::from([
            ("Report".to_string(), rule(5, NamingRuleType::Hard)),
            ("Compensation".to_string(), rule(3, NamingRuleType::Soft)),
            ("Vendor".to_string(), rule(3, NamingRuleType::Soft)),
            ("Message".to_string(), rule(3, NamingRuleType::Soft)),
        ])
    }

    #[test]
    fn select_naming_rule_picks_the_single_match() {
        let rules = sample_rules();
        // "WeeklyReportType" contains exactly one pattern.
        let (pattern, r) = select_naming_rule("WeeklyReportType", &rules).expect("Report matches");
        assert_eq!(pattern, "Report");
        assert_eq!(r.score, 5);
    }

    #[test]
    fn select_naming_rule_longest_pattern_wins_over_hash_order() {
        let rules = sample_rules();
        // "WorkerCompensationReportType" contains both "Compensation" (12)
        // and "Report" (6): specificity (longest) must decide, not the
        // HashMap seed.
        let (pattern, r) =
            select_naming_rule("WorkerCompensationReportType", &rules).expect("two matches");
        assert_eq!(
            pattern, "Compensation",
            "longest contained pattern must win"
        );
        assert_eq!(r.score, 3);
        assert!(matches!(r.rule_type, NamingRuleType::Soft));
    }

    #[test]
    fn select_naming_rule_equal_length_tie_breaks_lexicographically() {
        let rules = HashMap::from([
            ("bbxyz".to_string(), rule(3, NamingRuleType::Soft)),
            ("aaxyz".to_string(), rule(5, NamingRuleType::Hard)),
        ]);
        // Both 5-char patterns are contained in the title; equal length →
        // lexicographically smallest pattern wins.
        let (pattern, _) = select_naming_rule("PREFbbxyzSUFaaxyzSUF", &rules).expect("two matches");
        assert_eq!(pattern, "aaxyz");
    }

    #[test]
    fn select_naming_rule_no_match_returns_none() {
        assert!(select_naming_rule("PersonType", &sample_rules()).is_none());
    }

    /// The #332 knife-edge pin: the winner for a multi-match title must not
    /// depend on the HashMap seed. Every `HashMap::from` construction bumps
    /// the thread-local RandomState keys, so iteration order varies across
    /// these builds even within one process (and across processes a priori).
    #[test]
    fn select_naming_rule_is_stable_across_hash_orders() {
        let title = "ScreeningVendorMessageTypeWorkerCompensationReportType";
        let mut winners: HashMap<String, usize> = HashMap::new();
        for _ in 0..200 {
            let rules = sample_rules();
            let (pattern, _) = select_naming_rule(title, &rules).expect("four matches");
            *winners.entry(pattern.clone()).or_default() += 1;
        }
        assert_eq!(
            winners.len(),
            1,
            "selection must be seed-independent: {winners:?}"
        );
        assert!(winners.contains_key("Compensation"));
    }
}
