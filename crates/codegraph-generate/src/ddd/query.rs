use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{ParentCandidate, PolicyKind};
use serde::Serialize;

use crate::api::api_model::resolve_entity_operations;
use crate::ddd::repository_emitter::RepositoryImplEmitter;
use crate::error::Result;
use crate::filter_fields::{FilterFieldInfo, resolve_filter_fields};
use crate::render_template_with_project;
use crate::traits::{EntityGenerator, EntityGeneratorKind, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct QueryContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub has_read: bool,
    pub has_create: bool,
    pub has_list: bool,
    /// The query handler emits `find_by_id` (issue #869 — the ONE loader
    /// derivation, [`crate::ddd::loaders::QueryLoaders`]).
    pub has_find_by_id: bool,
    /// The query handler emits `find_by_id_scoped` (issue #869).
    pub has_find_by_id_scoped: bool,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub filter_fields: Vec<FilterFieldInfo>,
    /// FK column for parent-scoped lookups (child entities only).
    pub parent_ref: Option<String>,
    /// Whether this entity supports soft-delete/audit.
    #[serde(default)]
    pub is_auditable: bool,
    /// Hierarchy parent field configured — emits `find_tree` on the query
    /// handler so the tree endpoint works on every persistence provider.
    #[serde(default)]
    pub hierarchy_field: bool,
    /// Whether the tree query returns JOINed `serde_json::Value` rows
    /// (tree_include configured) versus typed `{Entity}Response` rows.
    #[serde(default)]
    pub tree_include: bool,
    /// Whether the list query accepts the ux sort param (issue #306).
    /// When false, `list_filtered` keeps its pre-#306 signature so
    /// flag-off output stays byte-identical.
    #[serde(default)]
    pub ux_sort: bool,
    /// Mox derived feature names populated at SELECT time (issue #508).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mox_derived_names: Vec<String>,
    /// Pre-rendered cornucopia `?include=` hydration methods (issue #781 P1,
    /// default ON). Empty unless the provider is cornucopia and the entity
    /// models at least one implemented include shape.
    #[serde(default)]
    pub include_query_methods: String,
    /// The `use …;` lines the include methods need (target response DTOs).
    #[serde(default)]
    pub include_response_imports: String,
}

/// The op set the query plane serves for one entity — the #449 precedence:
/// the config/graph operations, replaced by the design's repository
/// built-ins when a design covers the title. ONE derivation for the query
/// handler's own emission and for every plane that asks which query
/// methods exist: the machine read model's loader check reads it here
/// rather than re-deriving it (issue #824).
pub(crate) async fn effective_operations(
    db: &dyn GraphQuerier,
    design_surface: &crate::ddd::design::DddDesignSurface,
    config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    entity_name: &str,
) -> Vec<String> {
    let mut operations = resolve_entity_operations(db, config, domain, entity_name).await;
    if let Some(design_ops) = design_surface.operations_for(schema_title) {
        operations = design_ops;
    }
    operations
}

pub struct QueryGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
}

impl QueryGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
        }
    }

    pub fn with_parent_candidates(mut self, candidates: Vec<ParentCandidate>) -> Self {
        self.parent_candidates = candidates;
        self
    }
}

#[async_trait]
impl EntityGenerator for QueryGenerator {
    fn kind(&self) -> EntityGeneratorKind {
        EntityGeneratorKind::Query
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

        // DDD design surface (issue #449): a design covering this title
        // replaces the operations with its repository built-in mapping and
        // drives the FTS surface + filter fields from its search
        // definition.
        let design_surface = crate::ddd::design::DddDesignSurface::from_graph(db).await?;
        let operations = effective_operations(
            db,
            &design_surface,
            config,
            &domain,
            schema_title,
            &entity_name,
        )
        .await;
        let design_search = design_surface.search_for(schema_title);

        let search = entity_cfg.map(|ec| &ec.search);
        let has_fts = match design_search {
            Some(design) => !design.text.is_empty(),
            None => search
                .and_then(|s| s.fts_columns.as_ref())
                .map(|cols| !cols.is_empty())
                .unwrap_or(false),
        };
        let has_embeddings = search
            .map(|s| !s.embedding_columns.is_empty())
            .unwrap_or(false);

        let filter_fields = match design_search {
            Some(design) => {
                crate::ddd::design::design_filter_fields(db, schema_title, design).await?
            }
            None => {
                resolve_filter_fields(
                    db,
                    schema_title,
                    entity_cfg
                        .and_then(|ec| ec.filter_fields.as_ref())
                        .map(|v| v.as_slice()),
                )
                .await?
            }
        };

        // Resolve parent_ref for child entities
        let parent_ref = crate::resolve_parent_fk_column_same_domain(
            schema_title,
            &self.parent_candidates,
            entity_cfg,
            &domain,
            config,
            db,
        )
        .await;

        // Tree support mirrors the handler/repository emitters: entities with
        // a hierarchy_field get a `find_tree` query-handler method so the
        // tree endpoint can route through it on every persistence provider.
        let hierarchy_field = entity_cfg
            .and_then(|ec| ec.hierarchy_field.as_ref())
            .is_some();
        let tree_include = if hierarchy_field {
            RepositoryImplEmitter
                .resolve_tree_include(db, schema_title, &domain, config, parent_ref.as_deref())
                .await
                .unwrap_or(false)
        } else {
            false
        };

        let policies = db.get_policies_for_schema(schema_title).await?;
        // Append-only snapshot semantics (issue #284) — mirror ddl.rs /
        // repository.rs: no audit columns on insert-only tables, so the
        // generated query layer must not reference `include_deleted`.
        let append_only = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(schema_title))
            .as_ref()
            .is_some_and(|ec| ec.is_append_only())
            || !operations.iter().any(|op| op == "update" || op == "delete");
        // Issue #449: when a design exists for this title, `flags.auditable`
        // WINS over the policy/domains.toml fallback ordering.
        let is_auditable = (if let Some(design) = design_surface.design_for(schema_title) {
            design.flags.auditable
        } else if policies.is_empty() {
            config
                .domains
                .get(&domain)
                .and_then(|d| d.auditable)
                .unwrap_or(true)
        } else {
            policies
                .iter()
                .any(|p| matches!(&p.kind, PolicyKind::Audit(a) if a.track_deleted))
        }) && !append_only;

        // ux sort plane (issue #306): thread the param only when the
        // allow-list is non-empty, so the trait/query/repository
        // signatures stay in sync and flag-off output is byte-identical.
        let ux_sort = operations.iter().any(|op| op == "list")
            && crate::ux::sort::resolve_ux_sort_plan(db, config, project, schema_title, &domain)
                .await
                .map(|plan| !plan.is_empty())
                .unwrap_or(false);

        // SELECT-time derived-field population (issue #508): the mox
        // derived response fields are computed from the loaded row. The
        // skip list is the DTO field set — the same decision dto.rs makes
        // when it splits stored-derived columns from computed extras.
        let dto_names: std::collections::HashSet<String> =
            match crate::ddd::dto::build_dto_context(db, schema_title, &domain, config).await {
                Ok(dto) => dto.fields.iter().map(|f| f.name.clone()).collect(),
                Err(_) => std::collections::HashSet::new(),
            };
        let mox_derived_names: Vec<String> = db
            .get_mox_derived_features()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|d| d.class == schema_title || d.class == entity_name)
            .map(|d| {
                codegraph_naming::escape_rust_keyword(&codegraph_naming::to_snake_case(&d.name))
            })
            .filter(|name| !dto_names.contains(name.as_str()))
            .collect();

        // `?include=` hydration wrappers (issue #781 P1, default ON):
        // cornucopia-only, pre-rendered — the query handler owns the client,
        // the transaction, and the RLS session vars, exactly like the tree /
        // CRUD methods above.
        let (include_query_methods, include_response_imports) = cornucopia_include_query_surface(
            db,
            config,
            project,
            &domain,
            schema_title,
            &entity_name,
            &module_name,
        )
        .await;

        let loaders =
            crate::ddd::loaders::QueryLoaders::for_operations(&operations, parent_ref.is_some());
        let ctx = QueryContext {
            has_read: operations.contains(&"read".to_string()),
            has_create: operations.contains(&"create".to_string()),
            has_list: operations.contains(&"list".to_string()),
            has_find_by_id: loaders.find_by_id,
            has_find_by_id_scoped: loaders.find_by_id_scoped,
            has_fts,
            has_embeddings,
            filter_fields,
            parent_ref,
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            is_auditable,
            hierarchy_field,
            tree_include,
            ux_sort,
            mox_derived_names,
            include_query_methods,
            include_response_imports,
        };

        /// Build the cornucopia `?include=` query-handler surface (issue #781 P1).
        ///
        /// Returns `(methods, imports)`:
        ///
        /// * `methods` — one GET wrapper + one LIST wrapper per IMPLEMENTED include
        ///   path, rendered at the `impl {Entity}QueryHandler` level. Each opens the
        ///   client + transaction, installs the RLS session vars, calls the adapter
        ///   fetch on that transaction, and commits — the tree handler's call shape,
        ///   so the session context (and therefore RLS) is exactly the same as every
        ///   other cornucopia read.
        /// * `imports` — the `use crate::domain::{target}::dto_response::{Resp};`
        ///   lines the method signatures need (deduped, self-targets skipped because
        ///   the header already imports them).
        ///
        /// Empty pair unless the provider is cornucopia and the entity models
        /// at least one implemented include shape (issue #781 P1, default ON).
        #[allow(clippy::too_many_arguments)]
        async fn cornucopia_include_query_surface(
            db: &dyn GraphQuerier,
            config: &DomainConfig,
            project: &ProjectConfig,
            domain: &str,
            schema_title: &str,
            entity_name: &str,
            module_name: &str,
        ) -> (String, String) {
            if !project.is_cornucopia() {
                return (String::new(), String::new());
            }
            let entity_cfg = config
                .domains
                .get(domain)
                .and_then(|d| d.get_entity_config(schema_title));
            let paths = crate::api::include_path::resolve_include_paths_for_entity(
                db,
                config,
                domain,
                schema_title,
                entity_cfg,
                project.is_workers_topology(),
            )
            .await
            .unwrap_or_default();
            // The SAME per-path verdict the handler and the emitters read: a shape
            // the plane does not implement gets no wrapper (its request answers 501).
            let support = crate::api::include_plan::include_support_for(project);
            let supported = support.supported_paths(&paths);
            if supported.is_empty() {
                return (String::new(), String::new());
            }

            let error_type = format!("{}Error", codegraph_naming::to_pascal_case(domain));
            let mut methods = String::new();
            let mut imports = String::new();
            let mut imported: std::collections::HashSet<String> =
                std::collections::HashSet::from([format!("{entity_name}Response")]);

            for path in supported {
                let Some(seg) = path.segments.first() else {
                    continue;
                };
                // The SAME classifier the handler and the SQL/adapter planes
                // read: it decides whether the wrapper returns a single value
                // or the collection of the reverse-FK shape.
                let Ok(shape) = crate::api::include_plan::CornucopiaIncludeShape::classify(path)
                else {
                    continue;
                };
                let resp = &path.response_rust_type;
                if shape == crate::api::include_plan::CornucopiaIncludeShape::Dot {
                    // The combined response type lives in THIS entity's
                    // `dto_included` (it carries the first hop's base fields
                    // plus the leaf), so it is a sibling import — not the
                    // target's `dto_response`.
                    if imported.insert(resp.clone()) {
                        imports.push_str(&format!("use super::dto_included::{resp};\n"));
                    }
                } else {
                    let same_module = seg.domain == domain && seg.module_name == module_name;
                    if !same_module && imported.insert(resp.clone()) && resp.ends_with("Response") {
                        imports.push_str(&format!(
                            "use crate::domain::{}::{}::dto_response::{};\n",
                            seg.domain, seg.module_name, resp
                        ));
                    }
                }
                // The GET return type tracks the DTO include field: a scalar
                // path is `Option<{Target}Response>`, an array path is the
                // whole `Vec<{Target}Response>` (the handler wraps it in
                // `Some`), and a dot path is the combined response the handler
                // merges (the LIST map values follow the same split — the dot
                // batch is `HashMap<Uuid, Combined>` with no `Option`, the
                // SeaORM dot batch's contract).
                let (get_ret, map_ret) = if shape.is_array() {
                    (
                        format!("Vec<{resp}>"),
                        format!("std::collections::HashMap<Uuid, Vec<{resp}>>"),
                    )
                } else if shape == crate::api::include_plan::CornucopiaIncludeShape::Dot {
                    (
                        format!("Option<{resp}>"),
                        format!("std::collections::HashMap<Uuid, {resp}>"),
                    )
                } else {
                    (
                        format!("Option<{resp}>"),
                        format!("std::collections::HashMap<Uuid, Option<{resp}>>"),
                    )
                };
                methods.push_str(&format!(
                    "\n    /// Eager-load `{}` for one {entity_name} row (issue #781 P1).\n",
                    path.alias
                ));
                methods.push_str(&format!(
            "    pub async fn {}(&self, source_id: Uuid, api_key_id: Uuid, organization_id: Uuid, user_id: Uuid, role: String) -> Result<{get_ret}, {error_type}> {{\n",
            path.fetch_method
        ));
                push_cornucopia_query_tx(
                    &mut methods,
                    &error_type,
                    &path.fetch_method,
                    "&tx, source_id",
                );
                methods.push_str("        Ok(result)\n    }\n");

                methods.push_str(&format!(
            "\n    /// Eager-load `{}` for many {entity_name} rows, keyed by source id (issue #781 P1).\n",
            path.alias
        ));
                methods.push_str(&format!(
            "    pub async fn {}(&self, source_ids: &[Uuid], api_key_id: Uuid, organization_id: Uuid, user_id: Uuid, role: String) -> Result<{map_ret}, {error_type}> {{\n",
            path.batch_fetch_method
        ));
                push_cornucopia_query_tx(
                    &mut methods,
                    &error_type,
                    &path.batch_fetch_method,
                    "&tx, source_ids",
                );
                methods.push_str("        Ok(result)\n    }\n");
            }

            (methods, imports)
        }

        /// The client/transaction/session preamble + fetch + commit shared by both
        /// include wrappers.
        fn push_cornucopia_query_tx(out: &mut String, error_type: &str, method: &str, args: &str) {
            out.push_str("        let mut client = self.client_source.client().await\n");
            out.push_str(&format!(
                "            .map_err(|e| {error_type}::from_repo_err(e))?;\n"
            ));
            out.push_str("        let tx = client.transaction().await\n");
            out.push_str(&format!(
                "            .map_err(|e| {error_type}::from_repo_err(e))?;\n"
            ));
            out.push_str(
        "        set_rls_session_vars(&tx, api_key_id, organization_id, user_id, &role).await?;\n",
    );
            out.push_str(&format!(
                "        let result = self.repo.{method}({args}).await\n"
            ));
            out.push_str(&format!(
                "            .map_err(|e| {error_type}::from_repo_err(e))?;\n"
            ));
            out.push_str("        tx.commit().await\n");
            out.push_str(&format!(
                "            .map_err(|e| {error_type}::from_repo_err(e))?;\n"
            ));
        }

        let content = render_template_with_project(tera, "ddd/query.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(&domain)
                .join(&module_name)
                .join("query.rs"),
            content,
        }])
    }
}
