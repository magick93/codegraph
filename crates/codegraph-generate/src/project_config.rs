use std::path::Path;

use codegraph_config::{DomainConfig, UiDomainConfig, UiOverrideConfig};
use codegraph_core::traits::GraphQuerier;
use tera::Tera;

use crate::emdash;

// =============================================================================
// Project-level configuration for template rendering.
// Threaded explicitly: `run_generators_with_opts` receives a
// `ProjectConfig` (from `GeneratorOpts.project_config` or the default)
// and passes it to every generator and helper that needs it.
// =============================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectConfig {
    pub app_name: String,
    /// Rust crate name of the generated library (the `[lib] name` in the
    /// generated Cargo.toml). Test templates reference the crate by this name.
    #[serde(default)]
    pub lib_name: String,
    pub domain_types_crate: String,
    pub hooks_api_crate: String,
    pub api_title: String,
    pub generator_name: String,
    /// Path to the domain-types crate root (e.g. "crates/placekit-domain-types").
    /// Used by the scaffold generator to add a path dependency in Cargo.toml.
    /// Empty string means "no separate domain-types crate" (types live in the app).
    pub domain_types_base: String,
    pub hooks_api_base: String,
    pub extensions_base: String,
    pub app_config_base: String,
    pub decision_engine_base: String,
    pub codegraph_workflow_base: String,
    pub type_contracts_base: String,
    /// Database target dialect for SQL generation ("postgres" or "sqlite").
    /// Used by DB templates to branch on dialect-specific syntax.
    pub database_target: String,
    /// Persistence provider for entity/repository code generation ("sea_orm" or "cornucopia").
    /// Used by templates to select provider-specific rendering paths.
    pub persistence_provider: String,
    /// DTO serde key casing for domain-types DTOs: "snake" (default — keys stay
    /// at the Rust field names) or "camel" (`rename_all = "camelCase"`).
    pub dto_key_casing: String,
    /// Deployment topology for the generated application ("monolith" or "workers").
    /// Used by templates to select topology-specific rendering paths.
    pub deployment_topology: String,
    /// Namespace-aware module layout (issue #268): schemas carrying a
    /// namespace emit under namespace-derived module paths
    /// (`cdm.base.datetime` → `cdm/base/datetime/...`). Default false =
    /// flat domain layout, byte-identical output.
    #[serde(default)]
    pub namespace_layout: bool,
    /// Canonical expression IR (issue #278): IFML guards render from the
    /// persisted `expr_json` AST via the TypeScript lowering. Default false
    /// = byte-identical output.
    #[serde(default)]
    pub expr_ir: bool,
    /// Public-operations consumer (issue #279): schemas carrying
    /// `access = Public` whose entity config declares `public_operations`
    /// emit `TO PUBLIC` RLS policies and mount routes without permission
    /// layers. Default false = byte-identical output.
    #[serde(default)]
    pub public_operations_rls: bool,
    /// Import prefix for structured wrapper types in generated re-exports.
    /// Default: "codegraph_type_contracts".
    /// Domain crates should set this to their own crate or module path (e.g. "crate").
    pub types_import_prefix: String,
    /// API version prefix used in URL path construction (e.g. "v1" → `/api/v1/...`).
    #[serde(default)]
    pub api_version: String,
    /// Git revision SHA used for fallback path dependencies in generated Cargo.toml.
    /// When domain_types_base is empty, the domain types Cargo.toml uses this rev
    /// to reference codegraph-type-contracts as a git dependency.
    #[serde(default)]
    pub codegraph_rev: String,
    /// Whether atproto generators are enabled via profile feature flag.
    pub has_atproto: bool,
    /// Whether Fern SDK generation is enabled via profile feature flag.
    #[serde(default)]
    pub has_fern: bool,
    /// Fern SDK languages to generate (e.g. ["typescript", "rust"]).
    #[serde(default)]
    pub fern_sdk_languages: Vec<String>,
    /// Whether EmDash plugin generation is enabled via profile feature flag.
    #[serde(default)]
    pub has_emdash: bool,
    /// Whether function post-conditions emit `debug_assert!` checks
    /// (issue #263). Gated by the `function_postconditions` profile
    /// feature; default OFF = the emitted functions module carries none.
    #[serde(default)]
    pub has_function_postconditions: bool,
    /// Repo-relative base path for the community site's public pages
    /// (emdash plugin generator). Default: "apps/community-site/src/pages".
    #[serde(default = "default_emdash_site_pages_base")]
    pub emdash_site_pages_base: String,
    /// Repo-relative base path for the community site's e2e suite
    /// (emdash plugin generator). Default: "apps/community-site/e2e".
    #[serde(default = "default_emdash_site_e2e_base")]
    pub emdash_site_e2e_base: String,
    /// AT Protocol namespace authority (e.g. "nz.gravy").
    /// Read from domain config or hard-coded default. Empty string = atproto disabled.
    pub atproto_authority: String,
    /// AT Protocol tenancy mode: "shared_pds" or "per_org_pds".
    pub atproto_tenancy: String,
    /// AT Protocol float policy: what to do with JSON Schema "number" types.
    /// "reject", "string", "integer_scaled", or "unknown"
    pub atproto_float_policy: String,
    /// Raw `[patch.'https://github.com/magick93/codegraph.git']` entries emitted
    /// into the generated Cargo.toml (dev environments pin the local codegraph
    /// checkout via path overrides). Empty string = no patch section.
    #[serde(default)]
    pub cargo_patch: String,
    /// Raw extra lines appended to the generated Cargo.toml [dependencies]
    /// section (e.g. `url = "2"` or local path deps).
    #[serde(default)]
    pub extra_dependencies: String,
    /// Whether the generated Cargo.toml declares a `[workspace]` with a `cli/`
    /// member (hand-maintained CLI crates that are not emitted by the
    /// `cli_scaffold` generator). Defaults to false.
    #[serde(default)]
    pub cargo_workspace: bool,
    /// Resolved ux-rules (issue #293): the built-in `ux-default` pack,
    /// optionally merged with a project `--ux-rules` file. `None` = flag
    /// off / plan-less run without a CLI file — templates see `project.ux`
    /// only when rules resolved, so unset keeps output byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ux: Option<codegraph_config::UxRules>,
}

fn default_emdash_site_pages_base() -> String {
    emdash::DEFAULT_SITE_PAGES_BASE.to_string()
}

fn default_emdash_site_e2e_base() -> String {
    emdash::DEFAULT_SITE_E2E_BASE.to_string()
}

impl ProjectConfig {
    /// The persistence provider as a typed enum (parsed from the config string).
    pub fn persistence_provider_enum(&self) -> crate::profile::PersistenceProvider {
        crate::profile::PersistenceProvider::from_config(&self.persistence_provider)
    }

    /// True when entity/repository code generation targets the Cornucopia backend.
    pub fn is_cornucopia(&self) -> bool {
        matches!(
            self.persistence_provider_enum(),
            crate::profile::PersistenceProvider::Cornucopia
        )
    }

    /// The deployment topology as a typed enum (parsed from the config string).
    ///
    /// The string is validated at `BuildPlan` construction time, so an invalid
    /// value here falls back to the default (Monolith).
    pub fn deployment_topology_enum(&self) -> crate::profile::DeploymentTopology {
        crate::profile::DeploymentTopology::from_config(&self.deployment_topology)
            .unwrap_or_default()
    }

    /// True when the generated app is split into per-domain Cloudflare Workers.
    pub fn is_workers_topology(&self) -> bool {
        matches!(
            self.deployment_topology_enum(),
            crate::profile::DeploymentTopology::Workers
        )
    }
}

/// Namespace-derived module directory for a schema under the
/// `namespace_layout` gate (issue #268).
///
/// Returns `Some(("cdm/base/datetime", "cdm::base::datetime"))` when the
/// gate is ON and the schema carries a namespace, `None` otherwise (gate
/// OFF or namespace-less schema) — callers fall back to the flat
/// domain-derived paths, keeping default output byte-identical.
pub fn namespace_module_dir(
    schema: &codegraph_core::types::SchemaNode,
    project: &ProjectConfig,
) -> Option<(String, String)> {
    if !project.namespace_layout {
        return None;
    }
    let ns = schema.namespace.as_deref()?.trim();
    if ns.is_empty() {
        return None;
    }
    Some((
        codegraph_core::types::namespace_module_path(ns),
        codegraph_core::types::namespace_module_rust(ns),
    ))
}

/// The namespace-derived directory segment for a raw namespace FQN under
/// the `namespace_layout` gate (`Some("cdm/base/datetime")`), `None` when
/// flat (gate off / namespace-less). Path-form counterpart of
/// [`namespace_rust_prefix`].
pub fn namespace_dir_prefix(ns: Option<&str>, project: &ProjectConfig) -> Option<String> {
    if !project.namespace_layout {
        return None;
    }
    let ns = ns?.trim();
    (!ns.is_empty()).then(|| codegraph_core::types::namespace_module_path(ns))
}

/// The namespace-derived Rust module path prefix for a raw namespace FQN
/// under the `namespace_layout` gate (`Some("cdm::base::datetime")`),
/// `None` when flat.
pub fn namespace_rust_prefix(ns: Option<&str>, project: &ProjectConfig) -> Option<String> {
    if !project.namespace_layout {
        return None;
    }
    let ns = ns?.trim();
    (!ns.is_empty()).then(|| codegraph_core::types::namespace_module_rust(ns))
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            app_name: "app".into(),
            lib_name: "cosmos".into(),
            domain_types_crate: "domain_types".into(),
            hooks_api_crate: String::new(),
            api_title: "HR Open API".into(),
            generator_name: "codegraph".into(),
            domain_types_base: String::new(),
            hooks_api_base: String::new(),
            extensions_base: String::new(),
            app_config_base: String::new(),
            decision_engine_base: String::new(),
            codegraph_workflow_base: String::new(),
            type_contracts_base: String::new(),
            codegraph_rev: String::new(),
            database_target: "postgres".to_string(),
            persistence_provider: "sea_orm".to_string(),
            dto_key_casing: "snake".to_string(),
            deployment_topology: "monolith".to_string(),
            namespace_layout: false,
            expr_ir: false,
            public_operations_rls: false,
            types_import_prefix: "codegraph_type_contracts".into(),
            has_atproto: false,
            has_fern: false,
            fern_sdk_languages: vec!["typescript".into()],
            has_emdash: false,
            has_function_postconditions: false,
            emdash_site_pages_base: default_emdash_site_pages_base(),
            emdash_site_e2e_base: default_emdash_site_e2e_base(),
            atproto_authority: String::new(),
            atproto_tenancy: "shared_pds".to_string(),
            atproto_float_policy: "integer_scaled".to_string(),
            cargo_patch: String::new(),
            extra_dependencies: String::new(),
            cargo_workspace: false,
            api_version: "v1".into(),
            ux: None,
        }
    }
}

/// An entity in the generation order with its graph schema_id and domain.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerationEntry {
    pub schema_title: String,
    pub domain: String,
    pub pg_schema: String,
    pub is_cyclic: bool,
}

/// Configuration for the code generation pipeline.
///
/// Groups all the various config inputs so generator entry points
/// don't need 10+ positional arguments.
pub struct GeneratorOpts<'a> {
    pub db: &'a dyn GraphQuerier,
    pub config: &'a DomainConfig,
    pub output_dir: &'a Path,
    pub tera: &'a Tera,
    pub ui_overrides: &'a UiOverrideConfig,
    pub ui_domains: &'a UiDomainConfig,
    /// Root of the HR Open Standards schema tree.
    /// Pass an empty path for tests that don't need real codelist files.
    pub schema_base_dir: &'a Path,
    /// Path to the optional seed.toml config file for demo seed data.
    /// If `None` or the file doesn't exist, the generator falls back to
    /// hardcoded HR-specific demo data.
    pub seed_config: Option<&'a Path>,
    /// Override target dir for domain-types crate generators.
    /// `None` defaults to the main output directory.
    pub domain_types_base: Option<&'a Path>,
    /// Override target dir for hooks generators.
    pub hooks_base: Option<&'a Path>,
    /// Extension points config for integration infrastructure generators.
    pub ext_points: Option<&'a codegraph_ext_points::ExtensionPointsConfig>,
    /// Build profile plan controlling which generators to run.
    pub build_plan: Option<&'a crate::profile::BuildPlan>,
    /// IFML framework targets (e.g. "svelte", "react").
    /// If empty, defaults to `["svelte"]` at dispatch.
    pub ifml_frameworks: Vec<String>,
    /// Optional IFML component mappings (`ifml-components.toml`). `None` or
    /// empty renders all components with the built-in templates.
    pub ifml_components: Option<&'a codegraph_config::IfmlComponentMappings>,
    /// Resolved ux-rules (issue #293): the built-in `ux-default` pack,
    /// optionally merged with a project `--ux-rules` file. `None` = flag
    /// off — generators keep their pre-#293 rendering byte-identical.
    pub ux_rules: Option<codegraph_config::UxRules>,
    /// Project-level config injected into all template contexts.
    pub project_config: Option<&'a ProjectConfig>,
    /// EmDash plugin packages config (plugins.toml), loaded by the CLI
    /// wrapper when the profile enables the `emdash_plugins` feature.
    /// `None` (or an empty map) disables the emdash generators.
    pub emdash_plugins: Option<crate::emdash::EmdashPluginsConfig>,
    /// Directory of the `domains.toml` config used for this run. Optional
    /// sibling configs (`reports.toml`) are discovered relative to this
    /// directory instead of the process current directory, so server
    /// composition no longer depends on the invoking shell's cwd.
    /// `None` falls back to the current directory (legacy behavior).
    pub domain_config_dir: Option<&'a Path>,
}
