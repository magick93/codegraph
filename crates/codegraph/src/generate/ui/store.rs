use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

/// Parent entity metadata exposed to the store template.
#[derive(Debug, Clone, Serialize)]
pub struct UiParentInfo {
    pub domain: String,
    pub path_segment: String,
    pub module_name: String,
    pub entity_name: String,
    /// Named path parameter for the parent's ID (e.g. `"worker_id"`).
    pub param_name: String,
    /// When the parent is itself a child, this holds the grandparent info.
    pub grandparent: Option<Box<UiGrandparentInfo>>,
}

/// Grandparent entity metadata for depth-2 nested routes.
#[derive(Debug, Clone, Serialize)]
pub struct UiGrandparentInfo {
    pub domain: String,
    pub path_segment: String,
    pub entity_name: String,
    /// Named path parameter for the grandparent's ID (e.g. `"timecard_id"`).
    pub param_name: String,
}

#[derive(Debug, Serialize)]
pub struct UiStoreContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub has_workflow: bool,
    /// Set when this entity is a child nested under a parent.
    pub parent: Option<UiParentInfo>,
}

/// Resolve grandparent info when a parent entity is itself a child.
///
/// Checks manual config first, then graph `parent_candidates`.
pub async fn resolve_grandparent(
    parent_title: &str,
    domain: &str,
    config: &DomainConfig,
    parent_candidates: &[ParentCandidate],
    db: &dyn GraphQuerier,
) -> Option<UiGrandparentInfo> {
    // 1. Manual config: check if parent has role=child with its own parent
    if let Some(parent_cfg) = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(parent_title))
    {
        if parent_cfg.role.as_deref() == Some("child") {
            if let Some(ref gp_title) = parent_cfg.parent {
                if let Ok(Some(gp_schema)) = db.get_schema(gp_title).await {
                    let gp_domain = if config
                        .domains
                        .get(domain)
                        .map(|d| d.entities.contains(gp_title))
                        .unwrap_or(false)
                    {
                        domain.to_string()
                    } else {
                        gp_schema
                            .domain
                            .clone()
                            .unwrap_or_else(|| domain.to_string())
                    };
                    return Some(UiGrandparentInfo {
                        param_name: crate::generate::api::router::param_name_from_path_segment(
                            &gp_schema.api_path_segment,
                        ),
                        domain: gp_domain,
                        path_segment: gp_schema.api_path_segment.clone(),
                        entity_name: gp_schema.rust_type_name.clone(),
                    });
                }
            }
        }
    }

    // 2. Graph: check if parent_title appears as a child in parent_candidates
    let parent_stripped = crate::generate::api::router::strip_suffix(parent_title, &config.defaults.type_suffix);
    for pc in parent_candidates {
        let child_name = crate::generate::api::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
        if child_name == parent_stripped {
            // Check same-domain
            let in_explicit = config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(&pc.parent_title))
                .unwrap_or(false);
            let in_same_domain = in_explicit
                || db
                    .get_schema(&pc.parent_title)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|s| s.domain.as_ref().map(|d| d == domain))
                    .unwrap_or(false);
            if !in_same_domain {
                return None;
            }
            if let Ok(Some(gp_schema)) = db.get_schema(&pc.parent_title).await {
                return Some(UiGrandparentInfo {
                    param_name: crate::generate::api::router::param_name_from_path_segment(
                        &gp_schema.api_path_segment,
                    ),
                    domain: domain.to_string(),
                    path_segment: gp_schema.api_path_segment.clone(),
                    entity_name: gp_schema.rust_type_name.clone(),
                });
            }
            break;
        }
    }
    None
}

pub struct UiStoreGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
}

impl UiStoreGenerator {
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
impl EntityGenerator for UiStoreGenerator {
    fn name(&self) -> &str {
        "ui-store"
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

        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let has_workflow = workflow
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);

        // Resolve parent info for child entities.
        // Manual config takes priority over graph detection.
        let parent = {
            let stripped = crate::generate::api::router::strip_suffix(schema_title, &config.defaults.type_suffix);
            let mut result = None;

            // 1. Check manual config first
            if let Some(ec) = config
                .domains
                .get(&domain)
                .and_then(|d| d.get_entity_config(schema_title))
            {
                if ec.role.as_deref() == Some("child") {
                    if let Some(ref parent_title) = ec.parent {
                        if let Ok(Some(parent_schema)) = db.get_schema(parent_title).await {
                            let parent_domain = if config
                                .domains
                                .get(&domain)
                                .map(|d| d.entities.contains(parent_title))
                                .unwrap_or(false)
                            {
                                domain.clone()
                            } else {
                                parent_schema
                                    .domain
                                    .clone()
                                    .unwrap_or_else(|| domain.clone())
                            };
                            let gp = resolve_grandparent(
                                parent_title,
                                &domain,
                                config,
                                &self.parent_candidates,
                                db,
                            )
                            .await
                            .map(Box::new);
                            result = Some(UiParentInfo {
                                param_name:
                                    crate::generate::api::router::param_name_from_path_segment(
                                        &parent_schema.api_path_segment,
                                    ),
                                domain: parent_domain,
                                path_segment: parent_schema.api_path_segment.clone(),
                                module_name: parent_schema.pg_table_name.clone(),
                                entity_name: parent_schema.rust_type_name.clone(),
                                grandparent: gp,
                            });
                        }
                    }
                }
            }

            // 2. Fall back to graph parent_candidates
            if result.is_none() {
                for pc in &self.parent_candidates {
                    let child_name =
                        crate::generate::api::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
                    if child_name == stripped {
                        let in_explicit = config
                            .domains
                            .get(&domain)
                            .map(|d| d.entities.contains(&pc.parent_title))
                            .unwrap_or(false);
                        let parent_in_domain = in_explicit
                            || db
                                .get_schema(&pc.parent_title)
                                .await
                                .ok()
                                .flatten()
                                .and_then(|s| s.domain.as_ref().map(|d| d == &domain))
                                .unwrap_or(false);
                        if !parent_in_domain {
                            break;
                        }
                        if let Ok(Some(parent_schema)) = db.get_schema(&pc.parent_title).await {
                            let gp = resolve_grandparent(
                                &pc.parent_title,
                                &domain,
                                config,
                                &self.parent_candidates,
                                db,
                            )
                            .await
                            .map(Box::new);
                            result = Some(UiParentInfo {
                                param_name:
                                    crate::generate::api::router::param_name_from_path_segment(
                                        &parent_schema.api_path_segment,
                                    ),
                                domain: domain.clone(),
                                path_segment: parent_schema.api_path_segment.clone(),
                                module_name: parent_schema.pg_table_name.clone(),
                                entity_name: parent_schema.rust_type_name.clone(),
                                grandparent: gp,
                            });
                        }
                        break;
                    }
                }
            }
            result
        };

        let ctx = UiStoreContext {
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            path_segment,
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_list: operations.contains(&"list".to_string()),
            has_workflow,
            parent,
        };

        let content = render_template_with_project(tera, "ui/entity_store.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("ui")
                .join("src")
                .join("lib")
                .join("stores")
                .join(format!("{}_{}.ts", domain, module_name)),
            content,
        }])
    }
}
