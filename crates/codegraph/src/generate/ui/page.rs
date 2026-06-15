use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

use super::common::{collect_child_sections, collect_ui_fields};
use super::store::UiParentInfo;

#[derive(Debug, Serialize)]
pub struct UiPageContext {
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
    pub workflow_states: Vec<String>,
    pub initial_state: String,
    pub terminal_states: Vec<String>,
    pub has_approval_status: bool,
    pub has_fts: bool,
    pub fields: Vec<UiField>,
    pub list_fields: Vec<UiField>,
    pub child_sections: Vec<ChildSection>,
    pub has_child_sections: bool,
    /// Named path parameter for this entity's ID (e.g. `"worker_id"`).
    pub param_name: String,
    /// Set when this entity is a child nested under a parent.
    pub parent: Option<UiParentInfo>,
}

/// A sub-field definition for a StructuredWrapper type, embedded in UiField
/// at generation time. Consumed by the StructuredWrapperField Svelte component.
#[derive(Debug, Clone, Serialize)]
pub struct UiSubField {
    /// camelCase name from JSON schema (e.g. "schemeId")
    pub name: String,
    /// snake_case name (e.g. "scheme_id")
    pub snake_name: String,
    /// Human-readable label (e.g. "Scheme ID")
    pub label: String,
    pub is_required: bool,
    pub description: String,
    /// True for required fields and the first optional field (index < 2 in ordered result).
    /// Drives which sub-fields are visible before the expand toggle in the UI.
    pub show_by_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiField {
    pub name: String,
    pub label: String,
    pub ts_type: String,
    pub input_type: String,
    pub is_required: bool,
    pub is_array: bool,
    pub is_entity_ref: bool,
    pub is_immutable: bool,
    pub is_codelist: bool,
    pub is_range: bool,
    pub codelist_values: Vec<String>,
    pub description: String,
    /// The Postgres column type (e.g., "TEXT", "TSTZRANGE", "TEXT[]").
    pub pg_type: String,
    /// Whether the range end is open (unbounded upper bound).
    pub open_end: bool,
    /// For entity_ref fields: the full API path prefix (e.g., "/common/organization")
    pub ref_api_path: Option<String>,
    /// Non-empty when this field is a StructuredWrapper (e.g. IdentifierType).
    /// Contains sub-field definitions queried from the graph at generation time.
    pub structured_sub_fields: Vec<UiSubField>,
    /// When set, this field is a nested ValueObject referencing another type.
    /// The `ts_type` holds the nested interface name (e.g. "WorkerPersonLegalResponse").
    #[serde(default)]
    pub nested_type_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChildSection {
    pub entity_name: String,
    pub module_name: String,
    pub label: String,
    pub path_segment: String,
    pub domain: String,
    pub has_children: bool,
    pub fields: Vec<UiField>,
}

pub struct UiPageGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<codegraph_core::types::ParentCandidate>,
}

impl UiPageGenerator {
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
impl EntityGenerator for UiPageGenerator {
    fn name(&self) -> &str {
        "ui-page"
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

        let dto_config = entity_cfg.map(|ec| &ec.dto);
        let immutable_fields: Vec<String> = dto_config
            .map(|d| d.immutable_fields.clone())
            .unwrap_or_default();
        let list_exclude: Vec<String> = dto_config
            .map(|d| d.list_exclude.clone())
            .unwrap_or_default();
        let list_include: Vec<String> = dto_config
            .map(|d| d.list_include.clone())
            .unwrap_or_default();

        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let has_workflow = workflow
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);
        let workflow_states = workflow.map(|wf| wf.states.clone()).unwrap_or_default();
        let initial_state = workflow
            .map(|wf| wf.initial_state.clone())
            .unwrap_or_default();
        let terminal_states = workflow
            .map(|wf| wf.terminal_states.clone())
            .unwrap_or_default();
        let has_approval_status = workflow
            .and_then(|wf| wf.approval_status_field.as_ref())
            .is_some();

        let has_fts = entity_cfg
            .map(|ec| {
                !ec.search.fts_weights.is_empty()
                    || ec
                        .search
                        .fts_columns
                        .as_ref()
                        .map(|c| !c.is_empty())
                        .unwrap_or(false)
            })
            .unwrap_or(false);

        let fields = collect_ui_fields(db, schema_title, &immutable_fields, Some(&domain)).await?;

        // Build list fields: if list_include is set, use those; otherwise all fields minus list_exclude
        let list_fields = if !list_include.is_empty() {
            fields
                .iter()
                .filter(|f| list_include.contains(&f.name))
                .cloned()
                .collect()
        } else {
            fields
                .iter()
                .filter(|f| !list_exclude.contains(&f.name))
                .cloned()
                .collect()
        };

        let child_sections = collect_child_sections(db, schema_title, config, &domain).await?;
        let has_child_sections = !child_sections.is_empty();

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
                            let gp = super::store::resolve_grandparent(
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
                            let gp = super::store::resolve_grandparent(
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

        let ctx = UiPageContext {
            entity_name,
            module_name,
            domain: domain.clone(),
            path_segment: path_segment.clone(),
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_list: operations.contains(&"list".to_string()),
            has_workflow,
            workflow_states,
            initial_state,
            terminal_states,
            has_approval_status,
            has_fts,
            fields,
            list_fields,
            child_sections,
            has_child_sections,
            param_name: crate::generate::api::router::param_name_from_path_segment(&path_segment),
            parent: parent.clone(),
        };

        let routes_base = self
            .output_dir
            .join("ui")
            .join("src")
            .join("routes")
            .join("(app)");
        let routes_dir = if let Some(ref p) = parent {
            if let Some(ref gp) = p.grandparent {
                // Depth-2: grandparent/[gp_param]/parent/[parent_param]/child
                routes_base
                    .join(&gp.domain)
                    .join(&gp.path_segment)
                    .join(format!("[{}]", gp.param_name))
                    .join(&p.path_segment)
                    .join(format!("[{}]", p.param_name))
                    .join(&path_segment)
            } else {
                routes_base
                    .join(&p.domain)
                    .join(&p.path_segment)
                    .join(format!("[{}]", p.param_name))
                    .join(&path_segment)
            }
        } else {
            routes_base.join(&domain).join(&path_segment)
        };

        let mut files = Vec::new();

        // List page
        if ctx.has_list {
            let content = render_template_with_project(tera, "ui/list_page.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: routes_dir.join("+page.svelte"),
                content,
            });
            let load = render_template_with_project(tera, "ui/list_load.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: routes_dir.join("+page.server.ts"),
                content: load,
            });
        }

        // Detail page
        if ctx.has_read {
            let content = render_template_with_project(tera, "ui/detail_page.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: routes_dir
                    .join(format!("[{}]", ctx.param_name))
                    .join("+page.svelte"),
                content,
            });
            let load = render_template_with_project(tera, "ui/detail_load.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: routes_dir
                    .join(format!("[{}]", ctx.param_name))
                    .join("+page.server.ts"),
                content: load,
            });
        }

        // Create page
        if ctx.has_create {
            let content = render_template_with_project(tera, "ui/form_page.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: routes_dir.join("new").join("+page.svelte"),
                content,
            });
        }

        // Edit page
        if ctx.has_update {
            let content = render_template_with_project(tera, "ui/edit_page.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: routes_dir
                    .join(format!("[{}]", ctx.param_name))
                    .join("edit")
                    .join("+page.svelte"),
                content,
            });
            let load = render_template_with_project(tera, "ui/edit_load.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: routes_dir
                    .join(format!("[{}]", ctx.param_name))
                    .join("edit")
                    .join("+page.server.ts"),
                content: load,
            });
        }

        Ok(files)
    }
}
