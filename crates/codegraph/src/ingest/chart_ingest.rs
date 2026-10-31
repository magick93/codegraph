//! Chart definition ingest (issue #577): the `[charts]` config table is
//! the authoring surface, the `ChartDefinition` graph nodes are the model
//! plane. Runs as a driver pass AFTER the schema passes so entity refs
//! resolve against the full ingested title set (the evt pass's position).
//!
//! Resolution contract (the `wire_alias_refs` chain): entity refs resolve
//! exact title first, then title + `defaults.type_suffix`; an unresolved
//! entity WARNs and the node stores `entity_title: None` — generation
//! hard-errors naming the chart (broken charts never half-generate).
//! Field references resolve against the bound entity's properties; misses
//! WARN and drop the edge. The deprecated per-entity `has_orgchart`
//! boolean is accepted as sugar: WARN + a synthesized org chart.

use std::collections::{BTreeMap, HashSet};

use codegraph_config::config::DomainConfig;
use codegraph_config::{ChartEntry, ChartPayload};
use codegraph_core::error::GraphError;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{ChartDefinitionNode, ChartKind, EdgeProperties, EdgeType};

use crate::error::{Error, Result};

#[derive(Debug, Default)]
pub struct ChartIngestStats {
    pub charts: usize,
    /// Org charts synthesized from the deprecated `has_orgchart` sugar.
    pub sugar_charts: usize,
    /// `has_orgchart` flags ignored because the entity declares no
    /// `hierarchy_field` (the dead generator never emitted for those).
    pub sugar_skipped_no_hierarchy: usize,
    pub unresolved_entities: usize,
    pub unresolved_fields: usize,
}

impl std::fmt::Display for ChartIngestStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "charts: {} definitions ingested ({} from has_orgchart sugar, \
             {} ignored for missing hierarchy_field), \
             {} unresolved entities, {} unresolved fields",
            self.charts,
            self.sugar_charts,
            self.sugar_skipped_no_hierarchy,
            self.unresolved_entities,
            self.unresolved_fields
        )
    }
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn ingest_chart_definitions(
    ingestor: &dyn GraphIngestor,
    querier: &dyn GraphQuerier,
    domain_config: &DomainConfig,
) -> Result<ChartIngestStats> {
    let mut stats = ChartIngestStats::default();

    // Kind↔payload agreement is config-checkable: hard error here (the
    // ingest bridge is the chart plane's validation point — pre-generation).
    domain_config
        .charts
        .validate()
        .map_err(|e| Error::Config(e.to_string()))?;

    // Declared charts plus the has_orgchart sugar (declared wins by name).
    let mut charts: BTreeMap<String, ChartEntry> = BTreeMap::new();
    for (name, entry) in domain_config.charts.iter() {
        charts.insert(name.clone(), entry.clone());
    }
    for (name, entry) in sugar_org_charts(domain_config) {
        match entry {
            Some(entry) => {
                if charts.contains_key(&name) {
                    continue;
                }
                println!(
                    "warning: charts: `has_orgchart = true` on entity '{name}' is \
                     deprecated — declare `[charts.\"{name}\"].kind = \"org\"` in \
                     domains.toml instead (synthesizing the org chart this run)"
                );
                stats.sugar_charts += 1;
                charts.insert(name, entry);
            }
            // The flag rides an entity with no `hierarchy_field`: the
            // dead generator never emitted for it, so the sugar must not
            // start failing generation for configs that were promised
            // nothing. WARN (once per run, above) and skip.
            None => {
                stats.sugar_skipped_no_hierarchy += 1;
            }
        }
    }
    if charts.is_empty() {
        return Ok(stats);
    }

    // Entity resolution universe (the wire_alias_refs chain).
    let schemas = querier.list_schemas(None).await.map_err(Error::Graph)?;
    let titles: HashSet<String> = schemas.iter().map(|s| s.title.clone()).collect();
    let type_suffix = &domain_config.defaults.type_suffix;

    for (name, entry) in &charts {
        let entity_title = resolve_entity(&entry.entity, type_suffix, &titles);
        if entity_title.is_none() {
            println!(
                "warning: charts: chart '{name}' references entity '{}' which is \
                 not in the schema graph — generation will fail until it resolves",
                entry.entity
            );
            stats.unresolved_entities += 1;
        }

        // Best-effort domain attribution from the config entity lists.
        let domain = entity_title.as_ref().and_then(|title| {
            domain_config
                .domains
                .iter()
                .find(|(_, d)| d.entities.contains(title))
                .map(|(name, _)| name.clone())
        });

        let payload = entry.payload();
        let config_json = serde_json::to_string(&payload)
            .map_err(|e| Error::Graph(GraphError::Ingest(format!("chart '{name}': {e}"))))?;

        let node = ChartDefinitionNode {
            name: name.clone(),
            kind: core_kind(entry.kind.as_str()),
            title: entry.title.clone(),
            domain,
            entity: Some(entry.entity.clone()),
            entity_title: entity_title.clone(),
            route: entry.route.clone(),
            config_json: Some(config_json),
        };
        ingestor
            .ingest_chart_definition(&node)
            .await
            .map_err(Error::Graph)?;

        // Referential edges: entity + every resolved field reference.
        if let Some(title) = &entity_title {
            ingestor
                .ingest_edge(
                    &format!("chart:{name}"),
                    title,
                    EdgeType::ChartBindsEntity,
                    None,
                )
                .await
                .map_err(Error::Graph)?;

            let props = querier.get_properties(title).await.map_err(Error::Graph)?;
            let prop_names: HashSet<String> = props.iter().map(|p| p.name.clone()).collect();
            for (field, role) in payload_fields(&payload, entry) {
                let (field, role) = match field {
                    Some(f) => (f, role),
                    // The org hierarchy default: the entity config's
                    // hierarchy_field (checked again at generation).
                    None => continue,
                };
                if !prop_names.contains(&field) {
                    println!(
                        "warning: charts: chart '{name}' references field '{field}' \
                         which does not exist on entity '{title}' — edge dropped"
                    );
                    stats.unresolved_fields += 1;
                    continue;
                }
                ingestor
                    .ingest_edge(
                        &format!("chart:{name}"),
                        &format!("{field}::{title}"),
                        EdgeType::ChartBindsProperty,
                        Some(&EdgeProperties {
                            role: Some(role),
                            ..Default::default()
                        }),
                    )
                    .await
                    .map_err(Error::Graph)?;
            }
        }

        stats.charts += 1;
    }

    Ok(stats)
}

/// Whether any entity config carries the deprecated `has_orgchart` flag
/// (the driver's cheap gate before the ingest pass).
pub fn has_orgchart_sugar(domain_config: &DomainConfig) -> bool {
    domain_config
        .domains
        .values()
        .any(|d| d.entity_config.values().any(|ec| ec.has_orgchart))
}
/// The deprecated `has_orgchart` sugar: one synthesized org chart per
/// flagged entity (keyed `{kebab entity}-org`) carrying the entity
/// config's `hierarchy_field`. Entities WITHOUT a `hierarchy_field`
/// yield `None` — skipped with a WARN (they never got output from the
/// dead generator, so the sugar must not start failing them).
fn sugar_org_charts(domain_config: &DomainConfig) -> BTreeMap<String, Option<ChartEntry>> {
    let mut out = BTreeMap::new();
    for domain_entry in domain_config.domains.values() {
        for (entity_key, ec) in &domain_entry.entity_config {
            if !ec.has_orgchart {
                continue;
            }
            let name = format!(
                "{}-org",
                codegraph_naming::to_kebab_case(&codegraph_naming::strip_suffix(
                    entity_key,
                    &domain_config.defaults.type_suffix
                ))
            );
            let Some(hierarchy_field) = ec.hierarchy_field.clone() else {
                println!(
                    "warning: charts: `has_orgchart = true` on entity '{entity_key}' is \
                     ignored — the entity declares no `hierarchy_field`, so there is no \
                     hierarchy to chart (declare `[charts]` with a kind instead)"
                );
                out.insert(name, None);
                continue;
            };
            out.insert(
                name,
                Some(ChartEntry {
                    kind: codegraph_config::ChartKind::Org,
                    entity: entity_key.clone(),
                    title: None,
                    route: None,
                    org: Some(codegraph_config::OrgChartConfig {
                        hierarchy_field: Some(hierarchy_field),
                        label_fields: Vec::new(),
                        detail_fields: Vec::new(),
                        max_depth: None,
                    }),
                    series: None,
                    metric: None,
                }),
            );
        }
    }
    out
}

/// Exact title first, then short name + `type_suffix` (the
/// `Customer` → `CustomerType` convention).
fn resolve_entity(entity: &str, type_suffix: &str, titles: &HashSet<String>) -> Option<String> {
    if titles.contains(entity) {
        return Some(entity.to_string());
    }
    let short = entity
        .rsplit("::")
        .next()
        .unwrap_or(entity)
        .rsplit('.')
        .next()
        .unwrap_or(entity);
    let suffixed = format!("{short}{type_suffix}");
    if titles.contains(&suffixed) {
        return Some(suffixed);
    }
    if titles.contains(short) {
        return Some(short.to_string());
    }
    None
}

/// `(field, role)` pairs for the referential edges; `None` field = the
/// hierarchy default (resolved again at generation from the entity
/// config), so nothing binds here.
fn payload_fields(payload: &ChartPayload, entry: &ChartEntry) -> Vec<(Option<String>, String)> {
    match payload {
        ChartPayload::Org(cfg) => {
            let mut out = Vec::new();
            if cfg.hierarchy_field.is_some() {
                out.push((cfg.hierarchy_field.clone(), "hierarchy".to_string()));
            }
            for f in &cfg.label_fields {
                out.push((Some(f.clone()), "label".to_string()));
            }
            for f in &cfg.detail_fields {
                out.push((Some(f.clone()), "detail".to_string()));
            }
            out
        }
        ChartPayload::Bar(cfg)
        | ChartPayload::Line(cfg)
        | ChartPayload::Pie(cfg)
        | ChartPayload::Radar(cfg) => {
            let mut out = vec![(Some(cfg.label_field.clone()), "label".to_string())];
            for f in &cfg.value_fields {
                out.push((Some(f.clone()), "value".to_string()));
            }
            out
        }
        ChartPayload::Metric(cfg) => vec![(Some(cfg.value_field.clone()), "value".to_string())]
            .into_iter()
            .chain(
                entry
                    .metric
                    .as_ref()
                    .and_then(|m| m.label.clone())
                    .map(|_| (None::<String>, String::new())),
            )
            .collect(),
    }
}

/// Config kind → graph vocabulary (string sets pinned equal by the
/// config-crate KAT).
fn core_kind(kind: &str) -> ChartKind {
    ChartKind::parse(kind).unwrap_or(ChartKind::Org)
}
