use serde::{Deserialize, Serialize};

/// The closed chart-type vocabulary (issue #577). An OrgChart is one kind
/// of chart — the chart plane models definitions structurally and the
/// renderer branches on the kind.
///
/// Mirrored by `codegraph_config::config::charts::ChartKind` (the TOML
/// surface); a KAT pins the two string sets identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartKind {
    /// Hierarchy over a self-referencing entity (`hierarchy_field`).
    Org,
    /// Categorical series over one entity's collection.
    Bar,
    /// Time-ordered or categorical line series.
    Line,
    /// Part-to-whole over one entity's collection.
    Pie,
    /// Multi-axis series over one entity's collection.
    Radar,
    /// Single aggregated value.
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

    /// Inverse of [`Self::as_str`]; `None` for unknown vocabulary.
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
}

/// A `ChartBindsProperty` edge readback: `(chart_name, field_name,
/// schema_title, role)`.
pub type ChartPropertyBinding = (String, String, String, Option<String>);

/// A modeled chart definition (issue #577): the presentation-neutral
/// binding (entity + referenced fields) plus a typed per-kind payload.
/// The graph node is the source of truth; generated routes, IFML views,
/// and future canvas projections are all renderers of it.
///
/// `config_json` carries the per-kind payload (`codegraph_config::
/// config::charts` serde JSON — the `type_json` precedent): opaque to
/// codegraph-core, rehydrated by codegraph-generate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartDefinitionNode {
    /// Config key (natural key); unique per run.
    pub name: String,
    pub kind: ChartKind,
    /// Display title; defaults to the name when absent.
    pub title: Option<String>,
    /// The `domains.toml` domain this chart is declared against.
    pub domain: Option<String>,
    /// The raw entity ref as declared (`Organization`, `OrganizationType`).
    pub entity: Option<String>,
    /// Resolved schema title (exact → `+type_suffix`). `None` = the
    /// binding never resolved — generation hard-errors naming the chart.
    pub entity_title: Option<String>,
    /// Route path segment under `(app)`; default derived from `name`.
    pub route: Option<String>,
    /// Per-kind typed payload (serde JSON), rehydrated by codegraph-generate.
    pub config_json: Option<String>,
}
