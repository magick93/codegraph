//! The chart plane (issue #577): modeled `ChartDefinition` graph nodes →
//! generated SvelteKit routes. An OrgChart is one chart kind among a
//! closed vocabulary; the generator is the route renderer of the plane,
//! not a bolt-on reading a bool.
//!
//! - Data contract: every chart fetches its bound entity's RLS-scoped
//!   collection through the #574 S1 neutral server contract (`apiFetch`,
//!   origin-relative paths) and aggregates CLIENT-SIDE (documented v1
//!   decision — no new aggregate API plane).
//! - Validation is hard: unresolved entities, list-less bindings, and org
//!   charts without a resolvable self-referencing hierarchy are
//!   generation errors naming the chart.
//! - Rendering: a mapped consumer component (the `ifml-components.toml`
//!   hook, `type = "{kind}chart"`) wins over the built-in self-contained
//!   markup.
//!
//! Presence gating: no chart definitions ⇒ zero files, byte-identical.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::{ChartPayload, DomainConfig, IfmlComponentMappings};
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::ifml::api_paths::resolve_entity_api;
use crate::output::render_template_with_project;
use crate::project_config::{GenerationEntry, ProjectConfig};
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};

pub struct UiChartsGenerator {
    output_dir: PathBuf,
    mappings: Option<IfmlComponentMappings>,
}

impl UiChartsGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            mappings: None,
        }
    }

    /// Component-mapping hook (`--ifml-components`): a chart whose kind
    /// tier resolves (`type = "orgchart"` etc.) renders through the
    /// consumer component instead of the built-in markup.
    pub fn with_mappings(mut self, mappings: Option<IfmlComponentMappings>) -> Self {
        self.mappings = mappings;
        self
    }
}

#[async_trait]
impl GlobalGenerator for UiChartsGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::UiCharts
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let definitions = db.get_chart_definitions().await.map_err(Error::Graph)?;
        if definitions.is_empty() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        for def in &definitions {
            let chart =
                ChartRoute::resolve(db, config, def, project, self.mappings.as_ref()).await?;

            let src = self
                .output_dir
                .join("ui")
                .join("src")
                .join("routes")
                .join("(app)")
                .join(&chart.route);

            let ctx = serde_json::json!({ "chart": chart });
            let load = render_template_with_project(tera, "ui/charts/load.tera", &ctx, project)?;
            let page = render_template_with_project(tera, "ui/charts/page.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: src.join("+page.server.ts"),
                content: load,
            });
            files.push(GeneratedFile {
                path: src.join("+page.svelte"),
                content: page,
            });
        }
        Ok(files)
    }
}

/// One resolved chart route — everything the templates render,
/// precomputed (the hurl-contract context discipline: no logic in Tera).
#[derive(Debug, Clone, Serialize)]
pub struct ChartRoute {
    pub name: String,
    /// `org` | `bar` | `line` | `pie` | `radar` | `metric`.
    pub kind: String,
    pub title: String,
    /// Route path segment under `(app)`.
    pub route: String,
    /// Stable testid vocabulary root (`chart-{route}`).
    pub testid: String,
    /// API collection path including version + domain, e.g.
    /// `/api/v1/common/organization`.
    pub api_base: String,
    // ── org ──
    pub hierarchy_field: String,
    pub label_fields: Vec<String>,
    pub detail_fields: Vec<String>,
    /// TS array literals of the field lists (Tera has no json filter —
    /// precomputed context, valid TS verbatim).
    pub label_fields_json: String,
    pub detail_fields_json: String,
    pub max_depth: Option<u32>,
    // ── series ──
    pub label_field: String,
    pub value_fields: Vec<String>,
    pub value_fields_json: String,
    pub aggregate: String,
    pub aggregate_ts: String,
    // ── metric ──
    pub metric_label: String,
    pub metric_label_json: String,
    /// Mapped consumer component (`--ifml-components`), if resolved.
    pub mapped: Option<MappedComponent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappedComponent {
    pub path: String,
    pub export: String,
}

impl ChartRoute {
    async fn resolve(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        def: &codegraph_core::types::ChartDefinitionNode,
        project: &ProjectConfig,
        mappings: Option<&IfmlComponentMappings>,
    ) -> Result<ChartRoute> {
        let name = &def.name;
        let hard = |msg: String| Error::Config(format!("charts: chart '{name}': {msg}"));

        let entity_title = def.entity_title.clone().ok_or_else(|| {
            hard(format!(
                "references entity '{}' which is not in the schema graph — \
                 fix the `entity` key or ingest the model",
                def.entity.clone().unwrap_or_default()
            ))
        })?;

        let payload: ChartPayload = def
            .config_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| hard(format!("corrupt payload: {e}")))?
            .ok_or_else(|| hard("missing payload — re-run ingest".to_string()))?;

        // Real endpoint resolution (API model > ApiResource > legacy
        // guess) — the same chain the IFML plane uses.
        let api = resolve_entity_api(db, config, &entity_title, &project.identity.api_version)
            .await
            .ok_or_else(|| hard("could not resolve an API base path".to_string()))?;
        if !api.has_list {
            return Err(hard(format!(
                "entity '{entity_title}' has no list operation — a chart needs \
                 the collection endpoint to aggregate over"
            )));
        }

        let props = db
            .get_properties(&entity_title)
            .await
            .map_err(Error::Graph)?;
        let prop_names: std::collections::HashSet<&str> =
            props.iter().map(|p| p.name.as_str()).collect();

        let title = def
            .title
            .clone()
            .unwrap_or_else(|| name.replace(['-', '_'], " "));
        let route = def
            .route
            .clone()
            .unwrap_or_else(|| codegraph_naming::to_kebab_case(name));
        let testid = format!("chart-{}", codegraph_naming::to_kebab_case(&route));

        let mapped = mappings.and_then(|m| {
            m.resolve(
                "charts",
                name,
                &format!("{}chart", def.kind.as_str()),
                "chart",
            )
            .map(|entry| MappedComponent {
                path: entry.path.clone(),
                export: entry.export_name().to_string(),
            })
        });

        let mut chart = ChartRoute {
            name: name.clone(),
            kind: def.kind.as_str().to_string(),
            title: title.clone(),
            route,
            testid,
            api_base: api.base_path,
            hierarchy_field: String::new(),
            label_fields: Vec::new(),
            detail_fields: Vec::new(),
            label_fields_json: "[]".to_string(),
            detail_fields_json: "[]".to_string(),
            max_depth: None,
            label_field: String::new(),
            value_fields: Vec::new(),
            value_fields_json: "[]".to_string(),
            aggregate: "count".to_string(),
            aggregate_ts: "countRows".to_string(),
            metric_label: String::new(),
            metric_label_json: serde_json::to_string(&name).unwrap_or_else(|_| "''".into()),
            mapped,
        };

        match &payload {
            ChartPayload::Org(cfg) => {
                let hierarchy = match &cfg.hierarchy_field {
                    Some(f) => f.clone(),
                    None => config
                        .domains
                        .values()
                        .find_map(|d| {
                            d.get_entity_config(&entity_title)
                                .and_then(|ec| ec.hierarchy_field.clone())
                        })
                        .ok_or_else(|| {
                            hard(format!(
                                "entity '{entity_title}' has no self-referencing hierarchy — \
                                 set `hierarchy_field` in [charts.\"{name}\".org] or on the \
                                 entity's config"
                            ))
                        })?,
                };
                if !prop_names.contains(hierarchy.as_str()) {
                    return Err(hard(format!(
                        "hierarchy field '{hierarchy}' does not exist on entity \
                         '{entity_title}' — an org chart lowers over a self-referencing FK"
                    )));
                }
                chart.hierarchy_field = hierarchy;
                chart.label_fields = resolve_fields(&cfg.label_fields, &prop_names);
                chart.detail_fields = resolve_fields(&cfg.detail_fields, &prop_names);
                chart.label_fields_json = ts_array(&chart.label_fields);
                chart.detail_fields_json = ts_array(&chart.detail_fields);
                chart.max_depth = cfg.max_depth;
            }
            ChartPayload::Bar(cfg)
            | ChartPayload::Line(cfg)
            | ChartPayload::Pie(cfg)
            | ChartPayload::Radar(cfg) => {
                if !prop_names.contains(cfg.label_field.as_str()) {
                    return Err(hard(format!(
                        "label field '{}' does not exist on entity '{entity_title}'",
                        cfg.label_field
                    )));
                }
                chart.label_field = cfg.label_field.clone();
                chart.value_fields = resolve_fields(&cfg.value_fields, &prop_names);
                chart.value_fields_json = ts_array(&chart.value_fields);
                chart.aggregate = cfg.aggregate.as_str().to_string();
                chart.aggregate_ts = cfg.aggregate.ts_fn_name().to_string();
            }
            ChartPayload::Metric(cfg) => {
                if !prop_names.contains(cfg.value_field.as_str()) {
                    return Err(hard(format!(
                        "value field '{}' does not exist on entity '{entity_title}'",
                        cfg.value_field
                    )));
                }
                chart.label_field = cfg.value_field.clone();
                chart.aggregate = cfg.aggregate.as_str().to_string();
                chart.aggregate_ts = cfg.aggregate.ts_fn_name().to_string();
                chart.metric_label = cfg.label.clone().unwrap_or_else(|| title.clone());
                chart.metric_label_json = ts_string(&chart.metric_label);
            }
        }

        Ok(chart)
    }
}

/// A TS string array literal (Tera has no json filter — precomputed
/// context, valid TS verbatim).
fn ts_array(fields: &[String]) -> String {
    let items: Vec<String> = fields.iter().map(|f| ts_string(f)).collect();
    format!("[{}]", items.join(", "))
}

/// A TS single-quoted string literal with the repo's `ts_string_literal`
/// escaping discipline (prose must never break out of the literal).
fn ts_string(s: &str) -> String {
    crate::forms::ts_string_literal(s)
}

/// Keep declared fields that exist on the entity (declared-but-missing
/// fields WARN at ingest and drop here — never a bare name in a template).
fn resolve_fields(fields: &[String], prop_names: &std::collections::HashSet<&str>) -> Vec<String> {
    fields
        .iter()
        .filter(|f| prop_names.contains(f.as_str()))
        .cloned()
        .collect()
}

/// Whether the build plan/config carries any chart (nav gating helper).
pub async fn chart_nav_items(db: &dyn GraphQuerier) -> Result<Vec<ChartNavItem>> {
    let defs = db.get_chart_definitions().await.map_err(Error::Graph)?;
    Ok(defs
        .iter()
        .map(|d| {
            let route = d
                .route
                .clone()
                .unwrap_or_else(|| codegraph_naming::to_kebab_case(&d.name));
            let title = d.title.clone().unwrap_or_else(|| d.name.clone());
            ChartNavItem {
                href: format!("/{route}"),
                label: title,
            }
        })
        .collect())
}

/// One app-layout nav entry for a chart route (presence-gated: no charts
/// ⇒ empty vec ⇒ byte-identical scaffold).
#[derive(Debug, Clone, Serialize)]
pub struct ChartNavItem {
    pub href: String,
    pub label: String,
}
