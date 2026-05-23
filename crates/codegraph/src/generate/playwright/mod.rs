// hr-graph/src/generate/playwright/mod.rs
pub mod entity_gen;
pub mod global_gen;

use serde::Serialize;

use super::ui::page::UiField;

/// Per-entity context passed to playwright/entity_page.tera and
/// playwright/test_data_factory.tera.
#[derive(Debug, Serialize)]
pub struct PlaywrightEntityContext {
    /// PascalCase entity name, e.g. "Person"
    pub entity_name: String,
    /// snake_case module name, e.g. "person"
    pub module_name: String,
    /// Domain name, e.g. "common"
    pub domain: String,
    /// URL path segment, e.g. "persons"
    pub path_segment: String,
    pub has_create: bool,
    pub has_read: bool,
    pub has_delete: bool,
    pub has_workflow: bool,
    pub workflow_states: Vec<String>,
    pub initial_state: String,
    /// Fields available for creation forms (excludes workflow-managed fields)
    pub create_fields: Vec<UiField>,
}

/// Summary of one entity — used by the global generator to build mod.rs.
#[derive(Debug, Serialize, Clone)]
pub struct PlaywrightEntitySummary {
    pub module_name: String,
    pub domain: String,
}

/// Per-domain grouping used by crate_lib.tera.
#[derive(Debug, Serialize, Clone)]
pub struct PlaywrightDomainSummary {
    pub name: String,
    pub entities: Vec<PlaywrightEntitySummary>,
}

/// Context for crate_lib.tera — all domains + entities.
#[derive(Debug, Serialize)]
pub struct PlaywrightCrateContext {
    pub domains: Vec<PlaywrightDomainSummary>,
}
