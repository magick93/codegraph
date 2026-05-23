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
}
