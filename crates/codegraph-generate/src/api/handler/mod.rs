use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ParentCandidate, PolicyKind};
use serde::Serialize;

use crate::ProjectConfig;
use crate::error::Result;
use crate::filter_fields::{
    FilterFieldInfo, NestedFilterFieldInfo, resolve_filter_fields, resolve_nested_filter_fields,
};
use crate::render_template_with_project;
use crate::traits::{EntityGenerator, EntityGeneratorKind, GeneratedFile};
use crate::type_registry::TypeRegistryHandle;
use codegraph_config::{DomainConfig, EntityConfig};

use super::include_path::ResolvedIncludePath;
use super::include_plan::{IncludePlanCtx, build_include_plan};
use super::router::{ChildInfo, CrossRefInfo};

use super::api_model::{resolve_entity_operations, resolve_path_segment};

mod guards;
mod imports;
mod invariants;

use guards::build_guard_blocks;
use imports::build_handler_imports;
use invariants::{
    build_invariant_blocks, build_transition_invariant_block, collect_invariant_members,
};

/// A mox operation routed as an action endpoint (issue #508 R2):
/// `POST {id}/op/{snake}`. Only qualifying ops route — bodied AND with
/// all-primitive signature slots; body-less ops never route.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MoxOpRoute {
    /// Operation name as written in mox.
    pub name: String,
    /// Rust method name (and route segment).
    pub snake: String,
    /// Parameters: (snake name, Rust type).
    pub params: Vec<MoxOpParam>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MoxOpParam {
    pub name: String,
    pub rust_type: String,
}

/// Qualifying mox ops for `schema_title` (issue #508 R2): a body
/// (`rust` or `expr`) and only primitive signature slots.
pub async fn routable_mox_ops(
    db: &dyn GraphQuerier,
    schema_title: &str,
    entity_name: &str,
) -> Vec<MoxOpRoute> {
    let all = db.get_mox_operations().await.unwrap_or_default();
    all.iter()
        .filter(|op| op.class == schema_title || op.class == entity_name)
        .filter(|op| op.bodies.contains_key("rust") || op.bodies.contains_key("expr"))
        .filter_map(|op| {
            let primitive = |t: &str| crate::mox_expr::mox_primitive_rust_type(t).is_some();
            if !primitive(&op.return_type) || !op.params.iter().all(|p| primitive(&p.type_)) {
                return None;
            }
            Some(MoxOpRoute {
                name: op.name.clone(),
                snake: codegraph_naming::to_snake_case(&op.name),
                params: op
                    .params
                    .iter()
                    .map(|p| MoxOpParam {
                        name: codegraph_naming::to_snake_case(&p.name),
                        rust_type: crate::mox_expr::mox_primitive_rust_type(&p.type_)
                            .unwrap_or("String")
                            .to_string(),
                    })
                    .collect(),
            })
        })
        .collect()
}

/// Per-operation guard presence (issue #338): whether the handler carries an
/// eligibility guard for each write/read operation. Derived from
/// [`crate::ddd::rules::EligibilityGuard`]s once, in Rust, so the handler
/// template's utoipa 403 documentation needs no guard-loop programming.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuardPresence {
    pub create: bool,
    pub read: bool,
    pub update: bool,
    pub delete: bool,
}

impl GuardPresence {
    /// True for an operation iff at least one guard gates it.
    pub fn from_guards(guards: &[crate::ddd::rules::EligibilityGuard]) -> Self {
        let gates = |op: &str| {
            guards
                .iter()
                .any(|guard| guard.operations.iter().any(|gated| gated == op))
        };
        Self {
            create: gates("create"),
            read: gates("read"),
            update: gates("update"),
            delete: gates("delete"),
        }
    }
}

/// The `{Entity}IncludedData` default fields for the `?include=` initialiser:
/// the unique `segments[0].module_name` values in FIRST-OCCURRENCE order.
/// Sibling dot-notation paths share a first segment, so the field is
/// initialised exactly once; an empty input yields an empty list (no include
/// surface). A defensively empty `segments` is skipped.
pub fn included_default_fields(include_paths: &[ResolvedIncludePath]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    include_paths
        .iter()
        .filter_map(|path| path.segments.first())
        .map(|segment| segment.module_name.clone())
        .filter(|field| seen.insert(field.clone()))
        .collect()
}

/// Pre-rendered `axum::extract::Path` parameter bindings (issue #778 wave 1 /
/// T1.3).
///
/// Each string is the EXACT rendered bytes of the role/depth conditional block
/// it replaces — INCLUDING the blank lines those control-tag lines emitted —
/// so splicing it onto its own template line keeps the surrounding parameter
/// list byte-identical. A shape with no binding at all (a root collection
/// endpoint: `create`/`list`/`search`) is the empty string, so the template
/// splices nothing and the site keeps its single blank line.
///
/// The whitespace convention is therefore: non-root shapes render one blank
/// line per removed control-tag line before the binding (two, for a shape
/// nested inside an outer `role` + inner `depth` pair) plus the binding line
/// itself; an item shape binds at root too (one leading blank line), while a
/// collection shape never does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathExtractors {
    /// Item routes binding the record `id`: `get_by_id`, `update`, `delete`.
    pub item: String,
    /// Item routes binding the entity's own path parameter (the service-op
    /// handlers, `{Entity}` path params substituted).
    pub item_named: String,
    /// Collection routes binding the parent id: `create`, `list`.
    pub collection: String,
    /// Collection routes that ignore the parent id: `search`.
    pub collection_unused: String,
    /// Item routes whose parent id is unused: `tree`.
    pub item_unused_parent: String,
    /// The child-op `add` arm's collection extractor. It sits one nesting
    /// level deeper than [`Self::collection`] (inside the operation's
    /// payload/kind branch), whose own control tags already emit the leading
    /// blank line, so this segment carries exactly one fewer.
    pub child_op_add: String,
}

/// Build the handler's pre-rendered `Path(…)` bindings for `role` (the config
/// topology role), `depth` (`ancestor_path_params`: 0 root, 1 child,
/// 2+ grandchild) and the entity's `param_name`.
pub fn path_extractors(role: &str, depth: usize, param_name: &str) -> PathExtractors {
    let child = role == "child";
    let grandchild = child && depth >= 2;

    // One binding line, 4-space indent + trailing newline.
    let line = |names: &str, ty: &str| format!("    Path({names}): Path<{ty}>,\n");

    let item_line = if grandchild {
        line("(_grandparent_id, parent_id, id)", "(Uuid, Uuid, Uuid)")
    } else if child {
        line("(parent_id, id)", "(Uuid, Uuid)")
    } else {
        line("id", "Uuid")
    };
    let named_line = if grandchild {
        line(
            &format!("(_grandparent_id, parent_id, {param_name})"),
            "(Uuid, Uuid, Uuid)",
        )
    } else if child {
        line(&format!("(parent_id, {param_name})"), "(Uuid, Uuid)")
    } else {
        line(param_name, "Uuid")
    };
    let collection_line = if grandchild {
        line("(_grandparent_id, parent_id)", "(Uuid, Uuid)")
    } else {
        line("parent_id", "Uuid")
    };
    let collection_unused_line = if grandchild {
        line("(_grandparent_id, _parent_id)", "(Uuid, Uuid)")
    } else {
        line("_parent_id", "Uuid")
    };
    let item_unused_parent_line = if grandchild {
        line("(_grandparent_id, _parent_id, id)", "(Uuid, Uuid, Uuid)")
    } else if child {
        line("(_parent_id, id)", "(Uuid, Uuid)")
    } else {
        line("id", "Uuid")
    };

    // A shape nested inside the outer `role` if AND the inner `depth` if
    // renders two leading blank lines (one per removed tag line) and the
    // inner `{% endif %}` line's trailing blank; a root item shape renders
    // one leading blank line only.
    let nested = |binding: String| format!("\n\n{binding}\n");
    let item_root = |binding: String| format!("\n{binding}");

    PathExtractors {
        item: if child {
            nested(item_line)
        } else {
            item_root(item_line)
        },
        item_named: if child {
            nested(named_line)
        } else {
            item_root(named_line)
        },
        // Root collection endpoints have no parent binding at all.
        collection: if child {
            nested(collection_line.clone())
        } else {
            String::new()
        },
        collection_unused: if child {
            nested(collection_unused_line)
        } else {
            String::new()
        },
        item_unused_parent: if child {
            nested(item_unused_parent_line)
        } else {
            item_root(item_unused_parent_line)
        },
        // The child-op `add` arm's own control tags supply the outer blank
        // line; only the inner tag line's blank plus the binding remain.
        child_op_add: if child {
            item_root(collection_line)
        } else {
            String::new()
        },
    }
}

#[derive(Debug, Serialize)]
pub struct HandlerContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub tag: String,
    pub operations: Vec<String>,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub parent_ref: Option<String>,
    pub parent_entity: Option<String>,
    pub parent_path_segment: Option<String>,
    pub parent_module_name: Option<String>,
    /// tera 2's `default` filter only fires on undefined (not null), so None
    /// must OMIT the key for `parent_domain | default(value=domain)` routes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_domain: Option<String>,
    pub role: String,
    /// Named path parameter for this entity's ID (e.g. `"worker_id"`).
    pub param_name: String,
    /// Named path parameter for the parent's ID (e.g. `"worker_id"` when this entity is a child of Worker).
    pub parent_param_name: Option<String>,
    /// Named path parameter for the grandparent's ID (depth-2 children only).
    pub grandparent_param_name: Option<String>,
    /// Path segment for the grandparent entity (depth-2 children only, e.g. `"workers"`).
    pub grandparent_path_segment: Option<String>,
    /// Pre-rendered `axum::extract::Path` parameter bindings (issue #778 wave
    /// 1), derived once from [`Self::role`],
    /// [`Self::ancestor_path_params`] and [`Self::param_name`] by
    /// [`path_extractors`]. Each string is whitespace-exact — see
    /// [`PathExtractors`] for the convention — and EMPTY where the shape has
    /// no binding (root collection endpoints).
    pub path_extractor_item: String,
    pub path_extractor_item_named: String,
    pub path_extractor_collection: String,
    pub path_extractor_collection_unused: String,
    pub path_extractor_item_unused_parent: String,
    pub path_extractor_child_op_add: String,
    /// Domain of the grandparent entity (depth-2 children only).
    /// Skipped when None: tera 2's `default` only fires on undefined keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grandparent_domain: Option<String>,
    pub children: Vec<ChildInfo>,
    pub cross_refs: Vec<CrossRefInfo>,
    /// When set, the list endpoint supports ?status= filtering on this column.
    pub status_field: Option<String>,
    /// Whether this entity has workflow support.
    pub has_workflow: bool,
    /// Whether this entity has full-text search enabled.
    pub has_fts: bool,
    /// REST surface for full-text search: "query_param", "dedicated", or "both".
    pub fts_rest_mode: String,
    /// Whether this entity has semantic search (pgvector embeddings) enabled.
    pub has_embeddings: bool,
    /// Fields exposed as JSON:API `?filter[field]=value` query params.
    pub filter_fields: Vec<FilterFieldInfo>,
    /// Nested (child/grandchild) fields exposed as `?filter[child.column]=value`.
    pub nested_filter_fields: Vec<NestedFilterFieldInfo>,
    /// Maximum number of items allowed in a bulk create request.
    pub max_bulk_size: usize,
    /// Number of ancestor path parameters captured before this handler's own routes.
    /// 0 for root entities, 1 for children, 2 for grandchildren, etc.
    pub ancestor_path_params: usize,
    /// When set, this entity supports the /tree endpoint (self-referencing hierarchy).
    pub hierarchy_field: Option<String>,
    /// When true, the find_tree method returns Vec<serde_json::Value> instead of Vec<Response>.
    #[serde(default)]
    pub tree_include: bool,
    /// When true, the get_by_id handler returns typed WithIncludeResponse instead of serde_json::Value.
    pub has_include: bool,
    /// Resolved include paths for `?include=` query parameter.
    pub include_paths: Vec<ResolvedIncludePath>,
    /// `{Entity}IncludedData`'s default-field list: the unique
    /// `segments[0].module_name` values of [`Self::include_paths`] in
    /// first-occurrence order (sibling dot-paths sharing a first segment
    /// initialise the field once). Empty = no include surface, so the key is
    /// omitted and the handler stays byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_default_fields: Vec<String>,
    /// Pre-rendered `?include=` hydration block for GET-by-id (issue #778
    /// wave 2): the provider-selected Rust lines the template renders inside
    /// the `has_include` block, produced by
    /// [`crate::api::include_plan::build_include_plan`]. Empty when the entity
    /// has no include surface, so the key is omitted and the handler stays
    /// byte-identical.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub include_get_block: String,
    /// Pre-rendered `?include=` hydration block for LIST (issue #778 wave 2),
    /// including the `let included = …` binding. Empty = no include surface.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub include_list_block: String,
    /// Pre-rendered `?include=` doc-comment LINE (indent + `///` included) for
    /// the shared query-params struct (issue #781). SeaORM keeps the
    /// historical "eager-load …" text; an unsupported persistence provider
    /// states that includes are modelled but answer 501. Empty = no include
    /// surface, so the key is omitted and the handler stays byte-identical.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub include_doc: String,
    /// Resolved `use` import statements for types referenced by this handler.
    /// Populated through this run's type registry instead of hard-coded
    /// template paths.
    pub handler_imports: Vec<String>,
    /// When true, the entity has a soft-delete audit policy and the query layer
    /// expects an `include_deleted: bool` argument on all read methods.
    pub is_auditable: bool,
    /// Sortable-field allow-list for `?sort=` on the list endpoint
    /// (issue #306), derived from the ux plan. Empty = the ux sort plane
    /// is inactive: no sort params are emitted and the handler stays
    /// byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ux_sort_fields: Vec<String>,
    /// Mox operations routed as action endpoints (issue #508 R2). Empty =
    /// no op routes and the handler stays byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mox_ops: Vec<MoxOpRoute>,
    /// Service operations routed as action endpoints (issue #590): POST
    /// `{id}/actions/{snake}` with the actor policy enforced server-side.
    /// Child entities nest under the parent route (WS4). Empty = no op
    /// routes, byte-identical handler.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub service_ops: Vec<crate::ddd::service_ops::ServiceOpRoute>,
    /// Handler-site eligibility guards (issue #338): the compiled rule
    /// booleans evaluated before the gated operations. Empty = no guard
    /// surface and the handler stays byte-identical. Populated only when
    /// the plan carries the `rules` generator (the emitted `rules.rs`
    /// module must exist for the call sites to resolve) and the topology
    /// is monolith (the rules module is not worker-routed).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub eligibility_guards: Vec<crate::ddd::rules::EligibilityGuard>,
    /// Guard presence flags (issue #338): true iff ANY entry of
    /// [`Self::eligibility_guards`] gates `create` / `read` / `update` /
    /// `delete`. These are the presence flags the template's utoipa 403
    /// documentation reads — the derivation lives here so the template does
    /// no guard-loop programming of its own.
    pub create_guarded: bool,
    pub read_guarded: bool,
    pub update_guarded: bool,
    pub delete_guarded: bool,
    /// Pre-rendered guard code blocks (issue #338), whitespace-exact so
    /// the template renders them inline; empty strings when inactive.
    /// `*_ownership_prefix` binds the existing ownership load (`let
    /// existing = `) when guards need the loaded record; `*_guard_block`
    /// carries the load-or-404 (root) and/or the guard call lines.
    #[serde(default)]
    pub update_ownership_prefix: String,
    #[serde(default)]
    pub update_guard_block: String,
    #[serde(default)]
    pub delete_ownership_prefix: String,
    #[serde(default)]
    pub delete_guard_block: String,
    #[serde(default)]
    pub read_guard_block: String,
    #[serde(default)]
    /// Media deletion cleanup (issue #631 R7): the pre-rendered post-delete
    /// key-recompute + storage.delete block. Empty unless the domain
    /// declares `[media]` and the topology is monolith.
    pub media_cleanup_block: String,
    #[serde(default)]
    pub create_guard_block: String,
    /// Pre-rendered design-invariant blocks (DD5, issue #666 slice 4d),
    /// whitespace-exact; empty strings when the entity carries no
    /// enforceable invariants (no plans and no invariant-carrying members),
    /// the plan lacks the `rules` generator, or the topology is workers
    /// (the rules plane is monolith-only — the gap is WARNed, never
    /// silent).
    #[serde(default)]
    pub create_invariant_block: String,
    #[serde(default)]
    pub update_invariant_block: String,
}

pub struct HandlerGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
    /// This run's type registry (issue #746). Defaults to the ambient
    /// registry for direct construction; the pipeline injects the run's own
    /// handle via [`Self::with_type_registry`].
    type_registry: TypeRegistryHandle,
    /// Whether the plan carries the `rules` domain generator (registry
    /// wiring): handler-site eligibility guards (issue #338) resolve only
    /// when the emitted `rules.rs` module exists to answer the calls.
    rules_guards: bool,
    boundaries: crate::ddd::dto_boundaries::BoundaryIndexCache,
}

impl HandlerGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            boundaries: crate::ddd::dto_boundaries::BoundaryIndexCache::new(),
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
            rules_guards: false,
            type_registry: TypeRegistryHandle::ambient(),
        }
    }

    /// Use `registry` for this generator's import resolution.
    pub fn with_type_registry(mut self, registry: TypeRegistryHandle) -> Self {
        self.type_registry = registry;
        self
    }

    pub fn with_parent_candidates(mut self, candidates: Vec<ParentCandidate>) -> Self {
        self.parent_candidates = candidates;
        self
    }

    /// Enable handler-site eligibility guards (issue #338). The registry
    /// derives this from the build plan's `rules` generator membership —
    /// guards call `crate::domain::{domain}::rules::…`, which only exists
    /// when the `RulesGenerator` runs.
    pub fn with_rules_guards(mut self, enabled: bool) -> Self {
        self.rules_guards = enabled;
        self
    }
}

#[async_trait]
impl EntityGenerator for HandlerGenerator {
    fn kind(&self) -> EntityGeneratorKind {
        EntityGeneratorKind::Handler
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = db
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let path_segment = resolve_path_segment(entity_cfg, &schema);

        let operations = resolve_entity_operations(db, config, &domain, &entity_name).await;

        let tag = entity_cfg
            .and_then(|ec| ec.tag.clone())
            .unwrap_or_else(|| entity_name.clone());

        let parent_entity = entity_cfg.and_then(|ec| ec.parent.clone());

        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let status_field = workflow.map(|wf| wf.status_field.clone());
        let has_workflow = workflow
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);

        let search = entity_cfg.map(|ec| &ec.search);
        let has_fts = search
            .and_then(|s| s.fts_columns.as_ref())
            .map(|cols| !cols.is_empty())
            .unwrap_or(false);
        let has_embeddings = search
            .map(|s| !s.embedding_columns.is_empty())
            .unwrap_or(false);
        let fts_rest_mode = entity_cfg
            .map(|ec| ec.search.fts_rest_mode.clone())
            .unwrap_or_else(|| "query_param".to_string());

        let filter_fields = resolve_filter_fields(
            db,
            schema_title,
            entity_cfg
                .and_then(|ec| ec.filter_fields.as_ref())
                .map(|v| v.as_slice()),
        )
        .await?;

        let max_bulk_size = entity_cfg
            .and_then(|ec| ec.max_bulk_size)
            .unwrap_or(config.defaults.max_bulk_size);

        // Resolve parent/child relationships from parent_candidates.
        let stripped_title =
            super::router::strip_suffix(schema_title, &config.defaults.type_suffix);
        let topo = resolve_handler_topology(
            db,
            config,
            &domain,
            &entity_name,
            schema_title,
            stripped_title,
            entity_cfg,
            &self.parent_candidates,
        )
        .await?;

        // Compute ancestor_path_params: how many {id} path segments Axum
        // accumulates before reaching this handler.  We walk up the resolved
        // parent chain, checking each ancestor's role in the domain it belongs to.
        let ancestor_path_params = compute_ancestor_path_params(
            config,
            &domain,
            stripped_title,
            entity_cfg,
            &self.parent_candidates,
            topo.parent_domain.as_deref(),
            &topo.role,
        );

        let param_name = super::router::param_name_from_path_segment(&path_segment);
        let parent_param_name = topo
            .parent_path_segment
            .as_deref()
            .map(super::router::param_name_from_path_segment);
        // For depth-2, resolve grandparent's param name, path segment, and domain.
        let (grandparent_param_name, grandparent_path_segment, grandparent_domain) =
            resolve_grandparent(
                db,
                config,
                &domain,
                entity_cfg,
                &self.parent_candidates,
                stripped_title,
                ancestor_path_params,
            )
            .await;

        // Resolve include paths from config. Skip non-root entities unless they
        // have explicit allow_include configuration.
        let include_paths =
            resolve_include_paths(db, config, &domain, schema_title, entity_cfg, project).await?;

        let nested_filter_fields =
            resolve_nested_filter_fields(db, schema_title, &module_name, &domain, config).await?;

        // Resolve handler imports via TypeRegistry instead of hard-coded template paths.
        let has_include = !include_paths.is_empty();
        let handler_imports = build_handler_imports(
            &self.type_registry,
            &operations,
            &entity_name,
            has_include,
            &include_paths,
            &domain,
            &module_name,
        );

        // Soft-delete audit policy: when the entity tracks deleted rows, the
        // query layer exposes an `include_deleted` argument on read methods.
        // Handlers must thread it through (passing `false` — APIs exclude
        // soft-deleted rows by default). Mirrors the logic in the query
        // generator (ddd/query.rs) so handler/query signatures stay in sync.
        let is_auditable = resolve_is_auditable(db, schema_title, config, &domain).await?;

        // ux sort plane (issue #306): the `?sort=` allow-list, resolved from
        // the SAME ux plan the list page renders. Empty when the flag is off
        // or no column is sortable — the handler then emits nothing.
        let ux_sort_fields = if operations.iter().any(|op| op == "list") {
            crate::ux::sort::resolve_ux_sort_plan(db, config, project, schema_title, &domain)
                .await?
                .fields
        } else {
            Vec::new()
        };

        // Mox op action routes (issue #508 R2).
        let mox_ops = routable_mox_ops(db, schema_title, &entity_name).await;

        // Handler-site eligibility guards (issue #338). Gated on the plan
        // carrying `rules` (the call targets must exist) and monolith
        // topology (rules.rs is not worker-routed, so a worker-crate
        // handler could not resolve the module). Empty otherwise —
        // byte-identical handler.
        let eligibility_guards = if self.rules_guards && !project.is_workers_topology() {
            crate::ddd::rules::eligibility_guards_for(db, config, &domain, schema_title).await?
        } else {
            Vec::new()
        };
        let guard_blocks = build_guard_blocks(
            &topo.role,
            &domain,
            &module_name,
            &entity_name,
            is_auditable,
            &project.identity.domain_types_crate,
            &eligibility_guards,
        );

        // Design-invariant enforcement (DD5, issue #666 slice 4d). The
        // SAME gating the eligibility guards use: the predicates live in
        // the `rules` generator's module (plan membership) and the rules
        // plane is monolith-only. Empty otherwise — byte-identical
        // handler. The blocks consume slice 2's lowering (`enforcement_for`)
        // — never re-derived.
        let invariant_enforcement = if self.rules_guards && !project.is_workers_topology() {
            crate::ddd::invariants::enforcement_for(db, config, &domain, schema_title).await
        } else {
            crate::ddd::invariants::InvariantEnforcement {
                class: entity_name.clone(),
                plans: Vec::new(),
            }
        };
        let invariant_members = if self.rules_guards && !project.is_workers_topology() {
            collect_invariant_members(
                db,
                config,
                &domain,
                schema_title,
                &self.parent_candidates,
                project,
            )
            .await
        } else {
            Vec::new()
        };
        let update_guards_bind_existing = eligibility_guards
            .iter()
            .any(|g| g.operations.iter().any(|op| op == "update"));
        let invariant_blocks = build_invariant_blocks(
            &topo.role,
            &domain,
            &module_name,
            &entity_name,
            is_auditable,
            operations.contains(&"delete".to_string()),
            &project.identity.domain_types_crate,
            &invariant_enforcement,
            &invariant_members,
            update_guards_bind_existing,
        );

        // Service-op action routes (issue #590) — with the transition-arm
        // invariant blocks attached (slice 4d): every executable
        // `save`/`update` op applies its patch to the candidate pre-write.
        // Child entities emit them too — the handler template nests the
        // utoipa path under the parent and loads the record through the
        // parent-scoped read (WS4). Lowering refusals (un-lowerable `when`
        // conditions) are hard generation errors naming the capability.
        let mut service_ops = crate::ddd::service_ops::routes_for_entity(
            db,
            config,
            &domain,
            schema_title,
            &crate::ddd::service_ops::BodyRenderContext::from_project(project)
                .with_boundary_cache(self.boundaries.clone()),
        )
        .await
        .map_err(crate::error::Error::Config)?;
        // Issue #767: a create-shaped route needs the entity's create DTO and
        // create command, which the template emits under `has_create`. The
        // route plane gates on the DESIGN surface's op set; the handler gate
        // uses the resolved op set. They agree in every reachable shape — this
        // fails LOUDLY if a future resolver change makes them disagree, rather
        // than emitting a handler that references an absent type.
        if service_ops.iter().any(|route| {
            matches!(
                route.execution,
                crate::ddd::service_ops::ExecutionKind::Create { .. }
            )
        }) && !operations.iter().any(|op| op == "create")
        {
            return Err(crate::error::Error::Config(format!(
                "ddd — entity `{entity_name}` emits a collection-level create-shaped service op, \
                 but its resolved operation set exposes no `create`; the generated handler would \
                 reference a Create DTO that was never emitted (check the design's repository \
                 builtins)"
            )));
        }
        // The template's 409 line is gated on the PRE-RENDERED block (which a
        // members-only root also carries) — the dedup must follow the SAME
        // predicate, not the entity's own plans.
        let documents_invariant_409 = !invariant_blocks.create_block.is_empty();
        // The two arms run INDEPENDENTLY: a root with no own invariants whose
        // aggregate MEMBERS carry them still renders the create block (and its
        // 409), and a members-only root has no transition block to attach.
        {
            for route in service_ops.iter_mut() {
                // Issue #767: a create-shaped action splices the same create
                // invariant block, and the template documents the 409 beside
                // the 201 — so a declared `throws Conflict` must not
                // double-document it (the Update arm's rule, one plane over).
                if documents_invariant_409
                    && matches!(
                        route.execution,
                        crate::ddd::service_ops::ExecutionKind::Create { .. }
                    )
                {
                    route.raise_responses.retain(|raise| raise.status != 409);
                }
                if !invariant_enforcement.is_empty()
                    && matches!(
                        route.execution,
                        crate::ddd::service_ops::ExecutionKind::Update { .. }
                    )
                    && route.record_load
                {
                    route.invariant_block = Some(build_transition_invariant_block(
                        &domain,
                        &entity_name,
                        &invariant_enforcement,
                    ));
                    // The invariant 409 is now part of the surface's base
                    // set (the template documents it beside the machine
                    // arm), so a declared `throws Conflict` on this route
                    // must NOT double-document 409 (issue #665 review M2 —
                    // the routes-level `covered_statuses` cannot see the
                    // block, which attaches only here).
                    route.raise_responses.retain(|raise| raise.status != 409);
                }
                // The create-shaped arm splices the same two handler-level
                // blocks the CRUD create handler carries (the #338
                // create-eligibility guards and the #666 slice-4d create
                // invariants) — they are rendered from `guard_blocks` /
                // `invariant_blocks`, which the routes plane cannot see.
                if matches!(
                    route.execution,
                    crate::ddd::service_ops::ExecutionKind::Create { .. }
                ) {
                    route.create_guard_block = guard_blocks.create_guard_block.clone();
                    route.create_invariant_block = invariant_blocks.create_block.clone();
                }
            }
            // Issue #778: the execution blocks are pre-rendered from the
            // route, so every route is re-rendered ONCE here — after the
            // post-pass attached the invariant/guard slots above, before
            // `service_ops` moves into the template context.
            for route in service_ops.iter_mut() {
                route.execution_block =
                    crate::ddd::service_ops::render::render_execution_block(route);
            }
        }

        // Media deletion cleanup (issue #631 R7). Monolith topology only
        // (the media slot lives in the monolith AppState — worker placement
        // is the recorded deferral); the domain must declare `[media]` and
        // the plans resolve (the driver's pre-generation pass already
        // hard-failed on a broken config). Empty otherwise — byte-identical.
        let media_cleanup_block = if !project.is_workers_topology()
            && config
                .domains
                .get(&domain)
                .is_some_and(|d| d.media.is_some())
        {
            crate::media::build_media_plans(db, config)
                .await
                .ok()
                .and_then(|plans| plans.get(&domain).cloned())
                .map(|plan| {
                    crate::media::keys::render_cleanup_block(
                        &domain,
                        &crate::media::keys::cleanup_fields_for(&plan, schema_title),
                    )
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        // The child update arm's ownership prefix: at most one plane
        // claims it (the guards when they gate update, else the invariant
        // plane) — merge here so the template keeps its single key.
        let update_ownership_prefix = invariant_blocks
            .update_ownership_prefix
            .clone()
            .unwrap_or_else(|| guard_blocks.update_ownership_prefix.clone());

        // Template decisions resolved in Rust (issue #778 wave 1): the guard
        // presence flags the utoipa 403 docs read, the `?include=`
        // initialiser's deduped field list, and the `Path(…)` parameter
        // bindings (derived from the resolved topology + path parameter).
        let guard_presence = GuardPresence::from_guards(&eligibility_guards);
        let included_default_fields = included_default_fields(&include_paths);
        let path_extractors = path_extractors(&topo.role, ancestor_path_params, &param_name);

        // `?include=` hydration blocks (issue #778 wave 2): the provider
        // decision and the rendered hydration lines live in Rust, so the
        // template renders one pre-computed fragment per endpoint. Empty
        // when the entity has no include surface.
        let include_plan = build_include_plan(
            &include_paths,
            &IncludePlanCtx {
                entity_name: &entity_name,
                domain: &domain,
                module_name: &module_name,
                role: &topo.role,
                is_auditable,
            },
            project,
        );

        let ctx = HandlerContext {
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_list: operations.contains(&"list".to_string()),
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            path_segment,
            tag,
            operations,
            mox_ops,
            service_ops,
            eligibility_guards,
            parent_ref: topo.parent_ref,
            parent_entity,
            parent_path_segment: topo.parent_path_segment,
            parent_module_name: topo.parent_module_name,
            parent_domain: topo.parent_domain,
            role: topo.role,
            param_name,
            parent_param_name,
            grandparent_param_name,
            grandparent_path_segment,
            grandparent_domain,
            path_extractor_item: path_extractors.item,
            path_extractor_item_named: path_extractors.item_named,
            path_extractor_collection: path_extractors.collection,
            path_extractor_collection_unused: path_extractors.collection_unused,
            path_extractor_item_unused_parent: path_extractors.item_unused_parent,
            path_extractor_child_op_add: path_extractors.child_op_add,
            children: topo.children,
            cross_refs: topo.cross_refs,
            status_field,
            has_workflow,
            has_fts,
            fts_rest_mode,
            has_embeddings,
            filter_fields,
            nested_filter_fields,
            max_bulk_size,
            ancestor_path_params,
            hierarchy_field: entity_cfg.and_then(|ec| ec.hierarchy_field.clone()),
            tree_include: entity_cfg
                .and_then(|ec| ec.tree_include.as_ref())
                .map(|v| !v.is_empty())
                .unwrap_or(false),
            has_include,
            include_paths: include_paths.clone(),
            included_default_fields,
            include_get_block: include_plan.get_block,
            include_list_block: include_plan.list_block,
            include_doc: include_plan.include_doc,
            handler_imports,
            is_auditable,
            ux_sort_fields,
            update_ownership_prefix,
            update_guard_block: guard_blocks.update_guard_block,
            delete_ownership_prefix: guard_blocks.delete_ownership_prefix,
            delete_guard_block: guard_blocks.delete_guard_block,
            read_guard_block: guard_blocks.read_guard_block,
            create_guard_block: guard_blocks.create_guard_block,
            create_guarded: guard_presence.create,
            read_guarded: guard_presence.read,
            update_guarded: guard_presence.update,
            delete_guarded: guard_presence.delete,
            create_invariant_block: invariant_blocks.create_block,
            update_invariant_block: invariant_blocks.update_block,
            media_cleanup_block,
        };

        let content = render_template_with_project(tera, "api/handler.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("api")
                .join(&domain)
                .join(format!("{}_handler.rs", module_name)),
            content,
        }])
    }
}

/// Resolved parent/child topology for a handler, gathered from manual config
/// and graph parent candidates.
struct HandlerTopology {
    parent_ref: Option<String>,
    parent_path_segment: Option<String>,
    parent_module_name: Option<String>,
    parent_domain: Option<String>,
    role: String,
    children: Vec<ChildInfo>,
    cross_refs: Vec<CrossRefInfo>,
}

impl Default for HandlerTopology {
    fn default() -> Self {
        Self {
            parent_ref: None,
            parent_path_segment: None,
            parent_module_name: None,
            parent_domain: None,
            role: "root".into(),
            children: vec![],
            cross_refs: vec![],
        }
    }
}

#[allow(clippy::too_many_arguments)] // TODO(#751): gather the title spellings into one param struct
async fn resolve_handler_topology(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_name: &str,
    schema_title: &str,
    stripped_title: &str,
    entity_cfg: Option<&EntityConfig>,
    parent_candidates: &[ParentCandidate],
) -> Result<HandlerTopology> {
    let mut topo = HandlerTopology {
        parent_ref: entity_cfg.and_then(|ec| ec.parent_ref.clone()),
        role: entity_cfg
            .and_then(|ec| ec.role.clone())
            .unwrap_or_else(|| "root".into()),
        ..Default::default()
    };

    // 1. Check manual config for parent (takes priority over graph)
    apply_config_parent(db, config, domain, entity_cfg, &mut topo).await;

    // 1.5 AUTHORED aggregate membership (issue #666 slice 4b): the design's
    // authored boundary wins over config for the parent — an authored
    // member re-points under its authored root; a class PROVEN root by a
    // member's authored declaration drops a config child role back to a
    // top-level root. Undecided boundaries change nothing.
    apply_authored_parent(db, config, domain, schema_title, &mut topo).await;

    // 2. Fall back to graph parent_candidates if manual config didn't resolve parent.
    // Only nest when the parent is in the same domain (matching router behavior).
    apply_graph_parent(
        db,
        config,
        domain,
        stripped_title,
        parent_candidates,
        &mut topo,
    )
    .await;

    // Check if this entity is a parent (graph detection)
    collect_graph_children(
        db,
        config,
        domain,
        stripped_title,
        parent_candidates,
        &mut topo.children,
    )
    .await;

    // Fallback: if entity is a parent in config but wasn't matched by parent_candidates
    if topo.children.is_empty() {
        collect_config_children(db, config, domain, stripped_title, &mut topo.children).await;
    }

    // Detect cross-aggregate entity references.
    topo.cross_refs =
        detect_cross_refs(db, config, domain, entity_name, entity_cfg, &topo.children).await;

    // Graph/config discovery order is not stable across processes (HashMap
    // iteration, unordered MATCH rows); sort so emitted handler code
    // (`with_child` links, route registrations) is byte-identical between runs.
    topo.children
        .sort_by(|a, b| a.path_segment.cmp(&b.path_segment));

    Ok(topo)
}

async fn apply_config_parent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_cfg: Option<&EntityConfig>,
    topo: &mut HandlerTopology,
) {
    if let Some(ec) = entity_cfg
        && ec.role.as_deref() == Some("child")
        && let Some(parent_title) = &ec.parent
    {
        let parent_name = super::router::strip_suffix(parent_title, &config.defaults.type_suffix);
        if topo.parent_ref.is_none() {
            topo.parent_ref = Some(format!(
                "{}_id",
                codegraph_naming::to_snake_case(parent_name)
            ));
        }
        if let Ok(Some(parent_schema)) = db.get_schema_in_domain(parent_title, domain).await {
            topo.parent_path_segment = Some(resolve_path_segment(None, &parent_schema));
            topo.parent_module_name = Some(parent_schema.pg_table_name.clone());
            topo.parent_domain = if config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(parent_title))
                .unwrap_or(false)
            {
                Some(domain.to_string())
            } else {
                parent_schema.domain.or_else(|| Some(domain.to_string()))
            };
        } else {
            topo.parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
            topo.parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
            topo.parent_domain = Some(domain.to_string());
        }
        topo.role = "child".to_string();
    }
}

/// Issue #666 slice 4b: the AUTHORED aggregate boundary wins over config
/// for the handler's parent topology. An authored member re-points under
/// its authored root (config still owns the FK column when it names one —
/// `parent_ref` is seeded before this runs and preserved); a class PROVEN
/// root by a member's authored declaration drops a config child role back
/// to a top-level root. Undecided boundaries change nothing.
async fn apply_authored_parent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    topo: &mut HandlerTopology,
) {
    let type_suffix = config.defaults.type_suffix.clone();
    if let Some(authored) =
        crate::ddd::aggregate::authored_routing_parent(db, config, domain, schema_title).await
    {
        if let Ok(Some(parent_schema)) = db
            .get_schema_in_domain(&authored.parent_title, domain)
            .await
        {
            topo.parent_path_segment = Some(resolve_path_segment(None, &parent_schema));
            topo.parent_module_name = Some(parent_schema.pg_table_name.clone());
            topo.parent_domain = Some(domain.to_string());
        } else {
            let parent_name = super::router::strip_suffix(&authored.parent_title, &type_suffix);
            topo.parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
            topo.parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
            topo.parent_domain = Some(domain.to_string());
        }
        if topo.parent_ref.is_none() {
            topo.parent_ref = Some(authored.fk_column);
        }
        topo.role = "child".to_string();
        return;
    }
    // Authored-root override: proven root + config child → top-level root.
    if topo.role == "child"
        && crate::ddd::aggregate::authored_root_proven(db, &type_suffix, schema_title).await
    {
        topo.role = "root".to_string();
        topo.parent_path_segment = None;
        topo.parent_module_name = None;
        topo.parent_domain = None;
        topo.parent_ref = None;
    }
}

async fn apply_graph_parent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    parent_candidates: &[ParentCandidate],
    topo: &mut HandlerTopology,
) {
    if topo.parent_path_segment.is_some() || topo.role == "root" {
        return;
    }
    for pc in parent_candidates {
        let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
        if child_name == stripped_title {
            // Check if parent is in the same domain: either explicitly listed
            // or its schema is classified into this domain.
            let parent_in_domain = config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(&pc.parent_title))
                .unwrap_or(false)
                || db
                    .get_schema_in_domain(&pc.parent_title, domain)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|s| s.domain.as_ref().map(|d| d == domain))
                    .unwrap_or(false);
            if !parent_in_domain {
                // Parent is in another domain — keep FK column but don't nest
                if topo.parent_ref.is_none() {
                    topo.parent_ref = Some(crate::fk_column_for_candidate(
                        pc,
                        &config.defaults.type_suffix,
                    ));
                }
                break;
            }
            let parent_name =
                super::router::strip_suffix(&pc.parent_title, &config.defaults.type_suffix);
            if topo.parent_ref.is_none() {
                topo.parent_ref = Some(crate::fk_column_for_candidate(
                    pc,
                    &config.defaults.type_suffix,
                ));
            }
            if let Ok(Some(parent_schema)) = db.get_schema_in_domain(&pc.parent_title, domain).await
            {
                topo.parent_path_segment = Some(resolve_path_segment(None, &parent_schema));
                topo.parent_module_name = Some(parent_schema.pg_table_name);
                topo.parent_domain = Some(domain.to_string());
            } else {
                topo.parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
                topo.parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
                topo.parent_domain = Some(domain.to_string());
            }
            topo.role = "child".to_string();
            break;
        }
    }
}

async fn collect_graph_children(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    parent_candidates: &[ParentCandidate],
    resolved_children: &mut Vec<ChildInfo>,
) {
    for pc in parent_candidates {
        let parent_name =
            super::router::strip_suffix(&pc.parent_title, &config.defaults.type_suffix);
        if parent_name == stripped_title {
            let child_name =
                super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
            if let Ok(Some(child_schema)) = db.get_schema_in_domain(&pc.child_title, domain).await {
                resolved_children.push(ChildInfo {
                    entity_name: child_schema.rust_type_name.clone(),
                    module_name: child_schema.pg_table_name.clone(),
                    path_segment: resolve_path_segment(None, &child_schema),
                });
            } else {
                resolved_children.push(ChildInfo {
                    entity_name: child_name.to_string(),
                    module_name: codegraph_naming::to_snake_case(child_name),
                    path_segment: codegraph_naming::to_kebab_case(child_name),
                });
            }
        }
    }
}

async fn collect_config_children(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    resolved_children: &mut Vec<ChildInfo>,
) {
    if let Some(domain_entry) = config.domains.get(domain) {
        // Iterate entity_config entries to find children of this entity
        {
            for (other_title, other_cfg) in &domain_entry.entity_config {
                if other_cfg.role.as_deref() == Some("child")
                    && let Some(parent_title) = &other_cfg.parent
                    && super::router::strip_suffix(parent_title, &config.defaults.type_suffix)
                        == stripped_title
                {
                    let child_name =
                        super::router::strip_suffix(other_title, &config.defaults.type_suffix);
                    if let Ok(Some(child_schema)) =
                        db.get_schema_in_domain(other_title, domain).await
                    {
                        resolved_children.push(ChildInfo {
                            entity_name: child_schema.rust_type_name.clone(),
                            module_name: child_schema.pg_table_name.clone(),
                            path_segment: resolve_path_segment(None, &child_schema),
                        });
                    } else {
                        resolved_children.push(ChildInfo {
                            entity_name: child_name.to_string(),
                            module_name: codegraph_naming::to_snake_case(child_name),
                            path_segment: codegraph_naming::to_kebab_case(child_name),
                        });
                    }
                }
            }
        }
    }
}

async fn detect_cross_refs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_name: &str,
    entity_cfg: Option<&EntityConfig>,
    resolved_children: &[ChildInfo],
) -> Vec<CrossRefInfo> {
    let mut resolved_cross_refs: Vec<CrossRefInfo> = vec![];
    {
        let schema_title_with_type = format!("{}Type", entity_name);
        if let Ok(referenced) = db.get_referenced_schemas(&schema_title_with_type).await {
            let parent_name = entity_cfg.and_then(|ec| ec.parent.as_deref()).unwrap_or("");
            let child_names: std::collections::HashSet<&str> = resolved_children
                .iter()
                .map(|c| c.entity_name.as_str())
                .collect();

            for ref_schema_node in &referenced {
                let ref_title = &ref_schema_node.title;
                let ref_entity_name =
                    super::router::strip_suffix(ref_title, &config.defaults.type_suffix);

                if ref_entity_name == entity_name {
                    continue;
                }
                if ref_entity_name == parent_name {
                    continue;
                }
                if child_names.contains(ref_entity_name) {
                    continue;
                }

                // Only include refs that are entities in any domain
                if let Ok(Some(ref_schema)) = db.get_schema_in_domain(ref_title, domain).await {
                    if ref_schema.pg_table_name.is_empty() {
                        continue;
                    }
                    // Find domain of referenced entity
                    let ref_domain = ref_schema
                        .domain
                        .clone()
                        .unwrap_or_else(|| domain.to_string());
                    let fk_col = codegraph_naming::to_snake_case(ref_entity_name) + "_id";
                    let link_rel = codegraph_naming::to_snake_case(ref_entity_name);
                    resolved_cross_refs.push(CrossRefInfo {
                        entity_name: ref_entity_name.to_string(),
                        module_name: ref_schema.pg_table_name.clone(),
                        domain: ref_domain,
                        path_segment: resolve_path_segment(None, &ref_schema),
                        fk_column: fk_col,
                        link_rel,
                    });
                }
            }
        }
    }
    resolved_cross_refs
}

fn compute_ancestor_path_params(
    config: &DomainConfig,
    domain: &str,
    stripped_title: &str,
    entity_cfg: Option<&EntityConfig>,
    parent_candidates: &[ParentCandidate],
    resolved_parent_domain: Option<&str>,
    resolved_role: &str,
) -> usize {
    if resolved_role != "child" {
        return 0;
    }
    let parent_title_for_lookup = entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
        parent_candidates
            .iter()
            .find(|pc| {
                super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                    == stripped_title
            })
            .map(|pc| pc.parent_title.clone())
    });
    let mut depth = 1usize;
    if let Some(parent_title) = parent_title_for_lookup {
        // Prefer the current entity's domain for looking up the parent's config,
        // since the parent may exist in multiple domains with different roles.
        let parent_dom = if config
            .domains
            .get(domain)
            .map(|d| d.entities.contains(&parent_title))
            .unwrap_or(false)
        {
            domain
        } else {
            resolved_parent_domain.unwrap_or(domain)
        };
        // Check if parent has explicit role=root in its domain config
        let parent_explicit_root = config
            .domains
            .get(parent_dom)
            .and_then(|d| d.get_entity_config(&parent_title))
            .map(|ec| ec.role.as_deref() == Some("root"))
            .unwrap_or(false);
        if !parent_explicit_root {
            // Check if parent is a child (either via manual config or graph)
            let parent_is_child_in_config = config
                .domains
                .get(parent_dom)
                .and_then(|d| d.get_entity_config(&parent_title))
                .map(|ec| ec.role.as_deref() == Some("child"))
                .unwrap_or(false);
            let parent_is_child_in_graph = parent_candidates
                .iter()
                .any(|pc| pc.child_title == parent_title);
            if parent_is_child_in_config || parent_is_child_in_graph {
                depth += 1;
            }
        }
    }
    depth
}

async fn resolve_grandparent(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_cfg: Option<&EntityConfig>,
    parent_candidates: &[ParentCandidate],
    stripped_title: &str,
    ancestor_path_params: usize,
) -> (Option<String>, Option<String>, Option<String>) {
    if ancestor_path_params < 2 {
        return (None, None, None);
    }
    let parent_title = entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
        parent_candidates
            .iter()
            .find(|pc| {
                super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                    == stripped_title
            })
            .map(|pc| pc.parent_title.clone())
    });
    let gp_title = parent_title.and_then(|pt| {
        config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&pt))
            .and_then(|ec| ec.parent.clone())
            .or_else(|| {
                let parent_stripped =
                    super::router::strip_suffix(&pt, &config.defaults.type_suffix);
                parent_candidates
                    .iter()
                    .find(|pc| {
                        super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                            == parent_stripped
                    })
                    .map(|pc| pc.parent_title.clone())
            })
    });
    if let Some(ref gpt) = gp_title {
        if let Ok(Some(gp_schema)) = db.get_schema_in_domain(gpt, domain).await {
            let gp_seg = resolve_path_segment(None, &gp_schema);
            let gp_param = super::router::param_name_from_path_segment(&gp_seg);
            // Grandparent domain: look up which domain owns it.
            let gp_domain = config
                .domains
                .iter()
                .find_map(|(d, dc)| {
                    if dc.get_entity_config(gpt).is_some() {
                        Some(d.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| domain.to_string());
            (Some(gp_param), Some(gp_seg), Some(gp_domain))
        } else {
            let gp_name = super::router::strip_suffix(gpt, &config.defaults.type_suffix);
            let gp_seg = codegraph_naming::to_kebab_case(gp_name);
            let gp_param = super::router::param_name_from_path_segment(&gp_seg);
            (Some(gp_param), Some(gp_seg), Some(domain.to_string()))
        }
    } else {
        (None, None, None)
    }
}

async fn resolve_include_paths(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    entity_cfg: Option<&EntityConfig>,
    project: &ProjectConfig,
) -> Result<Vec<ResolvedIncludePath>> {
    // ONE gate, shared with the generated hurl contract's `?include=` legs
    // (issue #781): the contract must assert exactly the surface this handler
    // hydrates.
    super::include_path::resolve_include_paths_for_entity(
        db,
        config,
        domain,
        schema_title,
        entity_cfg,
        project.is_workers_topology(),
    )
    .await
}

pub(crate) async fn resolve_is_auditable(
    db: &dyn GraphQuerier,
    schema_title: &str,
    config: &DomainConfig,
    domain: &str,
) -> Result<bool> {
    let policies = db.get_policies_for_schema(schema_title).await?;
    // Issue #449: a `.ddd` design overrides both the operation set and the
    // auditable flag — the handler's call shape must match the design-mapped
    // query layer exactly (include_deleted on read methods exists only when
    // the query layer accepts it), so mirror ddd/query.rs's resolution.
    let design_surface = crate::ddd::design::DddDesignSurface::from_graph(db).await?;
    let mut operations =
        crate::api::api_model::resolve_entity_operations(db, config, domain, schema_title).await;
    if let Some(design_ops) = design_surface.operations_for(schema_title) {
        operations = design_ops;
    }
    // Append-only snapshot semantics (issue #284) — same inference as
    // db/ddl.rs: insert-only entities carry no audit columns, so the API
    // layer never passes `include_deleted` to the query/repository traits.
    let append_only = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(schema_title))
        .as_ref()
        .is_some_and(|ec| ec.is_append_only())
        || !operations.iter().any(|op| op == "update" || op == "delete");
    let is_auditable = (if let Some(design) = design_surface.design_for(schema_title) {
        design.flags.auditable
    } else if policies.is_empty() {
        config
            .domains
            .get(domain)
            .and_then(|d| d.auditable)
            .unwrap_or(true)
    } else {
        policies
            .iter()
            .any(|p| matches!(&p.kind, PolicyKind::Audit(a) if a.track_deleted))
    }) && !append_only;
    Ok(is_auditable)
}

/// Template pins for the `?sort=` list surface (issue #306): the
/// generated handler parses/validates `sort`/`order` against the plan's
/// allow-list and names the valid fields in the 400; flag-off renders no
/// sort surface at all.
#[cfg(test)]
mod ux_sort_handler_template_tests {
    use super::*;
    use serde_json::json;

    /// Bare context is a ROOT read+list handler, so its pre-rendered `Path(…)`
    /// bindings are the root shapes (see [`path_extractors`]).
    fn base_ctx(ux_sort_fields: Vec<&str>) -> serde_json::Value {
        let extractors = path_extractors("root", 0, "task_id");
        json!({
            "entity_name": "Task",
            "module_name": "task",
            "domain": "common",
            "path_segment": "tasks",
            "tag": "Task",
            "operations": ["list", "read"],
            "has_create": false,
            "has_read": true,
            "has_update": false,
            "has_delete": false,
            "has_list": true,
            "role": "root",
            "param_name": "task_id",
            "path_extractor_item": extractors.item,
            "path_extractor_item_named": extractors.item_named,
            "path_extractor_collection": extractors.collection,
            "path_extractor_collection_unused": extractors.collection_unused,
            "path_extractor_item_unused_parent": extractors.item_unused_parent,
            "path_extractor_child_op_add": extractors.child_op_add,
            "children": [],
            "cross_refs": [],
            "status_field": null,
            "has_workflow": false,
            "has_fts": false,
            "fts_rest_mode": "query_param",
            "has_embeddings": false,
            "filter_fields": [],
            "nested_filter_fields": [],
            "max_bulk_size": 100,
            "ancestor_path_params": 0,
            "hierarchy_field": null,
            "tree_include": false,
            "has_include": false,
            "include_paths": [],
            "handler_imports": [],
            "is_auditable": false,
            "ux_sort_fields": ux_sort_fields,
        })
    }

    fn render(ctx: &serde_json::Value) -> String {
        let tera = crate::template_engine::create_tera(std::path::Path::new("."))
            .expect("embedded templates");
        crate::render_template_with_project(
            &tera,
            "api/handler.tera",
            ctx,
            &ProjectConfig::default(),
        )
        .expect("handler renders")
    }

    #[test]
    fn flag_on_handler_validates_sort_params_against_allow_list() {
        let out = render(&base_ctx(vec!["name", "total_amount"]));

        // ListParams gains optional sort/order with serde defaults (absent
        // params keep the default created_at DESC ordering).
        assert!(
            out.contains("#[serde(default)]\n    pub sort: Option<String>,"),
            "{out}"
        );
        assert!(
            out.contains("#[serde(default)]\n    pub order: Option<String>,"),
            "{out}"
        );
        // The allow-list const carries the plan's sortable fields.
        assert!(
            out.contains("const ALLOWED_SORT_FIELDS: &[&str] = &["),
            "{out}"
        );
        assert!(out.contains("\"name\","), "{out}");
        assert!(out.contains("\"total_amount\","), "{out}");
        // 400 texts name the valid sortable fields (include-path style).
        assert!(
            out.contains("Unknown sort field: {sort}. Valid sortable fields: {}"),
            "{out}"
        );
        assert!(
            out.contains("order requires sort; valid sortable fields: {}"),
            "{out}"
        );
        // `order` is restricted to asc|desc.
        assert!(
            out.contains("Invalid order: {other}. Valid values: asc, desc"),
            "{out}"
        );
        // The validated spec threads into the query layer.
        assert!(out.contains("let ux_sort: Option<(String, bool)>"), "{out}");
        assert!(
            out.contains(
                "list_filtered(params.page, params.page_size, &filters, ux_sort, api_key_info"
            ),
            "{out}"
        );
    }

    #[test]
    fn flag_off_handler_emits_no_sort_surface() {
        let out = render(&base_ctx(Vec::new()));
        for needle in [
            "ALLOWED_SORT_FIELDS",
            "pub sort:",
            "pub order:",
            "Unknown sort field",
            "Invalid order",
            "order requires sort",
            "ux_sort",
        ] {
            assert!(
                !out.contains(needle),
                "flag-off handler must not contain {needle:?}:\n{out}"
            );
        }
        // The pre-#306 call shape stays intact.
        assert!(
            out.contains("list_filtered(params.page, params.page_size, &filters, api_key_info"),
            "{out}"
        );
    }

    /// Issue #479: create/update bodies extract through a
    /// `serde_path_to_error`-wrapped extractor so 422s name
    /// `path.to.field: cause` instead of axum's opaque untagged-enum
    /// rejection. The create body branches on the payload shape BEFORE
    /// deserializing (the untagged enum discards per-variant serde
    /// errors, so path-tracking the enum is not enough); the extractor is
    /// emitted only when a write body exists.
    #[test]
    fn write_bodies_extract_through_serde_path_to_error() {
        // has_create renders the extractor + the create signature.
        let mut ctx = base_ctx(Vec::new());
        ctx["has_create"] = json!(true);
        let out = render(&ctx);
        for needle in [
            // The shared rejection mapper names the failing path + cause.
            "fn json_body_rejection(e: serde_path_to_error::Error<serde_json::Error>) -> AppError {",
            "field: e.path().to_string(),",
            "message: e.inner().to_string(),",
            // Non-JSON content types keep the 415 contract axum's Json had.
            "StatusCode::UNSUPPORTED_MEDIA_TYPE",
            "code: \"UNSUPPORTED_MEDIA_TYPE\"",
            // The create extractor branches on the payload shape and
            // path-tracks EACH attempt (Bulk|Single semantics preserved).
            "pub struct CreateBody(pub CreateTaskBody);",
            "let body = if value.is_array() {",
            "Vec<CreateTaskRequest> =\n                serde_path_to_error::deserialize(value)",
            "CreateTaskBody::Bulk(items)",
            "CreateTaskBody::Single(item)",
            "CreateBody(body): CreateBody,",
            "#[serde(untagged)]",
        ] {
            assert!(
                out.contains(needle),
                "handler must contain {needle:?}:\n{out}"
            );
        }
        // Update rides the generic path-tracked extractor.
        let mut ctx = base_ctx(Vec::new());
        ctx["has_update"] = json!(true);
        let out = render(&ctx);
        assert!(out.contains("pub struct JsonPathBody<T>(pub T);"), "{out}");
        assert!(
            out.contains("JsonPathBody(body): JsonPathBody<UpdateTaskRequest>,"),
            "{out}"
        );
        // Read-only entities emit neither extractor nor wrapper noise.
        let out = render(&base_ctx(Vec::new()));
        for needle in ["JsonPathBody", "CreateBody", "json_body_rejection"] {
            assert!(
                !out.contains(needle),
                "read-only handler must not emit {needle:?}:\n{out}"
            );
        }
    }
}

/// Template decisions resolved in Rust (issue #778 wave 1): the guard
/// presence flags and the `?include=` default-field dedup. These are
/// derivation tests — the template oracle (handler matrix) pins the rendered
/// bytes; this module pins the Rust values the template reads.
#[cfg(test)]
mod template_decisions_tests {
    use super::*;

    fn guard(operations: &[&str]) -> crate::ddd::rules::EligibilityGuard {
        crate::ddd::rules::EligibilityGuard {
            rule_name: "PositiveTotal".to_string(),
            fn_name: "task_positive_total".to_string(),
            call_path: "crate::domain::common::rules::task_positive_total".to_string(),
            operations: operations.iter().map(|op| op.to_string()).collect(),
        }
    }

    fn include_path(alias: &str, modules: &[&str]) -> ResolvedIncludePath {
        ResolvedIncludePath {
            alias: alias.to_string(),
            segments: modules
                .iter()
                .map(|module| super::super::include_path::IncludeSegment {
                    entity_name: module.to_string(),
                    schema_title: format!("{module}Type"),
                    module_name: module.to_string(),
                    domain: "common".to_string(),
                    table: format!("\"common\".\"{module}\""),
                    fk_column: format!("{module}_id"),
                    reverse_fk_column: "task_id".to_string(),
                    fk_is_required: false,
                    reverse_fk_is_required: false,
                    is_array: false,
                    child_table_override: None,
                })
                .collect(),
            response_rust_type: format!("{alias}Response"),
            fetch_method: format!("fetch_{alias}"),
            batch_fetch_method: format!("fetch_{alias}_batch"),
        }
    }

    #[test]
    fn guard_presence_flags_every_gated_operation() {
        let presence =
            GuardPresence::from_guards(&[guard(&["create", "read", "update", "delete"])]);
        assert_eq!(
            presence,
            GuardPresence {
                create: true,
                read: true,
                update: true,
                delete: true,
            }
        );
    }

    #[test]
    fn guard_presence_is_empty_without_guards() {
        assert_eq!(GuardPresence::from_guards(&[]), GuardPresence::default());
        assert!(!GuardPresence::default().create);
        assert!(!GuardPresence::default().read);
        assert!(!GuardPresence::default().update);
        assert!(!GuardPresence::default().delete);
    }

    #[test]
    fn guard_presence_unions_across_guards_and_ignores_unguarded_operations() {
        // Two rules, disjoint gates; `list`/`search` never guard (row
        // filtering is a documented follow-up) and must not leak a flag.
        let presence = GuardPresence::from_guards(&[
            guard(&["read", "list"]),
            guard(&["update", "delete", "search"]),
        ]);
        assert_eq!(
            presence,
            GuardPresence {
                create: false,
                read: true,
                update: true,
                delete: true,
            }
        );
    }

    #[test]
    fn included_default_fields_dedup_sibling_paths_in_first_occurrence_order() {
        // Two dot paths share the `assignee` first segment; the third is a
        // scalar path. One field per first segment, declaration order.
        let fields = included_default_fields(&[
            include_path("assignee.person", &["assignee", "person"]),
            include_path("assignee.organization", &["assignee", "organization"]),
            include_path("attachments", &["attachment"]),
        ]);
        assert_eq!(
            fields,
            vec!["assignee".to_string(), "attachment".to_string()]
        );
    }

    #[test]
    fn included_default_fields_are_empty_without_paths() {
        assert!(included_default_fields(&[]).is_empty());
        // Defensive: a path with no segments contributes no field.
        let no_segments = ResolvedIncludePath {
            segments: Vec::new(),
            ..include_path("broken", &["task"])
        };
        assert!(included_default_fields(&[no_segments]).is_empty());
    }

    // ── Pre-rendered `Path(…)` bindings (T1.3) ─────────────────────────

    /// The exact strings the template's removed conditional blocks produced,
    /// trailing blank lines included (the oracle pins the rendered bytes; these
    /// pin the derivation).
    #[test]
    fn path_extractors_item_shape_per_depth() {
        let root = path_extractors("root", 0, "task_id");
        assert_eq!(root.item, "\n    Path(id): Path<Uuid>,\n");

        let child = path_extractors("child", 1, "note_id");
        assert_eq!(
            child.item,
            "\n\n    Path((parent_id, id)): Path<(Uuid, Uuid)>,\n\n"
        );

        let grandchild = path_extractors("child", 2, "revision_id");
        assert_eq!(
            grandchild.item,
            "\n\n    Path((_grandparent_id, parent_id, id)): Path<(Uuid, Uuid, Uuid)>,\n\n"
        );

        // A non-`child` role takes the root branch, exactly like the template.
        assert_eq!(
            path_extractors("value_object", 2, "task_id").item,
            root.item
        );
    }

    #[test]
    fn path_extractors_named_item_substitutes_the_path_parameter() {
        assert_eq!(
            path_extractors("root", 0, "task_id").item_named,
            "\n    Path(task_id): Path<Uuid>,\n"
        );
        assert_eq!(
            path_extractors("child", 1, "task_note_id").item_named,
            "\n\n    Path((parent_id, task_note_id)): Path<(Uuid, Uuid)>,\n\n"
        );
        assert_eq!(
            path_extractors("child", 2, "task_note_revision_id").item_named,
            "\n\n    Path((_grandparent_id, parent_id, task_note_revision_id)): Path<(Uuid, Uuid, Uuid)>,\n\n"
        );
    }

    #[test]
    fn path_extractors_collection_is_empty_at_root() {
        // Root collection endpoints (`create`/`list`) bind no path at all, so
        // the template splices nothing and keeps its single blank line.
        let root = path_extractors("root", 0, "task_id");
        assert_eq!(root.collection, "");
        assert_eq!(root.collection_unused, "");

        let child = path_extractors("child", 1, "task_note_id");
        assert_eq!(child.collection, "\n\n    Path(parent_id): Path<Uuid>,\n\n");
        assert_eq!(
            path_extractors("child", 2, "task_note_id").collection,
            "\n\n    Path((_grandparent_id, parent_id)): Path<(Uuid, Uuid)>,\n\n"
        );
    }

    #[test]
    fn path_extractors_collection_unused_per_depth() {
        let child = path_extractors("child", 1, "task_note_id");
        assert_eq!(
            child.collection_unused,
            "\n\n    Path(_parent_id): Path<Uuid>,\n\n"
        );
        assert_eq!(
            path_extractors("child", 2, "task_note_id").collection_unused,
            "\n\n    Path((_grandparent_id, _parent_id)): Path<(Uuid, Uuid)>,\n\n"
        );
    }

    #[test]
    fn path_extractors_item_unused_parent_per_depth() {
        assert_eq!(
            path_extractors("root", 0, "task_id").item_unused_parent,
            "\n    Path(id): Path<Uuid>,\n"
        );
        assert_eq!(
            path_extractors("child", 1, "task_note_id").item_unused_parent,
            "\n\n    Path((_parent_id, id)): Path<(Uuid, Uuid)>,\n\n"
        );
        assert_eq!(
            path_extractors("child", 2, "task_note_id").item_unused_parent,
            "\n\n    Path((_grandparent_id, _parent_id, id)): Path<(Uuid, Uuid, Uuid)>,\n\n"
        );
    }

    #[test]
    fn path_extractors_child_op_add_carries_one_fewer_blank_line() {
        // The child-op `add` arm sits one nesting level deeper, so its own
        // control tags emit the outer blank line: the segment carries one
        // leading blank line, not two.
        assert_eq!(
            path_extractors("child", 1, "task_note_id").child_op_add,
            "\n    Path(parent_id): Path<Uuid>,\n"
        );
        assert_eq!(
            path_extractors("child", 2, "task_note_id").child_op_add,
            "\n    Path((_grandparent_id, parent_id)): Path<(Uuid, Uuid)>,\n"
        );
        // Never emitted for a root entity (the op plane nests the arm).
        assert_eq!(path_extractors("root", 0, "task_id").child_op_add, "");
    }
}
