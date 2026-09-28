//! POM (Playwright Object Model) context section (issues #315, #316).
//!
//! The per-entity inputs the POM generator assembles a page-object model
//! from: URL routes, per-field input knowledge, workflow method inputs,
//! and the ux plan mirror. [`PomCtx::build`] populates it in the
//! generator (`spec-infra` gated: whenever ANY spec is emitted, i.e.
//! any-op entities — NOT ux-flag gated, per the locked #316 contract);
//! [`UiE2eTestContext::pom`](super::context::UiE2eTestContext) stays
//! `None` (skipped from serialization) only when an entity emits no spec
//! files at all.

use serde::Serialize;

use super::context::UxE2eSpecCtx;
use super::page::UiField;
use super::store::UiParentInfo;

/// Per-entity POM inputs, rendered into `{seg}.page.ts` (issue #316).
#[derive(Debug, Serialize)]
pub struct PomCtx {
    /// Entity module name (snake_case), mirroring
    /// `UiE2eTestContext::module_name` — also the testid prefix family.
    pub module: String,
    /// Owning domain key (domains.toml), mirroring `UiE2eTestContext::domain`.
    pub domain: String,
    /// URL path segment, mirroring `UiE2eTestContext::path_segment`.
    pub path_segment: String,
    /// The UI routes the POM methods drive.
    pub urls: PomUrlsCtx,
    /// Per-field input knowledge, in create-form order.
    pub fields: Vec<PomFieldCtx>,
    /// Workflow method inputs when the entity has a workflow.
    pub workflow: Option<PomWorkflowCtx>,
    /// ux-rules plan mirror (columns/sort/actions/timeline/format) — the
    /// same projection the `.ux.test.ts` spec asserts, reused verbatim.
    pub ux: Option<UxE2eSpecCtx>,
}

/// The UI routes for one entity's POM.
#[derive(Debug, Serialize)]
pub struct PomUrlsCtx {
    /// List route: the entity's base UI path
    /// (`/{domain}/{path_segment}`, parent chain included when nested).
    pub list: String,
    /// Create route (`{list}/new`).
    pub create: String,
    /// Detail route pattern with the id placeholder left as `{param}`
    /// (the spec substitutes `entityId` at render time).
    pub detail_param: String,
    /// Suffix appended to the detail route for edit mode (`/edit`).
    pub edit_suffix: String,
    /// Parent chain segments for nested child routes, outermost first
    /// (grandparent, parent); empty for top-level entities.
    pub parent_chain: Vec<String>,
}

/// Input knowledge for one create-form field (crud template conventions).
#[derive(Debug, Serialize)]
pub struct PomFieldCtx {
    /// Field name — also the form input's DOM id (`#name`) for plain
    /// inputs, and the prefix of the structured variants
    /// (`{name}-value`, `{name}-row-{i}`, `{name}-add-btn`,
    /// `{name}-{sub_field}`).
    pub name: String,
    /// How the field input renders/drives (`text`, `number`, `checkbox`,
    /// `select`, `datetime-local`, `date-range`, structured …).
    pub input_kind: String,
    /// First structured sub-field name (`structured`/`structured-array`
    /// kinds): the `{name}-{sub}` locator suffix (`UiSubField::name`,
    /// mirroring the crud templates' `| first | get(key="name")`).
    pub sub_field: Option<String>,
    /// Codelist options when the field renders a dropdown (first value is
    /// the fixture default).
    pub codelist_values: Vec<String>,
    /// Entity-reference field (filled from a created dependency).
    pub is_entity_ref: bool,
}

/// Workflow method inputs for one entity.
#[derive(Debug, Serialize)]
pub struct PomWorkflowCtx {
    /// All state names (transition target vocabulary).
    pub states: Vec<String>,
    /// The state a fresh entity carries.
    pub initial: String,
    /// States that allow no further transitions (inactive shading, too).
    pub terminal: Vec<String>,
}

impl PomCtx {
    /// Assemble the POM inputs for one entity (#316).
    ///
    /// `parent` mirrors the generator's nested-route resolution: the list
    /// route embeds the parent chain as `{param}` placeholder segments
    /// (outermost first, domain on the first entry only — the depth-2 UI
    /// route shares the grandparent's domain), and nested specs pass the
    /// runtime-resolved base path to the page class.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build(
        module: &str,
        domain: &str,
        path_segment: &str,
        param_name: &str,
        parent: Option<&UiParentInfo>,
        create_fields: &[UiField],
        has_workflow: bool,
        workflow_states: &[String],
        initial_state: &str,
        terminal_states: &[String],
        ux: Option<UxE2eSpecCtx>,
    ) -> Self {
        let mut parent_chain: Vec<String> = Vec::new();
        if let Some(p) = parent {
            match &p.grandparent {
                Some(gp) => {
                    parent_chain.push(format!(
                        "/{}/{}/{{{}}}",
                        gp.domain, gp.path_segment, gp.param_name
                    ));
                    parent_chain.push(format!("/{}/{{{}}}", p.path_segment, p.param_name));
                }
                None => {
                    parent_chain.push(format!(
                        "/{}/{}/{{{}}}",
                        p.domain, p.path_segment, p.param_name
                    ));
                }
            }
        }
        let list = if parent_chain.is_empty() {
            format!("/{domain}/{path_segment}")
        } else {
            format!("{}{path_segment}", parent_chain.concat())
        };
        let create = format!("{list}/new");
        let detail_param = format!("{list}/{{{param_name}}}");

        PomCtx {
            module: module.to_string(),
            domain: domain.to_string(),
            path_segment: path_segment.to_string(),
            urls: PomUrlsCtx {
                list,
                create,
                detail_param,
                edit_suffix: "/edit".to_string(),
                parent_chain,
            },
            fields: create_fields
                .iter()
                .map(PomFieldCtx::from_ui_field)
                .collect(),
            workflow: has_workflow.then(|| PomWorkflowCtx {
                states: workflow_states.to_vec(),
                initial: initial_state.to_string(),
                terminal: terminal_states.to_vec(),
            }),
            ux,
        }
    }
}

impl PomFieldCtx {
    /// Project one UI field into POM input knowledge. `input_kind` mirrors
    /// the crud/owner templates' fill branch table exactly (the same order
    /// `test_value_for_field` branches in), so the emitted `fill{Field}`
    /// mechanics can never drift from the fixtures the specs pass.
    pub(super) fn from_ui_field(field: &UiField) -> Self {
        PomFieldCtx {
            name: field.name.clone(),
            input_kind: input_kind(field).to_string(),
            sub_field: field.structured_sub_fields.first().map(|s| s.name.clone()),
            codelist_values: field.codelist_values.clone(),
            is_entity_ref: field.is_entity_ref,
        }
    }
}

/// The fill-branch kind for one field, in the crud template's branch order:
/// value-object → checkbox → select (codelist, array or not) →
/// datetime-local → entity-ref → date-range → structured(-array) → array →
/// geometry → text (number/date/range/uuid/plain all drive the `#id`
/// text-fill branch).
fn input_kind(field: &UiField) -> &'static str {
    if field.nested_type_name.is_some() {
        "value-object"
    } else if field.input_type == "checkbox" {
        "checkbox"
    } else if field.is_codelist && !field.codelist_values.is_empty() {
        "select"
    } else if field.input_type == "datetime-local" {
        "datetime-local"
    } else if field.is_entity_ref {
        "entity-ref"
    } else if field.input_type == "date-range" {
        "date-range"
    } else if !field.structured_sub_fields.is_empty() && field.is_array {
        "structured-array"
    } else if !field.structured_sub_fields.is_empty() {
        "structured"
    } else if field.input_type == "array" {
        "array"
    } else if field.pg_type.contains("GEOMETRY") || field.input_type == "geometry" {
        "geometry"
    } else {
        "text"
    }
}
