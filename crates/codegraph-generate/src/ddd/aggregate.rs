//! Authored aggregate boundaries (DD4, issue #666 slice 4b): the
//! enforcement-adjacent consumer plane for the slice-1 mirrors.
//!
//! [`DddDesignNode::aggregate_boundary`] carries the authored verdict
//! (`is_root`) and — for a non-root — the resolved membership with its
//! origin ([`DddAggregateOrigin::Authored`] vs the containment
//! derivation). This module turns those facts into:
//!
//! - **routing** — an authored member nests under its authored root the
//!   way a config `role = "child"` entity does, when the root resolves to
//!   a schema in the same domain and a refers-derived FK column resolves
//!   (config still owns the FK column when it names one). Authored wins
//!   over config for the parent; an authored ROOT proven by a member's
//!   declaration overrides a config child declaration. An UNDECIDED
//!   boundary (no authored clause) changes nothing — every existing model
//!   stays byte-identical.
//! - **WARN diagnostics** — `aggregate_boundary_warnings` (the
//!   `dead_surface_warnings` posture): authored-member-vs-root-plane,
//!   authored-root-vs-config-child, config-parent-override, and
//!   unresolvable-root WARNs, each naming the class, the authored fact,
//!   and the derived fact. `Derived` memberships never warn (upstream's
//!   own derivation is informational).
//! - **doctor parity** — the compiled-model half of the same WARNs
//!   (doctor holds no graph).
//!
//! Note on authored ROOT detection: the phero-ir artifact records the
//! origin of a ROOT verdict nowhere (roots carry `isAggregateRoot` alone;
//! only a non-root's `aggregate` blob carries `origin`). A root is proven
//! AUTHORED when some sibling design declares `aggregate <ThisClass>` —
//! the detectable subset of "authored root" that never fires on the
//! containment-derived verdict most entities carry.

use std::collections::{HashMap, HashSet};

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{DddAggregateOrigin, DddModelGraph};

/// The authored aggregate-root class a membership names, parsed from the
/// serialized phero-ir `TypeRef` JSON (adjacently tagged:
/// `{"type":"class","value":{"package":…,"name":…}}`). Only a class tag
/// carries a placeable aggregate root; every other tag (primitive, enum,
/// …) is not an aggregate root and yields `None`.
pub fn authored_root_class_name(root_json: &serde_json::Value) -> Option<String> {
    if root_json.get("type").and_then(|t| t.as_str()) != Some("class") {
        return None;
    }
    root_json
        .pointer("/value/name")
        .and_then(|n| n.as_str())
        .map(str::to_string)
}

/// The unqualified leaf of a possibly package-qualified class reference
/// (`payroll.PayRunType` / `payroll::PayRunType` / `PayRunType`).
fn class_leaf(class: &str) -> &str {
    class
        .rsplit(['.', ':'])
        .next()
        .filter(|leaf| !leaf.is_empty())
        .unwrap_or(class)
}

/// The `(resolved title, owning domain)` of a class name looked up in the
/// graph — exact title first, then the `+type_suffix` spelling.
async fn resolve_class_title(
    db: &dyn GraphQuerier,
    class: &str,
    type_suffix: &str,
) -> Option<(String, Option<String>)> {
    let leaf = class_leaf(class);
    for candidate in [leaf.to_string(), format!("{leaf}{type_suffix}")] {
        if let Ok(Some(schema)) = db.get_schema(&candidate).await {
            return Some((schema.title, schema.domain));
        }
    }
    None
}

/// The authored boundary facts of one graph, title-keyed and resolved.
/// Cheap to build (one `get_ddd_models` read — cached in the generation
/// phase) and read-only after construction.
#[derive(Debug, Default)]
pub struct AuthoredBoundaryIndex {
    /// Authored member resolved title → (authored root resolved title,
    /// the root schema's owning domain).
    parent_of: HashMap<String, (String, String)>,
    /// Resolved titles proven aggregate roots by some authored membership.
    proven_roots: HashSet<String>,
}

impl AuthoredBoundaryIndex {
    /// Build the index from the graph's ddd models, resolving every
    /// authored root class to a schema title. Members whose root class
    /// resolves to no schema are dropped here — the WARN walker reports
    /// them.
    pub async fn from_graph(db: &dyn GraphQuerier, type_suffix: &str) -> Self {
        let Ok(models) = db.get_ddd_models().await else {
            return Self::default();
        };
        Self::from_models(&models, |class| {
            let class = class.to_string();
            let type_suffix = type_suffix.to_string();
            async move { resolve_class_title(db, &class, &type_suffix).await }
        })
        .await
    }

    /// The async constructor over already-fetched models with a pluggable
    /// class → `(title, domain)` resolver (unit-testable; the production
    /// resolver is [`AuthoredBoundaryIndex::from_graph`]).
    pub async fn from_models<F, Fut>(models: &[DddModelGraph], mut resolve: F) -> Self
    where
        F: FnMut(&str) -> Fut,
        Fut: std::future::Future<Output = Option<(String, Option<String>)>>,
    {
        let mut index = Self::default();
        for model in models {
            for design in &model.designs {
                let Some(title) = design.resolved_title.as_deref() else {
                    continue;
                };
                let Some(membership) = design
                    .aggregate_boundary
                    .as_ref()
                    .and_then(|boundary| boundary.membership.as_ref())
                else {
                    continue;
                };
                if membership.origin != DddAggregateOrigin::Authored {
                    continue;
                }
                let Some(root_class) = authored_root_class_name(&membership.root_json) else {
                    continue;
                };
                let Some((root_title, Some(root_domain))) = resolve(&root_class).await else {
                    continue;
                };
                if root_title == title {
                    // A member of itself is upstream-illegal (E0386); drop
                    // the fact rather than nest an entity under itself.
                    continue;
                }
                index
                    .parent_of
                    .insert(title.to_string(), (root_title.clone(), root_domain.clone()));
                index.proven_roots.insert(root_title);
            }
        }
        // A title that is BOTH an authored member and a proven root is a
        // contradictory graph (upstream E0386): the member fact loses —
        // proven-root wins, mirroring the routing precedence.
        let contradictory: Vec<String> = index
            .parent_of
            .keys()
            .filter(|title| index.proven_roots.contains(*title))
            .cloned()
            .collect();
        for title in contradictory {
            index.parent_of.remove(&title);
        }
        // Depth cap: authored chains deeper than ONE level do not route
        // (the depth-2 authored handler surface is a recorded follow-up) —
        // a member whose parent is itself a routed member drops out, fail
        // closed, and the WARN walker names the consequence.
        let deeper: Vec<String> = index
            .parent_of
            .iter()
            .filter(|(_, (parent, _))| index.parent_of.contains_key(parent))
            .map(|(title, _)| title.clone())
            .collect();
        for title in deeper {
            index.parent_of.remove(&title);
        }
        index
    }

    /// The authored parent title of `title` when it is an authored member
    /// whose root lives in `domain` (and the depth cap keeps it routable).
    pub fn routing_parent(&self, title: &str, domain: &str) -> Option<&str> {
        let (parent, parent_domain) = self.parent_of.get(title)?;
        (parent_domain == domain).then_some(parent.as_str())
    }

    /// `true` when some sibling design's AUTHORED membership names `title`
    /// as its aggregate root — the detectable authored-root proof.
    pub fn proven_root(&self, title: &str) -> bool {
        self.proven_roots.contains(title)
    }

    /// `true` when `title` carries an authored membership (it is a member).
    pub fn is_authored_member(&self, title: &str) -> bool {
        self.parent_of.contains_key(title)
    }

    /// The authored member titles whose routing parent is `root_title` in
    /// `domain`, sorted (slice 4d's member validation consumes the SAME
    /// facts the router routes through — one resolver, the planes cannot
    /// drift). Depth-capped members are already absent from the index.
    pub fn members_of(&self, root_title: &str, domain: &str) -> Vec<&str> {
        let mut members: Vec<&str> = self
            .parent_of
            .iter()
            .filter(|(_, (parent, parent_domain))| {
                parent.as_str() == root_title && parent_domain.as_str() == domain
            })
            .map(|(member, _)| member.as_str())
            .collect();
        members.sort();
        members.dedup();
        members
    }

    /// `true` when no design carries an authored membership — the early-
    /// return gate every consumer uses to stay byte-identical.
    pub fn is_empty(&self) -> bool {
        self.parent_of.is_empty()
    }
}

/// The AUTHORED routing parent of one entity: the resolved parent title
/// plus the FK column the nested surface writes. Config still owns the FK
/// column when it names one (`parent_ref`); otherwise the refers-derived
/// candidate pointing AT the authored parent resolves it. `None` — no
/// authored routing — for everything else, which is the byte-identical
/// path for every model without authored boundaries.
pub async fn authored_routing_parent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    child_title: &str,
) -> Option<AuthoredParent> {
    let type_suffix = config.defaults.type_suffix.clone();
    let index = AuthoredBoundaryIndex::from_graph(db, &type_suffix).await;
    let parent_title = index.routing_parent(child_title, domain)?.to_string();
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|entry| entry.get_entity_config(child_title));
    // Config still owns the FK column when it names one.
    if let Some(fk) = entity_cfg.and_then(|ec| ec.parent_ref.clone()) {
        return Some(AuthoredParent {
            parent_title,
            fk_column: fk,
        });
    }
    let candidates = db.get_parent_candidates().await.ok()?;
    let stripped = crate::api::router::strip_suffix(child_title, &type_suffix);
    let candidate = candidates.iter().find(|pc| {
        crate::api::router::strip_suffix(&pc.child_title, &type_suffix) == stripped
            && pc.parent_title == parent_title
    })?;
    Some(AuthoredParent {
        parent_title,
        fk_column: crate::fk_column_for_candidate(candidate, &type_suffix),
    })
}

/// The authored routing parent facts (issue #666 slice 4b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredParent {
    /// The authored root's resolved schema title.
    pub parent_title: String,
    /// The FK column the nested surface writes (`pay_run_id`).
    pub fk_column: String,
}

/// The router-plane Source 0 (issue #666 slice 4b): assign parent/children
/// from the AUTHORED aggregate boundary at both `build_router_context`
/// sites (the router generator + the shared context builder), so the
/// router and the api contract cannot disagree with the handler/query
/// planes (whose FK resolution rides the same authored facts).
///
/// Precedence: an authored MEMBER wins over any config parent; a class
/// PROVEN root by a member's authored declaration overrides a config child
/// declaration (top-level root). Undecided boundaries assign nothing —
/// byte-identical.
pub fn apply_authored_nesting(
    authored: &AuthoredBoundaryIndex,
    config: &DomainConfig,
    domain: &str,
    entities: &mut [crate::api::router::RouterEntity],
    title_to_entity_idx: &HashMap<String, usize>,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
) {
    if authored.is_empty() {
        return;
    }
    let type_suffix = config.defaults.type_suffix.clone();
    // Authored-root override: a proven root declared child by config drops
    // back to a top-level root (authored wins).
    let mut proven_roots: Vec<usize> = Vec::new();
    for (title, &ci) in title_to_entity_idx {
        if authored.proven_root(title)
            && entities[ci].role == "child"
            && !proven_roots.contains(&ci)
        {
            proven_roots.push(ci);
        }
    }
    for ci in proven_roots {
        entities[ci].role = "root".to_string();
        entities[ci].parent = None;
    }
    // Authored members nest under their authored root (Source 0 runs
    // BEFORE the config source, whose `parent.is_none()` guard then skips
    // them — an authored member beats a config parent).
    for (title, &ci) in title_to_entity_idx {
        let Some(parent_title) = authored.routing_parent(title, domain) else {
            continue;
        };
        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|entry| entry.get_entity_config(title));
        let fk_column = match entity_cfg.and_then(|ec| ec.parent_ref.clone()) {
            // Config still owns the FK column when it names one.
            Some(fk) => fk,
            None => {
                let stripped = crate::api::router::strip_suffix(title, &type_suffix);
                match parent_candidates.iter().find(|pc| {
                    crate::api::router::strip_suffix(&pc.child_title, &type_suffix) == stripped
                        && pc.parent_title == parent_title
                }) {
                    Some(pc) => crate::fk_column_for_candidate(pc, &type_suffix),
                    // No FK column resolves — nothing to nest through; the
                    // WARN walker names the consequence.
                    None => continue,
                }
            }
        };
        let Some(&pi) = title_to_entity_idx.get(parent_title) else {
            continue;
        };
        if pi == ci || entities[ci].parent.is_some() {
            continue;
        }
        entities[ci].role = "child".to_string();
        entities[ci].parent = Some(crate::api::router::ParentInfo {
            entity_name: entities[pi].entity_name.clone(),
            module_name: entities[pi].module_name.clone(),
            path_segment: entities[pi].path_segment.clone(),
            fk_column,
        });
        entities[pi].children.push(crate::api::router::ChildInfo {
            entity_name: entities[ci].entity_name.clone(),
            module_name: entities[ci].module_name.clone(),
            path_segment: entities[ci].path_segment.clone(),
        });
    }
}

/// `true` when some sibling design's AUTHORED membership names `title`
/// (by exact title or the `+type_suffix` spelling) as its aggregate root —
/// the per-entity form of [`AuthoredBoundaryIndex::proven_root`].
pub async fn authored_root_proven(db: &dyn GraphQuerier, type_suffix: &str, title: &str) -> bool {
    let Ok(models) = db.get_ddd_models().await else {
        return false;
    };
    models.iter().any(|model| {
        model.designs.iter().any(|design| {
            let Some(membership) = design
                .aggregate_boundary
                .as_ref()
                .and_then(|boundary| boundary.membership.as_ref())
            else {
                return false;
            };
            if membership.origin != DddAggregateOrigin::Authored {
                return false;
            }
            let Some(root_class) = authored_root_class_name(&membership.root_json) else {
                return false;
            };
            let leaf = class_leaf(&root_class);
            leaf == title || format!("{leaf}{type_suffix}") == title
        })
    })
}

/// The WARN walker (issue #666 slice 4b, the `dead_surface_warnings`
/// posture): Warning severity, deterministic order (model readback order,
/// declaration order within), empty for every model without authored
/// boundaries. Checks per authored member:
///
/// - **(iv) unresolvable authored root** — the root class resolves to no
///   schema in the graph;
/// - **(i) authored member vs the route plane** — the route plane keeps
///   the class a top-level (root) surface (the authored nesting does not
///   resolve in the entity's own domain, its FK column is missing, or the
///   chain runs deeper than one routed level) while the entity still
///   carries its own repository (a non-empty op set);
/// - **(ii) authored root vs config child** — a class PROVEN root by a
///   member's authored declaration while config declares it
///   `role = "child"`;
/// - **(iii) authored member vs config parent** — config declares a parent
///   that is NOT the authored root (authored wins for routing; the WARN
///   records the override).
///
/// `Derived` memberships never warn.
pub async fn aggregate_boundary_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Vec<String> {
    let Ok(models) = db.get_ddd_models().await else {
        return Vec::new();
    };
    let type_suffix = config.defaults.type_suffix.clone();
    let index = AuthoredBoundaryIndex::from_graph(db, &type_suffix).await;
    // The gate scans the RAW models: the index drops members whose root
    // class resolves to no schema, and the (iv) unresolvable-root WARN is
    // exactly about those.
    let has_authored = models.iter().any(|model| {
        model.designs.iter().any(|design| {
            design
                .aggregate_boundary
                .as_ref()
                .and_then(|boundary| boundary.membership.as_ref())
                .is_some_and(|membership| membership.origin == DddAggregateOrigin::Authored)
        })
    });
    if !has_authored {
        return Vec::new();
    }
    let mut warnings = Vec::new();
    for model in &models {
        for design in &model.designs {
            let Some(title) = design.resolved_title.as_deref() else {
                continue;
            };
            let membership = design
                .aggregate_boundary
                .as_ref()
                .and_then(|boundary| boundary.membership.as_ref());
            let authored_here = membership
                .is_some_and(|membership| membership.origin == DddAggregateOrigin::Authored);
            // A class PROVEN root by some member's authored declaration is
            // warned on too (check ii) — it need not itself be a member.
            if !authored_here && !index.proven_root(title) {
                continue;
            }
            let (domain_name, entity_cfg) = match db.get_schema(title).await {
                Ok(Some(schema)) => {
                    let domain = schema.domain.clone().unwrap_or_default();
                    let cfg = config
                        .domains
                        .get(&domain)
                        .and_then(|entry| entry.get_entity_config(title));
                    (domain, cfg)
                }
                _ => (String::new(), None),
            };
            let cfg_child_parent = entity_cfg
                .and_then(|ec| ec.parent.clone())
                .filter(|_| entity_cfg.and_then(|ec| ec.role.as_deref()) == Some("child"));

            // (ii) Authored ROOT vs config child: a class proven root by a
            // member's authored declaration while config declares IT a
            // child of another entity.
            if index.proven_root(title) && cfg_child_parent.is_some() {
                let cfg = cfg_child_parent.as_deref().unwrap_or_default();
                warnings.push(format!(
                    "aggregate boundary: entity '{title}' is proven an aggregate root (a \
                     member's design authors `aggregate {title}`), but domains.toml declares \
                     it `role = \"child\"` with `parent = \"{cfg}\"` — the authored root \
                     verdict wins, so the entity stays a top-level root; drop the config \
                     child declaration or re-author the aggregate clause"
                ));
                continue;
            }
            // The remaining checks need the authored membership.
            let Some(membership) = membership else {
                continue;
            };
            if membership.origin != DddAggregateOrigin::Authored {
                continue; // Derived: upstream's own derivation, informational
            }
            let Some(root_class) = authored_root_class_name(&membership.root_json) else {
                continue;
            };
            let root_leaf = class_leaf(&root_class);
            let cfg_names_root = cfg_child_parent.as_deref().is_some_and(|cfg| {
                cfg == root_class || cfg == root_leaf || format!("{root_leaf}{type_suffix}") == cfg
            });

            // (iv) The authored root class resolves to no schema.
            let resolved = resolve_class_title(db, &root_class, &type_suffix).await;
            let root_title = resolved.as_ref().map(|(title, _)| title.clone());
            if root_title.is_none() {
                warnings.push(format!(
                    "aggregate boundary: entity '{title}' authors it as a member of aggregate \
                     root '{root_class}', which resolves to no schema in the graph — the \
                     authored membership cannot drive routing; design the root entity (or \
                     correct the `aggregate` clause)"
                ));
            }

            // (i) Authored member whose route plane keeps it a top-level
            // (root) surface: the authored nesting does not resolve while
            // the entity still carries its own repository. Config nesting
            // is the agreement escape hatch.
            let authored_routable = index.routing_parent(title, &domain_name).is_some()
                && authored_routing_parent(db, config, &domain_name, title)
                    .await
                    .is_some();
            let has_own_surface = has_own_route_surface(db, config, &domain_name, title).await;
            if !authored_routable && has_own_surface && cfg_child_parent.is_none() {
                warnings.push(format!(
                    "aggregate boundary: entity '{title}' authors it as a member of aggregate \
                     root '{root_class}', but the route plane emits a top-level (root) surface \
                     for it — the authored nesting does not resolve (an unresolvable root, a \
                     missing refers-derived FK column, or an authored chain deeper than one \
                     routed level); nest it explicitly with domains.toml `role = \"child\"` + \
                     `parent` + `parent_ref`, or give the entity a refers feature to the root"
                ));
            }

            // (iii) Authored member vs config parent: authored WINS for
            // routing; the WARN records that config's parent is overridden.
            if let Some(cfg) = cfg_child_parent.as_deref()
                && !cfg_names_root
            {
                let winner = root_title.as_deref().unwrap_or(root_class.as_str());
                warnings.push(format!(
                    "aggregate boundary: entity '{title}' authors it as a member of aggregate \
                     root '{root_class}', but domains.toml declares `parent = \"{cfg}\"` — \
                     the authored boundary wins, so the route plane nests it under '{winner}' \
                     and config's parent is overridden"
                ));
            }
        }
    }
    warnings
}

/// `true` when the entity carries a non-empty route surface (the design
/// op set, or the config/schema-derived default list when no design
/// override applies). A route-less entity mounted top-level carries
/// nothing, so the route-plane WARN stays quiet for it.
async fn has_own_route_surface(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
) -> bool {
    let surface = crate::ddd::design::DddDesignSurface::from_graph(db)
        .await
        .ok();
    if let Some(ops) = surface.as_ref().and_then(|s| s.operations_for(title)) {
        return !ops.is_empty();
    }
    // No design for the title: the config/schema-derived default op list.
    !crate::api::api_model::resolve_entity_operations(db, config, domain, title)
        .await
        .is_empty()
}

/// The compiled-model half of the WARN walker (doctor parity — doctor
/// holds the phero-ir model and the config, never the graph). Config-side
/// checks only: authored-member-vs-config-parent (iii) and
/// proven-root-vs-config-child (ii). `config` absent → nothing to compare.
pub fn aggregate_boundary_warnings_from_compiled_model(
    model: &phero_ir::ddd::DddModel,
    config: Option<&DomainConfig>,
) -> Vec<String> {
    let Some(config) = config else {
        return Vec::new();
    };
    let type_suffix = config.defaults.type_suffix.clone();
    // Proven roots: classes some authored membership names as its root.
    let proven_roots: HashSet<&str> = model
        .modules
        .iter()
        .flat_map(|module| module.designs.iter())
        .filter_map(|design| design.aggregate.as_ref())
        .filter(|membership| membership.origin == phero_ir::ddd::AggregateOrigin::Authored)
        .filter_map(|membership| match &membership.root {
            phero_ir::TypeRef::Class { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let mut warnings = Vec::new();
    for module in &model.modules {
        for design in &module.designs {
            let Some(membership) = design.aggregate.as_ref() else {
                continue;
            };
            if membership.origin != phero_ir::ddd::AggregateOrigin::Authored {
                continue;
            }
            let root_class = match &membership.root {
                phero_ir::TypeRef::Class { name, .. } => name.clone(),
                _ => continue,
            };
            // DD3: `Design.class` is `Option<String>` — a standalone DTO
            // boundary names no class and carries no aggregate membership
            // (upstream refuses one); skip the defensive None arm.
            let Some(class) = design.class.as_deref() else {
                continue;
            };
            // Config lookup by the class name and its `+suffix` spelling —
            // doctor has no graph to resolve the title through.
            let Some((domain_key, ec)) = config.domains.iter().find_map(|(key, entry)| {
                let cfg = entry.get_entity_config(class).cloned().or_else(|| {
                    entry
                        .get_entity_config(&format!("{class}{type_suffix}"))
                        .cloned()
                })?;
                Some((key.clone(), cfg))
            }) else {
                continue;
            };
            let Some(cfg_parent) = ec
                .parent
                .clone()
                .filter(|_| ec.role.as_deref() == Some("child"))
            else {
                continue;
            };
            let cfg_leaf = class_leaf(&cfg_parent);
            let root_leaf = class_leaf(&root_class);
            let parent_matches = cfg_parent == root_class
                || cfg_leaf == root_leaf
                || cfg_parent == class
                || cfg_parent == format!("{class}{type_suffix}");
            if !parent_matches {
                warnings.push(format!(
                    "aggregate boundary: design '{class}' (module {}) authors it as a member \
                     of aggregate root '{root_class}', but domains.toml declares \
                     `parent = \"{cfg_parent}\"` for '{class}' in domain '{domain_key}' — the \
                     authored boundary wins, so the route plane nests it under '{root_class}' \
                     and config's parent is overridden",
                    module.name,
                ));
            }
            // (ii) proven root vs config child: THIS class is named as a
            // root by some authored membership while config declares it a
            // child.
            if proven_roots.contains(class) {
                warnings.push(format!(
                    "aggregate boundary: design '{class}' is proven an aggregate root (a \
                     member's design authors `aggregate {class}`), but domains.toml declares \
                     it `role = \"child\"` with `parent = \"{cfg_parent}\"` in domain \
                     '{domain_key}' — the authored root verdict wins, so the entity stays a \
                     top-level root; drop the config child declaration or re-author the \
                     aggregate clause"
                ));
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{
        DddAggregateBoundary, DddAggregateMembership, DddDesignFlags, DddDesignNode, SchemaNode,
    };

    fn class_json(name: &str) -> serde_json::Value {
        serde_json::json!({"type": "class", "value": {"package": "payroll", "name": name}})
    }

    fn schema(title: &str, domain: &str) -> SchemaNode {
        SchemaNode {
            schema_id: format!("payroll/{title}"),
            title: title.to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            namespace: None,
            rel_path: format!("payroll/{title}.json"),
            pg_type: "TABLE".to_string(),
            rust_type: "PayRun".to_string(),
            sea_orm_type: "Entity".to_string(),
            rust_type_name: "PayRun".to_string(),
            pg_table_name: "pay_run".to_string(),
            api_path_segment: "pay-runs".to_string(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        }
    }

    fn design(
        class: &str,
        title: Option<&str>,
        boundary: Option<DddAggregateBoundary>,
    ) -> DddDesignNode {
        DddDesignNode {
            application: "Payroll".to_string(),
            module: "payroll".to_string(),
            class: class.to_string(),
            resolved_title: title.map(str::to_string),
            stereotype: "entity".to_string(),
            is_abstract: false,
            flags: DddDesignFlags::default(),
            machine_json: None,
            child_operations: Vec::new(),
            aggregate_boundary: boundary,
            invariants: Vec::new(),
            ordinal: 0,
        }
    }

    fn authored_member(root: &str) -> DddAggregateBoundary {
        DddAggregateBoundary {
            is_root: Some(false),
            membership: Some(DddAggregateMembership {
                root_json: class_json(root),
                origin: DddAggregateOrigin::Authored,
            }),
        }
    }

    fn model(designs: Vec<DddDesignNode>) -> DddModelGraph {
        DddModelGraph {
            source_path: "authored.ddd".to_string(),
            application: codegraph_core::types::DddApplicationNode {
                name: "Payroll".to_string(),
                base: None,
                source_path: "authored.ddd".to_string(),
            },
            modules: Vec::new(),
            designs,
            repositories: Vec::new(),
            services: Vec::new(),
            searches: Vec::new(),
        }
    }

    // ── root_json parsing KATs ─────────────────────────────────────────

    #[test]
    fn parses_the_class_leaf_from_the_adjacently_tagged_typeref() {
        assert_eq!(
            authored_root_class_name(&class_json("PayRunType")).as_deref(),
            Some("PayRunType")
        );
    }

    #[test]
    fn a_non_class_tag_is_not_a_placeable_root() {
        assert_eq!(
            authored_root_class_name(&serde_json::json!({"type": "primitive", "value": "string"})),
            None
        );
        assert_eq!(
            authored_root_class_name(&serde_json::json!({"type": "class"})),
            None
        );
        assert_eq!(authored_root_class_name(&serde_json::Value::Null), None);
    }

    // ── index extraction KATs ──────────────────────────────────────────

    #[tokio::test]
    async fn the_index_keys_authored_members_and_proven_roots_by_title() {
        let models = vec![model(vec![
            design(
                "PayRunType",
                Some("PayRunType"),
                Some(DddAggregateBoundary {
                    is_root: Some(true),
                    membership: None,
                }),
            ),
            design(
                "PayRunLineType",
                Some("PayRunLineType"),
                Some(authored_member("PayRunType")),
            ),
            // Derived membership: never indexed as authored.
            design(
                "BookType",
                Some("BookType"),
                Some(DddAggregateBoundary {
                    is_root: Some(false),
                    membership: Some(DddAggregateMembership {
                        root_json: class_json("PayRunType"),
                        origin: DddAggregateOrigin::Derived,
                    }),
                }),
            ),
        ])];
        let index = AuthoredBoundaryIndex::from_models(&models, |class| {
            let class = class.to_string();
            async move { Some((class.clone(), Some("payroll".to_string()))) }
        })
        .await;
        assert_eq!(
            index.routing_parent("PayRunLineType", "payroll"),
            Some("PayRunType"),
            "the authored member routes under the authored root"
        );
        assert_eq!(
            index.routing_parent("PayRunLineType", "other"),
            None,
            "a cross-domain authored parent does not route"
        );
        assert!(
            index.proven_root("PayRunType"),
            "the authored membership proves the root"
        );
        assert!(
            !index.proven_root("BookType"),
            "a derived member proves nothing"
        );
        assert!(index.is_authored_member("PayRunLineType"));
        assert!(
            !index.is_authored_member("BookType"),
            "derived never counts"
        );
        assert!(!index.is_empty());
    }

    #[tokio::test]
    async fn an_undecided_boundary_indexes_nothing() {
        let models = vec![model(vec![design("PayRunType", Some("PayRunType"), None)])];
        let index = AuthoredBoundaryIndex::from_models(&models, |_| async { None }).await;
        assert!(index.is_empty());
        assert!(!index.proven_root("PayRunType"));
        assert_eq!(index.routing_parent("PayRunType", "payroll"), None);
    }

    /// The `DddDesignSurface` accessors slice 4d/4e consume: the effective
    /// root verdict (`is_aggregate_root`, including containment-derived
    /// roots — the artifact records a root's origin nowhere) and the
    /// AUTHORED member root (origin-keyed; derived never drives routing).
    #[test]
    fn the_surface_accessors_split_the_verdict_from_the_authored_origin() {
        let models = vec![model(vec![
            design(
                "PayRunType",
                Some("PayRunType"),
                Some(DddAggregateBoundary {
                    is_root: Some(true),
                    membership: None,
                }),
            ),
            design(
                "PayRunLineType",
                Some("PayRunLineType"),
                Some(authored_member("PayRunType")),
            ),
            design(
                "BookType",
                Some("BookType"),
                Some(DddAggregateBoundary {
                    is_root: Some(false),
                    membership: Some(DddAggregateMembership {
                        root_json: class_json("PayRunType"),
                        origin: DddAggregateOrigin::Derived,
                    }),
                }),
            ),
            design("GhostType", None, None),
        ])];
        let surface = crate::ddd::design::DddDesignSurface::from_models(&models);
        assert!(
            surface.is_aggregate_root("PayRunType"),
            "the root verdict mirrors"
        );
        assert!(
            !surface.is_aggregate_root("GhostType"),
            "an undecided design (upstream could not decide) carries NO verdict — \
             nothing routes on it"
        );
        assert_eq!(
            surface.authored_member_root("PayRunLineType").as_deref(),
            Some("PayRunType"),
            "the authored membership names the root class"
        );
        assert_eq!(
            surface.authored_member_root("BookType"),
            None,
            "a DERIVED membership never drives routing"
        );
        assert_eq!(
            surface.authored_member_root("PayRunType"),
            None,
            "a root carries no membership"
        );
    }

    #[tokio::test]
    async fn a_chain_degrades_to_one_routed_level_and_a_self_member_drops() {
        // An authored 3-chain is upstream-illegal (E0386: a root that is
        // itself a member) — but the mirror can carry one, and the index
        // must degrade it deterministically: the MIDDLE member (proven
        // root by the deepest member's declaration) loses its own member
        // fact and stays top-level; the deepest member nests under it —
        // never a chain deeper than one routed level, never a cycle.
        let models = vec![model(vec![
            design(
                "LedgerType",
                Some("LedgerType"),
                Some(DddAggregateBoundary {
                    is_root: Some(true),
                    membership: None,
                }),
            ),
            design(
                "PayRunType",
                Some("PayRunType"),
                Some(authored_member("LedgerType")),
            ),
            design(
                "PayRunLineType",
                Some("PayRunLineType"),
                Some(authored_member("PayRunType")),
            ),
        ])];
        let index = AuthoredBoundaryIndex::from_models(&models, |class| {
            let class = class.to_string();
            async move { Some((class.clone(), Some("payroll".to_string()))) }
        })
        .await;
        assert!(
            index.proven_root("PayRunType"),
            "the deepest member's authored declaration proves the middle a root"
        );
        assert_eq!(
            index.routing_parent("PayRunType", "payroll"),
            None,
            "the contradictory member fact loses — the middle stays top-level"
        );
        assert_eq!(
            index.routing_parent("PayRunLineType", "payroll"),
            Some("PayRunType"),
            "the deepest member nests under the proven root"
        );
    }

    #[tokio::test]
    async fn a_member_of_itself_drops_out() {
        let models = vec![model(vec![design(
            "PayRunLineType",
            Some("PayRunLineType"),
            Some(authored_member("PayRunLineType")),
        )])];
        let index = AuthoredBoundaryIndex::from_models(&models, |class| {
            let class = class.to_string();
            async move { Some((class.clone(), Some("payroll".to_string()))) }
        })
        .await;
        assert_eq!(
            index.routing_parent("PayRunLineType", "payroll"),
            None,
            "a member of itself is upstream-illegal; the fact drops"
        );
    }

    #[tokio::test]
    async fn a_contradictory_member_proven_root_keeps_the_root_verdict() {
        // Upstream E0386 forbids a root that is itself a member; a stale
        // graph carrying both facts keeps the ROOT verdict (member loses).
        let models = vec![model(vec![
            design(
                "PayRunType",
                Some("PayRunType"),
                Some(authored_member("LedgerType")),
            ),
            design(
                "PayRunLineType",
                Some("PayRunLineType"),
                Some(authored_member("PayRunType")),
            ),
        ])];
        let index = AuthoredBoundaryIndex::from_models(&models, |class| {
            let class = class.to_string();
            async move { Some((class.clone(), Some("payroll".to_string()))) }
        })
        .await;
        assert!(
            index.proven_root("PayRunType"),
            "the proven-root verdict survives"
        );
        assert_eq!(
            index.routing_parent("PayRunType", "payroll"),
            None,
            "the contradictory member fact loses"
        );
        assert_eq!(
            index.routing_parent("PayRunLineType", "payroll"),
            Some("PayRunType"),
            "the consistent member keeps its authored parent"
        );
    }

    // ── WARN walker KATs (MockEngine: no-config and unresolvable shapes) ─

    fn mock(designs: Vec<DddDesignNode>) -> MockEngine {
        let db = MockEngine::builder()
            .with_schema(schema("PayRunType", "payroll"))
            .with_schema(schema("PayRunLineType", "payroll"))
            .build();
        futures::executor::block_on(async {
            db.ingest_ddd_model(&model(designs)).await.unwrap();
        });
        db
    }

    fn config() -> DomainConfig {
        toml::from_str(
            r#"
[defaults]
operations = []

[domains.payroll]
label = "Payroll"
schema_dir = "schemas/payroll"
postgres_schema = "payroll"
"#,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn an_unresolvable_authored_root_warns_naming_the_class() {
        let db = mock(vec![design(
            "PayRunLineType",
            Some("PayRunLineType"),
            Some(authored_member("NoSuchRoot")),
        )]);
        let warnings = aggregate_boundary_warnings(&db, &config()).await;
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("PayRunLineType") && w.contains("NoSuchRoot")),
            "the unresolvable root must warn naming the class: {warnings:?}"
        );
    }

    #[tokio::test]
    async fn a_derived_membership_never_warns() {
        let db = mock(vec![design(
            "PayRunLineType",
            Some("PayRunLineType"),
            Some(DddAggregateBoundary {
                is_root: Some(false),
                membership: Some(DddAggregateMembership {
                    root_json: class_json("NoSuchRoot"),
                    origin: DddAggregateOrigin::Derived,
                }),
            }),
        )]);
        let warnings = aggregate_boundary_warnings(&db, &config()).await;
        assert!(
            warnings.is_empty(),
            "derived memberships are informational only: {warnings:?}"
        );
    }

    #[tokio::test]
    async fn a_model_without_authored_boundaries_produces_no_warnings() {
        let db = mock(vec![design("PayRunType", Some("PayRunType"), None)]);
        let warnings = aggregate_boundary_warnings(&db, &config()).await;
        assert!(
            warnings.is_empty(),
            "the undecided shape stays byte-identical: {warnings:?}"
        );
    }
}
