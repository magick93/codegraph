use crate::ProjectConfig;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::{PropertyNode, SchemaNode};
use serde::Serialize;

use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::ux::{Align, Display};
use codegraph_config::DomainConfig;

use crate::api::api_model::{
    resolve_entity_operations, resolve_path_segment, resolve_path_segment_with_config,
};
use crate::ux::plan::{build_ux_plan, CollectionPlan, RowAction};
use crate::ux::sort::{
    apply_list_scope, collect_ux_plan_context, list_order_is_pinned, sort_plan_from_plan,
};

use super::common::{collect_child_sections, collect_ui_fields};
use super::page::{ChildSection, UiField};
use super::store::UiParentInfo;

#[derive(Debug, Serialize)]
pub struct EntityRefDep {
    /// Rust field name on the main entity (e.g., "tenant_id" or "parties").
    pub field_name: String,
    /// Key into `depIds`, unique across the whole dependency closure.
    pub dep_id: String,
    /// API path for the referenced entity (e.g., "/platform/tenant").
    pub api_path: String,
    /// Whether the reference is a junction array (array-of-entity-ref).
    pub is_array: bool,
}

/// One entity creation step in the dependency `beforeAll`.
/// Steps are ordered leaf-first so an entity's FK deps already exist.
#[derive(Debug, Serialize)]
pub struct DependencyStep {
    /// Key into `depIds`.
    pub dep_id: String,
    /// API path for `createEntityAsAcme`.
    pub api_path: String,
    /// JS object body WITHOUT outer braces; excludes FK fields (see `fk_map`).
    pub fields_json: String,
    /// `[(fk_column, referenced_dep_id)]` — FK columns filled from created deps.
    pub fk_map: Vec<[String; 2]>,
    /// Whether this dep is referenced through a junction array.
    pub is_array: bool,
    /// True when the dependency entity has no `create` operation — its POST
    /// route does not exist (405). The emitted spec tolerates the failure
    /// (warn + leave the FK unset) because such rows may pre-exist via seed
    /// data; required-FK entities still fail their own create loudly.
    pub optional: bool,
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
    /// Main-entity required refs (consumed by `testData()`).
    pub entity_ref_deps: Vec<EntityRefDep>,
    /// Ordered leaf-first dependency creation steps (consumed by `beforeAll`).
    pub dependency_steps: Vec<DependencyStep>,
    /// Actionable messages when a required dep cannot be satisfied.
    pub dependency_errors: Vec<String>,
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
    /// ux-rules list-rendering spec contract (issue #302). `None` when the
    /// `ux_rules` plane is inactive for this entity (flag off, plan-less,
    /// or no list/create output) — no `.ux.test.ts` file is emitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ux_spec: Option<UxE2eSpecCtx>,
}

/// One ux column slot mirrored into the `{seg}.ux.test.ts` spec (issue #302).
#[derive(Debug, Serialize)]
pub struct UxE2eColumnCtx {
    /// Response property the cell reads (the plan's column key).
    pub key: String,
    /// Resolved dimension, kebab-case.
    pub dimension: String,
    /// Resolved display, kebab-case.
    pub display: String,
    /// Column alignment: `left` or `right`.
    pub align: String,
    pub sortable: bool,
    pub truncate_tooltip: bool,
    /// 1-based `td` position in a body row — data columns render in
    /// `column_order` sequence and the actions cell trails them.
    pub td_index: usize,
    /// Audit stamp (`created_at`/`updated_at`/`deleted_at`/`*_by`) — these
    /// trail every other column in `column_order`.
    pub is_audit: bool,
}

/// A chip column with a known fixture label (the codelist's first value or
/// the workflow initial state). Boolean chip columns never match — they
/// render through the boolean Badge branch, which carries no chip testid.
#[derive(Debug, Serialize)]
pub struct UxE2eChipCheck {
    /// The exact chip text the fixture row renders.
    pub text: String,
}

/// The copy-chip column the clipboard/tooltip assertions read.
#[derive(Debug, Serialize)]
pub struct UxE2eCopyCheck {
    pub key: String,
    pub td_index: usize,
    /// JS expression evaluating to the full cell value on the fixture row
    /// (`createdId`, or a string literal). Random-valued fixtures (plain
    /// uuid literals) never qualify.
    pub expected_expr: String,
    /// The cell renders through a Tooltip (the trigger carries the chip).
    pub truncate_tooltip: bool,
}

/// One formatting assertion: the cell text must equal the SAME
/// `Intl.*Format` output the page computes for the fixture value.
#[derive(Debug, Serialize)]
pub struct UxE2eFormatCheck {
    /// `money` | `quantity` | `time-point` (drives the formatter).
    pub kind: String,
    pub td_index: usize,
    /// JS literal of the fixture value (a number, or a quoted instant).
    pub fixture_literal: String,
}

/// A right-aligned column: th and td must carry `text-right tabular-nums`.
#[derive(Debug, Serialize)]
pub struct UxE2eAlignCheck {
    pub key: String,
    pub td_index: usize,
    /// The th renders a sort button — the only per-column th hook.
    pub sortable: bool,
}

/// The list-sort assertions (issue #306 contract surfaced in the spec).
#[derive(Debug, Serialize)]
pub struct UxE2eSortCtx {
    /// Sortable keys in `column_order` sequence — the th order and the
    /// API allow-list must both follow it.
    pub fields: Vec<String>,
    /// The allow-list text the 400 message enumerates.
    pub valid_fields_text: String,
    /// First sortable column with two distinct stable fixture values —
    /// drives the `?sort=&order=` first-row flip assertion. `None` when no
    /// sortable column has a controlled fixture pair.
    pub flip: Option<UxE2eSortFlip>,
}

/// The asc/desc first-row flip fixture pair for one sortable column.
#[derive(Debug, Serialize)]
pub struct UxE2eSortFlip {
    pub key: String,
    pub td_index: usize,
    /// Distinct alternative fixture literal for the second row.
    pub alt_literal: String,
}

/// Row-action overflow assertions (menu-driven Delete + confirm-cancel).
#[derive(Debug, Serialize)]
pub struct UxE2eActionsCtx {
    /// The menu carries a Delete that requires confirmation — the
    /// dialog-cancel-keeps-the-row block is emitted.
    pub confirm_delete: bool,
}

/// The readable-lead assertion: the FIRST rendered column shows the
/// human-readable fixture text, not the system id (plan rows 17/25).
#[derive(Debug, Serialize)]
pub struct UxE2eFirstColumnCtx {
    pub key: String,
    /// Exact first-cell text on the fixture row (a Text/Raw column only —
    /// the readable-lead field by construction).
    pub expected_literal: String,
}

/// The ux-rules spec contract for one entity (issue #302).
///
/// Every assertion block in `ux.test.tera` is gated on one of these
/// fields being non-empty/`Some` — the spec contains only tests for
/// features the entity's list page actually renders.
#[derive(Debug, Serialize)]
pub struct UxE2eSpecCtx {
    /// Full `column_order` sequence (the table-mode header-count check).
    pub columns: Vec<UxE2eColumnCtx>,
    /// The collection renders as a table (chips/format/align/sort/zebra
    /// blocks are table-only — timelines render title + preview entries).
    pub has_table: bool,
    /// Timeline collection parameters (the timeline block).
    pub timeline: Option<super::page::UxTimelineCtx>,
    /// Timeline `order_by` fixture literal, when the field is fixture-
    /// controlled — the spec asserts its formatted date renders.
    pub timeline_order_fixture: Option<String>,
    pub actions: Option<UxE2eActionsCtx>,
    pub chip_checks: Vec<UxE2eChipCheck>,
    pub copy_check: Option<UxE2eCopyCheck>,
    pub format_checks: Vec<UxE2eFormatCheck>,
    pub align_checks: Vec<UxE2eAlignCheck>,
    pub sort: Option<UxE2eSortCtx>,
    pub first_column: Option<UxE2eFirstColumnCtx>,
    /// Alternate row shading is rendered (table mode).
    pub zebra: bool,
    /// BCP-47 locale mirror — the spec computes expected strings with the
    /// same `Intl` calls the page runs.
    pub locale: String,
    pub currency: Option<String>,
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
        let parent_stripped =
            crate::api::router::strip_suffix(parent_title, &config.defaults.type_suffix);

        // 1. Check manual config for parent's parent
        if let Some(parent_ec) = config
            .domains
            .get(parent_domain)
            .and_then(|d| d.get_entity_config(parent_title))
        {
            if parent_ec.role.as_deref() == Some("child") {
                if let Some(ref gp_title) = parent_ec.parent {
                    if let Ok(Some(gp_schema)) =
                        db.get_schema_in_domain(gp_title, parent_domain).await
                    {
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
                            param_name: crate::api::router::param_name_from_path_segment(
                                &resolve_path_segment_with_config(None, &gp_schema, config),
                            ),
                            domain: gp_domain,
                            path_segment: resolve_path_segment_with_config(
                                None, &gp_schema, config,
                            ),
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
            let gpc_child =
                crate::api::router::strip_suffix(&gpc.child_title, &config.defaults.type_suffix);
            if gpc_child == parent_stripped {
                if let Ok(Some(gp_schema)) = db
                    .get_schema_in_domain(&gpc.parent_title, parent_domain)
                    .await
                {
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
                        param_name: crate::api::router::param_name_from_path_segment(
                            &resolve_path_segment_with_config(None, &gp_schema, config),
                        ),
                        domain: gp_domain,
                        path_segment: resolve_path_segment_with_config(None, &gp_schema, config),
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
        let entity_label =
            codegraph_naming::to_display_name(&config.defaults.strip_suffix(&schema.title));
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

        let mut fields =
            collect_ui_fields(db, schema_title, &immutable_fields, Some(&domain), config).await?;

        let all_props = match Some(domain.as_str()) {
            Some(d) => db.get_properties_in_domain(schema_title, d).await?,
            None => db.get_properties(schema_title).await?,
        };
        // Required plain-uuid FK columns without a `$ref`/graph edge (e.g.
        // `party.case_id`) resolve to their entity by naming convention and
        // must be marked before create/update fields are derived.
        apply_convention_refs(db, config, &domain, &all_props, &mut fields).await;

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

        // ux-rules spec contract (issue #302): emitted only when the
        // entity's plan is active AND the list fixture path exists (the
        // blocks assert API-created fixtures against the list page).
        let ux_spec = if has_list && has_create {
            let list_include = dto_config
                .map(|d| d.list_include.clone())
                .unwrap_or_default();
            let list_exclude = dto_config
                .map(|d| d.list_exclude.clone())
                .unwrap_or_default();
            build_ux_e2e_spec(
                db,
                config,
                project,
                schema_title,
                &domain,
                &fields,
                &create_fields,
                &list_include,
                &list_exclude,
                &operations,
                &initial_state,
            )
            .await?
        } else {
            None
        };

        // Build the required entity-ref dependency closure (leaf-first).
        // Required FKs (e.g. case.tenant_id) are created in beforeAll; optional
        // refs are omitted from payloads and never created.
        let (entity_ref_deps, dependency_steps, dependency_errors) = build_required_dependencies(
            db,
            config,
            schema_title,
            &domain,
            &all_props,
            &create_fields,
        )
        .await;
        let has_entity_ref_deps = !entity_ref_deps.is_empty() || !dependency_errors.is_empty();

        // Resolve include test config
        let e2e_include = if let Some(ec) = entity_cfg {
            if ec.allow_include.as_ref().is_some_and(|v| !v.is_empty()) {
                let resolved = crate::api::include_path::resolve_include_paths(
                    db,
                    config,
                    &domain,
                    schema_title,
                    ec.allow_include.as_ref(),
                )
                .await?;
                resolve_e2e_include_config(
                    db,
                    config,
                    &domain,
                    schema_title,
                    &resolved,
                    has_list,
                    project,
                )
                .await?
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
            let stripped =
                crate::api::router::strip_suffix(schema_title, &config.defaults.type_suffix);
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
                        if let Ok(Some(parent_schema)) =
                            db.get_schema_in_domain(parent_title, &domain).await
                        {
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
                                param_name: crate::api::router::param_name_from_path_segment(
                                    &resolve_path_segment_with_config(None, &parent_schema, config),
                                ),
                                domain: parent_domain,
                                path_segment: resolve_path_segment_with_config(
                                    None,
                                    &parent_schema,
                                    config,
                                ),
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
                    let child_name = crate::api::router::strip_suffix(
                        &pc.child_title,
                        &config.defaults.type_suffix,
                    );
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
                        if let Ok(Some(parent_schema)) =
                            db.get_schema_in_domain(&pc.parent_title, &domain).await
                        {
                            let parent_domain = domain.clone();

                            let grandparent = self
                                .resolve_grandparent(db, config, &pc.parent_title, &parent_domain)
                                .await;

                            parent_title_str = pc.parent_title.clone();
                            result = Some(UiParentInfo {
                                param_name: crate::api::router::param_name_from_path_segment(
                                    &resolve_path_segment_with_config(None, &parent_schema, config),
                                ),
                                domain: parent_domain,
                                path_segment: resolve_path_segment_with_config(
                                    None,
                                    &parent_schema,
                                    config,
                                ),
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
            build_test_data_json(db, &parent_title_for_data, parent_domain, config).await
        } else {
            String::new()
        };

        // Build grandparent entity test data for depth-2 nesting
        let grandparent_test_data_json = if let Some(ref p) = parent {
            if let Some(ref gp) = p.grandparent {
                // Find grandparent's schema title from parent_candidates
                let parent_stripped = crate::api::router::strip_suffix(
                    &parent_title_for_data,
                    &config.defaults.type_suffix,
                );
                let mut gp_title = String::new();
                for gpc in &self.parent_candidates {
                    let gpc_child = crate::api::router::strip_suffix(
                        &gpc.child_title,
                        &config.defaults.type_suffix,
                    );
                    if gpc_child == parent_stripped {
                        gp_title = gpc.parent_title.clone();
                        break;
                    }
                }
                if !gp_title.is_empty() {
                    build_test_data_json(db, &gp_title, Some(&gp.domain), config).await
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
            dependency_steps,
            dependency_errors,
            has_entity_ref_deps,
            child_sections,
            has_child_sections,
            param_name: crate::api::router::param_name_from_path_segment(&path_segment),
            parent,
            parent_test_data_json,
            grandparent_test_data_json,
            e2e_include,
            ux_spec,
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
            let content =
                render_template_with_project(tera, "ui/test/crud.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.api.crud.test.ts", path_segment)),
                content,
            });
        }

        // Validation test
        if has_create {
            let content =
                render_template_with_project(tera, "ui/test/validation.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.validation.test.ts", path_segment)),
                content,
            });
        }

        // Workflow test
        if has_workflow {
            let content =
                render_template_with_project(tera, "ui/test/workflow.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.workflow.test.ts", path_segment)),
                content,
            });
        }

        // Persona-based tests
        if has_list || has_create || has_read || has_update || has_delete {
            let content =
                render_template_with_project(tera, "ui/test/owner_crud.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.owner.crud.test.ts", path_segment)),
                content,
            });

            let content = render_template_with_project(
                tera,
                "ui/test/employee_view.test.tera",
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.employee.view.test.ts", path_segment)),
                content,
            });

            let content = render_template_with_project(
                tera,
                "ui/test/manager_team.test.tera",
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.manager.team.test.ts", path_segment)),
                content,
            });

            let content =
                render_template_with_project(tera, "ui/test/isolation.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.isolation.test.ts", path_segment)),
                content,
            });
        }

        // Search tests (only for FTS-enabled entities)
        if has_fts && has_list {
            let content =
                render_template_with_project(tera, "ui/test/search.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.search.test.ts", path_segment)),
                content,
            });

            let content = render_template_with_project(
                tera,
                "ui/test/search_isolation.test.tera",
                &ctx,
                project,
            )?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.search.isolation.test.ts", path_segment)),
                content,
            });
        }

        // Include test
        if ctx.e2e_include.is_some() && has_read {
            let content =
                render_template_with_project(tera, "ui/test/include.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.include.test.ts", path_segment)),
                content,
            });
        }

        // UX list-rendering spec (issue #302) — first-class alongside the
        // crud/validation/workflow specs, gated per feature.
        if ctx.ux_spec.is_some() {
            let content =
                render_template_with_project(tera, "ui/test/ux.test.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: tests_dir.join(format!("{}.ux.test.ts", path_segment)),
                content,
            });
        }

        Ok(files)
    }
}

/// Resolve the referenced schema for an entity-ref property via the graph,
/// falling back to parsing the raw `$ref` string when the graph has no edge.
/// Mirrors `collect_ui_fields` resolution: handles `.schema.json` stems,
/// filename/title divergence, and cross-domain refs.
async fn resolve_ref_schema(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    schema_title: &str,
    current_domain: Option<&str>,
) -> Option<SchemaNode> {
    let mut resolved = if prop.is_array {
        db.get_array_item_schema(&prop.name, schema_title)
            .await
            .ok()
            .flatten()
    } else {
        db.get_property_ref_target(&prop.name, schema_title)
            .await
            .ok()
            .flatten()
    };
    if resolved.is_none() {
        if let Some(ref target) = prop.ref_target {
            let last_segment = target.rsplit('/').next().unwrap_or(target);
            let ref_schema_title = last_segment
                .strip_suffix(".schema.json")
                .or_else(|| last_segment.strip_suffix(".json#"))
                .or_else(|| last_segment.strip_suffix(".json"))
                .unwrap_or(last_segment);
            if let Ok(Some(ref_schema)) = db
                .get_schema_in_domain(ref_schema_title, current_domain.unwrap_or(""))
                .await
            {
                resolved = Some(ref_schema);
            }
            if resolved.is_none() {
                if let Ok(Some(ref_schema)) = db.get_schema(ref_schema_title).await {
                    resolved = Some(ref_schema);
                }
            }
        }
    }
    // Final fallback: a required plain `format: uuid` scalar ending in `_id`
    // with no `$ref`/graph edge (e.g. `party.case_id`) resolves to an entity by
    // naming convention.
    if resolved.is_none() {
        resolved = resolve_convention_ref(db, prop, current_domain).await;
    }
    // Prefer a same-domain schema when the resolved one lives elsewhere.
    if let (Some(cur_domain), Some(found)) = (current_domain, &resolved) {
        if found.domain.as_deref() != Some(cur_domain) {
            if let Ok(schemas) = db.list_schemas(Some(cur_domain)).await {
                if let Some(same_domain) = schemas.iter().find(|s| s.title == found.title) {
                    resolved = Some(same_domain.clone());
                }
            }
        }
    }
    resolved
}

/// Normalize an identifier for convention matching: keep only lowercase
/// alphanumerics, so `case`, `Case`, and `case_id`'s stem all collapse to
/// `case` (and `CaseType` would become `casetype`, which does not match).
fn normalize_convention_stem(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Resolve a required plain `format: uuid` scalar FK column with no `$ref`/
/// graph edge (e.g. `case_id`) to its entity by naming convention: strip the
/// `_id` suffix, normalize, and find the unique `is_entity` schema whose
/// normalized `pg_table_name` or `rust_type_name` equals the stem. On multiple
/// matches, prefer a same-domain schema; return `None` if still ambiguous.
async fn resolve_convention_ref(
    db: &dyn GraphQuerier,
    prop: &PropertyNode,
    current_domain: Option<&str>,
) -> Option<SchemaNode> {
    if prop.is_array || !prop.is_required || prop.ref_target.is_some() {
        return None;
    }
    if prop.format.as_deref() != Some("uuid") {
        return None;
    }
    let field = prop
        .rust_field_name
        .strip_prefix("r#")
        .unwrap_or(&prop.rust_field_name);
    let stem = field.strip_suffix("_id")?;
    if stem.is_empty() {
        return None;
    }
    let stem_norm = normalize_convention_stem(stem);
    let matches: Vec<SchemaNode> = db
        .list_schemas(None)
        .await
        .ok()?
        .into_iter()
        .filter(|s| {
            s.is_entity
                && (normalize_convention_stem(&s.pg_table_name) == stem_norm
                    || normalize_convention_stem(&s.rust_type_name) == stem_norm)
        })
        .collect();
    match matches.len() {
        0 => None,
        1 => matches.into_iter().next(),
        _ => {
            let same_domain: Vec<SchemaNode> = matches
                .into_iter()
                .filter(|s| s.domain.as_deref() == current_domain)
                .collect();
            if same_domain.len() == 1 {
                same_domain.into_iter().next()
            } else {
                None
            }
        }
    }
}

/// Post-process collected UI fields: mark required plain-uuid convention FK
/// columns (e.g. `case_id`) as entity refs so they participate in dependency
/// setup and `testData()` emits the dep branch instead of a literal.
async fn apply_convention_refs(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    props: &[PropertyNode],
    fields: &mut [UiField],
) {
    for field in fields.iter_mut() {
        if field.is_entity_ref || !field.is_required {
            continue;
        }
        let Some(prop) = find_ref_property(props, &field.name) else {
            continue;
        };
        if let Some(target) = resolve_convention_ref(db, prop, Some(domain)).await {
            field.is_entity_ref = true;
            field.ref_api_path = Some(api_path_for_schema(&target, config));
        }
    }
}

/// Match a UI entity-ref field to its graph property. Entity-ref UI fields use
/// the `_id`-suffixed DTO name for scalars; arrays keep the raw field name.
fn find_ref_property<'a>(props: &'a [PropertyNode], field_name: &str) -> Option<&'a PropertyNode> {
    let raw = field_name.strip_suffix("_id").unwrap_or(field_name);
    props.iter().find(|p| {
        p.rust_field_name == field_name
            || p.rust_field_name == raw
            || p.name == raw
            || p.pg_column_name == field_name
    })
}

fn api_path_for_schema(schema: &SchemaNode, config: &DomainConfig) -> String {
    let domain = schema.domain.clone().unwrap_or_default();
    format!(
        "/{}/{}",
        domain,
        resolve_path_segment_with_config(None, schema, config)
    )
}

/// Allocate a unique `depIds` key, suffixing `_2`, `_3`, ... on collision.
fn unique_dep_id(base: &str, used: &mut HashSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut n = 2usize;
    while used.contains(&candidate) {
        candidate = format!("{}_{}", base, n);
        n += 1;
    }
    used.insert(candidate.clone());
    candidate
}

/// A discovered dependency node, keyed by `SchemaNode::schema_id`.
struct DepNode {
    title: String,
    dep_id: String,
    api_path: String,
    fields_json: String,
    is_array: bool,
    /// No `create` operation — POST will 405; the step is emitted tolerantly.
    optional: bool,
    /// `(fk_field_name, child_key)` for each required entity ref.
    children: Vec<(String, String)>,
}

/// Build the required-only transitive entity-ref closure for the main entity.
///
/// Returns `(entity_ref_deps, dependency_steps, errors)`:
/// - `entity_ref_deps` maps the main entity's required refs to `depIds` keys.
/// - `dependency_steps` is ordered leaf-first so every FK is created first.
/// - `errors` holds actionable messages for unsatisfiable/cyclic required deps.
async fn build_required_dependencies(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    main_title: &str,
    main_domain: &str,
    main_props: &[PropertyNode],
    create_fields: &[UiField],
) -> (Vec<EntityRefDep>, Vec<DependencyStep>, Vec<String>) {
    let mut assigned: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut nodes: std::collections::HashMap<String, DepNode> = std::collections::HashMap::new();
    let mut node_order: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut entity_ref_deps: Vec<EntityRefDep> = Vec::new();

    // Seed the worklist from the main entity's required entity-ref fields.
    let mut queue: std::collections::VecDeque<(SchemaNode, String, bool)> =
        std::collections::VecDeque::new();
    for field in create_fields
        .iter()
        .filter(|f| f.is_entity_ref && f.is_required)
    {
        let Some(prop) = find_ref_property(main_props, &field.name) else {
            errors.push(format!(
                "required entity reference '{}' on {} could not be matched to a schema property",
                field.name, main_title
            ));
            continue;
        };
        match resolve_ref_schema(db, prop, main_title, Some(main_domain)).await {
            Some(target) => {
                let key = target.schema_id.clone();
                let dep_id = if let Some(existing) = assigned.get(&key) {
                    existing.clone()
                } else {
                    let id = unique_dep_id(&field.name, &mut used);
                    assigned.insert(key.clone(), id.clone());
                    id
                };
                entity_ref_deps.push(EntityRefDep {
                    field_name: field.name.clone(),
                    dep_id: dep_id.clone(),
                    api_path: api_path_for_schema(&target, config),
                    is_array: field.is_array,
                });
                queue.push_back((target, dep_id, field.is_array));
            }
            None => errors.push(format!(
                "required entity reference '{}' on {} has no resolvable target schema",
                field.name, main_title
            )),
        }
    }

    // Discover the closure breadth-first; leaf-first ordering happens below.
    while let Some((target, dep_id, is_array)) = queue.pop_front() {
        let key = target.schema_id.clone();
        if nodes.contains_key(&key) {
            continue;
        }
        let target_domain = target
            .domain
            .clone()
            .unwrap_or_else(|| main_domain.to_string());
        let api_path = api_path_for_schema(&target, config);
        let fields_json =
            build_test_data_json(db, &target.title, Some(&target_domain), config).await;
        let props = db
            .get_properties_in_domain(&target.title, &target_domain)
            .await
            .unwrap_or_default();
        let dep_fields = collect_ui_fields(db, &target.title, &[], Some(&target_domain), config)
            .await
            .unwrap_or_default();

        // Required plain-uuid FK columns on the dependency (e.g. `party.case_id`
        // when creating a `Claim` whose closure pulls in `Party`) must also be
        // resolved by convention so the transitive closure stays complete.
        let mut dep_fields = dep_fields;
        apply_convention_refs(db, config, &target_domain, &props, &mut dep_fields).await;

        // Required entity refs on this dep become child steps.
        let mut children: Vec<(String, String)> = Vec::new();
        for rf in dep_fields
            .iter()
            .filter(|f| f.is_entity_ref && f.is_required)
        {
            let Some(prop) = find_ref_property(&props, &rf.name) else {
                errors.push(format!(
                    "required entity reference '{}' on {} could not be matched to a schema property",
                    rf.name, target.title
                ));
                continue;
            };
            match resolve_ref_schema(db, prop, &target.title, Some(&target_domain)).await {
                Some(child_target) => {
                    let child_key = child_target.schema_id.clone();
                    let child_dep_id = if let Some(existing) = assigned.get(&child_key) {
                        existing.clone()
                    } else {
                        let id = unique_dep_id(&rf.name, &mut used);
                        assigned.insert(child_key.clone(), id.clone());
                        id
                    };
                    children.push((rf.name.clone(), child_key.clone()));
                    queue.push_back((child_target, child_dep_id, rf.is_array));
                }
                None => errors.push(format!(
                    "required entity reference '{}' on {} has no resolvable target schema",
                    rf.name, target.title
                )),
            }
        }

        // Required scalar fields that cannot be populated make the dep unsatisfiable.
        for f in dep_fields.iter().filter(|f| f.is_required) {
            if f.is_entity_ref || f.nested_type_name.is_some() || f.name == "id" {
                continue;
            }
            if test_value_for_field(f).is_empty() {
                errors.push(format!(
                    "required field '{}' on {} cannot be populated with test data",
                    f.name, target.title
                ));
            }
        }

        let dep_operations = crate::api::api_model::resolve_entity_operations(
            db,
            config,
            &target_domain,
            &target.title,
        )
        .await;
        let optional = !dep_operations.iter().any(|op| op == "create");

        nodes.insert(
            key.clone(),
            DepNode {
                title: target.title.clone(),
                dep_id,
                api_path,
                fields_json,
                is_array,
                optional,
                children,
            },
        );
        node_order.push(key);
    }

    // Post-order DFS over the discovered graph (leaf-first) + cycle detection.
    let mut steps: Vec<DependencyStep> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut visiting: HashSet<String> = HashSet::new();
    let mut order: Vec<String> = Vec::new();
    for key in &node_order {
        post_order_visit(
            key,
            &nodes,
            &mut visited,
            &mut visiting,
            &mut order,
            &mut errors,
        );
    }
    for key in &order {
        let Some(node) = nodes.get(key) else {
            continue;
        };
        let fk_map = node
            .children
            .iter()
            .filter_map(|(fk, child_key)| {
                nodes
                    .get(child_key)
                    .map(|child| [fk.clone(), child.dep_id.clone()])
            })
            .collect();
        steps.push(DependencyStep {
            dep_id: node.dep_id.clone(),
            api_path: node.api_path.clone(),
            fields_json: node.fields_json.clone(),
            fk_map,
            is_array: node.is_array,
            optional: node.optional,
        });
    }

    (entity_ref_deps, steps, errors)
}

fn post_order_visit(
    key: &str,
    nodes: &std::collections::HashMap<String, DepNode>,
    visited: &mut HashSet<String>,
    visiting: &mut HashSet<String>,
    order: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    if visited.contains(key) {
        return;
    }
    if !visiting.insert(key.to_string()) {
        // Already on the current DFS stack — a required-dependency cycle.
        let title = nodes
            .get(key)
            .map(|n| n.title.clone())
            .unwrap_or_else(|| key.to_string());
        let msg = format!(
            "required dependency cycle detected involving '{}' — break the required FK cycle or make one ref optional",
            title
        );
        if !errors.iter().any(|e| e == &msg) {
            errors.push(msg);
        }
        return;
    }
    if let Some(node) = nodes.get(key) {
        for (_, child_key) in &node.children {
            post_order_visit(child_key, nodes, visited, visiting, order, errors);
        }
    }
    visiting.remove(key);
    visited.insert(key.to_string());
    order.push(key.to_string());
}

/// Build a JS object-literal body (without outer braces) for creating an entity
/// via the API.  Used for parent and grandparent entity creation in tests.
async fn build_test_data_json(
    db: &dyn GraphQuerier,
    schema_title: &str,
    domain: Option<&str>,
    config: &DomainConfig,
) -> String {
    let fields = match collect_ui_fields(db, schema_title, &[], domain, config).await {
        Ok(f) => f,
        Err(_) => {
            // Fallback: if collect_ui_fields failed (e.g. codelist entities with
            // no UI fields in the graph), generate a minimal payload with a
            // code placeholder to satisfy NOT NULL constraints.
            // Only common-domain codelists have code columns.
            if !schema_title.is_empty() && (domain == Some("common")) {
                return "code: `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`"
                    .to_string();
            }
            return String::new();
        }
    };
    // Also handle empty-success: collect_ui_fields may return Ok(vec![]) when
    // the schema has no properties (e.g. enum-only code-list schemas).
    // Only common-domain codelists have code columns.
    if fields.is_empty() && !schema_title.is_empty() && (domain == Some("common")) {
        return "code: `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`"
            .to_string();
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

/// A runtime-valid v4 UUID as a TypeScript template literal (fixed version
/// nibble `4` and variant nibble `8`, random 12-hex tail).
fn uuid_literal() -> String {
    "`00000000-0000-4000-8000-${Math.random().toString(16).slice(2, 14).padStart(12, '0')}`"
        .to_string()
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
    // Plain UUID columns (resolved entity refs are handled above) must serialize
    // as a valid v4 UUID; a literal like `'Test Reviewer Id'` fails validation.
    if field.pg_type == "UUID" && !field.is_entity_ref {
        return uuid_literal();
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
    include_paths: &[crate::api::include_path::ResolvedIncludePath],
    has_list: bool,
    _project: &ProjectConfig,
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
            let api_path = format!(
                "/{}/{}",
                seg.domain,
                resolve_path_segment_with_config(None, &target_schema, _config)
            );

            let fields_json =
                build_test_data_json(db, &seg.schema_title, Some(&seg.domain), _config).await;

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
        if let Some(first_seg) = path.segments.first() {
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
        let main_api_path = format!(
            "/{}/{}",
            domain,
            resolve_path_segment_with_config(None, &source_schema, _config)
        );
        let main_fields = build_test_data_json(db, schema_title, Some(domain), _config).await;

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

// ── ux-rules spec context (issue #302) ──────────────────────────────────

/// Build the ux-rules `.ux.test.ts` context for one entity.
///
/// Reuses [`collect_ux_plan_context`] + [`build_ux_plan`] — the EXACT
/// plan assembly the list page renders from (via `resolve_ux_context`),
/// so the spec can never drift from the markup it asserts. `None` when
/// the `ux_rules` plane is off or the entity is plan-less.
#[allow(clippy::too_many_arguments)]
async fn build_ux_e2e_spec(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    project: &ProjectConfig,
    schema_title: &str,
    domain: &str,
    fields: &[UiField],
    create_fields: &[UiField],
    list_include: &[String],
    list_exclude: &[String],
    operations: &[String],
    initial_state: &str,
) -> Result<Option<UxE2eSpecCtx>> {
    let Some(rules) = project.ux.as_ref() else {
        return Ok(None);
    };

    // Mirror the list-page scope exactly (dto list pins, #306 helpers).
    let list_fields = apply_list_scope(fields, list_include, list_exclude);
    let pinned = list_order_is_pinned(list_include, list_exclude);
    let plan_ctx =
        collect_ux_plan_context(db, config, schema_title, domain, list_fields, pinned).await?;
    let workflow_status_field = plan_ctx.workflow_status_field().map(str::to_string);
    let input = plan_ctx.plan_input(schema_title);
    let Some(plan) = build_ux_plan(Some(rules), &input)? else {
        return Ok(None);
    };

    let create_by_name: BTreeMap<&str, &UiField> =
        create_fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let has_read = operations.iter().any(|op| op == "read");
    let has_update = operations.iter().any(|op| op == "update");
    let has_delete = operations.iter().any(|op| op == "delete");
    let has_table = matches!(plan.collection, CollectionPlan::Table);

    let mut columns = Vec::with_capacity(plan.column_order.len());
    let mut chip_checks = Vec::new();
    let mut format_checks = Vec::new();
    let mut align_checks = Vec::new();
    let mut copy_check = None;
    let mut first_column = None;

    for (index, name) in plan.column_order.iter().enumerate() {
        let Some(col) = plan.columns.get(name) else {
            continue;
        };
        let td_index = index + 1;
        let field = fields.iter().find(|f| &f.name == name);
        let create_field = create_by_name.get(name.as_str()).copied();
        let kebab_dimension = col.dimension.as_str().to_string();

        columns.push(UxE2eColumnCtx {
            key: name.clone(),
            dimension: kebab_dimension.clone(),
            display: display_kebab(col.display).to_string(),
            align: align_kebab(col.align).to_string(),
            sortable: col.sortable,
            truncate_tooltip: col.truncate_tooltip,
            td_index,
            is_audit: is_audit_stamp(name),
        });

        // Chip assertions: a chip column whose fixture label is known.
        // Booleans are excluded — they render through the boolean Badge
        // branch, which carries no chip testid.
        if col.display == Display::Chip {
            if let Some(text) =
                chip_fixture_text(field, workflow_status_field.as_deref(), name, initial_state)
            {
                chip_checks.push(UxE2eChipCheck { text });
            }
        }

        // Clipboard/tooltip assertions: the FIRST copy-chip column with a
        // stable (non-random) fixture value.
        if copy_check.is_none() && col.display == Display::CopyChip {
            if let Some(expr) = stable_fixture_expr(
                name,
                create_field,
                workflow_status_field.as_deref(),
                initial_state,
            ) {
                copy_check = Some(UxE2eCopyCheck {
                    key: name.clone(),
                    td_index,
                    expected_expr: expr,
                    truncate_tooltip: col.truncate_tooltip,
                });
            }
        }

        // Intl formatting assertions for money/quantity/time-point cells.
        // Audit stamps are excluded — they may be server-stamped, so their
        // cell values are not fixture-controlled.
        if !is_audit_stamp(name) {
            if let Some(fixture) = create_field.and_then(stable_fixture_literal) {
                let kind = match col.dimension {
                    codegraph_config::ux::Dimension::Money => Some("money"),
                    codegraph_config::ux::Dimension::Quantity => Some("quantity"),
                    codegraph_config::ux::Dimension::TimePoint => Some("time-point"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    format_checks.push(UxE2eFormatCheck {
                        kind: kind.to_string(),
                        td_index,
                        fixture_literal: fixture,
                    });
                }
            }
        }

        // Right-aligned columns: th (when sortable — the only th hook)
        // and td must carry `text-right tabular-nums`.
        if col.align == Align::Right {
            align_checks.push(UxE2eAlignCheck {
                key: name.clone(),
                td_index,
                sortable: col.sortable,
            });
        }

        // Readable-lead assertion: the FIRST column is the human-readable
        // Text field (plan row 17) — its raw cell shows the fixture text.
        // Money/time/copy renders transform the cell, and server-stamped
        // audit columns are not predictable, so they never qualify.
        if index == 0
            && col.dimension == codegraph_config::ux::Dimension::Text
            && col.display == Display::Raw
            && !is_audit_stamp(name)
        {
            if let Some(literal) = create_field.and_then(stable_fixture_literal) {
                first_column = Some(UxE2eFirstColumnCtx {
                    key: name.clone(),
                    expected_literal: literal,
                });
            }
        }
    }

    // Asc/desc flip: the first sortable table column with two distinct
    // stable fixture values (timelines keep their fixed DESC order).
    // Audit stamps are skipped — a server-stamped column is not a
    // controlled fixture value.
    let flip = if has_table {
        plan.column_order
            .iter()
            .enumerate()
            .find_map(|(index, name)| {
                if is_audit_stamp(name) {
                    return None;
                }
                let col = plan.columns.get(name)?;
                if !col.sortable {
                    return None;
                }
                let field = create_by_name.get(name.as_str())?;
                let alt = flip_alt_literal(field)?;
                Some(UxE2eSortFlip {
                    key: name.clone(),
                    td_index: index + 1,
                    alt_literal: alt,
                })
            })
    } else {
        None
    };

    let sort = if has_table {
        let sort_plan = sort_plan_from_plan(&plan);
        if sort_plan.is_empty() {
            None
        } else {
            let valid_fields_text = sort_plan.valid_fields_text();
            Some(UxE2eSortCtx {
                fields: sort_plan.fields,
                valid_fields_text,
                flip,
            })
        }
    } else {
        None
    };

    // Timeline params + a fixture-controlled order_by value (the spec
    // asserts its formatted date actually renders in a `<time>` cell).
    let (timeline, timeline_order_fixture) = match &plan.collection {
        CollectionPlan::Timeline {
            order_by,
            title_field,
            preview,
        } => {
            // A server-stamped order_by (audit) is not fixture-controlled.
            let fixture = (!is_audit_stamp(order_by))
                .then(|| create_by_name.get(order_by.as_str()))
                .and_then(|f| f.copied().and_then(stable_literal))
                .or_else(|| {
                    (workflow_status_field.as_deref() == Some(order_by.as_str()))
                        .then(|| format!("'{initial_state}'"))
                });
            (
                Some(super::page::UxTimelineCtx {
                    order_by: order_by.clone(),
                    title_field: title_field.clone(),
                    preview: preview.clone(),
                }),
                fixture,
            )
        }
        CollectionPlan::Table => (None, None),
    };

    // Actions are real UI affordances: filter to the entity's enabled
    // operations exactly like the list page does.
    let action_allowed = |action: RowAction| match action {
        RowAction::Open => has_read,
        RowAction::Edit => has_update,
        RowAction::Delete => has_delete,
    };
    let menu_live = plan
        .actions
        .menu
        .iter()
        .any(|spec| action_allowed(spec.action));
    let menu_deletes = plan.actions.menu.iter().any(|spec| {
        spec.action == RowAction::Delete
            && action_allowed(RowAction::Delete)
            && plan.actions.confirm.iter().any(|c| c == "delete")
    });
    let actions = menu_live.then_some(UxE2eActionsCtx {
        confirm_delete: menu_deletes,
    });

    Ok(Some(UxE2eSpecCtx {
        columns,
        has_table,
        timeline,
        timeline_order_fixture,
        actions,
        chip_checks,
        copy_check,
        format_checks,
        align_checks,
        sort,
        first_column,
        zebra: plan.visuals.zebra,
        locale: plan.format.locale.clone(),
        currency: plan.format.currency.clone(),
    }))
}

/// The kebab-case spelling of a resolved display (mirrors the list page).
fn display_kebab(display: Display) -> &'static str {
    match display {
        Display::Raw => "raw",
        Display::Chip => "chip",
        Display::CopyChip => "copy-chip",
        Display::Link => "link",
    }
}

/// The kebab-case spelling of a resolved alignment.
fn align_kebab(align: Align) -> &'static str {
    match align {
        Align::Left => "left",
        Align::Right => "right",
    }
}

/// Audit-stamp names trail `column_order` (`created_at`/`updated_at`/
/// `deleted_at`/`*_by`) — mirrors `ux::plan::is_audit_field`.
fn is_audit_stamp(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "created_at"
        || lower == "updated_at"
        || lower == "deleted_at"
        || lower.ends_with("_by")
}

/// The fixture value a column shows on the ux fixture row, as a JS
/// expression: `createdId` for the id column, the workflow initial state
/// for the status column, the create-field literal otherwise.
fn stable_fixture_expr(
    name: &str,
    create_field: Option<&UiField>,
    workflow_status_field: Option<&str>,
    initial_state: &str,
) -> Option<String> {
    if name == "id" {
        return Some("createdId".to_string());
    }
    if workflow_status_field == Some(name) && !initial_state.is_empty() {
        return Some(format!("'{initial_state}'"));
    }
    stable_literal(create_field?)
}

/// A create-field fixture literal the spec can predict exactly — random
/// placeholders (uuid templates) are excluded.
fn stable_literal(field: &UiField) -> Option<String> {
    let value = test_value_for_field(field);
    let stable = !value.is_empty() && !value.contains("${");
    stable.then_some(value)
}

/// Alias with the `Option` shape the column loop consumes.
fn stable_fixture_literal(field: &UiField) -> Option<String> {
    stable_literal(field)
}

/// The chip label a fixture row renders: the workflow initial state for
/// the status column, else the codelist's first value. Array codelists
/// render joined text, not a single chip — excluded.
fn chip_fixture_text(
    field: Option<&UiField>,
    workflow_status_field: Option<&str>,
    name: &str,
    initial_state: &str,
) -> Option<String> {
    if workflow_status_field == Some(name) && !initial_state.is_empty() {
        return Some(initial_state.to_string());
    }
    let field = field?;
    if !field.is_codelist || field.is_array {
        return None;
    }
    field.codelist_values.first().cloned()
}

/// A distinct alternative fixture literal for the asc/desc flip row.
fn flip_alt_literal(field: &UiField) -> Option<String> {
    if field.is_array || field.is_entity_ref || field.is_range {
        return None;
    }
    if !field.structured_sub_fields.is_empty() {
        return None;
    }
    if field.is_codelist {
        if field.codelist_values.len() < 2 {
            return None;
        }
        return Some(format!("'{}'", field.codelist_values.last()?));
    }
    match field.input_type.as_str() {
        "number" => Some("7".to_string()),
        "date" => Some("'2024-06-01'".to_string()),
        "datetime-local" => Some("'2024-06-01T00:00:00Z'".to_string()),
        "text" => Some(format!("'Test {} B'", field.label)),
        _ => None,
    }
}
