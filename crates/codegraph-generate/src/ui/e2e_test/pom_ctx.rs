//! POM (Playwright Object Model) context section (issue #315).
//!
//! The per-entity inputs the POM generator assembles a page-object model
//! from: URL routes, per-field input knowledge, workflow method inputs,
//! and the ux plan mirror. Defined here in step 2 as the inert shape —
//! [`UiE2eTestContext::pom`](super::context::UiE2eTestContext) stays
//! `None` (skipped in serialization) until step 3 (#316) populates it, so
//! emitted context bytes are unchanged.

use serde::Serialize;

use super::context::UxE2eSpecCtx;

/// Per-entity POM inputs. Populated by step 3 (#316); `None` before that
/// (and skipped from the serialized context while `None`).
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
