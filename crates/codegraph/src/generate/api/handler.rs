use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;
use serde::Serialize;

use crate::error::Result;
use crate::generate::filter_fields::{
    resolve_filter_fields, resolve_nested_filter_fields, FilterFieldInfo, NestedFilterFieldInfo,
};
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

use super::router::{ChildInfo, CrossRefInfo};

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
    /// Domain of the grandparent entity (depth-2 children only).
    pub grandparent_domain: Option<String>,
    pub children: Vec<ChildInfo>,
    pub cross_refs: Vec<CrossRefInfo>,
    /// When set, the list endpoint supports ?status= filtering on this column.
    pub status_field: Option<String>,
    /// Whether this entity has workflow support.
    pub has_workflow: bool,
    /// Whether this entity has full-text search enabled.
    pub has_fts: bool,
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
}

pub struct HandlerGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
}

impl HandlerGenerator {
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
impl EntityGenerator for HandlerGenerator {
    fn name(&self) -> &str {
        "handler"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = db
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();
        let path_segment = schema.api_path_segment.clone();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let operations = entity_cfg
            .and_then(|ec| ec.operations.clone())
            .unwrap_or_else(|| config.defaults.operations.clone());

        let tag = entity_cfg
            .and_then(|ec| ec.tag.clone())
            .unwrap_or_else(|| entity_name.clone());

        let mut parent_ref = entity_cfg.and_then(|ec| ec.parent_ref.clone());
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
        let stripped_title = super::router::strip_suffix(schema_title, &config.defaults.type_suffix);
        let mut resolved_parent_path_segment = None;
        let mut resolved_parent_module_name = None;
        let mut resolved_parent_domain = None;
        let mut resolved_role = entity_cfg
            .and_then(|ec| ec.role.clone())
            .unwrap_or_else(|| "root".into());
        let mut resolved_children: Vec<ChildInfo> = vec![];

        // 1. Check manual config for parent (takes priority over graph)
        if let Some(ec) = entity_cfg {
            if ec.role.as_deref() == Some("child") {
                if let Some(parent_title) = &ec.parent {
                    let parent_name = super::router::strip_suffix(parent_title, &config.defaults.type_suffix);
                    if parent_ref.is_none() {
                        parent_ref = Some(format!("{}_id", codegraph_naming::to_snake_case(parent_name)));
                    }
                    if let Ok(Some(parent_schema)) = db.get_schema(parent_title).await {
                        resolved_parent_path_segment = Some(parent_schema.api_path_segment.clone());
                        resolved_parent_module_name = Some(parent_schema.pg_table_name.clone());
                        resolved_parent_domain = if config
                            .domains
                            .get(&domain)
                            .map(|d| d.entities.contains(parent_title))
                            .unwrap_or(false)
                        {
                            Some(domain.clone())
                        } else {
                            parent_schema
                                .domain
                                .clone()
                                .or_else(|| Some(domain.clone()))
                        };
                    } else {
                        resolved_parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
                        resolved_parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
                        resolved_parent_domain = Some(domain.clone());
                    }
                    resolved_role = "child".to_string();
                }
            }
        }

        // 2. Fall back to graph parent_candidates if manual config didn't resolve parent.
        // Only nest when the parent is in the same domain (matching router behavior).
        if resolved_parent_path_segment.is_none() {
            for pc in &self.parent_candidates {
                let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
                if child_name == stripped_title {
                    // Check if parent is in the same domain: either explicitly listed
                    // or its schema is classified into this domain.
                    let parent_in_domain = config
                        .domains
                        .get(&domain)
                        .map(|d| d.entities.contains(&pc.parent_title))
                        .unwrap_or(false)
                        || db
                            .get_schema(&pc.parent_title)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|s| s.domain.as_ref().map(|d| d == &domain))
                            .unwrap_or(false);
                    if !parent_in_domain {
                        // Parent is in another domain — keep FK column but don't nest
                        if parent_ref.is_none() {
                            parent_ref = Some(crate::generate::fk_column_for_candidate(pc, &config.defaults.type_suffix));
                        }
                        break;
                    }
                    let parent_name = super::router::strip_suffix(&pc.parent_title, &config.defaults.type_suffix);
                    if parent_ref.is_none() {
                        parent_ref = Some(crate::generate::fk_column_for_candidate(pc, &config.defaults.type_suffix));
                    }
                    if let Ok(Some(parent_schema)) = db.get_schema(&pc.parent_title).await {
                        resolved_parent_path_segment = Some(parent_schema.api_path_segment.clone());
                        resolved_parent_module_name = Some(parent_schema.pg_table_name.clone());
                        resolved_parent_domain = Some(domain.clone());
                    } else {
                        resolved_parent_path_segment = Some(codegraph_naming::to_kebab_case(parent_name));
                        resolved_parent_module_name = Some(codegraph_naming::to_snake_case(parent_name));
                        resolved_parent_domain = Some(domain.clone());
                    }
                    resolved_role = "child".to_string();
                    break;
                }
            }
        }

        // Check if this entity is a parent (graph detection)
        for pc in &self.parent_candidates {
            let parent_name = super::router::strip_suffix(&pc.parent_title, &config.defaults.type_suffix);
            if parent_name == stripped_title {
                let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
                if let Ok(Some(child_schema)) = db.get_schema(&pc.child_title).await {
                    resolved_children.push(ChildInfo {
                        entity_name: child_schema.rust_type_name.clone(),
                        module_name: child_schema.pg_table_name.clone(),
                        path_segment: child_schema.api_path_segment.clone(),
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

        // Fallback: if entity is a parent in config but wasn't matched by parent_candidates
        if resolved_children.is_empty() {
            if let Some(domain_entry) = config.domains.get(&domain) {
                // Iterate entity_config entries to find children of this entity
                {
                    for (other_title, other_cfg) in &domain_entry.entity_config {
                        if other_cfg.role.as_deref() == Some("child") {
                            if let Some(parent_title) = &other_cfg.parent {
                                if super::router::strip_suffix(parent_title, &config.defaults.type_suffix) == stripped_title
                                {
                                    let child_name = super::router::strip_suffix(other_title, &config.defaults.type_suffix);
                                    if let Ok(Some(child_schema)) = db.get_schema(other_title).await
                                    {
                                        resolved_children.push(ChildInfo {
                                            entity_name: child_schema.rust_type_name.clone(),
                                            module_name: child_schema.pg_table_name.clone(),
                                            path_segment: child_schema.api_path_segment.clone(),
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
            }
        }

        // Detect cross-aggregate entity references.
        let mut resolved_cross_refs: Vec<CrossRefInfo> = vec![];
        {
            let schema_title_with_type = format!("{}Type", entity_name);
            if let Ok(referenced) = db.get_referenced_schemas(&schema_title_with_type).await {
                let parent_name = entity_cfg.and_then(|ec| ec.parent.as_deref()).unwrap_or("");
                let child_names: std::collections::HashSet<&str> = resolved_children
                    .iter()
                    .map(|c| c.entity_name.as_str())
                    .collect();

                for ref_title in &referenced {
                    let ref_entity_name = super::router::strip_suffix(ref_title, &config.defaults.type_suffix);

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
                    if let Ok(Some(ref_schema)) = db.get_schema(ref_title).await {
                        if ref_schema.pg_table_name.is_empty() {
                            continue;
                        }
                        // Find domain of referenced entity
                        let ref_domain =
                            ref_schema.domain.clone().unwrap_or_else(|| domain.clone());
                        let fk_col = codegraph_naming::to_snake_case(ref_entity_name) + "_id";
                        let link_rel = codegraph_naming::to_snake_case(ref_entity_name);
                        resolved_cross_refs.push(CrossRefInfo {
                            entity_name: ref_entity_name.to_string(),
                            module_name: ref_schema.pg_table_name.clone(),
                            domain: ref_domain,
                            path_segment: ref_schema.api_path_segment.clone(),
                            fk_column: fk_col,
                            link_rel,
                        });
                    }
                }
            }
        }

        // Compute ancestor_path_params: how many {id} path segments Axum
        // accumulates before reaching this handler.  We walk up the resolved
        // parent chain, checking each ancestor's role in the domain it belongs to.
        let ancestor_path_params = if resolved_role == "child" {
            let parent_title_for_lookup =
                entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
                    self.parent_candidates
                        .iter()
                        .find(|pc| {
                            super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix) == stripped_title
                        })
                        .map(|pc| pc.parent_title.clone())
                });
            let mut depth = 1usize;
            if let Some(parent_title) = parent_title_for_lookup {
                // Prefer the current entity's domain for looking up the parent's config,
                // since the parent may exist in multiple domains with different roles.
                let parent_dom = if config
                    .domains
                    .get(&domain)
                    .map(|d| d.entities.contains(&parent_title))
                    .unwrap_or(false)
                {
                    domain.as_str()
                } else {
                    resolved_parent_domain.as_deref().unwrap_or(&domain)
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
                    let parent_is_child_in_graph = self
                        .parent_candidates
                        .iter()
                        .any(|pc| pc.child_title == parent_title);
                    if parent_is_child_in_config || parent_is_child_in_graph {
                        depth += 1;
                    }
                }
            }
            depth
        } else {
            0
        };

        let param_name = super::router::param_name_from_path_segment(&path_segment);
        let parent_param_name = resolved_parent_path_segment
            .as_deref()
            .map(super::router::param_name_from_path_segment);
        // For depth-2, resolve grandparent's param name, path segment, and domain.
        let (grandparent_param_name, grandparent_path_segment, grandparent_domain) =
            if ancestor_path_params >= 2 {
                let parent_title = entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
                    self.parent_candidates
                        .iter()
                        .find(|pc| {
                            super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix) == stripped_title
                        })
                        .map(|pc| pc.parent_title.clone())
                });
                let gp_title = parent_title.and_then(|pt| {
                    config
                        .domains
                        .get(&domain)
                        .and_then(|d| d.get_entity_config(&pt))
                        .and_then(|ec| ec.parent.clone())
                        .or_else(|| {
                            let parent_stripped = super::router::strip_suffix(&pt, &config.defaults.type_suffix);
                            self.parent_candidates
                                .iter()
                                .find(|pc| {
                                    super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix)
                                        == parent_stripped
                                })
                                .map(|pc| pc.parent_title.clone())
                        })
                });
                if let Some(ref gpt) = gp_title {
                    if let Ok(Some(gp_schema)) = db.get_schema(gpt).await {
                        let gp_seg = if !gp_schema.api_path_segment.is_empty() {
                            gp_schema.api_path_segment.clone()
                        } else {
                            codegraph_naming::to_kebab_case(super::router::strip_suffix(gpt, &config.defaults.type_suffix))
                        };
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
                            .unwrap_or_else(|| domain.clone());
                        (Some(gp_param), Some(gp_seg), Some(gp_domain))
                    } else {
                        let gp_name = super::router::strip_suffix(gpt, &config.defaults.type_suffix);
                        let gp_seg = codegraph_naming::to_kebab_case(gp_name);
                        let gp_param = super::router::param_name_from_path_segment(&gp_seg);
                        (Some(gp_param), Some(gp_seg), Some(domain.clone()))
                    }
                } else {
                    (None, None, None)
                }
            } else {
                (None, None, None)
            };

        let nested_filter_fields =
            resolve_nested_filter_fields(db, schema_title, &module_name, &domain, config).await?;

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
            parent_ref,
            parent_entity,
            parent_path_segment: resolved_parent_path_segment,
            parent_module_name: resolved_parent_module_name,
            parent_domain: resolved_parent_domain,
            role: resolved_role,
            param_name,
            parent_param_name,
            grandparent_param_name,
            grandparent_path_segment,
            grandparent_domain,
            children: resolved_children,
            cross_refs: resolved_cross_refs,
            status_field,
            has_workflow,
            has_fts,
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
        };

        let content = render_template(tera, "api/handler.tera", &ctx)?;
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
