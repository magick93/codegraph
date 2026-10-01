use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::DomainConfigError;

mod domain;
mod entity;
mod namespace;

#[cfg(test)]
mod tests;

use namespace::deserialize_namespaces;

pub use domain::{DomainDependency, DomainEntry};
pub use entity::{
    default_min_roles, default_roles_hierarchy, ApprovalChainDef, ApprovalStepDef, DataGuard,
    DtoConfig, EntityConfig, ErrorDefConfig, PermissionConfig, RbacConfig, SearchConfig, TimerDef,
    TreeIncludeConfig, WorkflowConfig,
};
pub use namespace::NamespaceEntry;

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
    /// Namespace declarations (issue #267). Keys are dotted FQNs. Both TOML
    /// spellings are accepted and normalize to the same FQN:
    /// `[namespaces."cdm.base.datetime"]` (quoted — the recommended form)
    /// and `[namespaces.cdm.base.datetime]` (nested tables flattened to
    /// dotted paths). The optional `domain` key assigns the namespace to a
    /// bounded context (optional, many-to-one). Namespaces discovered from
    /// source without an explicit entry are allowed — the declared set is
    /// only the validation baseline.
    #[serde(default, deserialize_with = "deserialize_namespaces")]
    pub namespaces: HashMap<String, NamespaceEntry>,
    /// Role hierarchy for the DB-level role policies (#169). Absent → the
    /// basejump default hierarchy.
    #[serde(default)]
    pub rbac: Option<RbacConfig>,
}

fn default_app_name() -> String {
    "codegraph-app".to_string()
}

fn default_max_bulk_size() -> usize {
    100
}

fn default_generation_mode() -> String {
    "full".to_string()
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
    /// Default generation mode for entities.
    /// "full" (default): generate everything.
    /// "handler_only": generate handler but not router.
    /// "ddd_only": generate DDD layer (repo, command, query) but not API layer.
    /// "none": skip all generation for this entity.
    #[serde(default = "default_generation_mode")]
    pub generation_mode: String,
    /// API version prefix used in URL path construction (e.g. "v1" → `/api/v1/...`).
    #[serde(default = "default_api_version")]
    pub api_version: String,
}

fn default_api_version() -> String {
    "v1".to_string()
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
            generation_mode: default_generation_mode(),
            api_version: default_api_version(),
        }
    }
}

impl DefaultsConfig {
    /// Strip the configured type suffix from a schema title.
    pub fn strip_suffix(&self, title: &str) -> String {
        codegraph_naming::strip_suffix(title, &self.type_suffix)
    }
}

/// Parse a `domains.toml` file into a `DomainConfig`.
pub fn parse_domain_config(path: &Path) -> Result<DomainConfig, DomainConfigError> {
    let content = std::fs::read_to_string(path)?;
    parse_domain_config_str(&content)
}

/// Parse a TOML string into a `DomainConfig`.
pub fn parse_domain_config_str(content: &str) -> Result<DomainConfig, DomainConfigError> {
    let config: DomainConfig = toml::from_str(content)?;
    validate_rbac_config(&config)?;
    validate_append_only_config(&config)?;
    validate_dependencies(&config)?;
    Ok(config)
}

/// Validate the #169 RBAC config: `min_roles` keys are known operations and
/// `min_roles` values rank in the configured hierarchy. Unknown roles would
/// silently deny every request once the policies are generated, so this is a
/// hard parse error.
fn validate_rbac_config(config: &DomainConfig) -> Result<(), DomainConfigError> {
    const OPS: [&str; 5] = ["create", "read", "update", "delete", "list"];

    let hierarchy = config
        .rbac
        .as_ref()
        .map(|r| r.hierarchy_or_default())
        .unwrap_or_else(default_roles_hierarchy);
    if hierarchy.is_empty() {
        return Err(DomainConfigError::Invalid(
            "[rbac] roles_hierarchy must not be empty".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for role in &hierarchy {
        if !seen.insert(role.as_str()) {
            return Err(DomainConfigError::Invalid(format!(
                "[rbac] roles_hierarchy lists {role:?} more than once"
            )));
        }
    }

    for (domain, entry) in &config.domains {
        for (entity, ec) in &entry.entity_config {
            let Some(min_roles) = &ec.permissions.min_roles else {
                continue;
            };
            for (op, role) in min_roles {
                if !OPS.contains(&op.as_str()) {
                    return Err(DomainConfigError::Invalid(format!(
                        "[domains.{domain}.entity_config.{entity}.permissions.min_roles] \
                         unknown operation {op:?} (expected one of {OPS:?})"
                    )));
                }
                if !hierarchy.contains(role) {
                    return Err(DomainConfigError::Invalid(format!(
                        "[domains.{domain}.entity_config.{entity}.permissions.min_roles] \
                         role {role:?} is not in the [rbac] roles_hierarchy"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Append-only entities (`append_only = true`, issue #284) must not carry
/// `update`/`delete` in their effective operations — an append-only table
/// has no UPDATE/DELETE grants, so the API surface must agree (parse-time
/// config error, gRPC/ops strict-feature precedent).
fn validate_append_only_config(config: &DomainConfig) -> Result<(), DomainConfigError> {
    for (domain, entry) in &config.domains {
        for (entity, ec) in &entry.entity_config {
            if !ec.is_append_only() {
                continue;
            }
            let effective = ec
                .operations
                .clone()
                .unwrap_or_else(|| config.defaults.operations.clone());
            for op in ["update", "delete"] {
                if effective.iter().any(|o| o == op) {
                    return Err(DomainConfigError::Invalid(format!(
                        "[domains.{domain}.entity_config.{entity}] append_only = true \
                         forbids {op:?} in the effective operations (configured: {effective:?})"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Domain dependencies (issue #276): every entry must name a foreign
/// domain, a source path, and a version pin; a domain cannot depend on
/// itself, and the same foreign domain cannot be pinned twice by one
/// consumer domain.
fn validate_dependencies(config: &DomainConfig) -> Result<(), DomainConfigError> {
    for (domain, entry) in &config.domains {
        let mut seen = std::collections::HashSet::new();
        for dep in &entry.dependencies {
            let at = format!("[domains.{domain}.dependencies]");
            if dep.domain.is_empty() {
                return Err(DomainConfigError::Invalid(format!(
                    "{at} dependency `domain` must not be empty"
                )));
            }
            if dep.source.trim().is_empty() {
                return Err(DomainConfigError::Invalid(format!(
                    "{at} dependency `{}` has an empty `source` path",
                    dep.domain
                )));
            }
            if dep.version.trim().is_empty() {
                return Err(DomainConfigError::Invalid(format!(
                    "{at} dependency `{}` has an empty `version` pin",
                    dep.domain
                )));
            }
            if dep.domain == *domain {
                return Err(DomainConfigError::Invalid(format!(
                    "{at} domain {domain:?} cannot depend on itself"
                )));
            }
            if !seen.insert(dep.domain.as_str()) {
                return Err(DomainConfigError::Invalid(format!(
                    "{at} domain {domain:?} is pinned more than once"
                )));
            }
        }
    }
    Ok(())
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
