use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct WorkflowActionContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub tag: String,
    pub has_approval_status: bool,
    pub status_field: String,
    pub approval_status_field: Option<String>,
    pub initial_state: String,
    pub terminal_states: Vec<String>,
    pub states: Vec<String>,
    /// State transition entries for guard validation.
    pub transitions: Vec<TransitionEntry>,
    /// Whether transition guards are defined (non-empty transitions map).
    pub has_transition_guards: bool,
    /// Dual-status guard entries (status requires approval state).
    pub dual_status_guards: Vec<DualStatusGuard>,
    /// Whether dual-status guards are defined.
    pub has_dual_status_guards: bool,
    /// Entity role: "child" when nested under a parent.
    pub role: String,
    /// Number of ancestor path params (1 for depth-1 child, 2 for depth-2).
    pub ancestor_path_params: usize,
    /// Named path parameter for this entity's ID (e.g. `"worker_id"`).
    pub param_name: String,
    /// Named path parameter for the parent's ID (child entities only).
    pub parent_param_name: Option<String>,
    /// Path segment for the parent entity (child entities only, e.g. `"orders"`).
    pub parent_path_segment: Option<String>,
    /// Domain of the parent entity (child entities only).
    pub parent_domain: Option<String>,
}

/// A single transition rule for template rendering.
#[derive(Debug, Serialize)]
pub struct TransitionEntry {
    pub from_state: String,
    pub to_states: Vec<String>,
}

/// A dual-status guard entry.
#[derive(Debug, Serialize)]
pub struct DualStatusGuard {
    pub status_value: String,
    pub required_approval_state: String,
}

pub struct WorkflowActionGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<codegraph_core::types::ParentCandidate>,
}

impl WorkflowActionGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
        }
    }

    pub fn with_parent_candidates(
        mut self,
        candidates: Vec<codegraph_core::types::ParentCandidate>,
    ) -> Self {
        self.parent_candidates = candidates;
        self
    }
}

#[async_trait]
impl EntityGenerator for WorkflowActionGenerator {
    fn name(&self) -> &str {
        "workflow_action"
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

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        // Check for workflow config — skip if not configured
        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(schema_title));

        let workflow = match entity_cfg.and_then(|ec| ec.workflow.as_ref()) {
            Some(wf) if wf.generate_action_endpoints => wf,
            _ => return Ok(Vec::new()),
        };

        let tag = entity_cfg
            .and_then(|ec| ec.tag.clone())
            .unwrap_or_else(|| entity_name.clone());

        // Build sorted transition entries from the transitions map
        let mut transitions: Vec<TransitionEntry> = workflow
            .transitions
            .iter()
            .map(|(from, to)| TransitionEntry {
                from_state: from.clone(),
                to_states: to.clone(),
            })
            .collect();
        transitions.sort_by(|a, b| a.from_state.cmp(&b.from_state));

        let has_transition_guards = !transitions.is_empty();

        // Build dual-status guard entries
        let mut dual_status_guards: Vec<DualStatusGuard> = workflow
            .dual_status_guards
            .iter()
            .map(|(status, approval)| DualStatusGuard {
                status_value: status.clone(),
                required_approval_state: approval.clone(),
            })
            .collect();
        dual_status_guards.sort_by(|a, b| a.status_value.cmp(&b.status_value));

        let has_dual_status_guards = !dual_status_guards.is_empty();

        // Determine role and ancestor_path_params (same logic as handler generator)
        let stripped_title = super::router::strip_suffix(schema_title, &config.defaults.type_suffix);
        let mut resolved_role = entity_cfg
            .and_then(|ec| ec.role.clone())
            .unwrap_or_else(|| "root".into());

        // Check manual config for child role
        if entity_cfg
            .map(|ec| ec.role.as_deref() == Some("child"))
            .unwrap_or(false)
        {
            resolved_role = "child".to_string();
        }

        // Fall back to graph parent_candidates
        if resolved_role != "child" {
            for pc in &self.parent_candidates {
                let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
                if child_name == stripped_title {
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
                    if parent_in_domain {
                        resolved_role = "child".to_string();
                    }
                    break;
                }
            }
        }

        let ancestor_path_params = if resolved_role == "child" {
            // Check if the parent is itself a child (depth-2)
            let parent_title = entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
                self.parent_candidates.iter().find_map(|pc| {
                    let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
                    if child_name == stripped_title {
                        Some(pc.parent_title.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(ref pt) = parent_title {
                let parent_is_child = config
                    .domains
                    .get(&domain)
                    .and_then(|d| d.get_entity_config(pt))
                    .map(|ec| ec.role.as_deref() == Some("child"))
                    .unwrap_or(false)
                    || self.parent_candidates.iter().any(|pc| {
                        let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
                        let parent_name = super::router::strip_suffix(pt, &config.defaults.type_suffix);
                        child_name == parent_name
                            && config
                                .domains
                                .get(&domain)
                                .map(|d| d.entities.contains(&pc.parent_title))
                                .unwrap_or(false)
                    });
                if parent_is_child {
                    2
                } else {
                    1
                }
            } else {
                1
            }
        } else {
            0
        };

        let param_name = super::router::param_name_from_path_segment(&schema.api_path_segment);
        // Resolve parent param name, path segment, and domain for child entities.
        let (parent_param_name, parent_path_segment, parent_domain) = if resolved_role == "child" {
            let pt = entity_cfg.and_then(|ec| ec.parent.clone()).or_else(|| {
                self.parent_candidates.iter().find_map(|pc| {
                    let child_name = super::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
                    if child_name == stripped_title {
                        Some(pc.parent_title.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(ref pt) = pt {
                if let Ok(Some(parent_schema)) = db.get_schema(pt).await {
                    let seg = if !parent_schema.api_path_segment.is_empty() {
                        parent_schema.api_path_segment.clone()
                    } else {
                        codegraph_naming::to_kebab_case(super::router::strip_suffix(pt, &config.defaults.type_suffix))
                    };
                    let param = super::router::param_name_from_path_segment(&seg);
                    let pdomain = parent_schema
                        .domain
                        .clone()
                        .unwrap_or_else(|| domain.clone());
                    (Some(param), Some(seg), Some(pdomain))
                } else {
                    let pn = super::router::strip_suffix(pt, &config.defaults.type_suffix);
                    let seg = codegraph_naming::to_kebab_case(pn);
                    let param = super::router::param_name_from_path_segment(&seg);
                    (Some(param), Some(seg), Some(domain.clone()))
                }
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        let ctx = WorkflowActionContext {
            entity_name: entity_name.clone(),
            module_name: module_name.clone(),
            domain: domain.clone(),
            path_segment: schema.api_path_segment.clone(),
            tag,
            has_approval_status: workflow.approval_status_field.is_some(),
            status_field: workflow.status_field.clone(),
            approval_status_field: workflow.approval_status_field.clone(),
            initial_state: workflow.initial_state.clone(),
            terminal_states: workflow.terminal_states.clone(),
            states: workflow.states.clone(),
            transitions,
            has_transition_guards,
            dual_status_guards,
            has_dual_status_guards,
            role: resolved_role,
            ancestor_path_params,
            param_name,
            parent_param_name,
            parent_path_segment,
            parent_domain,
        };

        let content = render_template(tera, "api/workflow_action.tera", &ctx)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("api")
                .join(&domain)
                .join(format!("{}_workflow.rs", module_name)),
            content,
        }])
    }
}
