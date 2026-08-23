// hr-graph/src/generate/playwright/mod.rs
pub mod entity_gen;
pub mod global_gen;
pub mod ts_entity_gen;
pub mod ts_global_gen;

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

/// Per-entity context for TypeScript spec + fixture + API client templates.
#[derive(Debug, Serialize)]
pub struct TsEntityContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub nsid: String,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub create_fields: Vec<TsFieldDef>,
    /// True when at least one create field is required (gates the
    /// missing-required-fields test in ts_spec.tera).
    pub has_required_fields: bool,
    /// Required entity-ref FK fields (with target metadata) — drives the
    /// parent-creation `beforeAll` in ts_spec.tera and `withParent*` helpers
    /// in ts_fixture.tera. Always serialized (empty vec when no required FKs)
    /// so Tera templates can safely reference it.
    pub fk_fields: Vec<TsFkField>,
    pub schema_name: String,
    /// Whether this entity has full-text search (search.fts_* config).
    pub has_fts: bool,
    /// camelCase create-DTO field used to seed FTS search terms.
    pub fts_search_field: String,
    /// True when `fts_search_field` is a required create field.
    pub fts_search_field_required: bool,
    /// camelCase create-DTO field of a secondary (D-weight) search column,
    /// usable in a create payload. Empty when no such column exists.
    pub fts_secondary_field: String,
    /// True when the entity is permission-gated (`permissions.scope` set).
    /// The generated spec then uses a DID persona token so requests carry an
    /// actor DID the AuthorizationService can evaluate.
    pub use_persona_token: bool,
    /// True when the entity has `permissions.record_scoped` set. The generated
    /// spec then expects 403 (authz-before-handler) for unknown record ids
    /// instead of 404.
    pub permission_record_scoped: bool,
    /// DID injected as the test persona (both in the auth token and in
    /// did-carrying fixture fields). Defaults to "did:plc:test.generated".
    pub persona_did: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TsFieldDef {
    pub name: String,
    pub label: String,
    pub ts_type: String,
    pub required: bool,
    pub example_value: String,
    /// FK target metadata for required entity-ref FKs — used by the spec
    /// generator to create parent rows in `beforeAll` so the child fixture
    /// can reference a real parent id. None for non-FK / optional-FK fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fk_target_domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fk_target_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fk_target_module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fk_target_entity_name: Option<String>,
    /// JS-safe variable name for the captured parent id (camelCase of
    /// `name`), present when `fk_target_entity_name` is Some.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js_var: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TsFkField {
    /// camelCase create-DTO field name, e.g. "campaignId".
    pub name: String,
    /// PascalCase target entity name, e.g. "Campaign".
    pub entity_name: String,
    /// Domain of the FK target entity, e.g. "campaigns".
    pub target_domain: String,
    /// REST path segment of the FK target, e.g. "campaign".
    pub target_path: String,
    /// snake_case module of the FK target, e.g. "campaign".
    pub target_module: String,
    /// JS-safe variable name for the captured parent id, e.g. "campaignId".
    pub js_var: String,
}

/// Per-entity summary for global generators.
#[derive(Debug, Serialize, Clone)]
pub struct TsEntitySummary {
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub entity_name: String,
}

/// Domain grouping for TypeScript E2E tests.
#[derive(Debug, Serialize, Clone)]
pub struct TsDomainSummary {
    pub name: String,
    pub entities: Vec<TsEntitySummary>,
}

/// Global context for playwright config, auth, docker-compose.
#[derive(Debug, Serialize)]
pub struct TsGlobalContext {
    pub domains: Vec<TsDomainSummary>,
    pub project_name: String,
    pub api_base_url: String,
}
