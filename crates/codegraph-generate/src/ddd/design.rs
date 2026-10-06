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
//! Mapping summary (Sculptor semantics, `rexlang/docs/DDD.md`):
//! - repository built-ins → CRUD operations: `findById→read`, `findAll→list`,
//!   `save→create`+`update`, `delete→delete`. A design without a repository
//!   maps to an EMPTY operation set (the entity gets no CRUD).
//! - declared repository operations → design finders (equality queries) when
//!   their signature resolves against the designing entity.
//! - search definitions → full-text surface (`has_fts`, filter fields,
//!   analyzer-driven FTS language).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{DddDesignNode, DddRepositoryNode, DddSearchNode};
use serde::Serialize;

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
    /// The mapped Rust parameter type.
    pub rust_type: String,
}

/// A lowered declared repository operation: an equality query over the
/// designing entity's columns.
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
}

/// Per-application index of the ingested `.ddd` design models, keyed by
/// resolved schema title.
#[derive(Debug, Default)]
pub struct DddDesignSurface {
    designs: HashMap<String, DddDesignNode>,
    repositories: HashMap<String, DddRepositoryNode>,
    searches: HashMap<String, DddSearchNode>,
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
                surface.designs.insert(title, design.clone());
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
        }
        surface
    }

    /// `true` when the graph carries no usable designs — every consumer
    /// gates on this to keep flag-off output byte-identical.
    pub fn is_empty(&self) -> bool {
        self.designs.is_empty()
    }

    /// The design decision for the entity with this schema title.
    pub fn design_for(&self, schema_title: &str) -> Option<&DddDesignNode> {
        self.designs.get(schema_title)
    }

    /// The repository designed for the entity with this schema title.
    pub fn repository_for(&self, schema_title: &str) -> Option<&DddRepositoryNode> {
        self.repositories.get(schema_title)
    }

    /// The search definition over the entity with this schema title.
    pub fn search_for(&self, schema_title: &str) -> Option<&DddSearchNode> {
        self.searches.get(schema_title)
    }

    /// The design-mapped operation set for the entity, or `None` when no
    /// design exists (the caller keeps its schema-derived operations).
    ///
    /// A design WITH a repository maps its built-ins
    /// (`findById→read`, `findAll→list`, `save→create`+`update`,
    /// `delete→delete`); a design WITHOUT a repository maps to an empty
    /// set. Declared (non-built-in) operations are finder candidates, not
    /// CRUD operations.
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
                .any(|op| op.builtin.as_deref() == Some(builtin))
        };
        // Canonical order matches the codegraph default operation list
        // (create/read/update/delete/list) so templates stay stable.
        let mut ops: Vec<String> = Vec::with_capacity(5);
        if has("save") {
            ops.push("create".to_string());
            ops.push("update".to_string());
        }
        if has("findById") {
            ops.push("read".to_string());
        }
        if has("delete") {
            ops.push("delete".to_string());
        }
        if has("findAll") {
            ops.push("list".to_string());
        }
        Some(ops)
    }

    /// Lower the repository's declared operations into design finders.
    /// `entity_name` is the entity's Rust type name; finders are returned
    /// with it patched into `entity_name`/`return_rust_type`.
    ///
    /// Skipped finders (and the reason) print a one-time stderr warning:
    /// no params, non-entity return, unresolvable param type, or array
    /// parameters.
    pub fn finders_for(&self, schema_title: &str, entity_name: &str) -> Vec<DesignFinder> {
        let Some(repo) = self.repository_for(schema_title) else {
            return Vec::new();
        };
        let Some(design) = self.design_for(schema_title) else {
            return Vec::new();
        };
        let design_class_leaf = leaf_name(&design.class);
        let mut finders = Vec::new();
        for op in &repo.operations {
            if op.builtin.is_some() {
                continue;
            }
            let finder = lower_finder(op, design_class_leaf, entity_name);
            match finder {
                Ok(finder) => finders.push(finder),
                Err(reason) => warn_once(
                    &format!("finder:{}::{}", repo.name, op.name),
                    &format!("finder `{}.{}` skipped: {reason}", repo.name, op.name),
                ),
            }
        }
        finders
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
        if flags.optimistic_locking {
            warn_once(
                &format!("flag:optimistic_locking:{app}::{}", design.class),
                &format!(
                    "design `{}`: optimisticLocking not yet mapped — recorded only",
                    design.class
                ),
            );
        }
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

/// Lower one declared repository operation into a [`DesignFinder`].
fn lower_finder(
    op: &codegraph_core::types::DddRepositoryOperation,
    design_class_leaf: &str,
    entity_name: &str,
) -> std::result::Result<DesignFinder, String> {
    // The return type must resolve to the designing entity.
    let Some(return_json) = &op.return_type else {
        return Err("no return type declared".to_string());
    };
    let return_type: rex_ir::TypeRef = serde_json::from_value(return_json.clone())
        .map_err(|e| format!("unresolvable return type ({e})"))?;
    let class_name = match &return_type {
        rex_ir::TypeRef::Class { name, .. } => name.clone(),
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
        .map(|m| serde_json::from_value::<rex_ir::Multiplicity>(m.clone()).map(|m| m.is_many()))
        .transpose()
        .map_err(|e| format!("unresolvable return multiplicity ({e})"))?
        .unwrap_or(false);

    if op.params.is_empty() {
        return Err("no parameters declared".to_string());
    }

    let mut params = Vec::with_capacity(op.params.len());
    for param in &op.params {
        if param.multiplicity.is_some() {
            return Err(format!(
                "parameter `{}` is an array — array parameters are not mapped",
                param.name
            ));
        }
        let type_ref: rex_ir::TypeRef = serde_json::from_value(param.type_json.clone())
            .map_err(|e| format!("unresolvable parameter type for `{}` ({e})", param.name))?;
        let Some(rust_type) = rex_type_to_rust(&type_ref) else {
            return Err(format!(
                "unresolvable parameter type `{}` for `{}`",
                type_ref
                    .qualified_name()
                    .unwrap_or_else(|| "primitive".to_string()),
                param.name
            ));
        };
        params.push(FinderParam {
            name: codegraph_naming::to_snake_case(&param.name),
            rust_type,
        });
    }

    Ok(DesignFinder {
        method_name: codegraph_naming::to_snake_case(&op.name),
        name: op.name.clone(),
        entity_name: entity_name.to_string(),
        params,
        returns_many,
        return_rust_type: format!("{entity_name}Response"),
    })
}

/// Map a rex-ir type reference to the Rust type the generated repositories
/// use for it. Returns `None` for anything the equality-finder lowering
/// cannot express (struct/class/enum/vocabulary parameters).
pub fn rex_type_to_rust(type_ref: &rex_ir::TypeRef) -> Option<String> {
    match type_ref {
        rex_ir::TypeRef::Primitive(prim) => match prim {
            rex_ir::PrimitiveType::String | rex_ir::PrimitiveType::Char => {
                Some("String".to_string())
            }
            rex_ir::PrimitiveType::Boolean => Some("bool".to_string()),
            rex_ir::PrimitiveType::Int
            | rex_ir::PrimitiveType::Long
            | rex_ir::PrimitiveType::Short
            | rex_ir::PrimitiveType::Byte => Some("i64".to_string()),
            rex_ir::PrimitiveType::Float | rex_ir::PrimitiveType::Double => Some("f64".to_string()),
            rex_ir::PrimitiveType::Date => Some("NaiveDate".to_string()),
        },
        // Named types match by their name leaf against the datatypes the
        // generated code models (the rex `Uuid`/`DateTime`/`Decimal` family
        // and the `.mox` stdlib datatypes).
        rex_ir::TypeRef::Class { name, .. }
        | rex_ir::TypeRef::Enum { name, .. }
        | rex_ir::TypeRef::Datatype { name, .. }
        | rex_ir::TypeRef::Interface { name, .. }
        | rex_ir::TypeRef::Vocabulary { name, .. } => match name.as_str() {
            "Uuid" => Some("Uuid".to_string()),
            "DateTime" | "Instant" | "Timestamp" => Some("DateTime<Utc>".to_string()),
            "Decimal" => Some("Decimal".to_string()),
            "Date" => Some("NaiveDate".to_string()),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
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
            },
            modules: vec![DddModuleNode {
                application: "Library".to_string(),
                name: "media".to_string(),
                ordinal: 0,
            }],
            designs,
            repositories,
            services: Vec::new(),
            searches,
        }
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
    fn finder_parsed_single_scalar_param() {
        let op = DddRepositoryOperation {
            name: "findByTitle".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: None,
            params: vec![param("title", string_json())],
            ordinal: 4,
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
        let finders = surface.finders_for("BookType", "Book");
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
        };
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository("BookRepository", "Book", vec![op])],
            vec![],
        )]);
        let finders = surface.finders_for("BookType", "Book");
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
        };
        let surface = DddDesignSurface::from_models(&[model(
            vec![design("Book", Some("BookType"), "entity")],
            vec![repository("BookRepository", "Book", vec![op])],
            vec![],
        )]);
        let finders = surface.finders_for("BookType", "Book");
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
        };
        let unresolvable = DddRepositoryOperation {
            name: "findByAuthor".to_string(),
            builtin: None,
            return_type: Some(class_json("Book")),
            return_multiplicity: None,
            params: vec![param("author", class_json("Author"))],
            ordinal: 6,
        };
        let wrong_entity = DddRepositoryOperation {
            name: "findMovieByTitle".to_string(),
            builtin: None,
            return_type: Some(class_json("Movie")),
            return_multiplicity: None,
            params: vec![param("title", string_json())],
            ordinal: 7,
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
        assert!(surface.finders_for("BookType", "Book").is_empty());
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
