//! Chart definitions (issue #577): modeled charts declared in a top-level
//! `[charts]` table in domains.toml. An OrgChart is one chart kind among a
//! closed vocabulary — the graph plane (`ChartDefinition` nodes) is the
//! source of truth; the generated routes/loads/pages and future IFML or
//! canvas renderers are projections of it.
//!
//! ```toml
//! [charts."org"]
//! kind = "org"                 # org | bar | line | pie | radar | metric
//! entity = "Organization"
//! title = "Org Chart"          # optional display title
//! route = "org-chart"          # optional route segment (default: name)
//!
//! [charts."org".org]           # per-kind payload — must match `kind`
//! hierarchy_field = "parent_organization_id"   # default: entity's hierarchy_field
//! label_fields = ["name"]
//!
//! [charts."headcount"]
//! kind = "bar"
//! entity = "Position"
//!
//! [charts."headcount".series]
//! label_field = "department"
//! value_fields = ["headcount"]
//! aggregate = "sum"            # sum | avg | count | min | max (default count)
//! ```
//!
//! Absent table (or entries the strict validation rejects are absent —
//! validation is hard, never silent) ⇒ no chart nodes ⇒ byte-identical
//! output. The deprecated per-entity `has_orgchart` boolean is accepted as
//! sugar (WARN + a synthesized org chart) by the ingest bridge.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::DomainConfigError;

/// The closed chart-kind vocabulary (TOML surface). Mirrored by
/// `codegraph_core::types::ChartKind`; the string sets are pinned equal
/// by a KAT in this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartKind {
    Org,
    Bar,
    Line,
    Pie,
    Radar,
    Metric,
}

impl ChartKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Radar => "radar",
            Self::Metric => "metric",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "org" => Some(Self::Org),
            "bar" => Some(Self::Bar),
            "line" => Some(Self::Line),
            "pie" => Some(Self::Pie),
            "radar" => Some(Self::Radar),
            "metric" => Some(Self::Metric),
            _ => None,
        }
    }

    /// The per-kind payload table key this kind demands.
    fn payload_key(&self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::Bar | Self::Line | Self::Pie | Self::Radar => "series",
            Self::Metric => "metric",
        }
    }
}

/// Aggregation applied to a chart's value fields (series + metric kinds).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregate {
    Sum,
    Avg,
    #[default]
    Count,
    Min,
    Max,
}

impl Aggregate {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Count => "count",
            Self::Min => "min",
            Self::Max => "max",
        }
    }

    /// The TS reducer this aggregate lowers to (client-side aggregation
    /// over the entity's RLS-scoped list endpoint — no new API plane).
    pub fn ts_fn_name(&self) -> &'static str {
        match self {
            Self::Sum => "sumBy",
            Self::Avg => "avgBy",
            Self::Count => "countBy",
            Self::Min => "minBy",
            Self::Max => "maxBy",
        }
    }
}

/// `kind = "org"` payload: a hierarchy over a self-referencing entity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrgChartConfig {
    /// Self-referential FK column. Defaults to the entity's
    /// `hierarchy_field`; a chart for an entity without one is a hard
    /// generation error naming the fix.
    #[serde(default)]
    pub hierarchy_field: Option<String>,
    /// Node label fields in priority order (first resolvable wins as the
    /// primary label, the rest render as sub-labels).
    #[serde(default)]
    pub label_fields: Vec<String>,
    /// Additional node detail fields rendered under the label.
    #[serde(default)]
    pub detail_fields: Vec<String>,
    /// Optional max render depth (client-side pruning only).
    #[serde(default)]
    pub max_depth: Option<u32>,
}

/// `kind = "bar" | "line" | "pie" | "radar"` payload: a categorical series
/// over one entity's collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesChartConfig {
    /// Field grouped by (the x axis / slice name).
    pub label_field: String,
    /// Fields aggregated into the value(s).
    #[serde(default)]
    pub value_fields: Vec<String>,
    #[serde(default)]
    pub aggregate: Aggregate,
}

/// `kind = "metric"` payload: one aggregated number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricChartConfig {
    pub value_field: String,
    #[serde(default)]
    pub aggregate: Aggregate,
    /// Optional display label (defaults to the chart title).
    #[serde(default)]
    pub label: Option<String>,
}

/// The typed per-kind payload, tagged for `config_json` persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ChartPayload {
    Org(OrgChartConfig),
    Bar(SeriesChartConfig),
    Line(SeriesChartConfig),
    Pie(SeriesChartConfig),
    Radar(SeriesChartConfig),
    Metric(MetricChartConfig),
}

impl ChartPayload {
    /// The payload's chart kind (round-trips `ChartKind::as_str`).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Org(_) => "org",
            Self::Bar(_) => "bar",
            Self::Line(_) => "line",
            Self::Pie(_) => "pie",
            Self::Radar(_) => "radar",
            Self::Metric(_) => "metric",
        }
    }

    /// Every entity field this payload references (validation + binding
    /// edges). Empty for org hierarchies carrying no fields.
    pub fn referenced_fields(&self) -> Vec<&str> {
        match self {
            Self::Org(cfg) => cfg
                .label_fields
                .iter()
                .chain(cfg.detail_fields.iter())
                .map(String::as_str)
                .collect(),
            Self::Bar(cfg) | Self::Line(cfg) | Self::Pie(cfg) | Self::Radar(cfg) => {
                std::iter::once(cfg.label_field.as_str())
                    .chain(cfg.value_fields.iter().map(String::as_str))
                    .collect()
            }
            Self::Metric(cfg) => vec![cfg.value_field.as_str()],
        }
    }
}

/// One `[charts."name"]` entry.
#[derive(Debug, Clone, Deserialize)]
pub struct ChartEntry {
    pub kind: ChartKind,
    /// The entity this chart binds to (exact or `Type`-suffixed title).
    pub entity: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Route path segment under `(app)`; default derived from the name.
    #[serde(default)]
    pub route: Option<String>,
    #[serde(default)]
    pub org: Option<OrgChartConfig>,
    #[serde(default)]
    pub series: Option<SeriesChartConfig>,
    #[serde(default)]
    pub metric: Option<MetricChartConfig>,
}

impl ChartEntry {
    /// Strict kind↔payload agreement: the kind's payload table MUST be
    /// present and no foreign payload table may ride along.
    pub fn validate(&self, name: &str) -> Result<(), DomainConfigError> {
        let key = self.kind.payload_key();
        let present = match self.kind {
            ChartKind::Org => self.org.is_some(),
            ChartKind::Bar | ChartKind::Line | ChartKind::Pie | ChartKind::Radar => {
                self.series.is_some()
            }
            ChartKind::Metric => self.metric.is_some(),
        };
        if !present {
            return Err(DomainConfigError::Invalid(format!(
                "charts.\"{name}\": kind \"{}\" requires the `[{key}]` payload table \
                 — add `[charts.\"{name}\".{key}]`",
                self.kind.as_str()
            )));
        }
        let foreign = [
            ("org", self.org.is_some()),
            ("series", self.series.is_some()),
            ("metric", self.metric.is_some()),
        ]
        .into_iter()
        .filter(|(k, present)| *present && *k != key)
        .map(|(k, _)| k)
        .collect::<Vec<_>>()
        .join(", ");
        if !foreign.is_empty() {
            return Err(DomainConfigError::Invalid(format!(
                "charts.\"{name}\": kind \"{}\" does not take the `{foreign}` payload \
                 table — remove it (the `{key}` table carries this kind's config)",
                self.kind.as_str()
            )));
        }
        Ok(())
    }

    /// The typed payload as it persists onto the `ChartDefinition` node.
    pub fn payload(&self) -> ChartPayload {
        match self.kind {
            ChartKind::Org => ChartPayload::Org(self.org.clone().unwrap_or_default()),
            ChartKind::Bar => {
                ChartPayload::Bar(self.series.clone().unwrap_or_else(|| SeriesChartConfig {
                    label_field: String::new(),
                    value_fields: Vec::new(),
                    aggregate: Aggregate::Count,
                }))
            }
            ChartKind::Line => {
                ChartPayload::Line(self.series.clone().unwrap_or_else(|| SeriesChartConfig {
                    label_field: String::new(),
                    value_fields: Vec::new(),
                    aggregate: Aggregate::Count,
                }))
            }
            ChartKind::Pie => {
                ChartPayload::Pie(self.series.clone().unwrap_or_else(|| SeriesChartConfig {
                    label_field: String::new(),
                    value_fields: Vec::new(),
                    aggregate: Aggregate::Count,
                }))
            }
            ChartKind::Radar => {
                ChartPayload::Radar(self.series.clone().unwrap_or_else(|| SeriesChartConfig {
                    label_field: String::new(),
                    value_fields: Vec::new(),
                    aggregate: Aggregate::Count,
                }))
            }
            ChartKind::Metric => {
                ChartPayload::Metric(self.metric.clone().unwrap_or_else(|| MetricChartConfig {
                    value_field: String::new(),
                    aggregate: Aggregate::Count,
                    label: None,
                }))
            }
        }
    }
}

/// The whole `[charts]` table, keyed by chart name. `BTreeMap` keeps
/// iteration deterministic for ingest + generation.
///
/// `Deserialize` is MANUAL: the derived newtype path mis-deserializes
/// nested table-of-maps under toml 0.8 (the entry's fields are looked up
/// on the table header, failing with a misleading "missing field" on the
/// first required key); `BTreeMap::deserialize` directly is correct.
#[derive(Debug, Clone, Default)]
pub struct ChartDefinitions(BTreeMap<String, ChartEntry>);

impl<'de> Deserialize<'de> for ChartDefinitions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(BTreeMap::deserialize(deserializer)?))
    }
}

impl ChartDefinitions {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, name: &str) -> Option<&ChartEntry> {
        self.0.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ChartEntry)> {
        self.0.iter()
    }

    /// Strict validation for everything checkable WITHOUT the graph:
    /// kind↔payload agreement. Entity/field references are validated by
    /// the ingest bridge (the layer that has the graph titles).
    pub fn validate(&self) -> Result<(), DomainConfigError> {
        for (name, entry) in &self.0 {
            entry.validate(name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[charts]` content, as it sits inside DomainConfig under the
    /// `charts` key — the tests parse through the same shape.
    #[derive(Deserialize)]
    struct Doc {
        charts: ChartDefinitions,
    }

    fn parse(toml: &str) -> ChartDefinitions {
        toml::from_str::<Doc>(toml).expect("parse").charts
    }

    #[test]
    fn parses_all_kinds() {
        let charts = parse(
            r#"
            [charts."org"]
            kind = "org"
            entity = "OrganizationType"
            [charts."org".org]
            hierarchy_field = "parent_organization_id"
            label_fields = ["name"]
            max_depth = 6

            [charts."headcount"]
            kind = "bar"
            entity = "PositionType"
            [charts."headcount".series]
            label_field = "department"
            value_fields = ["headcount"]
            aggregate = "sum"

            [charts."total"]
            kind = "metric"
            entity = "PositionType"
            [charts."total".metric]
            value_field = "headcount"
            aggregate = "count"
            "#,
        );
        assert_eq!(charts.len(), 3);
        charts.validate().expect("valid");
        let org = charts.get("org").unwrap();
        assert_eq!(org.kind, ChartKind::Org);
        assert_eq!(
            org.payload(),
            ChartPayload::Org(OrgChartConfig {
                hierarchy_field: Some("parent_organization_id".into()),
                label_fields: vec!["name".into()],
                detail_fields: vec![],
                max_depth: Some(6),
            })
        );
        let series = charts.get("headcount").unwrap();
        assert!(
            matches!(series.payload(), ChartPayload::Bar(cfg) if cfg.aggregate == Aggregate::Sum)
        );
    }

    #[test]
    fn unknown_kind_is_parse_error() {
        let result = toml::from_str::<Doc>(
            r#"
            [charts."x"]
            kind = "treemap"
            entity = "Foo"
            "#,
        );
        assert!(result.is_err(), "unknown kinds are hard parse errors");
    }

    #[test]
    fn missing_payload_table_is_validation_error() {
        let charts = parse(
            r#"
            [charts."org"]
            kind = "org"
            entity = "OrganizationType"
            "#,
        );
        let err = charts.validate().unwrap_err().to_string();
        assert!(err.contains("requires the `[org]` payload table"), "{err}");
    }

    #[test]
    fn foreign_payload_table_is_validation_error() {
        let charts = parse(
            r#"
            [charts."h"]
            kind = "bar"
            entity = "PositionType"
            [charts."h".series]
            label_field = "d"
            [charts."h".metric]
            value_field = "c"
            "#,
        );
        let err = charts.validate().unwrap_err().to_string();
        assert!(
            err.contains("does not take the `metric` payload table"),
            "{err}"
        );
    }

    #[test]
    fn kind_string_sets_match_core_mirror() {
        // The TOML surface and the graph vocabulary must never drift.
        let config_kinds = [
            ChartKind::Org,
            ChartKind::Bar,
            ChartKind::Line,
            ChartKind::Pie,
            ChartKind::Radar,
            ChartKind::Metric,
        ];
        for kind in config_kinds {
            assert_eq!(
                codegraph_core::types::ChartKind::parse(kind.as_str()),
                Some(match kind {
                    ChartKind::Org => codegraph_core::types::ChartKind::Org,
                    ChartKind::Bar => codegraph_core::types::ChartKind::Bar,
                    ChartKind::Line => codegraph_core::types::ChartKind::Line,
                    ChartKind::Pie => codegraph_core::types::ChartKind::Pie,
                    ChartKind::Radar => codegraph_core::types::ChartKind::Radar,
                    ChartKind::Metric => codegraph_core::types::ChartKind::Metric,
                }),
                "config kind '{}' missing from the core mirror",
                kind.as_str()
            );
        }
    }
}
