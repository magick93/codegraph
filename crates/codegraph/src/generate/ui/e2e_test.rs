use crate::generate::ProjectConfig;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

use super::common::{collect_child_sections, collect_ui_fields};
use super::page::{ChildSection, UiField};
use super::store::UiParentInfo;

#[derive(Debug, Serialize)]
pub struct EntityRefDep {
    /// The field name in the form (e.g., "order_id")
    pub field_name: String,
    /// The API path for the referenced entity (e.g., "/assessments/order")
    pub api_path: String,
    /// JSON object literal (without outer braces) for minimal valid creation payload
    /// e.g. `'language': 'aa', 'name': 'Test Dep'`
    pub test_data_json: String,
}

/// Configuration for generated include E2E tests.
#[derive(Debug, Serialize)]
pub struct E2eIncludeConfig {
    /// Entity creation steps, ordered by dependency (deps first, main last).
    pub setup_steps: Vec<IncludeSetupStep>,
    /// depIds key of the main entity (last step).
    pub main_entity_id_ref: String,
    /// Include paths to test via get_by_id.
    pub test_paths: Vec<IncludeTestPath>,
    /// Whether multiple single-segment paths exist (for multi-include test).
    pub has_multi_include: bool,
    /// Whether to generate list-with-include tests.
    pub test_list_include: bool,
}

/// One entity creation step for include test setup.
#[derive(Debug, Serialize)]
pub struct IncludeSetupStep {
    /// depIds key, e.g. "person" or "candidate"
    pub dep_id: String,
    /// API path for createEntityAsAcme, e.g. "/api/common/person"
    pub api_path: String,
    /// JS object entries for required fields (without outer braces)
    pub fields_json: String,
    /// FK mappings: (field_on_this_entity, depId_of_target)
    pub fk_map: Vec<[String; 2]>,
}

/// An include path to test.
#[derive(Debug, Serialize)]
pub struct IncludeTestPath {
    /// Query parameter value, e.g. "person" or "deployment.position"
    pub alias: String,
    /// depIds key of the target (last segment) entity
    pub target_dep_id: String,
    /// Whether this is a dot-notation path
    pub is_dot_path: bool,
    /// Whether the relationship is 1:many
    pub is_array: bool,
}

#[derive(Debug, Serialize)]
pub struct UiE2eTestContext {
    pub entity_name: String,
    pub entity_label: String,
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
    pub fields: Vec<UiField>,
    pub create_fields: Vec<UiField>,
    pub required_create_fields: Vec<UiField>,
    pub update_fields: Vec<UiField>,
    pub first_list_column: Option<String>,
    pub has_fts: bool,
    pub fts_search_field: String,
    /// Entity reference dependencies that must be created before the main entity
    pub entity_ref_deps: Vec<EntityRefDep>,
    pub has_entity_ref_deps: bool,
    /// Child sections (child entities) displayed on the detail page
    pub child_sections: Vec<ChildSection>,
    pub has_child_sections: bool,
    /// Named path parameter for this entity's ID (e.g. `"worker_id"`).
    pub param_name: String,
    /// Set when this entity is a child nested under a parent.
    pub parent: Option<UiParentInfo>,
    /// JS object literal body for creating the parent entity via API (only set when parent is Some)
    pub parent_test_data_json: String,
    /// JS object literal for creating the grandparent entity (only set for depth-2 nesting)
    pub grandparent_test_data_json: String,
    /// Include E2E test configuration. None when include is not configured.
    pub e2e_include: Option<E2eIncludeConfig>,
}

pub struct UiE2eTestGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<codegraph_core::types::ParentCandidate>,
}

impl UiE2eTestGenerator {
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

    /// Resolve grandparent info for a given parent entity (depth-2 nesting).
    /// Checks manual config first, then graph parent_candidates.
    async fn resolve_grandparent(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        parent_title: &str,
        parent_domain: &str,
    ) -> Option<Box<super::store::UiGrandparentInfo>> {
        let parent_stripped = crate::generate::api::router::strip_suffix(parent_title, &config.defaults.type_suffix);

        // 1. Check manual config for parent's parent
        if let Some(parent_ec) = config
            .domains
            .get(parent_domain)
            .and_then(|d| d.get_entity_config(parent_title))
        {
            if parent_ec.role.as_deref() == Some("child") {
                if let Some(ref gp_title) = parent_ec.parent {
                    if let Ok(Some(gp_schema)) = db.get_schema_in_domain(gp_title, parent_domain).await {
                        let gp_domain = if config
                            .domains
                            .get(parent_domain)
                            .map(|d| d.entities.contains(gp_title))
                            .unwrap_or(false)
                        {
                            parent_domain.to_string()
                        } else {
                            gp_schema
                                .domain
                                .clone()
                                .unwrap_or_else(|| parent_domain.to_string())
                        };
                        return Some(Box::new(super::store::UiGrandparentInfo {
                            param_name: crate::generate::api::router::param_name_from_path_segment(
                                &gp_schema.api_path_segment,
                            ),
                            domain: gp_domain,
                            path_segment: gp_schema.api_path_segment.clone(),
                            entity_name: gp_schema.rust_type_name.clone(),
                        }));
                    }
                }
            } else if parent_ec.role.as_deref() == Some("root") {
                return None; // Explicitly root — no grandparent
            }
        }

        // 2. Check graph parent_candidates
        for gpc in &self.parent_candidates {
            let gpc_child = crate::generate::api::router::strip_suffix(&gpc.child_title, &config.defaults.type_suffix);
            if gpc_child == parent_stripped {
                if let Ok(Some(gp_schema)) = db.get_schema_in_domain(&gpc.parent_title, parent_domain).await {
                    let gp_domain = if config
                        .domains
                        .get(parent_domain)
                        .map(|d| d.entities.contains(&gpc.parent_title))
                        .unwrap_or(false)
                    {
                        parent_domain.to_string()
                    } else {
                        gp_schema
                            .domain
                            .clone()
                            .unwrap_or_else(|| parent_domain.to_string())
                    };
                    return Some(Box::new(super::store::UiGrandparentInfo {
                        param_name: crate::generate::api::router::param_name_from_path_segment(
                            &gp_schema.api_path_segment,
                        ),
                        domain: gp_domain,
                        path_segment: gp_schema.api_path_segment.clone(),
                        entity_name: gp_schema.rust_type_name.clone(),
                    }));
                }
                break;
            }
        }

        None
    }
}

#[async_trait]
impl EntityGenerator for UiE2eTestGenerator {
    fn name(&self) -> &str {
        "ui-e2e-test"
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
        let entity_label = codegraph_naming::to_display_name(&config.defaults.strip_suffix(&schema.title));
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

        // Workflow-excluded fields for create/update
        let mut all_excluded: Vec<String> = immutable_fields.clone();
        if let Some(wf) = workflow {
            all_excluded.push(wf.status_field.clone());
            if let Some(ref approval_field) = wf.approval_status_field {
                all_excluded.push(approval_field.clone());
            }
        }

        let fields = collect_ui_fields(db, schema_title, &immutable_fields, Some(&domain)).await?;

        let mut create_fields: Vec<UiField> = fields
            .iter()
            .filter(|f| !all_excluded.contains(&f.name))
            .cloned()
            .collect();
        // For codelist entities with no UI fields (enum-only schemas), inject a
        // synthetic code field so testData() produces a valid create payload.
        if create_fields.is_empty() {
            if let Ok(Some(schema)) = db.get_schema_in_domain(schema_title, &domain).await {
                if schema.is_codelist && domain == "common" {
                    create_fields.push(UiField {
                        name: "code".to_string(),
                        label: "Code".to_string(),
                        ts_type: "string".to_string(),
                        input_type: "code".to_string(),
                        is_required: true,
                        is_array: false,
                        is_entity_ref: false,
                        is_immutable: false,
                        is_codelist: false,
                        is_range: false,
                        codelist_values: vec![],
                        description: String::new(),
                        pg_type: "TEXT".to_string(),
                        open_end: false,
                        ref_api_path: None,
                        structured_sub_fields: vec![],
                        nested_type_name: None,
                    });
                }
            }
        }

        let required_create_fields: Vec<UiField> = create_fields
            .iter()
            .filter(|f| f.is_required)
            .cloned()
            .collect();

        let update_fields: Vec<UiField> = fields
            .iter()
            .filter(|f| !f.is_immutable && !all_excluded.contains(&f.name))
            .cloned()
            .collect();

        let first_list_column = fields.first().map(|f| f.name.clone());

        // Determine FTS availability and search field
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

        let fts_search_field = if has_fts {
            // Pick the field with the highest FTS weight (A > B > C > D)
            let weights = &entity_cfg.unwrap().search.fts_weights;
            let weight_order = ["A", "B", "C", "D"];
            weight_order
                .iter()
                .find_map(|w| {
                    weights
                        .iter()
                        .find(|(_, v)| v.as_str() == *w)
                        .map(|(k, _)| k.clone())
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        let has_create = operations.contains(&"create".to_string());
        let has_read = operations.contains(&"read".to_string());
        let has_update = operations.contains(&"update".to_string());
        let has_delete = operations.contains(&"delete".to_string());
        let has_list = operations.contains(&"list".to_string());

        // Collect entity reference dependencies (deduplicated) with minimal test data
        let mut entity_ref_deps = Vec::new();
        let mut seen_deps = std::collections::HashSet::new();
        // Get all properties so we can find ref_target for each entity ref field
        let all_props = match Some(domain.as_str()) {
            Some(d) => db.get_properties_in_domain(schema_title, d).await?,
            None => db.get_properties(schema_title).await?,
        };
        for field in &create_fields {
            if field.is_entity_ref {
                if let Some(ref api_path) = field.ref_api_path {
                    if seen_deps.insert(field.name.clone()) {
                        let test_data_json =
                            build_dep_test_data(db, &all_props, &field.name, Some(&domain)).await;
                        entity_ref_deps.push(EntityRefDep {
                            field_name: field.name.clone(),
                            api_path: api_path.clone(),
                            test_data_json,
                        });
                    }
                }
            }
        }
        let has_entity_ref_deps = !entity_ref_deps.is_empty();

        // Resolve include test config
        let e2e_include = if let Some(ec) = entity_cfg {
            if ec.allow_include.as_ref().map_or(false, |v| !v.is_empty()) {
                let resolved = crate::generate::api::include_path::resolve_include_paths(
                    db, config, &domain, schema_title, ec.allow_include.as_ref(),
                ).await?;
                resolve_e2e_include_config(db, config, &domain, schema_title, &resolved, has_list).await?
            } else {
                None
            }
        } else {
            None
        };

        // Collect child sections for detail page testing
        let child_sections = collect_child_sections(db, schema_title, config, &domain).await?;
        let has_child_sections = !child_sections.is_empty();

        // Resolve parent info for child entities.
        // Manual config (role = "child", parent = "...") takes priority over graph detection.
        let (parent, parent_title_for_data) = {
            let stripped = crate::generate::api::router::strip_suffix(schema_title, &config.defaults.type_suffix);
            let mut result = None;
            let mut parent_title_str = String::new();

            // 1. Check manual config first
            if let Some(ec) = config
                .domains
                .get(&domain)
                .and_then(|d| d.get_entity_config(schema_title))
            {
                if ec.role.as_deref() == Some("child") {
                    if let Some(ref parent_title) = ec.parent {
                        if let Ok(Some(parent_schema)) = db.get_schema_in_domain(parent_title, &domain).await {
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

                            let grandparent = self
                                .resolve_grandparent(db, config, parent_title, &parent_domain)
                                .await;

                            parent_title_str = parent_title.clone();
                            result = Some(UiParentInfo {
                                param_name:
                                    crate::generate::api::router::param_name_from_path_segment(
                                        &parent_schema.api_path_segment,
                                    ),
                                domain: parent_domain,
                                path_segment: parent_schema.api_path_segment.clone(),
                                module_name: parent_schema.pg_table_name.clone(),
                                entity_name: parent_schema.rust_type_name.clone(),
                                grandparent,
                            });
                        }
                    }
                }
            }

            // 2. Fall back to graph parent_candidates (only if parent is in same domain,
            //    and the entity is not explicitly/implicitly configured as root).
            let effective_role = entity_cfg
                .and_then(|ec| ec.role.as_deref())
                .unwrap_or("root");
            if result.is_none() && effective_role != "root" {
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
                            .get_schema_in_domain(&pc.parent_title, &domain)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|s| s.domain.as_ref().map(|d| *d == domain))
                            .unwrap_or(false);
                        if !parent_in_domain {
                            break;
                        }
                        if let Ok(Some(parent_schema)) = db.get_schema_in_domain(&pc.parent_title, &domain).await {
                            let parent_domain = domain.clone();

                            let grandparent = self
                                .resolve_grandparent(db, config, &pc.parent_title, &parent_domain)
                                .await;

                            parent_title_str = pc.parent_title.clone();
                            result = Some(UiParentInfo {
                                param_name:
                                    crate::generate::api::router::param_name_from_path_segment(
                                        &parent_schema.api_path_segment,
                                    ),
                                domain: parent_domain,
                                path_segment: parent_schema.api_path_segment.clone(),
                                module_name: parent_schema.pg_table_name.clone(),
                                entity_name: parent_schema.rust_type_name.clone(),
                                grandparent,
                            });
                        }
                        break;
                    }
                }
            }
            (result, parent_title_str)
        };

        // Build parent entity test data for creating parent in beforeAll
        let parent_test_data_json = if parent.is_some() && !parent_title_for_data.is_empty() {
            let parent_domain = parent.as_ref().map(|p| p.domain.as_str());
            build_test_data_json(db, &parent_title_for_data, parent_domain).await
        } else {
            String::new()
        };

        // Build grandparent entity test data for depth-2 nesting
        let grandparent_test_data_json = if let Some(ref p) = parent {
            if let Some(ref gp) = p.grandparent {
                // Find grandparent's schema title from parent_candidates
                let parent_stripped =
                    crate::generate::api::router::strip_suffix(&parent_title_for_data, &config.defaults.type_suffix);
                let mut gp_title = String::new();
                for gpc in &self.parent_candidates {
                    let gpc_child =
                        crate::generate::api::router::strip_suffix(&gpc.child_title, &config.defaults.type_suffix);
                    if gpc_child == parent_stripped {
                        gp_title = gpc.parent_title.clone();
                        break;
                    }
                }
                if !gp_title.is_empty() {
                    build_test_data_json(db, &gp_title, Some(&gp.domain)).await
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let ctx = UiE2eTestContext {
            entity_name,
            entity_label,
            module_name: module_name.clone(),
            domain: domain.clone(),
            path_segment: path_segment.clone(),
            has_create,
            has_read,
            has_update,
            has_delete,
            has_list,
            has_workflow,
            workflow_states,
            initial_state,
            terminal_states,
            fields,
            create_fields,
            required_create_fields,
            update_fields,
            first_list_column,
            has_fts,
            fts_search_field,
            entity_ref_deps,
            has_entity_ref_deps,
            child_sections,
            has_child_sections,
            param_name: crate::generate::api::router::param_name_from_path_segment(&path_segment),
            parent,
            parent_test_data_json,
            grandparent_test_data_json,
            e2e_include,
        };

        let tests_dir = self
            .output_dir
            .join("ui")
            .join("tests")
            .join("generated")
            .join(&domain);

        let mut files = Vec::new();

        // CRUD test
        if has_list || has_create || has_read || has_update || has_delete {
            let content = render_template_with_project(tera, "ui/test/crud.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.api.crud.test.ts", path_segment)),
                content,
            });
        }

        // Validation test
        if has_create {
            let content = render_template_with_project(tera, "ui/test/validation.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.validation.test.ts", path_segment)),
                content,
            });
        }

        // Workflow test
        if has_workflow {
            let content = render_template_with_project(tera, "ui/test/workflow.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.workflow.test.ts", path_segment)),
                content,
            });
        }

        // Persona-based tests
        if has_list || has_create || has_read || has_update || has_delete {
            let content = render_template_with_project(tera, "ui/test/owner_crud.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.owner.crud.test.ts", path_segment)),
                content,
            });

            let content = render_template_with_project(tera, "ui/test/employee_view.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.employee.view.test.ts", path_segment)),
                content,
            });

            let content = render_template_with_project(tera, "ui/test/manager_team.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.manager.team.test.ts", path_segment)),
                content,
            });

            let content = render_template_with_project(tera, "ui/test/isolation.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.isolation.test.ts", path_segment)),
                content,
            });
        }

        // Search tests (only for FTS-enabled entities)
        if has_fts && has_list {
            let content = render_template_with_project(tera, "ui/test/search.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.search.test.ts", path_segment)),
                content,
            });

            let content = render_template_with_project(tera, "ui/test/search_isolation.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.search.isolation.test.ts", path_segment)),
                content,
            });
        }

        // Include test
        if ctx.e2e_include.is_some() && has_read {
            let content = render_template_with_project(tera, "ui/test/include.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.include.test.ts", path_segment)),
                content,
            });
        }

        Ok(files)
    }
}

/// Build a JS object-literal body (without outer braces) containing minimal valid
/// test data for creating a dependency entity. Returns empty string if we can't
/// resolve the dep's fields.
async fn build_dep_test_data(
    db: &dyn GraphQuerier,
    parent_props: &[codegraph_core::types::PropertyNode],
    field_name: &str,
    current_domain: Option<&str>,
) -> String {
    // Find the property that matches this field to get its ref_target.
    // Entity ref UI fields have _id suffix (e.g. "deployment_id") while
    // the PropertyNode uses the raw name (e.g. "deployment").
    let raw_name = field_name.strip_suffix("_id").unwrap_or(field_name);
    let prop = parent_props
        .iter()
        .find(|p| p.rust_field_name == field_name || p.rust_field_name == raw_name);
    let ref_target = match prop.and_then(|p| p.ref_target.as_ref()) {
        Some(t) => t,
        None => return String::new(),
    };

    // Extract schema title from ref_target
    let last_segment = ref_target.rsplit('/').next().unwrap_or(ref_target);
    let ref_schema_title = last_segment
        .strip_suffix(".json#")
        .or_else(|| last_segment.strip_suffix(".json"))
        .unwrap_or(last_segment);

    // Resolve the referenced schema to find its domain.
    // Try current domain first, then fallback to cross-domain lookup
    // (e.g., timecard.leave_request → common.WorkerType).
    let dep_domain = match db.get_schema_in_domain(ref_schema_title, current_domain.unwrap_or("")).await {
        Ok(Some(s)) => s.domain.clone(),
        _ => match db.get_schema(ref_schema_title).await {
            Ok(Some(s)) => s.domain.clone(),
            _ => current_domain.map(|s| s.to_string()),
        },
    };

    // Collect UI fields for the dependency entity
    let dep_fields = match collect_ui_fields(
        db,
        ref_schema_title,
        &[],
        dep_domain.as_deref().or(current_domain),
    )
    .await
    {
        Ok(f) => f,
        Err(_) => return String::new(),
    };

    // Build test value entries — include all non-entity-ref fields to maximize
    // the chance of passing server-side validation (NOT NULL constraints, codelist FKs).
    let mut entries = Vec::new();
    for f in &dep_fields {
        // Skip entity refs in deps (would cause recursive dep creation).
        // Also skip fields ending in _id that aren't codelists — they're likely
        // FK references that need real UUIDs, not string test data.
        if f.is_entity_ref {
            continue;
        }
        if f.name.ends_with("_id") && !f.is_codelist {
            continue;
        }
        // Skip bare "id" field — it's the primary key, auto-generated
        if f.name == "id" {
            continue;
        }
        let value = test_value_for_field(f);
        if !value.is_empty() {
            entries.push(format!("'{}': {}", f.name, value));
        }
    }

    entries.join(", ")
}

/// Build a JS object-literal body (without outer braces) for creating an entity
/// via the API.  Used for parent and grandparent entity creation in tests.
async fn build_test_data_json(
    db: &dyn GraphQuerier,
    schema_title: &str,
    domain: Option<&str>,
) -> String {
    let fields = match collect_ui_fields(db, schema_title, &[], domain).await {
        Ok(f) => f,
        Err(_) => {
            // Fallback: if collect_ui_fields failed (e.g. codelist entities with
            // no UI fields in the graph), generate a minimal payload with a
            // code placeholder to satisfy NOT NULL constraints.
            // Only common-domain codelists have code columns.
            if !schema_title.is_empty() && domain.map_or(false, |d| d == "common") {
                return "code: `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`".to_string();
            }
            return String::new();
        }
    };
    // Also handle empty-success: collect_ui_fields may return Ok(vec![]) when
    // the schema has no properties (e.g. enum-only code-list schemas).
    // Only common-domain codelists have code columns.
    if fields.is_empty() && !schema_title.is_empty() && domain.map_or(false, |d| d == "common") {
        return "code: `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`".to_string();
    }
    let mut entries = Vec::new();
    for f in &fields {
        if f.is_entity_ref || f.name == "id" {
            continue;
        }
        if f.name.ends_with("_id") && !f.is_codelist {
            continue;
        }
        // ValueObject fields: omit non-array nested types entirely.
        // All are Option<T> or Vec<T> with #[serde(default)] — omitting
        // the key lets serde use None / empty vec.  Only emit [] for arrays.
        if f.nested_type_name.is_some() {
            if f.is_array {
                entries.push(format!("'{}': []", f.name));
            } else {
                // Omitted — serde uses #[serde(default)] → None
            }
            continue;
        }
        let value = test_value_for_field(f);
        if !value.is_empty() {
            entries.push(format!("'{}': {}", f.name, value));
        }
    }
    entries.join(", ")
}

/// Generate a JS literal value for a UiField, matching the same logic used in
/// test templates' testData() function.
fn test_value_for_field(field: &UiField) -> String {
    // StructuredWrapper fields emit JSONB objects
    if !field.structured_sub_fields.is_empty() {
        if field.is_array {
            return format!("[{{ value: 'Test {}' }}]", field.label);
        }
        return format!("{{ value: 'Test {}' }}", field.label);
    }
    if field.is_codelist && !field.codelist_values.is_empty() {
        if field.is_array {
            return format!("[{{ code: '{}' }}]", field.codelist_values[0]);
        }
        return format!("'{}'", field.codelist_values[0]);
    }
    match field.input_type.as_str() {
        "number" => "42".to_string(),
        "checkbox" => "true".to_string(),
        "date" => "'2025-01-15'".to_string(),
        "datetime-local" => "'2025-01-15T10:30:00Z'".to_string(),
        "date-range" => "'[2025-01-15T00:00:00Z,2025-12-31T23:59:59Z)'".to_string(),
        "code" => {
            // code column in codelist entities: must be globally unique at
            // runtime across parallel test invocations. Use a TS template
            // literal with Date.now() + random suffix.
            "`TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`".to_string()
        }
        _ => {
            if field.pg_type.contains("GEOMETRY") {
                // Geometry fields omitted — plain WKT strings are not accepted without ST_GeomFromText
                String::new()
            } else if field.is_array {
                format!("['Test {}']", field.label)
            } else if field.is_range {
                "'[2025-01-01T00:00:00Z,2025-12-31T23:59:59Z]'".to_string()
            } else if field.name == "code" && !field.is_codelist {
                // code column in codelist entities: must be globally unique at
                // runtime across parallel test invocations. Use a TS template
                // literal with Date.now() + random suffix.
                "`TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`".to_string()
            } else {
                format!("'Test {}'", field.label)
            }
        }
    }
}

/// Build E2E include test configuration from resolved include paths.
/// Creates setup steps (entity creation in dependency order) and test path info.
async fn resolve_e2e_include_config(
    db: &dyn GraphQuerier,
    _config: &DomainConfig,
    domain: &str,
    schema_title: &str,
    include_paths: &[crate::generate::api::include_path::ResolvedIncludePath],
    has_list: bool,
) -> Result<Option<E2eIncludeConfig>> {
    let mut all_steps: Vec<IncludeSetupStep> = Vec::new();
    let mut test_paths: Vec<IncludeTestPath> = Vec::new();
    let mut seen_deps: HashSet<String> = HashSet::new();

    // Collect FK map for the main entity, deduplicated by FK column name
    let mut main_fk_map: Vec<[String; 2]> = Vec::new();
    let mut seen_main_fk_cols: HashSet<String> = HashSet::new();

    for path in include_paths {
        let mut prev_dep_id: Option<String> = None;
        // fk_column of the previously processed (deeper) segment — this is the FK
        // column on the CURRENT segment's entity pointing to the deeper entity.
        let mut prev_fk_column: Option<String> = None;

        // Process segments in REVERSE order (leaf entity first)
        for (seg_idx, seg) in path.segments.iter().enumerate().rev() {
            let dep_id = format!("{}_{}", seg.module_name, seg_idx);

            if seen_deps.contains(&dep_id) {
                prev_dep_id = Some(dep_id);
                prev_fk_column = Some(seg.fk_column.clone());
                continue;
            }
            seen_deps.insert(dep_id.clone());

            // Resolve the target schema for api_path using the canonical schema_title.
            let target_schema = db
                .get_schema_in_domain(&seg.schema_title, domain)
                .await?
                .ok_or_else(|| crate::error::Error::SchemaNotFound(seg.schema_title.clone()))?;
            let api_path = format!("/api/{}/{}", seg.domain, target_schema.api_path_segment);

            let fields_json = build_test_data_json(db, &seg.schema_title, Some(&seg.domain)).await;

            // FK map: this entity has a FK to the previously created (deeper) entity.
            // The FK column is the fk_column of the deeper segment — it describes
            // the column on this entity's table that references the deeper entity.
            let mut fk_map: Vec<[String; 2]> = Vec::new();
            if let Some(ref prev_id) = prev_dep_id {
                if let Some(ref fk_col) = prev_fk_column {
                    fk_map.push([fk_col.clone(), prev_id.clone()]);
                }
            }

            all_steps.push(IncludeSetupStep {
                dep_id: dep_id.clone(),
                api_path,
                fields_json,
                fk_map,
            });

            prev_dep_id = Some(dep_id);
            prev_fk_column = Some(seg.fk_column.clone());
        }

        // Record the test path
        if let Some(_first_seg) = path.segments.first() {
            let last_idx = path.segments.len() - 1;
            let last_seg = &path.segments[last_idx];
            let target_dep_id = format!("{}_{}", last_seg.module_name, last_idx);

            test_paths.push(IncludeTestPath {
                alias: path.alias.clone(),
                target_dep_id,
                is_dot_path: path.segments.len() > 1,
                is_array: last_seg.is_array,
            });
        }

        // Add main entity FK to the first segment of this path (deduplicated)
        if let Some(ref first_seg) = path.segments.first() {
            let first_dep_id = format!("{}_{}", first_seg.module_name, 0);
            if seen_main_fk_cols.insert(first_seg.fk_column.clone()) {
                main_fk_map.push([first_seg.fk_column.clone(), first_dep_id]);
            }
        }
    }

    if test_paths.is_empty() {
        return Ok(None);
    }

    // Add main entity as the LAST setup step
    let source_schema = db
        .get_schema_in_domain(schema_title, domain)
        .await?
        .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;
    let main_dep_id = source_schema.pg_table_name.clone();

    if !seen_deps.contains(&main_dep_id) {
        let main_api_path = format!("/api/{}/{}", domain, source_schema.api_path_segment);
        let main_fields = build_test_data_json(db, schema_title, Some(domain)).await;

        all_steps.push(IncludeSetupStep {
            dep_id: main_dep_id.clone(),
            api_path: main_api_path,
            fields_json: main_fields,
            fk_map: main_fk_map,
        });
    }

    let has_multi = test_paths.len() >= 2;

    Ok(Some(E2eIncludeConfig {
        setup_steps: all_steps,
        main_entity_id_ref: main_dep_id,
        test_paths,
        has_multi_include: has_multi,
        test_list_include: has_list,
    }))
}
