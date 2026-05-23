use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionPointsConfig {
    #[serde(flatten)]
    pub points: HashMap<String, ExtensionPointDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionPointDef {
    pub name: String,
    pub description: String,
    pub cardinality: Cardinality,
    pub entities: Vec<String>,
    pub directions: Vec<Direction>,
    #[serde(default)]
    pub config: HashMap<String, ConfigFieldDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    Exclusive,
    Multiple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Push,
    Pull,
    Bidirectional,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigFieldDef {
    #[serde(rename = "type")]
    pub field_type: ConfigFieldType,
    #[serde(default)]
    pub required: bool,
    pub label: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub default: Option<toml::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldType {
    Text,
    Select,
    Toggle,
    Secret,
}

pub fn parse_extension_points(
    path: &Path,
) -> Result<ExtensionPointsConfig, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    parse_extension_points_str(&content)
}

pub fn parse_extension_points_str(
    content: &str,
) -> Result<ExtensionPointsConfig, Box<dyn std::error::Error>> {
    Ok(toml::from_str(content)?)
}

/// Returns extension point IDs that reference the given `domain.entity`.
pub fn points_for_entity(
    config: &ExtensionPointsConfig,
    domain: &str,
    entity: &str,
) -> Vec<String> {
    let target = format!("{}.{}", domain, entity);
    config
        .points
        .iter()
        .filter(|(_, def)| def.entities.contains(&target))
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[payroll-sync]
name = "Payroll Sync"
description = "Synchronise employee and pay data"
cardinality = "exclusive"
entities = ["common.worker", "compensation.pay_run"]
directions = ["push", "pull", "bidirectional"]

[payroll-sync.config]
pay_calendar = { type = "text", required = true, label = "Pay calendar name" }
sync_frequency = { type = "select", required = true, label = "Sync frequency", options = ["realtime", "hourly", "daily"] }

[government-filing]
name = "Government Filing"
description = "File statutory returns"
cardinality = "multiple"
entities = ["compensation.pay_run", "common.worker"]
directions = ["push"]

[government-filing.config]
employer_number = { type = "text", required = true, label = "Employer IRD number" }
"#;

    #[test]
    fn parses_extension_points() {
        let config = parse_extension_points_str(SAMPLE).unwrap();
        assert_eq!(config.points.len(), 2);

        let ps = &config.points["payroll-sync"];
        assert_eq!(ps.name, "Payroll Sync");
        assert_eq!(ps.cardinality, Cardinality::Exclusive);
        assert_eq!(ps.entities, vec!["common.worker", "compensation.pay_run"]);
        assert_eq!(ps.directions.len(), 3);
        assert_eq!(ps.config.len(), 2);

        let freq = &ps.config["sync_frequency"];
        assert_eq!(freq.field_type, ConfigFieldType::Select);
        assert!(freq.required);
        assert_eq!(freq.options, vec!["realtime", "hourly", "daily"]);
    }

    #[test]
    fn points_for_entity_finds_matches() {
        let config = parse_extension_points_str(SAMPLE).unwrap();
        let mut points = points_for_entity(&config, "compensation", "pay_run");
        points.sort();
        assert_eq!(points, vec!["government-filing", "payroll-sync"]);
    }

    #[test]
    fn points_for_entity_returns_empty_for_unmatched() {
        let config = parse_extension_points_str(SAMPLE).unwrap();
        let points = points_for_entity(&config, "recruiting", "candidate");
        assert!(points.is_empty());
    }
}
