//! The DDD design surface (issue #449 Phase 3): an in-memory index over the
//! ingested `.ddd` design models ([`DddModelGraph`]) that the repository,
//! DTO, command, and query generators consult to make their output
//! model-driven.
//!
//! The surface is cheap to construct (one `get_ddd_models` query) and
//! read-only after construction. Every consumer gates on it being non-empty
//! for the entity's schema title, so a graph without ddd models keeps every
//! generator byte-identical to the pre-#449 output.
//!
//! Mapping summary (Sculptor semantics, `pherolang/docs/DDD.md`):
//! - repository built-ins → CRUD operations: `findById→read`, `findAll→list`,
//!   `save→create`+`update`, `delete→delete`. A design without a repository
//!   maps to an EMPTY operation set (the entity gets no CRUD).
//! - declared repository operations carrying a DD9 `query` block → typed
//!   queries ([`crate::ddd::repository_query`]): the predicate is lowered to
//!   a parameterized store query, `sort` to an ORDER BY (identity
//!   tiebreaker), and `page` to a bounded window. A construct outside the
//!   documented subset is a HARD, named rejection — never a broader query.
//! - declared repository operations WITHOUT a `query` block → equality
//!   finders (the pre-DD9 Sculptor heuristic; still exercised by consumer
//!   models such as `findByTitle(String title)`). DEPRECATED: new models
//!   should declare a `where` predicate, which expresses ordering,
//!   pagination, and compound predicates the equality path cannot.
//! - search definitions → full-text surface (`has_fts`, filter fields,
//!   analyzer-driven FTS language).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{DddDesignNode, DddRepositoryNode, DddSearchNode};
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::ddd::repository_query::{QueryColumn, QueryColumnKind, QueryPlan};
use crate::error::Result;

/// One-time stderr warnings (per process). Generators run once per entity,
/// but the surface is consulted by several generators per entity — dedupe
/// so each design/finder/search warns at most once.
static WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(crate) fn warn_once(key: &str, message: &str) {
    let set = WARNED.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut set) = set.lock()
        && set.insert(key.to_string())
    {
        eprintln!("warning: ddd design: {message}");
    }
}

/// One declared parameter of a [`DesignFinder`].
#[derive(Debug, Clone, Serialize)]
pub struct FinderParam {
    /// The snake_cased parameter name (the entity column it filters on).
    pub name: String,
    /// The mapped Rust parameter type (never carries the `Option<>` wrapper
    /// — optionality is [`FinderParam::optional`], wrapped by the emitters).
    pub rust_type: String,
    /// `true` for `findByExample` example fields: `None` filters the column
    /// out of the predicate; `Some(value)` equality-matches it.
    pub optional: bool,
}

/// Which DSL construct a [`DesignFinder`] lowers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinderKind {
    /// A declared `findByX(x, ...)` signature — required equality params
    /// (the authored signature is the contract). DEPRECATED in favour of a
    /// DD9 `query` block, kept for query-less declared operations.
    Declared,
    /// A DD9 typed query (issue #659): a parameterized predicate, an
    /// ORDER BY, and a bounded page, lowered from the operation's `query`
    /// block.
    Query,
    /// The `findByKeys` repository built-in: equality on every `id` feature
    /// of the designing entity (the natural key), single row.
    FindByKeys,
    /// The `findByExample` repository built-in: `Option`-wrapped equality on
    /// every stored non-id feature — `None` filters out, `Some` matches.
    FindByExample,
}

/// A lowered repository operation: either an equality finder or a DD9 typed
/// query over the designing entity's columns.
///
/// `method_name` is the snake_cased Rust method name both the trait
/// template and the impl emitters agree on; `return_rust_type` is the
/// entity-native response type (`{Entity}Response`).
#[derive(Debug, Clone, Serialize)]
pub struct DesignFinder {
    /// The operation name exactly as authored (e.g. `findByTitle`).
    pub name: String,
    /// The entity's Rust type name (patched by the generators from the
    /// schema; initialized with the resolved schema title).
    pub entity_name: String,
    /// Parameters in declaration order.
    pub params: Vec<FinderParam>,
    /// `true` when the return cardinality is many (`Vec<Response>`);
    /// `false` for a single optional row (`Option<Response>`).
    pub returns_many: bool,
    /// The resolved return rust type for the entity case
    /// (`{Entity}Response`).
    pub return_rust_type: String,
    /// The snake_cased Rust method name (e.g. `find_by_title`).
    pub method_name: String,
    /// Which DSL construct this finder lowers (declared signature, DD9
    /// query, `findByKeys`, or `findByExample`).
    pub kind: FinderKind,
    /// The lowered DD9 query plan — `Some` exactly when
    /// [`DesignFinder::kind`] is [`FinderKind::Query`]. The emitters render
    /// their store's query from it; the equality finders leave it `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<QueryPlan>,
}

/// Per-application index of the ingested `.ddd` design models, keyed by
/// resolved schema title.
#[derive(Debug, Default)]
pub struct DddDesignSurface {
    designs: HashMap<String, DddDesignNode>,
    repositories: HashMap<String, DddRepositoryNode>,
    searches: HashMap<String, DddSearchNode>,
    /// Modeled DTO boundaries by boundary name (DD3, issue #667). Boundary
    /// names are application-unique upstream, so one map is enough.
    dto_shapes: HashMap<String, codegraph_core::types::DddDtoShapeNode>,
}

impl DddDesignSurface {
    /// Build the surface from every ingested `.ddd` model. Empty (and
    /// warning-free) when the graph carries no models.
    pub async fn from_graph(db: &dyn GraphQuerier) -> Result<Self> {
        let models = db.get_ddd_models().await?;
        Ok(Self::from_models(&models))
    }

    /// Index constructor over already-fetched models (unit-testable).
    pub fn from_models(models: &[codegraph_core::types::DddModelGraph]) -> Self {
        let mut surface = Self::default();
        for model in models {
            let app = &model.application.name;
            for design in &model.designs {
                let Some(title) = design.resolved_title.clone() else {
                    continue;
                };
                surface.warn_flags(app, design);
                // DD3: a `dto` design (a projection or mirror) is a BOUNDARY,
                // not a second design of its source class — upstream rule 3 is
                // explicit that a projection "may coexist with its source's
                // entity design". The boundary carries no entity plane (no
                // repository, every flag off), so it must never SHADOW the
                // entity design that maps onto the same schema title (it would
                // silently empty the operation set and drop the auditable
                // flag). Keep the boundary in the graph mirror; prefer the
                // entity/value design here.
                if let Some(existing) = surface.designs.get(&title)
                    && existing.stereotype == "dto"
                    && design.stereotype != "dto"
                {
                    surface.designs.insert(title, design.clone());
                    continue;
                }
                surface
                    .designs
                    .entry(title)
                    .or_insert_with(|| design.clone());
            }
            for search in &model.searches {
                let Some(title) = search.entity_title.clone() else {
                    continue;
                };
                surface.warn_search_analyzers(app, search);
                surface.searches.insert(title, search.clone());
            }
            for repo in &model.repositories {
                // Attach the repository to its designing class's resolved
                // title (authored names match exactly — rule 2 uniqueness).
                let Some(design) = model
                    .designs
                    .iter()
                    .find(|d| d.application == repo.application && d.class == repo.design_class)
                else {
                    continue;
                };
                let Some(title) = design.resolved_title.clone() else {
                    continue;
                };
                surface.repositories.insert(title, repo.clone());
            }
            // DD3 (issue #667): boundary shapes are indexed by the name a
            // `DddTypeRef::Dto` slot references.
            for shape in &model.dto_shapes {
                surface.dto_shapes.insert(shape.name.clone(), shape.clone());
            }
        }
        surface
    }

    /// The modeled DTO boundary with this name, when the graph carries one.
    pub fn dto_shape(&self, name: &str) -> Option<&codegraph_core::types::DddDtoShapeNode> {
        self.dto_shapes.get(name)
    }

    /// `true` when the graph carries no usable designs — every consumer
    /// gates on this to keep flag-off output byte-identical.
    pub fn is_empty(&self) -> bool {
        self.designs.is_empty()
    }

    /// `true` when the graph carries at least one modeled DTO boundary
    /// (DD3, issue #667).
    pub fn has_dto_boundaries(&self) -> bool {
        !self.dto_shapes.is_empty()
    }

    /// The design decision for the entity with this schema title.
    pub fn design_for(&self, schema_title: &str) -> Option<&DddDesignNode> {
        self.designs.get(schema_title)
    }

    /// The design's effective aggregate-root verdict (issue #666 slice 4b):
    /// `true` when the DD4 boundary blob carries `is_root == Some(true)`.
    /// NOTE: the phero-ir artifact records the origin of a ROOT verdict
    /// nowhere (only a non-root's membership carries `origin`), so this
    /// includes containment-derived roots — the AUTHORED proof of a root
    /// is `crate::ddd::aggregate`'s proven-root check (a member's authored
    /// declaration naming this class).
    pub fn is_aggregate_root(&self, schema_title: &str) -> bool {
        self.designs
            .get(schema_title)
            .and_then(|design| design.aggregate_boundary.as_ref())
            .and_then(|boundary| boundary.is_root)
            == Some(true)
    }

    /// The AUTHORED aggregate-root class the design's membership names
    /// (issue #666 slice 4b): `Some(root class name)` only when the
    /// membership's origin is `Authored` — a `Derived` membership is
    /// upstream's own containment derivation and never drives routing.
    pub fn authored_member_root(&self, schema_title: &str) -> Option<String> {
        let design = self.designs.get(schema_title)?;
        let membership = design.aggregate_boundary.as_ref()?.membership.as_ref()?;
        if membership.origin != codegraph_core::types::DddAggregateOrigin::Authored {
            return None;
        }
        crate::ddd::aggregate::authored_root_class_name(&membership.root_json)
    }

    /// The lifecycle state machine designed for the entity, when the graph
    /// carries one (`.ddd` `machine <field>: <Enum>`). The stored form is
    /// opaque phero-ir JSON; codegraph-generate (which may depend on phero-ir)
    /// rehydrates it here.
    pub fn machine_for(&self, schema_title: &str) -> Option<phero_ir::ddd::StateMachine> {
        let design = self.designs.get(schema_title)?;
        let value = design.machine_json.as_ref()?;
        serde_json::from_value(value.clone()).ok()
    }

    /// Every declared machine state for the entity, in canonical order
    /// (initial, terminals, then transition endpoints). Empty when the
    /// entity carries no machine.
    pub fn machine_states_for(&self, schema_title: &str) -> Vec<String> {
        self.machine_for(schema_title)
            .map(|machine| machine.states().into_iter().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// The repository designed for the entity with this schema title.
    pub fn repository_for(&self, schema_title: &str) -> Option<&DddRepositoryNode> {
        self.repositories.get(schema_title)
    }

    /// The design-mapped operation set for an entity addressed by its RUST
    /// type name — the title with the mox bridge's `Type` suffix stripped
    /// ONCE and PascalCased (`Timecard` for `TimecardType`), the same rule
    /// the ingest applies to build `rust_type_name`
    /// (`sanitize_rust_type_name(strip_suffix(title, suffix))`; the
    /// sanitize step's leading-digit strip and 200-char cap apply to a
    /// handful of exotic titles and are not replicated here — those titles
    /// miss the alias and fall back exactly as before). Handlers, routers,
    /// and the OpenAPI plane key entities by the rust name while the
    /// surface is keyed by resolved TITLE; a miss here would silently fall
    /// back to the config defaults and re-add operations the design's
    /// repository does not implement (the crewbase compile-gate class, hit
    /// again by the #663 guard fixture whose Timecard design declares no
    /// `delete`). Exact titles still win; the alias candidates resolve in
    /// sorted-title order (deterministic — a genuine two-titles-one-rust-
    /// name collision picks the alphabetically first, documented).
    pub fn operations_for_rust_name(&self, rust_name: &str) -> Option<Vec<String>> {
        if let Some(ops) = self.operations_for(rust_name) {
            return Some(ops);
        }
        let mut candidates: Vec<&String> = self
            .designs
            .keys()
            .filter(|title| {
                // `strip_suffix` strips ONCE — `trim_end_matches` (the
                // resource-name convention) would over-strip repeated
                // suffixes (`TaskTypeType` → `Task`, whose rust name is
                // `TaskType`).
                let stripped = title.strip_suffix("Type").unwrap_or(title);
                codegraph_naming::to_pascal_case(stripped) == rust_name
            })
            .collect();
        candidates.sort();
        let title = candidates.first()?;
        self.operations_for(title)
    }

    /// The search definition over the entity with this schema title.
    pub fn search_for(&self, schema_title: &str) -> Option<&DddSearchNode> {
        self.searches.get(schema_title)
    }

    /// The design-mapped operation set for the entity, or `None` when no
    /// design exists (the caller keeps its schema-derived operations).
    ///
    /// A design WITH a repository maps its built-ins
    /// (`findById`/`findByKeys→read`, `findAll`/`findByExample→list`,
    /// `save→create`+`update`, `delete→delete`); a design WITHOUT a
    /// repository maps to an empty set. Declared (non-built-in) operations
    /// are finder candidates, not CRUD operations.
    ///
    /// `protected` built-ins (Sculptor visibility) stay OFF the public
    /// interface: they lower onto the repository but map no operation here.
    pub fn operations_for(&self, schema_title: &str) -> Option<Vec<String>> {
        if !self.designs.contains_key(schema_title) {
            return None;
        }
        let repo = match self.repositories.get(schema_title) {
            // A design without a repository maps to an EMPTY operation set.
            None => return Some(Vec::new()),
            Some(repo) => repo,
        };
        let has = |builtin: &str| {
            repo.operations
                .iter()
                .any(|op| op.builtin.as_deref() == Some(builtin) && !op.is_protected)
        };
        // Canonical order matches the codegraph default operation list
        // (create/read/update/delete/list) so templates stay stable.
        let mut ops: Vec<String> = Vec::with_capacity(5);
        if has("save") {
            ops.push("create".to_string());
            ops.push("update".to_string());
        }
        if has("findById") || has("findByKeys") {
            ops.push("read".to_string());
        }
        if has("delete") {
            ops.push("delete".to_string());
        }
        if has("findAll") || has("findByExample") {
            ops.push("list".to_string());
        }
        Some(ops)
    }

    /// Lower the repository's declared operations into design finders.
    /// `entity_name` is the entity's Rust type name; finders are returned
    /// with it patched into `entity_name`/`return_rust_type`.
    ///
    /// A DD9 `query`-carrying operation lowers through
    /// [`crate::ddd::repository_query`]; a construct outside the documented
    /// subset is a HARD error naming the repository, the operation, and the
    /// construct — never a silently broader query. Query-less declared
    /// operations lower through the equality-finder heuristic (deprecated,
    /// kept for consumer models that predate DD9) and are skipped with a
    /// one-time warning when their signature does not resolve (no params,
    /// non-entity return, unresolvable param type, or array parameters).
    pub fn finders_for(
        &self,
        schema_title: &str,
        entity_name: &str,
        features: &EntityFeatures,
    ) -> Result<Vec<DesignFinder>> {
        let Some(repo) = self.repository_for(schema_title) else {
            return Ok(Vec::new());
        };
        let Some(design) = self.design_for(schema_title) else {
            return Ok(Vec::new());
        };
        let design_class_leaf = leaf_name(&design.class);
        let mut finders = Vec::new();
        for op in &repo.operations {
            // Declared signatures lower as equality finders (existing
            // Sculptor semantics). `protected` declared finders still lower:
            // the repository is internal, protection governs the API plane.
            if op.builtin.is_none() {
                // DD9 (issue #659): a `query` block routes to the typed-query
                // plane. A lowering failure is a hard, named rejection.
                if op.query_json.is_some() {
                    finders.push(lower_query_op(
                        op,
                        &repo.name,
                        design_class_leaf,
                        entity_name,
                        features,
                    )?);
                    continue;
                }
                match lower_finder(op, design_class_leaf, entity_name, features) {
                    Ok(mut finder) => {
                        finder.kind = FinderKind::Declared;
                        finders.push(finder);
                    }
                    Err(reason) => warn_once(
                        &format!("finder:{}::{}", repo.name, op.name),
                        &format!("finder `{}.{}` skipped: {reason}", repo.name, op.name),
                    ),
                }
                continue;
            }
            // The two query built-ins lower onto the finder plane with
            // consumer-known signatures; the four CRUD built-ins map via
            // `operations_for` instead.
            match op.builtin.as_deref() {
                Some("findByKeys") => match keys_finder(op, entity_name, features) {
                    Some(finder) => finders.push(finder),
                    None => warn_once(
                        &format!("finder:{}::{}", repo.name, op.name),
                        &format!(
                            "built-in `{}.{}` skipped: `{}` declares no lowerable id feature                              (findByKeys matches the natural key)",
                            repo.name, op.name, design.class
                        ),
                    ),
                },
                Some("findByExample") => match example_finder(op, entity_name, features) {
                    Some(finder) => finders.push(finder),
                    None => warn_once(
                        &format!("finder:{}::{}", repo.name, op.name),
                        &format!(
                            "built-in `{}.{}` skipped: `{}` declares no lowerable stored \
                                 feature (findByExample matches example fields)",
                            repo.name, op.name, design.class
                        ),
                    ),
                },
                _ => {}
            }
        }
        Ok(finders)
    }

    /// The search's text fields ordered by descending boost, mapped to the
    /// FTS weight letters (A/B/C/D) the `SearchConfig.fts_weights` shape
    /// carries. Recorded for the DDL-side follow-up; nothing in the v1 ddd
    /// contexts consumes weights (see the module docs).
    pub fn search_boost_weights(&self, schema_title: &str) -> Option<Vec<(String, char)>> {
        let search = self.search_for(schema_title)?;
        let mut fields: Vec<&codegraph_core::types::DddSearchField> = search.text.iter().collect();
        fields.sort_by(|a, b| {
            b.boost
                .unwrap_or(1.0)
                .partial_cmp(&a.boost.unwrap_or(1.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.property.cmp(&b.property))
        });
        let letters = ['A', 'B', 'C', 'D'];
        Some(
            fields
                .into_iter()
                .enumerate()
                .map(|(i, f)| (f.property.clone(), letters[i.min(letters.len() - 1)]))
                .collect(),
        )
    }

    /// Warn once when the design's stereotype disagrees with the schema
    /// graph's entity classification. mox owns entity/VO — this never flips
    /// behavior.
    pub fn warn_stereotype_mismatch(&self, schema_title: &str, schema_is_entity: bool) {
        let Some(design) = self.design_for(schema_title) else {
            return;
        };
        let matches = match design.stereotype.as_str() {
            "entity" => schema_is_entity,
            "value" | "dto" => !schema_is_entity,
            _ => true,
        };
        if !matches {
            warn_once(
                &format!("stereotype:{}::{}", design.application, design.class),
                &format!(
                    "design stereotypes `{}` as `{}`, but the schema graph classifies `{}` as {} — mox owns the classification; generation follows the schema",
                    design.class,
                    design.stereotype,
                    schema_title,
                    if schema_is_entity {
                        "an entity"
                    } else {
                        "a value object"
                    }
                ),
            );
        }
    }

    fn warn_flags(&self, app: &str, design: &DddDesignNode) {
        let flags = &design.flags;
        // `optimisticLocking` is defaulted ON for entity designs (pherolang
        // #47) and recorded on the flag plane; its version-column emission
        // remains a tracked deferral, so it no longer warns (a warning on
        // every entity design would be noise). Explicit opt-outs keep their
        // signals below.
        if flags.cache {
            warn_once(
                &format!("flag:cache:{app}::{}", design.class),
                &format!(
                    "design `{}`: cache not yet mapped — recorded only",
                    design.class
                ),
            );
        }
        if flags.non_persistent {
            warn_once(
                &format!("flag:non_persistent:{app}::{}", design.class),
                &format!(
                    "design `{}`: nonPersistent not yet mapped (DDL suppression deferred — mox exclusion covers it today) — recorded only",
                    design.class
                ),
            );
        }
        // `scaffold` is a no-op: everything scaffolds today.
    }

    fn warn_search_analyzers(&self, app: &str, search: &DddSearchNode) {
        let default = search.analyzer.clone().unwrap_or_default();
        for field in &search.text {
            if let Some(field_analyzer) = &field.analyzer
                && field_analyzer != &default
            {
                warn_once(
                    &format!("search:analyzer:{app}::{}", search.name),
                    &format!(
                        "search `{}`: field `{}` overrides the analyzer to `{field_analyzer}` (default `{}`) — per-field analyzers are not mapped; the default analyzer applies",
                        search.name, field.property, default
                    ),
                );
            }
        }
    }
}

/// Every DD9 query that cannot be lowered, as fully-qualified messages
/// (`query {Repository}.{operation}: …`).
///
/// This is the run-path validation seam: a design query whose predicate,
/// sort key, or page falls outside the documented subset must stop a run
/// BEFORE generation writes a tree — silently skipping the method would let
/// a caller fall back to a broader read. Entity titles the graph cannot
/// resolve are skipped here (the ingest already warned).
pub async fn query_refusals(db: &dyn GraphQuerier) -> Result<Vec<String>> {
    let models = db.get_ddd_models().await?;
    let mut refusals = Vec::new();
    for model in &models {
        for repo in &model.repositories {
            let Some(design) = model.designs.iter().find(|design| {
                design.application == repo.application && design.class == repo.design_class
            }) else {
                continue;
            };
            let Some(title) = design.resolved_title.as_deref() else {
                continue;
            };
            if !repo.operations.iter().any(|op| op.query_json.is_some()) {
                continue;
            }
            let features = EntityFeatures::from_graph(db, title).await?;
            let design_class_leaf = leaf_name(&design.class);
            for op in repo.operations.iter().filter(|op| op.query_json.is_some()) {
                if let Err(error) =
                    lower_query_op(op, &repo.name, design_class_leaf, title, &features)
                {
                    refusals.push(match error {
                        crate::error::Error::Config(message) => message,
                        other => other.to_string(),
                    });
                }
            }
        }
    }
    Ok(refusals)
}

/// The last segment of an authored (possibly package-qualified) class name.
fn leaf_name(class: &str) -> &str {
    class.rsplit(['.', ':']).next().unwrap_or(class)
}

/// Build filter fields from a design search's declared `filters`, in the
/// design's declaration order. Names match entity property names or their
/// rust/column forms; unresolvable names warn once and are skipped.
pub async fn design_filter_fields(
    db: &dyn GraphQuerier,
    schema_title: &str,
    search: &DddSearchNode,
) -> Result<Vec<crate::filter_fields::FilterFieldInfo>> {
    let all_props = db.get_properties(schema_title).await?;
    let mut fields = Vec::new();
    let mut seen = HashSet::new();
    for key in &search.filters {
        let Some(prop) = all_props.iter().find(|p| {
            let fd = codegraph_core::types::resolve_field(p);
            &p.name == key || &fd.rust_field_name == key || &fd.column_name == key
        }) else {
            warn_once(
                &format!("search:filter:{}::{}", search.name, key),
                &format!(
                    "search `{}`: filter `{key}` does not name a property of `{schema_title}` — skipped",
                    search.name
                ),
            );
            continue;
        };
        if prop.is_array {
            warn_once(
                &format!("search:filter:{}::{}", search.name, key),
                &format!(
                    "search `{}`: filter `{key}` is an array property — array filters are not mapped",
                    search.name
                ),
            );
            continue;
        }
        let fd = codegraph_core::types::resolve_field(prop);
        if seen.insert(fd.rust_field_name.clone()) {
            fields.push(crate::filter_fields::FilterFieldInfo {
                field_name: fd.rust_field_name,
                pg_column_name: fd.column_name,
                rust_type: prop.rust_field_type.clone(),
                is_nullable: !prop.is_required,
            });
        }
    }
    Ok(fields)
}

/// The designing entity's resolved signature shape shared by the equality
/// finders and the DD9 query plane: the declared return cardinality and the
/// mapped parameters.
struct FinderSignature {
    params: Vec<FinderParam>,
    returns_many: bool,
}

/// Resolve a declared operation's return type (the designing entity) and
/// parameters (the finder/query bind surface).
fn resolve_finder_signature(
    op: &codegraph_core::types::DddRepositoryOperation,
    design_class_leaf: &str,
    features: &EntityFeatures,
    signature_word: &str,
) -> std::result::Result<FinderSignature, String> {
    // The return type must resolve to the designing entity.
    let Some(return_json) = &op.return_type else {
        return Err("no return type declared".to_string());
    };
    // DD3: the slot is a `DddTypeRef` — a domain type or a DTO boundary.
    let return_slot = crate::ddd::signature::signature_type(return_json);
    let Some(return_type) = return_slot.as_domain() else {
        return Err(format!(
            "return type is a {} — a {signature_word} must return the designing entity",
            return_slot.describe()
        ));
    };
    let class_name = match return_type {
        phero_ir::TypeRef::Class { name, .. } => name.clone(),
        _ => {
            return Err(format!(
                "return type `{}` does not resolve to the designing entity",
                return_type.qualified_name().unwrap_or_default()
            ));
        }
    };
    if class_name != design_class_leaf {
        return Err(format!(
            "return type `{class_name}` does not resolve to the designing entity `{design_class_leaf}`"
        ));
    }

    let returns_many = op
        .return_multiplicity
        .as_ref()
        .map(|m| serde_json::from_value::<phero_ir::Multiplicity>(m.clone()).map(|m| m.is_many()))
        .transpose()
        .map_err(|e| format!("unresolvable return multiplicity ({e})"))?
        .unwrap_or(false);

    let mut params = Vec::with_capacity(op.params.len());
    for param in &op.params {
        if param.multiplicity.is_some() {
            return Err(format!(
                "parameter `{}` is an array — array parameters are not mapped",
                param.name
            ));
        }
        let slot = crate::ddd::signature::signature_type(&param.type_json);
        let Some(type_ref) = slot.as_domain() else {
            return Err(format!(
                "parameter `{}` is a {} — a {signature_word} parameter must be a domain type",
                param.name,
                slot.describe()
            ));
        };
        let snake = codegraph_naming::to_snake_case(&param.name);
        // Primitives and the `Uuid`/`DateTime`/`Decimal` named family map
        // directly; any other named type (`.mox` wraps datatypes such as
        // `JurisdictionCode`) resolves to its COLUMN PROJECTION — the named
        // feature's base type (issue #523), so the signature binds the same
        // columns the DTOs expose.
        let rust_type = match phero_type_to_rust(type_ref) {
            Some(rt) => rt,
            None => match type_ref {
                // A codelist enum/vocabulary filter binds the stored TEXT
                // code: the DTO exposes the enum, but the column is text and
                // the equality finder compares codes (P2.5).
                phero_ir::TypeRef::Enum { .. } | phero_ir::TypeRef::Vocabulary { .. } => {
                    "String".to_string()
                }
                _ => features.resolve_type(&snake).ok_or_else(|| {
                    format!(
                        "unresolvable parameter type `{}` for `{}`; use `String` for \
                         a codelist filter (the stored code)",
                        type_ref
                            .qualified_name()
                            .unwrap_or_else(|| "primitive".to_string()),
                        param.name
                    )
                })?,
            },
        };
        params.push(FinderParam {
            name: snake,
            rust_type,
            optional: false,
        });
    }

    Ok(FinderSignature {
        params,
        returns_many,
    })
}

/// Lower one declared repository operation into a [`DesignFinder`].
///
/// DEPRECATED path (kept for query-less declared operations): every
/// parameter lowers to an equality predicate on the column it names. New
/// models should declare a DD9 `query` block instead — only that path can
/// express ordering, pagination, and compound predicates.
fn lower_finder(
    op: &codegraph_core::types::DddRepositoryOperation,
    design_class_leaf: &str,
    entity_name: &str,
    features: &EntityFeatures,
) -> std::result::Result<DesignFinder, String> {
    let signature = resolve_finder_signature(op, design_class_leaf, features, "finder")?;
    if signature.params.is_empty() {
        return Err("no parameters declared".to_string());
    }

    Ok(DesignFinder {
        method_name: codegraph_naming::to_snake_case(&op.name),
        name: op.name.clone(),
        entity_name: entity_name.to_string(),
        params: signature.params,
        returns_many: signature.returns_many,
        return_rust_type: format!("{entity_name}Response"),
        kind: FinderKind::Declared,
        query: None,
    })
}

/// Lower one DD9 `query`-carrying repository operation into a
/// [`DesignFinder`]. Every refusal is a hard, fully-qualified message
/// (`query {Repository}.{operation}: …`) — the caller propagates it as a
/// generation error rather than skipping the method, because a skipped query
/// is indistinguishable from a broader one at the call site.
fn lower_query_op(
    op: &codegraph_core::types::DddRepositoryOperation,
    repository: &str,
    design_class_leaf: &str,
    entity_name: &str,
    features: &EntityFeatures,
) -> Result<DesignFinder> {
    let label = format!("query `{repository}.{}`", op.name);
    let fail = |reason: String| crate::error::Error::Config(format!("{label}: {reason}"));
    let Some(query_json) = op.query_json.as_ref() else {
        return Err(fail("the operation carries no query block".to_string()));
    };
    // The graph stores the spec as phero-ir `QuerySpec` JSON (codegraph-core
    // carries no phero-ir dependency); rehydrate it here.
    let spec: phero_ir::ddd::QuerySpec = serde_json::from_value(query_json.clone())
        .map_err(|e| fail(format!("the query block is not a valid QuerySpec ({e})")))?;
    let signature =
        resolve_finder_signature(op, design_class_leaf, features, "query").map_err(&fail)?;
    for reserved in ["limit", "offset"] {
        if signature.params.iter().any(|param| param.name == reserved) {
            return Err(fail(format!(
                "the operation declares a parameter `{reserved}`, which collides with the \
                 generated pagination parameter — rename it"
            )));
        }
    }
    let plan = crate::ddd::repository_query::lower_query_spec(
        &spec,
        &label,
        &op.capabilities,
        &signature.params,
        &features.query_columns,
    )
    .map_err(crate::error::Error::Config)?;
    Ok(DesignFinder {
        method_name: codegraph_naming::to_snake_case(&op.name),
        name: op.name.clone(),
        entity_name: entity_name.to_string(),
        params: signature.params,
        returns_many: signature.returns_many,
        return_rust_type: format!("{entity_name}Response"),
        kind: FinderKind::Query,
        query: Some(plan),
    })
}

/// The designing entity's lowerable features, sourced from the graph
/// schema — the param universe for the `findByKeys` / `findByExample`
/// repository built-ins (whose signatures the consumer knows) and the
/// wraps-datatype resolution for declared finder parameters (issue #523).
#[derive(Debug, Default, Clone)]
pub struct EntityFeatures {
    /// `(name, rust_type)` for every `id` feature (the natural key), in
    /// declaration order.
    pub id_features: Vec<(String, String)>,
    /// `(name, rust_type)` for every stored non-id scalar feature
    /// (codelist/class references excluded — they are not scalar equality
    /// material), in declaration order.
    pub stored_features: Vec<(String, String)>,
    /// `(name, rust_type)` for every scalar column of a contained value
    /// (child + grandchild tables), bare column names in walk order — the
    /// typing view of the emitter's `ChildTableInfo`/`ChildColumn` universe.
    pub contained_features: Vec<(String, String)>,
    /// The DD9 query column universe (issue #659): every modeled feature the
    /// predicate or a sort key may reference, resolved to its PostgreSQL
    /// column, with unsupported kinds recorded so a refusal can name what it
    /// found. Built from the graph's property set — the same source the
    /// repository emitter derives its tree columns from — plus the identity
    /// column when the entity models none.
    pub query_columns: Vec<QueryColumn>,
}

impl EntityFeatures {
    /// The column-projection type for a finder parameter name: direct `id`
    /// features first, then stored features, then contained-value columns
    /// (bare names). `None` when the name matches no lowerable feature.
    pub fn resolve_type(&self, snake_name: &str) -> Option<String> {
        self.id_features
            .iter()
            .chain(self.stored_features.iter())
            .chain(self.contained_features.iter())
            .find(|(name, _)| name == snake_name)
            .map(|(_, ty)| ty.clone())
    }
}

impl EntityFeatures {
    /// Collect the feature universe from the entity's graph properties.
    /// Features whose types the equality-finder lowering cannot express
    /// (enums/vocabularies, class references) are excluded from
    /// `stored_features`; `id` features are excluded from the example set
    /// (an example matches attributes, not identity).
    pub async fn from_graph(db: &dyn GraphQuerier, schema_title: &str) -> Result<Self> {
        let props = db.get_properties(schema_title).await?;
        let mut features = Self::default();
        for prop in &props {
            let rust_type = prop.rust_field_type.clone();
            let is_scalar = matches!(
                prop.effective_kind(),
                None | Some(RefClassificationKind::PrimitiveWrapper)
            ) && !prop.is_array;
            let name = codegraph_naming::to_snake_case(&prop.rust_field_name);
            if prop.is_id {
                if is_scalar {
                    features.id_features.push((name, rust_type));
                }
                continue;
            }
            if is_scalar && !matches!(name.as_str(), "created_at" | "updated_at") {
                features.stored_features.push((name, rust_type));
            }
        }
        // Surrogate fallback: an entity with no explicit `id` feature
        // (the common mox shape) keys on the DDL's synthetic UUID primary
        // key — the consumer-known `findByKeys` signature lowers to it.
        if features.id_features.is_empty() {
            features
                .id_features
                .push(("id".to_string(), "Uuid".to_string()));
        }
        // Contained-value columns (issue #523): the typing view of the
        // child/grandchild tables the repository emitter builds — bare
        // column names, child then grandchild, so wraps-typed finder params
        // can name them.
        for prop in &props {
            if prop.effective_kind() != Some(RefClassificationKind::ValueObject) {
                continue;
            }
            let target = if prop.is_array {
                db.get_array_item_schema(&prop.name, schema_title).await
            } else {
                db.get_property_ref_target(&prop.name, schema_title).await
            };
            let Ok(Some(target)) = target else {
                continue;
            };
            collect_contained_features(db, &target.title, 1, &mut features.contained_features)
                .await;
        }
        // DD9 (issue #659): the query column universe. The predicate may
        // only typecheck against MODELED features, so the modeled property
        // set is exactly the universe — audit columns codegraph synthesizes
        // are not properties and are correctly absent.
        features.query_columns = query_columns_from_properties(&props);
        Ok(features)
    }
}

/// Build the DD9 query column universe from an entity's graph properties.
///
/// The caller passes the same property slice the repository emitter derives
/// its tree columns from, so predicate resolution and emission agree. Class
/// references, collections, contains-targets, and structured wrappers are
/// recorded with their kind (a refusal names what it found) but are never
/// comparable.
fn query_columns_from_properties(
    props: &[codegraph_core::types::PropertyNode],
) -> Vec<QueryColumn> {
    let mut columns: Vec<QueryColumn> = Vec::with_capacity(props.len() + 1);
    let mut has_identity = false;
    for prop in props {
        let field = codegraph_core::types::resolve_field(prop);
        let kind = match prop.effective_kind() {
            Some(RefClassificationKind::ValueObject) => QueryColumnKind::Contained,
            Some(RefClassificationKind::EntityReference) => QueryColumnKind::ClassReference,
            // Codelists are stored TEXT codes; primitives map directly.
            Some(RefClassificationKind::CodelistReference)
            | Some(RefClassificationKind::InlineEnum) => QueryColumnKind::Scalar,
            _ => QueryColumnKind::Scalar,
        };
        let kind = if prop.is_array {
            QueryColumnKind::Collection
        } else {
            kind
        };
        if prop.is_id {
            has_identity = true;
        }
        columns.push(QueryColumn {
            feature: prop.name.clone(),
            field: field.rust_field_name.clone(),
            pg_column: field.column_name.clone(),
            // Class references expose their FK column (`x_id`, a UUID); the
            // storage type is the column's, not the referenced class's.
            rust_type: if prop.is_id {
                "Uuid".to_string()
            } else {
                prop.rust_field_type.clone()
            },
            is_nullable: prop.is_nullable || !prop.is_required,
            kind,
        });
    }
    // Surrogate identity: the DDL always carries the `id UUID` primary key,
    // even when the model declares no `id` feature (the common mox shape).
    if !has_identity {
        columns.push(QueryColumn {
            feature: "id".to_string(),
            field: "id".to_string(),
            pg_column: "id".to_string(),
            rust_type: "Uuid".to_string(),
            is_nullable: false,
            kind: QueryColumnKind::Scalar,
        });
    }
    columns
}

/// Recursively collect scalar columns of contained value children
/// (`depth` 1 = child, 2 = grandchild — the emitter's joinable universe).
async fn collect_contained_features(
    db: &dyn GraphQuerier,
    schema_title: &str,
    depth: usize,
    out: &mut Vec<(String, String)>,
) {
    if depth > 2 {
        return;
    }
    let Ok(props) = db.get_properties(schema_title).await else {
        return;
    };
    for prop in &props {
        if prop.rust_field_name == "id" || prop.is_array {
            continue;
        }
        out.push((
            codegraph_naming::to_snake_case(&prop.rust_field_name),
            prop.rust_field_type.clone(),
        ));
    }
    for prop in &props {
        if prop.effective_kind() != Some(RefClassificationKind::ValueObject) {
            continue;
        }
        let target = if prop.is_array {
            db.get_array_item_schema(&prop.name, schema_title).await
        } else {
            db.get_property_ref_target(&prop.name, schema_title).await
        };
        let Ok(Some(target)) = target else {
            continue;
        };
        Box::pin(collect_contained_features(
            db,
            &target.title,
            depth + 1,
            out,
        ))
        .await;
    }
}

/// Lower the `findByKeys` built-in: equality on every id feature (the
/// natural key), single row. `None` when the entity has no lowerable id
/// feature or an id feature whose type the equality lowering cannot bind.
fn keys_finder(
    op: &codegraph_core::types::DddRepositoryOperation,
    entity_name: &str,
    features: &EntityFeatures,
) -> Option<DesignFinder> {
    if features.id_features.is_empty() {
        return None;
    }
    let params = features
        .id_features
        .iter()
        .map(|(name, rust_type)| FinderParam {
            name: name.clone(),
            rust_type: rust_type.clone(),
            optional: false,
        })
        .collect();
    Some(DesignFinder {
        method_name: codegraph_naming::to_snake_case(&op.name),
        name: op.name.clone(),
        entity_name: entity_name.to_string(),
        params,
        returns_many: false,
        return_rust_type: format!("{entity_name}Response"),
        kind: FinderKind::FindByKeys,
        query: None,
    })
}

/// Lower the `findByExample` built-in: `Option`-wrapped equality on every
/// stored non-id scalar feature — `None` filters the column out, `Some`
/// matches. Many rows.
fn example_finder(
    op: &codegraph_core::types::DddRepositoryOperation,
    entity_name: &str,
    features: &EntityFeatures,
) -> Option<DesignFinder> {
    if features.stored_features.is_empty() {
        return None;
    }
    let params = features
        .stored_features
        .iter()
        .map(|(name, rust_type)| FinderParam {
            name: name.clone(),
            rust_type: rust_type.clone(),
            optional: true,
        })
        .collect();
    Some(DesignFinder {
        method_name: codegraph_naming::to_snake_case(&op.name),
        name: op.name.clone(),
        entity_name: entity_name.to_string(),
        params,
        returns_many: true,
        return_rust_type: format!("{entity_name}Response"),
        kind: FinderKind::FindByExample,
        query: None,
    })
}

/// Map a phero-ir type reference to the Rust type the generated repositories
/// use for it. Returns `None` for anything the equality-finder lowering
/// cannot express (struct/class/enum/vocabulary parameters).
pub fn phero_type_to_rust(type_ref: &phero_ir::TypeRef) -> Option<String> {
    match type_ref {
        phero_ir::TypeRef::Primitive(prim) => match prim {
            phero_ir::PrimitiveType::String | phero_ir::PrimitiveType::Char => {
                Some("String".to_string())
            }
            phero_ir::PrimitiveType::Boolean => Some("bool".to_string()),
            phero_ir::PrimitiveType::Int
            | phero_ir::PrimitiveType::Long
            | phero_ir::PrimitiveType::Short
            | phero_ir::PrimitiveType::Byte => Some("i64".to_string()),
            phero_ir::PrimitiveType::Float | phero_ir::PrimitiveType::Double => {
                Some("f64".to_string())
            }
            phero_ir::PrimitiveType::Date => Some("chrono::NaiveDate".to_string()),
        },
        // Named types match by their name leaf against the datatypes the
        // generated code models (the pherolang `Uuid`/`DateTime`/`Decimal` family
        // and the `.mox` stdlib datatypes).
        phero_ir::TypeRef::Class { name, .. }
        | phero_ir::TypeRef::Enum { name, .. }
        | phero_ir::TypeRef::Datatype { name, .. }
        | phero_ir::TypeRef::Interface { name, .. }
        | phero_ir::TypeRef::Vocabulary { name, .. } => match name.as_str() {
            "Uuid" => Some("Uuid".to_string()),
            "DateTime" | "Instant" | "Timestamp" => {
                Some("chrono::DateTime<chrono::Utc>".to_string())
            }
            "Decimal" => Some("rust_decimal::Decimal".to_string()),
            "Date" => Some("NaiveDate".to_string()),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {

    /// Declared-finder tests never reach the builtin path — an empty
    /// feature universe keeps them focused on the signature lowering.
    fn features(_title: &str) -> EntityFeatures {
        EntityFeatures::default()
    }

    use super::*;
    use codegraph_core::types::{
        DddApplicationNode, DddDesignFlags, DddModelGraph, DddModuleNode, DddParam,
        DddRepositoryNode, DddRepositoryOperation, DddSearchField, DddSearchNode,
    };

    fn class_json(name: &str) -> serde_json::Value {
        serde_json::json!({"type": "class", "value": {"package": "nz.example.library", "name": name}})
    }

    fn string_json() -> serde_json::Value {
        serde_json::json!({"type": "primitive", "value": "string"})
    }

    fn enum_json(name: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "enum",
            "value": {"package": "nz.example.library", "name": name}
        })
    }

    fn many_json() -> serde_json::Value {
        serde_json::json!({"lower": 0, "upper": "unbounded"})
    }

    fn param(name: &str, type_json: serde_json::Value) -> DddParam {
        DddParam {
            name: name.to_string(),
            type_json,
            multiplicity: None,
        }
    }

    fn design(class: &str, title: Option<&str>, stereotype: &str) -> DddDesignNode {
        DddDesignNode {
            application: "Library".to_string(),
            module: "media".to_string(),
            class: class.to_string(),
            resolved_title: title.map(|t| t.to_string()),
            stereotype: stereotype.to_string(),
            is_abstract: false,
            flags: DddDesignFlags::default(),
            machine_json: None,
            child_operations: Vec::new(),
            aggregate_boundary: None,
            invariants: Vec::new(),
            ordinal: 0,
        }
    }

    fn repository(name: &str, class: &str, ops: Vec<DddRepositoryOperation>) -> DddRepositoryNode {
        DddRepositoryNode {
            application: "Library".to_string(),
            name: name.to_string(),
            design_class: class.to_string(),
            operations: ops,
        }
    }

    fn builtin(name: &str, builtin: &str, ordinal: usize) -> DddRepositoryOperation {
        DddRepositoryOperation {
            name: name.to_string(),
            builtin: Some(builtin.to_string()),
            return_type: None,
            return_multiplicity: None,
            params: Vec::new(),
            ordinal,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        }
    }

    fn model(
        designs: Vec<DddDesignNode>,
        repositories: Vec<DddRepositoryNode>,
        searches: Vec<DddSearchNode>,
    ) -> DddModelGraph {
        DddModelGraph {
            source_path: "library.ddd".to_string(),
            application: DddApplicationNode {
                name: "Library".to_string(),
                base: Some("nz.example.library".to_string()),
                source_path: "library.ddd".to_string(),
                errors: Vec::new(),
            },
            modules: vec![DddModuleNode {
                application: "Library".to_string(),
                name: "media".to_string(),
                ordinal: 0,
                errors: Vec::new(),
            }],
            designs,
            repositories,
            services: Vec::new(),
            searches,
            dto_shapes: Vec::new(),
        }
    }

    #[test]
    fn a_projection_dto_never_shadows_the_entity_design_of_its_source() {
        // DD3: `dto BookSummary from Book` lowers a `dto` design whose class
        // is Book — a BOUNDARY beside the entity design, with every flag off
        // and no repository. Indexing it over the entity design would empty
        // the operation set and drop `auditable` for the whole Book plane.
        let mut projection = design("Book", Some("BookType"), "dto");
        projection.ordinal = 1;
        let surface = DddDesignSurface::from_models(&[model(
            vec![
                design("Book", Some("BookType"), "entity"),
                projection,
                // ...and the mirror ordering (DTO first, entity second) must
                // resolve the same way.
                design("Movie", Some("MovieType"), "dto"),
                design("Movie", Some("MovieType"), "entity"),
            ],
            vec![repository(
                "BookRepository",
                "Book",
                vec![
                    builtin("findById", "findById", 0),
                    builtin("save", "save", 1),
                ],
            )],
            vec![],
        )]);
        for title in ["BookType", "MovieType"] {
            let design = surface.design_for(title).expect("the entity design");
            assert_eq!(
                design.stereotype, "entity",
                "the projection DTO must not shadow the entity design for {title}"
            );
        }
        assert_eq!(
            surface.operations_for("BookType"),
            Some(vec![
                "create".to_string(),
                "update".to_string(),
                "read".to_string()
            ]),
            "the entity design's repository must still map its operations"
        );
    }

    #[test]
    fn surface_indexes_designs_by_resolved_title_and_skips_unresolved() {
        let surface = DddDesignSurface::from_models(&[model(
            vec![
                design("Book", Some("BookType"), "entity"),
                design("Chapter", None, "entity"),
            ],
            vec![],
            vec![],
        )]);
        assert!(!surface.is_empty());
        assert!(surface.design_for("BookType").is_some());
        assert!(surface.design_for("Chapter").is_none());
    }

    #[test]
    fn surface_spans_two_applications_and_modules() {
        let mut second = design("Movie", Some("MovieType"), "entity");
        second.application = "Cinema".to_string();
        second.module = "film".to_string();
        let surface = DddDesignSurface::from_models(&[
            model(
                vec![design("Book", Some("BookType"), "entity")],
                vec![],
                vec![],
            ),
            model(vec![second], vec![], vec![]),
        ]);
        assert!(surface.design_for("BookType").is_some());
        assert!(surface.design_for("MovieType").is_some());
    }

    #[test]
    fn operations_map_from_repository_builtins() {
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository(
                "BookRepository",
                "Book",
                vec![
                    builtin("findById", "findById", 0),
                    builtin("findAll", "findAll", 1),
                    builtin("save", "save", 2),
                    builtin("delete", "delete", 3),
                ],
            )],
            vec![],
        )]);
        let ops = surface.operations_for("BookType").expect("design exists");
        assert_eq!(ops, vec!["create", "update", "read", "delete", "list"]);
    }

    #[test]
    fn design_without_repository_maps_to_empty_operations() {
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![],
            vec![],
        )]);
        let ops = surface.operations_for("BookType").expect("design exists");
        assert!(ops.is_empty());
        assert!(surface.operations_for("Unknown").is_none());
    }

    #[test]
    fn rust_name_keyed_callers_resolve_the_type_suffixed_design() {
        // Issue #663 compile-gate finding (the crewbase class): handlers,
        // routers, and openapi key entities by `rust_type_name` — the title
        // with the mox bridge's `Type` suffix stripped (`Timecard` for
        // `TimecardType`) — while the surface is keyed by TITLE. A miss
        // falls back to the config defaults and silently re-adds operations
        // the design's repository does not implement (its Timecard design
        // declares no `delete`, yet the emitted DELETE handler called a
        // command plane that gated it out — E0599).
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Timecard", Some("TimecardType"), "entity")],
            vec![repository(
                "TimecardRepository",
                "Timecard",
                vec![
                    builtin("findById", "findById", 0),
                    builtin("findAll", "findAll", 1),
                    builtin("save", "save", 2),
                ],
            )],
            vec![],
        )]);
        // The exact-title lookup still wins.
        assert_eq!(
            surface.operations_for("TimecardType"),
            Some(vec![
                "create".to_string(),
                "update".to_string(),
                "read".to_string(),
                "list".to_string()
            ])
        );
        // The rust name resolves the same design — NO `delete` (the
        // design's repository declares none).
        let ops = surface
            .operations_for_rust_name("Timecard")
            .expect("the rust-name alias resolves the design");
        assert!(
            !ops.contains(&"delete".to_string()),
            "the design's op set must gate the route plane: {ops:?}"
        );
        // A rust name no design claims stays `None` (defaults flow).
        assert!(surface.operations_for_rust_name("Shelf").is_none());
    }

    #[test]
    fn repository_binds_to_its_design_class_title() {
        let surface = DddDesignSurface::from_models(&[model(
            vec![
                design("Book", Some("BookType"), "entity"),
                design("Movie", Some("MovieType"), "entity"),
            ],
            vec![
                repository(
                    "BookRepository",
                    "Book",
                    vec![builtin("findById", "findById", 0)],
                ),
                repository(
                    "MovieRepository",
                    "Movie",
                    vec![builtin("findAll", "findAll", 0)],
                ),
            ],
            vec![],
        )]);
        assert_eq!(
            surface.operations_for("BookType"),
            Some(vec!["read".to_string()])
        );
        assert_eq!(
            surface.operations_for("MovieType"),
            Some(vec!["list".to_string()])
        );
    }

    #[test]
    fn finder_parsed_enum_param_binds_the_text_code() {
        // P2.5: `findByStatus(BookStatus status)` must resolve to the text
        // column, not be skipped as an unresolvable parameter type.
        let op = DddRepositoryOperation {
            name: "findByStatus".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: None,
            params: vec![param("status", enum_json("BookStatus"))],
            ordinal: 4,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository(
                "BookRepository",
                "Book",
                vec![builtin("findById", "findById", 0), op],
            )],
            vec![],
        )]);
        let finders = surface
            .finders_for("BookType", "Book", &features("Book"))
            .expect("finders lower");
        assert_eq!(finders.len(), 1);
        assert_eq!(finders[0].params[0].rust_type, "String");
    }

    #[test]
    fn finder_parsed_single_scalar_param() {
        let op = DddRepositoryOperation {
            name: "findByTitle".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: None,
            params: vec![param("title", string_json())],
            ordinal: 4,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository(
                "BookRepository",
                "Book",
                vec![builtin("findById", "findById", 0), op],
            )],
            vec![],
        )]);
        let finders = surface
            .finders_for("BookType", "Book", &features("Book"))
            .expect("finders lower");
        assert_eq!(finders.len(), 1);
        let finder = &finders[0];
        assert_eq!(finder.name, "findByTitle");
        assert_eq!(finder.method_name, "find_by_title");
        assert_eq!(finder.params.len(), 1);
        assert_eq!(finder.params[0].name, "title");
        assert_eq!(finder.params[0].rust_type, "String");
        assert!(!finder.returns_many);
        assert_eq!(finder.return_rust_type, "BookResponse");
        assert_eq!(finder.entity_name, "Book");
    }

    #[test]
    fn finder_multiplicity_drives_returns_many() {
        let op = DddRepositoryOperation {
            name: "findByStatus".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: Some(many_json()),
            params: vec![param("status", string_json())],
            ordinal: 4,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository("BookRepository", "Book", vec![op])],
            vec![],
        )]);
        let finders = surface
            .finders_for("BookType", "Book", &features("Book"))
            .expect("finders lower");
        assert!(finders[0].returns_many);
    }

    #[test]
    fn finder_multi_params_and_supported_primitives() {
        let op = DddRepositoryOperation {
            name: "findByNameAndYear".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: Some(many_json()),
            params: vec![
                param("authorName", string_json()),
                param(
                    "publishedYear",
                    serde_json::json!({"type": "primitive", "value": "int"}),
                ),
                param(
                    "published",
                    serde_json::json!({"type": "primitive", "value": "boolean"}),
                ),
            ],
            ordinal: 4,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository("BookRepository", "Book", vec![op])],
            vec![],
        )]);
        let finders = surface
            .finders_for("BookType", "Book", &features("Book"))
            .expect("finders lower");
        assert_eq!(finders[0].params.len(), 3);
        assert_eq!(finders[0].params[0].name, "author_name");
        assert_eq!(finders[0].params[0].rust_type, "String");
        assert_eq!(finders[0].params[1].name, "published_year");
        assert_eq!(finders[0].params[1].rust_type, "i64");
        assert_eq!(finders[0].params[2].name, "published");
        assert_eq!(finders[0].params[2].rust_type, "bool");
        assert!(finders[0].returns_many);
    }

    #[test]
    fn finder_skips_no_params_non_entity_return_and_unresolvable_param() {
        let no_params = DddRepositoryOperation {
            name: "findAllSorted".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: None,
            params: vec![],
            ordinal: 4,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let non_entity = DddRepositoryOperation {
            name: "countBooks".to_string(),
            builtin: None,
            return_type: Some(serde_json::json!({
                "type": "primitive", "value": "int"
            })),
            return_multiplicity: None,
            params: vec![param(
                "minYear",
                serde_json::json!({"type": "primitive", "value": "int"}),
            )],
            ordinal: 5,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let unresolvable = DddRepositoryOperation {
            name: "findByAuthor".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: None,
            params: vec![param("author", class_json("Author"))],
            ordinal: 6,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let wrong_entity = DddRepositoryOperation {
            name: "findMovieByTitle".to_string(),
            builtin: None,
            return_type: Some(class_json("Movie")),
            return_multiplicity: None,
            params: vec![param("title", string_json())],
            ordinal: 7,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository(
                "BookRepository",
                "Book",
                vec![no_params, non_entity, unresolvable, wrong_entity],
            )],
            vec![],
        )]);
        assert!(
            surface
                .finders_for("BookType", "Book", &features("Book"))
                .expect("finders lower")
                .is_empty()
        );
    }

    #[test]
    fn finder_with_a_dto_boundary_slot_names_the_boundary() {
        // DD3: a signature slot may be a DTO boundary (`{"dto":"Name"}`).
        // A finder binds and returns the designing entity, so a boundary is
        // refused by NAME — never reported as a generic parse failure.
        let dto = || serde_json::json!({ "dto": "BookSummary" });

        let dto_return = DddRepositoryOperation {
            name: "summaries".to_string(),
            builtin: None,
            return_type: Some(dto()),
            return_multiplicity: None,
            params: vec![param("title", string_json())],
            ordinal: 8,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let reason = lower_finder(&dto_return, "Book", "Book", &features("Book"))
            .expect_err("a DTO return type cannot lower to a finder");
        assert!(
            reason.contains("DTO boundary `BookSummary`"),
            "reason must name the boundary: {reason}"
        );

        let dto_param = DddRepositoryOperation {
            name: "findBySummary".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: None,
            params: vec![param("request", dto())],
            ordinal: 9,
            is_protected: false,
            query_json: None,
            capabilities: Vec::new(),
        };
        let reason = lower_finder(&dto_param, "Book", "Book", &features("Book"))
            .expect_err("a DTO parameter cannot lower to a finder");
        assert!(
            reason.contains("DTO boundary `BookSummary`"),
            "reason must name the boundary: {reason}"
        );
    }

    #[test]
    fn search_indexed_by_entity_title_with_boosts() {
        let search = DddSearchNode {
            application: "Library".to_string(),
            module: "media".to_string(),
            name: "BookSearch".to_string(),
            description: None,
            entity_class: "Book".to_string(),
            entity_title: Some("BookType".to_string()),
            text: vec![
                DddSearchField {
                    property: "title".to_string(),
                    boost: Some(3.0),
                    analyzer: None,
                },
                DddSearchField {
                    property: "blurb".to_string(),
                    boost: None,
                    analyzer: Some("simple".to_string()),
                },
            ],
            filters: vec!["status".to_string()],
            sorts: vec!["published_year".to_string()],
            document: Vec::new(),
            ranking: Some("bm25".to_string()),
            analyzer: Some("english".to_string()),
            pagination: None,
            capabilities: Vec::new(),
            ordinal: 0,
        };
        let surface = DddDesignSurface::from_models(&[model(vec![], vec![], vec![search])]);
        let s = surface.search_for("BookType").expect("search indexed");
        assert_eq!(s.text.len(), 2);
        assert_eq!(s.filters, vec!["status"]);
        assert_eq!(s.sorts, vec!["published_year"]);
        let weights = surface.search_boost_weights("BookType").expect("weights");
        // Higher boost first: title gets A.
        assert_eq!(weights[0], ("title".to_string(), 'A'));
        assert_eq!(weights[1], ("blurb".to_string(), 'B'));
    }

    #[test]
    fn stereotype_mismatch_detector() {
        let surface = DddDesignSurface::from_models(&[model(
            vec![
                design("Book", Some("BookType"), "value"),
                design("Movie", Some("MovieType"), "entity"),
            ],
            vec![],
            vec![],
        )]);
        // value design over an entity schema → mismatch
        surface.warn_stereotype_mismatch("BookType", true);
        // entity design over an entity schema → no mismatch
        surface.warn_stereotype_mismatch("MovieType", true);
        // no design → silent
        surface.warn_stereotype_mismatch("Unknown", false);
    }
}
