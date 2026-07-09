use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::DomainConfigError;

/// Default operations for entities.
fn default_operations() -> Vec<String> {
    vec![
        "create".to_string(),
        "read".to_string(),
        "update".to_string(),
        "delete".to_string(),
        "list".to_string(),
    ]
}

/// Raw TOML configuration file structure for domain boundaries.
///
/// This is the shared configuration consumed by `codegraph`.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainConfig {
    #[serde(default)]
    pub defaults: DefaultsConfig,
    pub domains: HashMap<String, DomainEntry>,
}

fn default_app_name() -> String {
    "codegraph-app".to_string()
}

fn default_max_bulk_size() -> usize {
    100
}

fn default_type_suffix() -> String {
    "Type".to_string()
}

fn default_types_import_prefix() -> String {
    "codegraph_type_contracts".to_string()
}

/// Global defaults for all entities.
#[derive(Debug, Clone, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default = "default_operations")]
    pub operations: Vec<String>,
    /// When true, auto-discover entities from schema files for all domains.
    #[serde(default)]
    pub auto_discover: bool,
    /// When true, generate per-domain OpenAPI specs in addition to the unified spec.
    #[serde(default)]
    pub split_openapi_by_domain: bool,
    /// Application name used in generated scaffolding (package.json, etc.).
    #[serde(default = "default_app_name")]
    pub app_name: String,
    /// Maximum number of items allowed in a bulk create request (default: 100).
    #[serde(default = "default_max_bulk_size")]
    pub max_bulk_size: usize,
    /// Suffix to strip from schema titles (e.g. "Type" for HR Open). Default: "Type".
    #[serde(default = "default_type_suffix")]
    pub type_suffix: String,
    /// Import prefix for structured wrapper types in generated code.
    /// Default: "codegraph_type_contracts" (the crate where IdentifierType etc. live).
    /// Domain crates should set this to their own crate or module path (e.g. "crate").
    #[serde(default = "default_types_import_prefix")]
    pub types_import_prefix: String,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            operations: default_operations(),
            auto_discover: false,
            split_openapi_by_domain: false,
            app_name: default_app_name(),
            max_bulk_size: default_max_bulk_size(),
            type_suffix: default_type_suffix(),
            types_import_prefix: default_types_import_prefix(),
        }
    }
}

impl DefaultsConfig {
    /// Strip the configured type suffix from a schema title.
    pub fn strip_suffix(&self, title: &str) -> String {
        codegraph_naming::strip_suffix(title, &self.type_suffix)
    }
}

/// A single domain entry in the TOML configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainEntry {
    pub label: String,
    pub schema_dir: String,
    pub postgres_schema: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub entities: Vec<String>,
    /// Per-entity configuration for API generation.
    #[serde(default)]
    pub entity_config: HashMap<String, EntityConfig>,
    /// Auto-discover entities from schema files. None = inherit from [defaults].
    #[serde(default)]
    pub auto_discover: Option<bool>,
    /// Entity names to force-exclude from auto-discovery (treated as value objects).
    #[serde(default)]
    pub exclude_entities: Vec<String>,
    /// Override graph classification → force as entity.
    #[serde(default)]
    pub force_entities: Vec<String>,
    /// Override graph classification → force as value object.
    #[serde(default)]
    pub force_value_objects: Vec<String>,
    /// Skip these types entirely (meta/infra schemas).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Whether entities in this domain support soft delete and audit columns.
    /// Defaults to `true` for entity-bearing domains.
    #[serde(default)]
    pub auditable: Option<bool>,
    /// Domain tier for progressive disclosure: "core" or "extended".
    #[serde(default = "default_tier")]
    pub tier: String,
}

fn default_tier() -> String {
    "extended".to_string()
}

impl DomainEntry {
    /// Look up entity config by name, trying both `name` and `nameType` variants.
    ///
    /// HR Open schemas use `XxxType` titles, so config keys are conventionally
    /// stored as `XxxType`. Custom schemas (e.g. pricing) may omit the `Type`
    /// suffix from their schema titles, causing `entity_name` = `"Subscription"`.
    /// This helper tries the plain name first, then the `Type`-suffixed form,
    /// so configs work regardless of naming convention.
    pub fn get_entity_config<'a>(&'a self, name: &str) -> Option<&'a EntityConfig> {
        self.entity_config
            .get(name)
            .or_else(|| self.entity_config.get(&format!("{}Type", name)))
            .or_else(|| {
                // Also try stripping a trailing "Type" from the key to match plain config keys
                let stripped = name.strip_suffix("Type").unwrap_or(name);
                if stripped != name {
                    self.entity_config.get(stripped)
                } else {
                    None
                }
            })
            .or_else(|| {
                // Fallback: match config keys whose normalized form (hyphens removed)
                // equals the input name. Handles LER-RSType → LERRS.
                let normalized = name.replace('-', "");
                self.entity_config.iter().find_map(|(key, cfg)| {
                    let key_normalized = key.replace('-', "");
                    if key_normalized == normalized
                        || key_normalized == format!("{}Type", normalized)
                    {
                        Some(cfg)
                    } else {
                        None
                    }
                })
            })
    }
}

/// Per-entity workflow configuration.
///
/// When present, the entity participates in a stateful workflow.
/// The generated code provides action endpoints that delegate to a
/// hand-crafted `WorkflowService` trait from the platform runtime.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowConfig {
    /// The field on this entity that holds its workflow status.
    pub status_field: String,
    /// Optional approval status field (dual-status pattern from HR Open).
    pub approval_status_field: Option<String>,
    /// Named workflow states (for documentation/validation).
    #[serde(default)]
    pub states: Vec<String>,
    /// Initial state when entity is created.
    pub initial_state: String,
    /// Terminal states (workflow is complete).
    #[serde(default)]
    pub terminal_states: Vec<String>,
    /// Whether to generate workflow action API endpoints (transition, approve, reject).
    #[serde(default)]
    pub generate_action_endpoints: bool,
    /// State transition map: from_state → \[valid target states\].
    /// When empty, any non-terminal state can transition to any other state.
    #[serde(default)]
    pub transitions: HashMap<String, Vec<String>>,
    /// HR Open codelist name that defines valid status values
    /// (e.g. "RecruitingDocumentStatusCodeList").
    /// When set, a CHECK constraint validates the status column value.
    pub status_codelist: Option<String>,
    /// HR Open codelist name for the approval status field.
    pub approval_status_codelist: Option<String>,
    /// Compound guard conditions for dual-status entities.
    /// Maps "status_value" → required approval_status value
    /// (e.g. "active" requires "Approved").
    #[serde(default)]
    pub dual_status_guards: HashMap<String, String>,
    /// Data guard conditions evaluated by the rule engine.
    #[serde(default)]
    pub data_guards: Vec<DataGuard>,
    /// SLA timer definitions keyed by name.
    #[serde(default)]
    pub timers: HashMap<String, TimerDef>,
    /// Approval chain definitions keyed by name.
    #[serde(default)]
    pub approval_chains: HashMap<String, ApprovalChainDef>,
}

/// A data guard condition evaluated by the rule engine.
#[derive(Debug, Clone, Deserialize)]
pub struct DataGuard {
    pub transition_to: String,
    pub rule: String,
    pub message: String,
}

/// SLA timer definition.
#[derive(Debug, Clone, Deserialize)]
pub struct TimerDef {
    pub trigger_on_enter: String,
    #[serde(rename = "type")]
    pub timer_type: String,
    pub duration_hours: i64,
    pub target_state: Option<String>,
}

/// Approval chain definition for a specific transition.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalChainDef {
    pub from: String,
    pub to: String,
    pub steps: Vec<ApprovalStepDef>,
}

/// A single step in an approval chain.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalStepDef {
    pub role: String,
    #[serde(default = "default_true")]
    pub required: bool,
    pub timeout_hours: Option<i32>,
    pub auto_delegate: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

/// Configuration for a single entity's API generation.
#[derive(Debug, Clone, Deserialize)]
pub struct EntityConfig {
    /// Source JSON schema path relative to the schema root.
    pub source_schema: Option<String>,
    /// Which CRUD operations are enabled. Defaults to global defaults.
    pub operations: Option<Vec<String>>,
    /// FK field name that defines URL nesting under a parent entity.
    pub parent_ref: Option<String>,
    /// URL path segment override (default: auto-pluralized kebab-case).
    pub path_segment: Option<String>,
    /// utoipa tag for grouping in OpenAPI docs.
    pub tag: Option<String>,
    /// Entity role: "root", "child", or "value_object".
    pub role: Option<String>,
    /// Parent entity name (for child entities or roots with optional parent nesting).
    pub parent: Option<String>,
    /// DTO configuration overrides.
    #[serde(default)]
    pub dto: DtoConfig,
    /// Workflow configuration (opt-in).
    pub workflow: Option<WorkflowConfig>,
    /// Path to external workflow config file (relative to workspace root).
    pub workflow_file: Option<String>,
    /// Search configuration (full-text search + semantic/embedding search).
    #[serde(default)]
    pub search: SearchConfig,
    /// Columns exposed as JSON:API `?filter[field]=value` query params on the list endpoint.
    /// `None` (default) = auto-discover from graph classifications.
    /// Explicit `[]` = disable filtering for this entity.
    #[serde(default)]
    pub filter_fields: Option<Vec<String>>,
    /// Maximum number of items allowed in a bulk create request.
    /// `None` = inherit from domain defaults or global default (100).
    #[serde(default)]
    pub max_bulk_size: Option<usize>,
    /// Self-referential FK column name for hierarchy/tree queries.
    /// When set, generators produce recursive CTE endpoints and parent FK indexes.
    pub hierarchy_field: Option<String>,
    /// Tree include — resolve related entity data into tree responses.
    /// Each entry adds a LEFT JOIN LATERAL from the via entity to its parent,
    /// returning resolved data as a JSONB field in the tree response.
    /// Requires `hierarchy_field` to be set.
    #[serde(default)]
    pub tree_include: Option<Vec<TreeIncludeConfig>>,
    /// Whether to generate an org-chart SvelteKit page for this entity.
    /// When set on an entity (e.g. OrganizationType), the pipeline produces
    /// (app)/org-chart/+page.server.ts and +page.svelte.
    #[serde(default)]
    pub has_orgchart: bool,
    /// Allowed eager-load include paths for `?include=` query parameter.
    /// Each entry is a relationship path like `"person"`, `"deployment"`,
    /// or `"deployment.position"` (dot-delimited, max 3 levels).
    /// `None` (default) = auto-discover from graph (children + entity-refs).
    /// Explicit `[]` = disable includes for this entity.
    #[serde(default)]
    pub allow_include: Option<Vec<String>>,
}

/// Configuration for resolving a related entity into a tree response.
/// The pipeline joins from the `via_entity` table (which references the hierarchy
/// entity via an FK) to the `via_entity`'s parent (via its `parent_ref`),
/// and returns the resolved data under the given `alias`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeIncludeConfig {
    /// The entity type to join through (e.g. "DeploymentType").
    /// Must be a generated entity with a `parent_ref` and an FK to the hierarchy entity.
    pub via_entity: String,
    /// Field name in the tree response JSON (e.g. "deployed_worker").
    pub alias: String,
}

fn default_fts_language() -> String {
    "english".to_string()
}

fn default_embedding_dimensions() -> u32 {
    1536
}

/// Per-entity search configuration.
///
/// When `fts_columns` is `None`, full-text search columns are auto-discovered
/// from the graph (all `TEXT` data columns). Set to an explicit empty vec `[]`
/// to disable FTS for this entity.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    /// Columns to include in the tsvector. `None` = auto-discover from graph.
    /// Explicit empty `[]` = disable FTS.
    #[serde(default)]
    pub fts_columns: Option<Vec<String>>,
    /// Per-column FTS weight (A/B/C/D). Unspecified columns default to D.
    #[serde(default)]
    pub fts_weights: HashMap<String, String>,
    /// Postgres text search configuration name (default: "english").
    #[serde(default = "default_fts_language")]
    pub fts_language: String,
    /// Columns to generate embedding vectors for (opt-in only, never auto-discovered).
    #[serde(default)]
    pub embedding_columns: Vec<String>,
    /// Vector dimensions for pgvector (default: 1536 for OpenAI text-embedding-ada-002).
    #[serde(default = "default_embedding_dimensions")]
    pub embedding_dimensions: u32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            fts_columns: None,
            fts_weights: HashMap::new(),
            fts_language: default_fts_language(),
            embedding_columns: Vec::new(),
            embedding_dimensions: default_embedding_dimensions(),
        }
    }
}

/// DTO field configuration for an entity.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DtoConfig {
    /// Fields that cannot be changed after creation (excluded from Update DTO).
    #[serde(default)]
    pub immutable_fields: Vec<String>,
    /// Fields excluded from the list/summary response.
    #[serde(default)]
    pub list_exclude: Vec<String>,
    /// Fields that should be included in the list response even if normally excluded.
    #[serde(default)]
    pub list_include: Vec<String>,
    /// Fields to expand in the detail response (show related entity inline).
    #[serde(default)]
    pub expand_in_response: Vec<String>,
    /// Custom field grouping for documentation/SDK clarity.
    #[serde(default)]
    pub groups: HashMap<String, Vec<String>>,
}

/// Parse a `domains.toml` file into a `DomainConfig`.
pub fn parse_domain_config(path: &Path) -> Result<DomainConfig, DomainConfigError> {
    let content = std::fs::read_to_string(path)?;
    parse_domain_config_str(&content)
}

/// Parse a TOML string into a `DomainConfig`.
pub fn parse_domain_config_str(content: &str) -> Result<DomainConfig, DomainConfigError> {
    Ok(toml::from_str(content)?)
}

/// A single schema type's UI override — maps render contexts to component paths.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiOverrideEntry {
    pub detail: Option<String>,
    #[serde(rename = "list-cell")]
    pub list_cell: Option<String>,
    pub form: Option<String>,
    pub inline: Option<String>,
}

/// Top-level ui-overrides.toml config.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiOverrideConfig {
    #[serde(default)]
    pub overrides: HashMap<String, UiOverrideEntry>,
}

/// Parse a `ui-overrides.toml` file into a `UiOverrideConfig`.
pub fn parse_ui_overrides_config(path: &Path) -> Result<UiOverrideConfig, DomainConfigError> {
    let content = std::fs::read_to_string(path)?;
    parse_ui_overrides_config_str(&content)
}

/// Parse a TOML string into a `UiOverrideConfig`.
pub fn parse_ui_overrides_config_str(content: &str) -> Result<UiOverrideConfig, DomainConfigError> {
    if content.trim().is_empty() {
        return Ok(UiOverrideConfig::default());
    }
    Ok(toml::from_str(content)?)
}

/// Per-entity UI configuration from ui-domains.toml.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiEntityEntry {
    pub wizard: Option<bool>,
    pub wizard_config: Option<UiWizardConfig>,
}

/// Explicit wizard step configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiWizardConfig {
    #[serde(default)]
    pub steps: Vec<String>,
}

/// Top-level ui-domains.toml config.
/// Structure: { domain_name: { entity_name: UiEntityEntry } }
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiDomainConfig {
    #[serde(flatten)]
    pub domains: HashMap<String, HashMap<String, UiEntityEntry>>,
}

impl UiDomainConfig {
    /// Look up UI config for a specific entity.
    pub fn get_entity(&self, domain: &str, entity: &str) -> Option<&UiEntityEntry> {
        self.domains.get(domain)?.get(entity)
    }
}

/// Parse a `ui-domains.toml` file into a `UiDomainConfig`.
pub fn parse_ui_domains_config(path: &Path) -> Result<UiDomainConfig, DomainConfigError> {
    let content = std::fs::read_to_string(path)?;
    parse_ui_domains_config_str(&content)
}

/// Parse a TOML string into a `UiDomainConfig`.
pub fn parse_ui_domains_config_str(content: &str) -> Result<UiDomainConfig, DomainConfigError> {
    if content.trim().is_empty() {
        return Ok(UiDomainConfig::default());
    }
    Ok(toml::from_str(content)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
depends_on = []
entities = []

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
depends_on = ["common"]
entities = ["PayRunType"]
"#;
        let config = parse_domain_config_str(toml).unwrap();
        assert_eq!(config.domains.len(), 2);

        let common = &config.domains["common"];
        assert_eq!(common.label, "Common");
        assert!(common.depends_on.is_empty());

        let payroll = &config.domains["payroll"];
        assert_eq!(payroll.depends_on, vec!["common"]);
        assert_eq!(payroll.entities, vec!["PayRunType"]);
    }

    #[test]
    fn test_parse_entity_config() {
        let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
operations = ["create", "read", "update", "list"]
role = "root"

[domains.recruiting.entity_config.CandidateType.dto]
immutable_fields = ["ssn"]
list_exclude = ["detailed_notes"]
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let recruiting = &config.domains["recruiting"];
        let candidate = &recruiting.entity_config["CandidateType"];
        assert_eq!(candidate.role.as_deref(), Some("root"));
        assert_eq!(candidate.dto.immutable_fields, vec!["ssn"]);
        assert_eq!(candidate.dto.list_exclude, vec!["detailed_notes"]);
    }

    #[test]
    fn test_parse_workflow_config() {
        let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["PositionOpeningType"]

[domains.recruiting.entity_config.PositionOpeningType]
role = "root"
path_segment = "position-openings"

[domains.recruiting.entity_config.PositionOpeningType.workflow]
status_field = "document_status_code"
approval_status_field = "approval_status_code"
states = ["draft", "active", "closed", "cancelled"]
initial_state = "draft"
terminal_states = ["closed", "cancelled"]
generate_action_endpoints = true
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let recruiting = &config.domains["recruiting"];
        let po = &recruiting.entity_config["PositionOpeningType"];
        let wf = po.workflow.as_ref().expect("should have workflow config");
        assert_eq!(wf.status_field, "document_status_code");
        assert_eq!(
            wf.approval_status_field.as_deref(),
            Some("approval_status_code")
        );
        assert_eq!(wf.states.len(), 4);
        assert_eq!(wf.initial_state, "draft");
        assert_eq!(wf.terminal_states, vec!["closed", "cancelled"]);
        assert!(wf.generate_action_endpoints);
    }

    #[test]
    fn test_parse_workflow_transitions() {
        let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["PositionOpeningType"]

[domains.recruiting.entity_config.PositionOpeningType]
role = "root"

[domains.recruiting.entity_config.PositionOpeningType.workflow]
status_field = "document_status_code"
approval_status_field = "approval_status_code"
states = ["draft", "active", "closed", "cancelled"]
initial_state = "draft"
terminal_states = ["closed", "cancelled"]
generate_action_endpoints = true
status_codelist = "RecruitingDocumentStatusCodeList"
approval_status_codelist = "ApprovalStatusCodeList"

[domains.recruiting.entity_config.PositionOpeningType.workflow.transitions]
draft = ["active", "cancelled"]
active = ["closed", "cancelled"]

[domains.recruiting.entity_config.PositionOpeningType.workflow.dual_status_guards]
active = "Approved"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let po = &config.domains["recruiting"].entity_config["PositionOpeningType"];
        let wf = po.workflow.as_ref().unwrap();
        assert_eq!(wf.transitions.len(), 2);
        assert_eq!(wf.transitions["draft"], vec!["active", "cancelled"]);
        assert_eq!(wf.transitions["active"], vec!["closed", "cancelled"]);
        assert_eq!(
            wf.status_codelist.as_deref(),
            Some("RecruitingDocumentStatusCodeList")
        );
        assert_eq!(
            wf.approval_status_codelist.as_deref(),
            Some("ApprovalStatusCodeList")
        );
        assert_eq!(wf.dual_status_guards["active"], "Approved");
    }

    #[test]
    fn test_parse_workflow_data_guards_and_timers() {
        let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
role = "root"
workflow_file = "workflows/recruiting/candidate.toml"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let candidate = &config.domains["recruiting"].entity_config["CandidateType"];
        assert_eq!(
            candidate.workflow_file.as_deref(),
            Some("workflows/recruiting/candidate.toml")
        );
    }

    #[test]
    fn test_parse_entity_without_workflow() {
        let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]

[domains.common.entity_config.PersonType]
role = "root"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let person = &config.domains["common"].entity_config["PersonType"];
        assert!(person.workflow.is_none());
    }

    #[test]
    fn parse_new_override_format() {
        let toml = r#"
[defaults]
auto_discover = true

[domains.benefits]
label = "Benefits"
schema_dir = "benefits"
postgres_schema = "benefits"
depends_on = ["common"]
force_entities = ["CensusType"]
force_value_objects = ["CopayType", "CoinsuranceType"]
exclude = ["hros", "hyper4"]

[domains.benefits.entity_config.CensusType]
role = "root"
path_segment = "census"
tag = "Benefits"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let benefits = &config.domains["benefits"];
        assert_eq!(benefits.force_entities, vec!["CensusType"]);
        assert_eq!(
            benefits.force_value_objects,
            vec!["CopayType", "CoinsuranceType"]
        );
        assert_eq!(benefits.exclude, vec!["hros", "hyper4"]);
        assert!(config.defaults.auto_discover);
    }

    #[test]
    fn backward_compat_old_format() {
        let toml = r#"
[defaults]
auto_discover = false

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]
exclude_entities = ["NameType"]
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let common = &config.domains["common"];
        assert_eq!(common.entities, vec!["PersonType"]);
        assert_eq!(common.exclude_entities, vec!["NameType"]);
    }

    #[test]
    fn test_parse_defaults() {
        let toml = r#"
[domains.wellness]
label = "Wellness"
schema_dir = "wellness"
postgres_schema = "wellness"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let wellness = &config.domains["wellness"];
        assert!(wellness.depends_on.is_empty());
        assert!(wellness.entities.is_empty());
    }

    #[test]
    fn test_parse_search_config() {
        let toml = r#"
[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType]
role = "root"

[domains.recruiting.entity_config.CandidateType.search]
fts_columns = ["executive_summary", "objective"]
fts_language = "english"
embedding_columns = ["executive_summary"]
embedding_dimensions = 1536

[domains.recruiting.entity_config.CandidateType.search.fts_weights]
executive_summary = "A"
objective = "B"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let candidate = &config.domains["recruiting"].entity_config["CandidateType"];
        let search = &candidate.search;
        assert_eq!(
            search.fts_columns.as_deref(),
            Some(&["executive_summary".to_string(), "objective".to_string()][..])
        );
        assert_eq!(search.fts_weights["executive_summary"], "A");
        assert_eq!(search.fts_weights["objective"], "B");
        assert_eq!(search.fts_language, "english");
        assert_eq!(search.embedding_columns, vec!["executive_summary"]);
        assert_eq!(search.embedding_dimensions, 1536);
    }

    #[test]
    fn test_search_config_defaults() {
        let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]

[domains.common.entity_config.PersonType]
role = "root"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let person = &config.domains["common"].entity_config["PersonType"];
        // No search config → defaults
        assert!(person.search.fts_columns.is_none());
        assert!(person.search.fts_weights.is_empty());
        assert_eq!(person.search.fts_language, "english");
        assert!(person.search.embedding_columns.is_empty());
        assert_eq!(person.search.embedding_dimensions, 1536);
    }

    #[test]
    fn parse_max_bulk_size_defaults() {
        let toml_str = r#"
[defaults]
max_bulk_size = 200

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(config.defaults.max_bulk_size, 200);
    }

    #[test]
    fn parse_max_bulk_size_entity_override() {
        let toml_str = r#"
[defaults]
max_bulk_size = 100

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.CandidateType]
max_bulk_size = 500
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        let entity_cfg = config.domains["recruiting"]
            .entity_config
            .get("CandidateType")
            .unwrap();
        assert_eq!(entity_cfg.max_bulk_size, Some(500));
    }

    #[test]
    fn parse_max_bulk_size_absent_uses_default() {
        let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(config.defaults.max_bulk_size, 100);
    }

    #[test]
    fn test_search_config_fts_disabled() {
        let toml = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["PersonType"]

[domains.common.entity_config.PersonType]
role = "root"

[domains.common.entity_config.PersonType.search]
fts_columns = []
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let person = &config.domains["common"].entity_config["PersonType"];
        // Explicit empty = FTS disabled
        assert_eq!(person.search.fts_columns.as_deref(), Some(&[][..]));
    }

    #[test]
    fn test_hierarchy_field_parsing() {
        let toml_str = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[domains.common.entity_config.OrganizationType]
role = "root"
path_segment = "organizations"
tag = "Organizations"
hierarchy_field = "parent_organization_id"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        let entity = config.domains["common"]
            .entity_config
            .get("OrganizationType")
            .unwrap();
        assert_eq!(
            entity.hierarchy_field.as_deref(),
            Some("parent_organization_id")
        );
    }

    #[test]
    fn parse_types_import_prefix_default() {
        let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(
            config.defaults.types_import_prefix,
            "codegraph_type_contracts"
        );
    }

    #[test]
    fn parse_types_import_prefix_custom() {
        let toml_str = r#"
[defaults]
types_import_prefix = "crate::structured"

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(config.defaults.types_import_prefix, "crate::structured");
    }

    #[test]
    fn test_hierarchy_field_absent() {
        let toml_str = r#"
[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"

[domains.common.entity_config.SomeType]
role = "root"
path_segment = "some"
tag = "Some"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        let entity = config.domains["common"]
            .entity_config
            .get("SomeType")
            .unwrap();
        assert!(entity.hierarchy_field.is_none());
    }

    #[test]
    fn parse_auditable_defaults() {
        let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        let recruiting = &config.domains["recruiting"];
        assert!(recruiting.auditable.is_none());
    }

    #[test]
    fn parse_auditable_explicit() {
        let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
auditable = false
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        let recruiting = &config.domains["recruiting"];
        assert_eq!(recruiting.auditable, Some(false));
    }

    #[test]
    fn parse_tier_default() {
        let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(config.domains["recruiting"].tier, "extended");
    }

    #[test]
    fn parse_tier_explicit() {
        let toml_str = r#"
[defaults]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"
tier = "core"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(config.domains["recruiting"].tier, "core");
    }

    #[test]
    fn parse_types_import_prefix_domain_override() {
        let toml_str = r#"
[defaults]
types_import_prefix = "codegraph_type_contracts"

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.CandidateType]
role = "root"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(
            config.defaults.types_import_prefix,
            "codegraph_type_contracts"
        );
    }

    #[test]
    fn parse_types_import_prefix_entity_override() {
        let toml_str = r#"
[defaults]
types_import_prefix = "crate::structured"

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting/json"
postgres_schema = "recruiting"

[domains.recruiting.entity_config.CandidateType]
role = "root"
"#;
        let config = parse_domain_config_str(toml_str).unwrap();
        assert_eq!(config.defaults.types_import_prefix, "crate::structured");
    }

    #[test]
    fn parse_allow_include_present() {
        let toml = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["person", "deployment", "deployment.position"]
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let hr = &config.domains["hr"];
        let worker = &hr.entity_config["WorkerType"];
        assert_eq!(
            worker.allow_include.as_deref(),
            Some(&["person".to_string(), "deployment".to_string(), "deployment.position".to_string()][..])
        );
    }

    #[test]
    fn parse_allow_include_absent() {
        let toml = r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let worker = &config.domains["hr"].entity_config["WorkerType"];
        assert!(worker.allow_include.is_none());
    }

    #[test]
    fn parse_allow_include_empty() {
        let toml = r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = []
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let worker = &config.domains["hr"].entity_config["WorkerType"];
        assert_eq!(worker.allow_include, Some(vec![]));
    }

    #[test]
    fn parse_allow_include_non_ascii() {
        let toml = r#"
[domains.hr]
label = "HR"
schema_dir = "hr"
postgres_schema = "hr"
entities = ["WorkerType"]

[domains.hr.entity_config.WorkerType]
role = "root"
allow_include = ["person"]
"#;
        let config = parse_domain_config_str(toml).unwrap();
        let worker = &config.domains["hr"].entity_config["WorkerType"];
        assert_eq!(
            worker.allow_include.as_deref(),
            Some(&["person".to_string()][..])
        );
    }
}
