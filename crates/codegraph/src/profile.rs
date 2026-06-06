use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

/// Generator kind — determines how the build planner invokes the generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorKind {
    Entity,
    Domain,
    Global,
}

/// Which target (project output section) a generator belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeneratorTarget {
    Api,
    Ui,
    Cli,
    Mobile,
    Common,
}

impl GeneratorTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            GeneratorTarget::Api => "api",
            GeneratorTarget::Ui => "ui",
            GeneratorTarget::Cli => "cli",
            GeneratorTarget::Mobile => "mobile",
            GeneratorTarget::Common => "common",
        }
    }
}

/// A capability descriptor for a single generator.
///
/// Each generator's `name()` return value is the canonical ID used in
/// `[profile.X.generators]` lists. The registry maps those names to
/// their kind, target, and feature requirements for validation.
#[derive(Debug, Clone)]
pub struct GeneratorCapability {
    pub name: String,
    pub kind: GeneratorKind,
    pub target: GeneratorTarget,
    pub features_required: Vec<String>,
    pub features_optional: Vec<String>,
}

/// The full registry of generator capabilities.
///
/// Indexed by generator name (matching the `name()` trait method).
/// Use [`capabilities`] to get the singleton registry.
pub struct CapabilityRegistry {
    pub generators: HashMap<String, GeneratorCapability>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            generators: capabilities(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&GeneratorCapability> {
        self.generators.get(name)
    }

    pub fn validate_profile(&self, profile: &ResolvedProfile) -> Result<()> {
        for (section_name, section) in &profile.sections {
            for gen_name in &section.generators {
                let cap = self.get(gen_name).ok_or_else(|| {
                    let known: Vec<_> = self.generators.keys().cloned().collect();
                    Error::Config(format!(
                        "generator \"{gen_name}\" in [{}] section not found in capability registry. \
                         known generators: {known:?}",
                        section_name
                    ))
                })?;

                // Validate that the generator's target matches the section.
                // Common generators can appear in any section.
                let section_target = match section_name.as_str() {
                    "api" => GeneratorTarget::Api,
                    "ui" => GeneratorTarget::Ui,
                    "cli" => GeneratorTarget::Cli,
                    "mobile" => GeneratorTarget::Mobile,
                    _ => GeneratorTarget::Common,
                };

                if cap.target != GeneratorTarget::Common && cap.target != section_target {
                    return Err(Error::Config(format!(
                        "generator \"{gen_name}\" (target={}) cannot be used in [{}] section",
                        cap.target.as_str(),
                        section_name
                    )));
                }

                // Validate feature requirements.
                for req in &cap.features_required {
                    let val = profile.features.get(req);
                    let enabled = val.and_then(|v| v.as_bool()).unwrap_or(false);
                    if !enabled {
                        return Err(Error::Config(format!(
                            "generator \"{gen_name}\" requires feature \"{req}\" but it is \
                             not enabled in the profile features"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Build Plan ───────────────────────────────────────────────────────────────

/// Resolved execution plan for a profile.
///
/// Groups generator names by their [`GeneratorKind`] so the generator dispatch
/// in `run_generators_with_opts` can filter which generators to instantiate.
#[derive(Debug, Clone)]
pub struct BuildPlan {
    /// Entity generator names to run (per-entity, across all sections).
    pub entity_generators: Vec<String>,
    /// Domain generator names to run (per-domain, across all sections).
    pub domain_generators: Vec<String>,
    /// Global generator names to run (once, across all sections).
    pub global_generators: Vec<String>,
    /// Post-gen scripts collected from all sections, ordered alphabetically by section name.
    pub post_gen_scripts: Vec<(String, Vec<String>)>,
    /// IFML framework targets configured for this build.
    pub ifml_frameworks: Vec<IfmlFrameworkTarget>,
    /// Optional template pack directory override from the selected variant.
    pub template_pack_path: Option<PathBuf>,
}

impl BuildPlan {
    /// Construct a build plan from a resolved profile and the capability registry.
    ///
    /// Returns an error if any generator name is unknown or if feature requirements
    /// are not met.
    pub fn from_profile(profile: &ResolvedProfile, registry: &CapabilityRegistry) -> Result<Self> {
        let expanded_sections =
            Self::expand_ifml_sections(&profile.sections, &profile.ifml_frameworks);

        let validation_profile = ResolvedProfile {
            sections: expanded_sections.clone(),
            ..profile.clone()
        };
        registry.validate_profile(&validation_profile)?;

        let mut entity_gens = Vec::new();
        let mut domain_gens = Vec::new();
        let mut global_gens = Vec::new();
        let mut post_gen_scripts = Vec::new();

        let mut section_names: Vec<&String> = expanded_sections.keys().collect();
        section_names.sort();
        for section_name in section_names {
            let section = &expanded_sections[section_name];
            if section.generators.is_empty() {
                continue;
            }
            for gen_name in &section.generators {
                let cap = registry.get(gen_name).expect("already validated");
                match cap.kind {
                    GeneratorKind::Entity => {
                        if !entity_gens.contains(gen_name) {
                            entity_gens.push(gen_name.clone());
                        }
                    }
                    GeneratorKind::Domain => {
                        if !domain_gens.contains(gen_name) {
                            domain_gens.push(gen_name.clone());
                        }
                    }
                    GeneratorKind::Global => {
                        if !global_gens.contains(gen_name) {
                            global_gens.push(gen_name.clone());
                        }
                    }
                }
            }
            if !section.scripts.is_empty() {
                post_gen_scripts.push((section_name.clone(), section.scripts.clone()));
            }
        }

        Ok(BuildPlan {
            entity_generators: entity_gens,
            domain_generators: domain_gens,
            global_generators: global_gens,
            post_gen_scripts,
            ifml_frameworks: profile.ifml_frameworks.clone(),
            template_pack_path: profile.template_pack_path.clone(),
        })
    }

    /// Expand IFML generators in sections based on configured frameworks.
    ///
    /// When `ifml_frameworks` is non-empty, each `ifml_route` generator is
    /// replaced with `ifml_route_{framework}` for every configured framework,
    /// and similarly for `ifml_navigation`. If no frameworks are configured,
    /// sections are returned unchanged (backward compatible).
    fn expand_ifml_sections(
        sections: &HashMap<String, ResolvedSection>,
        ifml_frameworks: &[IfmlFrameworkTarget],
    ) -> HashMap<String, ResolvedSection> {
        if ifml_frameworks.is_empty() {
            return sections.clone();
        }

        sections
            .iter()
            .map(|(name, section)| {
                let generators: Vec<String> = section
                    .generators
                    .iter()
                    .flat_map(|gen| match gen.as_str() {
                        "ifml_route" | "ifml_navigation" => ifml_frameworks
                            .iter()
                            .map(|fw| format!("{}_{}", gen, fw.name))
                            .collect::<Vec<_>>(),
                        _ => vec![gen.clone()],
                    })
                    .collect();

                (
                    name.clone(),
                    ResolvedSection {
                        generators,
                        ..section.clone()
                    },
                )
            })
            .collect()
    }

    /// Returns true if the named entity generator is in the plan.
    pub fn has_entity_gen(&self, name: &str) -> bool {
        self.entity_generators.iter().any(|g| g == name)
    }

    /// Returns true if the named domain generator is in the plan.
    pub fn has_domain_gen(&self, name: &str) -> bool {
        self.domain_generators.iter().any(|g| g == name)
    }

    /// Returns true if the named global generator is in the plan.
    pub fn has_global_gen(&self, name: &str) -> bool {
        self.global_generators.iter().any(|g| g == name)
    }

    /// Returns the IFML framework targets configured for this build plan.
    pub fn ifml_framework_targets(&self) -> Vec<&IfmlFrameworkTarget> {
        self.ifml_frameworks.iter().collect()
    }

    /// Returns the template override directories for template resolution.
    ///
    /// When the profile variant specifies a `template_pack`, the resolved
    /// directory is returned so generators can look there first for overrides.
    pub fn template_override_dirs(&self) -> Vec<&Path> {
        self.template_pack_path.iter().map(|p| p.as_path()).collect()
    }
}

/// Build the registry of all known generator capabilities.
///
/// This is the single source of truth for which generators exist and what
/// they require. When adding a new generator, add its entry here.
fn capabilities() -> HashMap<String, GeneratorCapability> {
    let mut map = base_capabilities();
    // Merge IFML generator capabilities
    for cap in crate::generate::ifml::profiles::ifml_capabilities() {
        map.insert(cap.name.clone(), cap);
    }
    map
}

/// Base capabilities without IFML generators.
fn base_capabilities() -> HashMap<String, GeneratorCapability> {
    use GeneratorKind::{Domain, Entity, Global};
    use GeneratorTarget::*;

    #[rustfmt::skip]
    let entries = vec![
        // ── Entity generators ──────────────────────────────────────────
        cap("ddl",                  Entity, Api,  &[], &[]),
        cap("sea_orm_entity",       Entity, Api,  &[], &[]),
        cap("codelist",             Entity, Api,  &[], &[]),
        cap("dto",                  Entity, Api,  &[], &[]),
        cap("repository",           Entity, Api,  &[], &[]),
        cap("command",              Entity, Api,  &[], &[]),
        cap("query",                Entity, Api,  &[], &[]),
        cap("event",                Entity, Api,  &[], &[]),
        cap("handler",              Entity, Api,  &[], &[]),
        cap("workflow_action",      Entity, Api,  &[], &[]),
        cap("media_route",          Entity, Api,  &[], &[]),
        cap("test",                 Entity, Api,  &[], &[]),
        cap("lifecycle_trait",      Entity, Api,  &[], &[]),
        cap("domain_types_dto",     Entity, Api,  &[], &[]),
        cap("domain_types_query_service", Entity, Api, &[], &[]),

        cap("ui_page",              Entity, Ui,   &[], &[]),
        cap("ui_form",              Entity, Ui,   &[], &[]),
        cap("ui_store",             Entity, Ui,   &[], &[]),
        cap("ui_e2e_test",          Entity, Ui,   &[], &[]),
        cap("playwright-entity",    Entity, Ui,   &[], &[]),
        cap("ui_descriptor",        Entity, Ui,   &[], &[]),
        cap("ui-shell",             Entity, Ui,   &[], &[]),

        cap("cli_command",          Entity, Cli,  &[], &[]),

        // ── Domain generators ──────────────────────────────────────────
        cap("router",               Domain, Api,  &[], &[]),
        cap("links",                Domain, Api,  &[], &[]),
        cap("ui-domain-layout",     Domain, Ui,   &[], &[]),
        cap("cli_domain",           Domain, Cli,  &[], &[]),

        // ── Global generators ──────────────────────────────────────────
        cap("basejump_setup",       Global, Common, &[], &[]),
        cap("pgmq_setup",           Global, Common, &[], &[]),
        cap("platform_schema",      Global, Common, &[], &[]),
        cap("workflow_seed",        Global, Common, &[], &[]),
        cap("openapi",              Global, Common, &[], &[]),
        cap("scaffold",             Global, Common, &[], &[]),
        cap("ui_scaffold",          Global, Ui,    &[], &[]),
        cap("ui_types",             Global, Ui,    &[], &[]),
        cap("ui_codelist",          Global, Ui,    &[], &[]),
        cap("ui_orgchart",          Global, Ui,    &[], &[]),
        cap("hook_registry",        Global, Common, &[], &[]),
        cap("domain_types_scaffold", Global, Common, &[], &[]),
        cap("report_views",         Global, Common, &[], &[]),
        cap("cli_scaffold",         Global, Cli,   &[], &[]),
        cap("playwright-global",    Global, Ui,    &[], &[]),
        cap("integration_tables",   Global, Common, &[], &[]),
        cap("integration_config",   Global, Common, &[], &[]),
        cap("integration_dispatch", Global, Common, &[], &[]),
        cap("integration_catalog",  Global, Common, &[], &[]),
        cap("webhook_dispatch",     Global, Common, &[], &[]),
        cap("webhook_endpoint_api", Global, Common, &[], &[]),

        // ── gRPC generators ────────────────────────────────────────────
        cap("grpc_proto",           Entity,  Api, &["grpc_backend"], &[]),
        cap("grpc_service",         Entity,  Api, &["grpc_backend"], &[]),
        cap("grpc_router",          Domain,  Api, &["grpc_backend"], &[]),
        cap("grpc_scaffold",        Global,  Api, &["grpc_backend"], &[]),
    ];

    entries.into_iter().map(|c| (c.name.clone(), c)).collect()
}

fn cap(
    name: &str,
    kind: GeneratorKind,
    target: GeneratorTarget,
    required: &[&str],
    optional: &[&str],
) -> GeneratorCapability {
    GeneratorCapability {
        name: name.to_string(),
        kind,
        target,
        features_required: required.iter().map(|s| s.to_string()).collect(),
        features_optional: optional.iter().map(|s| s.to_string()).collect(),
    }
}

/// A single IFML framework target to generate for.
///
/// Each entry represents one framework (e.g. "svelte", "react") with
/// optional output directory override and target section.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct IfmlFrameworkTarget {
    pub name: String,
    pub output: Option<PathBuf>,
    #[serde(default = "default_framework_target")]
    pub target: String,
}

fn default_framework_target() -> String {
    "ui".to_string()
}

/// IFML framework configuration block parsed from `[profiles.X.ifml]`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProfileIfmlConfig {
    #[serde(default)]
    pub frameworks: Vec<IfmlFrameworkTarget>,
}

/// Top-level profile configuration.
///
/// Profiles are loaded from `profiles.toml`. Each profile declares one or more
/// project output sections (`[api]`, `[ui]`, `[cli]`, `[mobile]`) that specify
/// which generators to invoke, where to write output, and optional post-generation
/// scripts whose exit code gates success.
#[derive(Debug, Deserialize)]
pub struct ProfilesConfig {
    pub profiles: HashMap<String, ProfileDef>,
}

/// A single profile definition.
///
/// Every profile carries meta data (`[profiles.foo.meta]`), global features,
/// and one or more project output sections. Variants are resolved via the
/// `variants` map; the `--variant` CLI flag selects which variant to merge.
#[derive(Debug, Deserialize)]
pub struct ProfileDef {
    #[serde(default)]
    pub meta: Option<ProfileMeta>,

    #[serde(default)]
    pub features: Option<toml::Table>,

    /// Project output sections keyed by target name: `api`, `ui`, `cli`, `mobile`, etc.
    #[serde(flatten)]
    pub sections: HashMap<String, ProfileSection>,

    /// Optional variant overrides, keyed by variant name.
    #[serde(default)]
    pub variants: Option<HashMap<String, ProfileVariant>>,

    /// IFML framework configuration (multiplier targets).
    #[serde(default)]
    pub ifml: Option<ProfileIfmlConfig>,
}

/// Meta data for a profile.
#[derive(Debug, Deserialize, Clone)]
pub struct ProfileMeta {
    pub name: String,
    pub version: String,
    pub description: String,

    #[serde(default)]
    pub authors: Vec<String>,

    #[serde(default)]
    pub deprecates: Vec<String>,

    #[serde(default)]
    pub since: Option<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    /// Override the generated app's Rust crate name (default: "hr-app").
    #[serde(default)]
    pub app_name: Option<String>,

    /// Override the generated domain types crate module name (default: "hr_domain_types").
    #[serde(default)]
    pub domain_types_crate: Option<String>,

    /// Override the generated hooks API crate module name ("" disables hooks, default: "hr_hooks_api").
    #[serde(default)]
    pub hooks_api_crate: Option<String>,

    /// Override the OpenAPI info title (default: "HR Open API").
    #[serde(default)]
    pub api_title: Option<String>,

    /// Override the generator name in "Generated by" headers (default: "hr-graph").
    #[serde(default)]
    pub generator_name: Option<String>,

    /// Override the target directory for domain-types crate generators.
    /// Relative paths are resolved from the project root (CWD).
    /// Default: compiled-in workspace root (crates/hr-domain-types/src).
    #[serde(default)]
    pub domain_types_base: Option<String>,

    /// Override the target directory for hooks-api crate generators.
    /// Relative paths are resolved from the project root (CWD).
    /// Default: compiled-in workspace root.
    #[serde(default)]
    pub hooks_api_base: Option<String>,

    /// Override the target directory for extensions crate generators.
    #[serde(default)]
    pub extensions_base: Option<String>,

    /// Override the target directory for the app config crate.
    #[serde(default)]
    pub app_config_base: Option<String>,

    /// Override the target directory for the decision engine crate.
    #[serde(default)]
    pub decision_engine_base: Option<String>,

    /// Override the target directory for the codegraph-workflow crate.
    #[serde(default)]
    pub codegraph_workflow_base: Option<String>,

    /// Override the target directory for the codegraph-type-contracts crate.
    #[serde(default)]
    pub type_contracts_base: Option<String>,
}

/// A single project output section (e.g. `[api]`, `[ui]`).
///
/// Each section declares which generators to invoke, where to write output,
/// and optional post-generation scripts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSection {
    /// Generator names to invoke for this target (e.g. `["api_server", "db_migrations"]`).
    pub generators: Vec<String>,

    /// Output directory for this target's generated files.
    ///
    /// Can be overridden per-profile; otherwise inherited from the CLI `--output` flag.
    #[serde(default)]
    pub output: Option<String>,

    /// Post-generation scripts to run after all generators in this section complete.
    /// Each script is executed via `sh -c <cmd>`. Non-zero exit codes cause the
    /// pipeline to abort (after all generators have run and written files to disk).
    #[serde(default)]
    pub scripts: Option<ProfileScripts>,
}

/// Post-generation scripts for a project output section.
#[derive(Debug, Deserialize)]
pub struct ProfileScripts {
    /// Commands to run after generation completes.  Executed in order via `sh -c`.
    #[serde(default)]
    pub post_gen: Vec<String>,
}

/// A variant override for a profile.
///
/// Variants are defined inline in the profile TOML and selected via `--variant`.
/// Each variant can override sections, features, and the template pack.
#[derive(Debug, Deserialize)]
pub struct ProfileVariant {
    /// Override project output sections for this variant.
    #[serde(flatten)]
    pub sections: HashMap<String, ProfileSection>,

    /// Override features for this variant (merged over base profile features).
    #[serde(default)]
    pub features: Option<toml::Table>,

    /// Override template pack for this variant.
    #[serde(default)]
    pub template_pack: Option<String>,
}

/// Resolved profile after variant merging and composition.
///
/// This is what the build planner consumes. All `Option`s have been filled in
/// from defaults and variants merged.
#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub meta: ProfileMeta,
    pub features: toml::Table,
    pub sections: HashMap<String, ResolvedSection>,
    pub ifml_frameworks: Vec<IfmlFrameworkTarget>,
    /// Optional template pack directory override from the selected variant.
    /// When set, the build planner uses this as an additional template source
    /// (resolved relative to the profiles.toml directory when relative).
    pub template_pack_path: Option<PathBuf>,
}

/// A single resolved project output section ready for the build planner.
#[derive(Debug, Clone)]
pub struct ResolvedSection {
    pub generators: Vec<String>,
    pub output: Option<String>,
    pub scripts: Vec<String>,
}

/// Parse a `profiles.toml` file and return the resolved profile for the given name/variant.
///
/// When `variant` is `Some`, variant overrides are merged into the base profile.
pub fn load_and_resolve_profile(
    profiles_path: &Path,
    profile_name: &str,
    variant: Option<&str>,
) -> Result<ResolvedProfile> {
    let content = std::fs::read_to_string(profiles_path)
        .map_err(|e| Error::Config(format!("failed to read profiles config: {e}")))?;

    let config: ProfilesConfig = toml::from_str(&content)
        .map_err(|e| Error::Config(format!("failed to parse profiles config: {e}")))?;

    let def = config.profiles.get(profile_name).ok_or_else(|| {
        let available: Vec<_> = config.profiles.keys().collect();
        Error::Config(format!(
            "profile \"{profile_name}\" not found in {}. available: {:?}",
            profiles_path.display(),
            available
        ))
    })?;

    let mut resolved = resolve_profile(def, variant)?;

    // Resolve template_pack path from the variant, relative to profiles.toml directory.
    if let Some(variant_name) = variant {
        if let Some(variant_def) = def
            .variants
            .as_ref()
            .and_then(|vs| vs.get(variant_name))
        {
            resolved.template_pack_path = variant_def.template_pack.as_ref().map(|tp| {
                let path = PathBuf::from(tp);
                if path.is_absolute() {
                    path
                } else {
                    profiles_path
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(&path)
                }
            });
        }
    }

    Ok(resolved)
}

fn resolve_profile(def: &ProfileDef, variant: Option<&str>) -> Result<ResolvedProfile> {
    let meta = def.meta.clone().unwrap_or_else(|| ProfileMeta {
        name: String::new(),
        version: String::from("0.0.0"),
        description: String::new(),
        authors: vec![],
        deprecates: vec![],
        since: None,
        tags: vec![],
        app_name: None,
        domain_types_crate: None,
        hooks_api_crate: None,
        api_title: None,
        generator_name: None,
        domain_types_base: None,
        hooks_api_base: None,
        extensions_base: None,
        app_config_base: None,
        decision_engine_base: None,
        codegraph_workflow_base: None,
        type_contracts_base: None,
    });

    let mut features = def.features.clone().unwrap_or_default();

    let mut sections: HashMap<String, ResolvedSection> = def
        .sections
        .iter()
        .map(|(name, sec)| {
            (
                name.clone(),
                ResolvedSection {
                    generators: sec.generators.clone(),
                    output: sec.output.clone(),
                    scripts: sec
                        .scripts
                        .as_ref()
                        .map(|s| s.post_gen.clone())
                        .unwrap_or_default(),
                },
            )
        })
        .collect();

    // Merge variant overrides if selected.
    if let Some(variant_name) = variant {
        let variant_def = def
            .variants
            .as_ref()
            .and_then(|vs| vs.get(variant_name))
            .ok_or_else(|| {
                let available: Vec<_> = def
                    .variants
                    .as_ref()
                    .map(|vs| vs.keys().collect::<Vec<_>>())
                    .unwrap_or_default();
                Error::Config(format!(
                    "variant \"{variant_name}\" not found in profile \"{}\". available: {:?}",
                    meta.name, available
                ))
            })?;

        // Merge variant features over base.
        if let Some(ref vf) = variant_def.features {
            for (k, v) in vf.iter() {
                features.insert(k.clone(), v.clone());
            }
        }

        // Variant sections override base sections by name.
        for (name, sec) in &variant_def.sections {
            sections.insert(
                name.clone(),
                ResolvedSection {
                    generators: sec.generators.clone(),
                    output: sec.output.clone(),
                    scripts: sec
                        .scripts
                        .as_ref()
                        .map(|s| s.post_gen.clone())
                        .unwrap_or_default(),
                },
            );
        }
    }

    // If no output is specified for a section, use the CLI `--output` (which is
    // handled by the build planner / cmd_run).  We leave it as None here so the
    // caller can supply the global default.

    // If a section has no generators, it's a no-op (allowed — enables
    // selectively enabling targets via variants that omit the section).

    let ifml_frameworks = def
        .ifml
        .as_ref()
        .map(|c| c.frameworks.clone())
        .unwrap_or_default();

    Ok(ResolvedProfile {
        meta,
        features,
        sections,
        ifml_frameworks,
        template_pack_path: None, // set by caller (load_and_resolve_profile)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_profile() {
        let toml = r#"
[profiles.default.meta]
name = "default"
version = "1.0.0"
description = "Default profile"

[profiles.default.api]
generators = ["api_server", "db_migrations"]
scripts.post_gen = ["cargo check"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.profiles.len(), 1);

        let def = &config.profiles["default"];
        assert_eq!(def.meta.as_ref().unwrap().name, "default");
        assert_eq!(def.sections.len(), 1);
        assert!(def.sections.contains_key("api"));

        let api = &def.sections["api"];
        assert_eq!(api.generators, vec!["api_server", "db_migrations"]);
        assert_eq!(api.scripts.as_ref().unwrap().post_gen, vec!["cargo check"]);
    }

    #[test]
    fn parse_profile_with_ui_and_cli_sections() {
        let toml = r#"
[profiles.fullstack.meta]
name = "fullstack"
version = "1.0.0"
description = "Full stack profile"
tags = ["web"]

[profiles.fullstack.features]
auth = true
pagination = true
validation_level = "strict"

[profiles.fullstack.api]
generators = ["api_server", "db_migrations"]
output = "/tmp/api-out/"

[profiles.fullstack.ui]
generators = ["ui_routes", "ui_forms"]
scripts.post_gen = ["pnpm run check", "pnpm run build"]

[profiles.fullstack.cli]
generators = ["cli_commands"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["fullstack"];

        assert_eq!(def.sections.len(), 3);
        assert_eq!(def.sections["api"].output, Some("/tmp/api-out/".into()));

        let ui = &def.sections["ui"];
        assert_eq!(ui.generators, vec!["ui_routes", "ui_forms"]);
        assert_eq!(
            ui.scripts.as_ref().unwrap().post_gen,
            vec!["pnpm run check", "pnpm run build"]
        );

        let cli = &def.sections["cli"];
        assert!(cli.scripts.is_none());
        assert!(cli.output.is_none());
    }

    #[test]
    fn parse_profile_with_variants() {
        let toml = r#"
[profiles.api.meta]
name = "api"
version = "1.0.0"
description = "API profile"

[profiles.api.features]
auth = true
validation_level = "strict"

[profiles.api.api]
generators = ["api_server", "db_migrations"]
scripts.post_gen = ["cargo check"]

[profiles.api.variants.lite]
[profiles.api.variants.lite.api]
generators = ["api_server"]
[profiles.api.variants.lite.features]
auth = false
validation_level = "balanced"
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["api"];
        assert!(def.variants.is_some());
        assert!(def.variants.as_ref().unwrap().contains_key("lite"));
    }

    #[test]
    fn resolve_profile_without_variant() {
        let toml = r#"
[profiles.test.meta]
name = "test"
version = "1.0.0"
description = "Test profile"

[profiles.test.features]
auth = true

[profiles.test.api]
generators = ["api_server", "db_migrations"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["test"];

        let resolved = resolve_profile(def, None).unwrap();
        assert_eq!(resolved.meta.name, "test");
        assert_eq!(resolved.features.get("auth").unwrap().as_bool(), Some(true));
        assert_eq!(resolved.sections.len(), 1);
        assert_eq!(
            resolved.sections["api"].generators,
            vec!["api_server", "db_migrations"]
        );
        assert!(resolved.sections["api"].scripts.is_empty());
    }

    #[test]
    fn resolve_profile_with_variant_merges_features() {
        let toml = r#"
[profiles.api.meta]
name = "api"
version = "1.0.0"
description = "API profile"

[profiles.api.features]
auth = true
validation_level = "strict"

[profiles.api.api]
generators = ["api_server", "db_migrations"]
scripts.post_gen = ["cargo check"]

[profiles.api.variants.lite]
[profiles.api.variants.lite.api]
generators = ["api_server"]
[profiles.api.variants.lite.features]
auth = false
validation_level = "balanced"
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["api"];

        let resolved = resolve_profile(def, Some("lite")).unwrap();
        assert_eq!(resolved.sections.len(), 1);
        // Variant overrides the generators
        assert_eq!(resolved.sections["api"].generators, vec!["api_server"]);
        // Variant overrides features
        assert_eq!(
            resolved.features.get("auth").unwrap().as_bool(),
            Some(false)
        );
        assert_eq!(
            resolved.features.get("validation_level").unwrap().as_str(),
            Some("balanced")
        );
        // Variant didn't provide scripts → inherit from base if present... actually
        // in the current implementation, variant sections fully replace base sections.
        // post_gen scripts from the base are NOT inherited when a variant overrides
        // the section. This is by design: the variant explicitly lists what it wants.
        // Since the variant section doesn't have scripts, this section has none.
        assert!(resolved.sections["api"].scripts.is_empty());
    }

    #[test]
    fn resolve_unknown_profile_yields_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");

        std::fs::write(
            &path,
            r#"
[profiles.known.meta]
name = "known"
version = "1.0.0"
description = "Known"

[profiles.known.api]
generators = ["foo"]
"#,
        )
        .unwrap();

        let result = load_and_resolve_profile(&path, "unknown", None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "expected profile-not-found error, got: {err}"
        );
    }

    #[test]
    fn resolve_unknown_variant_yields_error() {
        let toml = r#"
[profiles.p.meta]
name = "p"
version = "1.0.0"
description = ".."

[profiles.p.api]
generators = ["x"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["p"];

        let result = resolve_profile(def, Some("nonexistent"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("variant \"nonexistent\" not found"));
    }

    #[test]
    fn parse_profile_no_meta_no_features() {
        let toml = r#"
[profiles.minimal.api]
generators = ["gen1", "gen2"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["minimal"];
        assert!(def.meta.is_none());
        assert!(def.features.is_none());
        assert_eq!(def.sections.len(), 1);

        let resolved = resolve_profile(def, None).unwrap();
        assert_eq!(resolved.meta.name, ""); // default
        assert!(resolved.features.is_empty());
    }

    #[test]
    fn round_trip_full_default_profile() {
        let toml = r#"
[profiles.default.meta]
name = "default"
version = "1.0.0"
description = "Current all-artifacts behavior"
authors = ["hr-graph team"]
since = "2026-05-08"

[profiles.default.features]
auth = true
pagination = true
validation_level = "strict"
offline_mode = false

[profiles.default.api]
generators = ["api_server", "api_openapi", "db_migrations"]
output = "review/generated-candidate/"
scripts.post_gen = ["cargo check --workspace 2>&1"]

[profiles.default.ui]
generators = ["ui_routes", "ui_forms", "ui_api_client", "ui_stores"]
output = "review/generated-candidate/"
scripts.post_gen = ["pnpm run check", "pnpm run lint"]

[profiles.default.cli]
generators = ["cli_commands"]
output = "review/generated-candidate/"
scripts.post_gen = ["cargo check -p crewbase-cli 2>&1"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["default"];
        assert_eq!(def.sections.len(), 3);
        assert_eq!(
            def.sections["api"].generators,
            vec!["api_server", "api_openapi", "db_migrations"]
        );
        assert_eq!(
            def.sections["ui"].generators,
            vec!["ui_routes", "ui_forms", "ui_api_client", "ui_stores"]
        );
        assert_eq!(def.sections["cli"].generators, vec!["cli_commands"]);
    }

    #[test]
    fn parse_profiles_file_from_disk_smoke_test() {
        // This test uses a tempfile to verify the full file→parse→resolve path.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");

        std::fs::write(
            &path,
            r#"
[profiles.smoke.meta]
name = "smoke"
version = "1.0.0"
description = "Smoke test profile"

[profiles.smoke.api]
generators = ["api_server"]
scripts.post_gen = ["cargo check"]
"#,
        )
        .unwrap();

        let resolved = load_and_resolve_profile(&path, "smoke", None).unwrap();
        assert_eq!(resolved.meta.name, "smoke");
        assert_eq!(resolved.sections["api"].generators, vec!["api_server"]);
    }

    #[test]
    fn parse_profiles_file_not_found_is_error() {
        let result = load_and_resolve_profile(Path::new("/nonexistent/path.toml"), "x", None);
        assert!(result.is_err());
    }

    // ── Capability Registry Tests ─────────────────────────────────────

    #[test]
    fn registry_has_all_known_generators() {
        let registry = CapabilityRegistry::new();
        // Spot-check some well-known generators
        assert!(registry.get("ddl").is_some());
        assert!(registry.get("dto").is_some());
        assert!(registry.get("handler").is_some());
        assert!(registry.get("ui_page").is_some());
        assert!(registry.get("openapi").is_some());
        assert!(registry.get("scaffold").is_some());
    }

    #[test]
    fn registry_unknown_generator_is_none() {
        let registry = CapabilityRegistry::new();
        assert!(registry.get("nonexistent_generator").is_none());
    }

    #[test]
    fn registry_validates_known_generators() {
        let registry = CapabilityRegistry::new();

        let toml = r#"
[profiles.valid.meta]
name = "valid"
version = "1.0.0"
description = "Valid"

[profiles.valid.api]
generators = ["ddl", "dto", "handler"]

[profiles.valid.ui]
generators = ["ui_page", "ui_form"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["valid"];
        let resolved = resolve_profile(def, None).unwrap();

        assert!(registry.validate_profile(&resolved).is_ok());
    }

    #[test]
    fn registry_rejects_unknown_generator() {
        let registry = CapabilityRegistry::new();

        let toml = r#"
[profiles.bad.meta]
name = "bad"
version = "1.0.0"
description = "Bad"

[profiles.bad.api]
generators = ["nonexistent_xyz"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["bad"];
        let resolved = resolve_profile(def, None).unwrap();

        let result = registry.validate_profile(&resolved);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nonexistent_xyz"));
    }

    #[test]
    fn registry_rejects_wrong_target() {
        let registry = CapabilityRegistry::new();

        let toml = r#"
[profiles.wrong.meta]
name = "wrong"
version = "1.0.0"
description = "Wrong"

[profiles.wrong.ui]
generators = ["ddl"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["wrong"];
        let resolved = resolve_profile(def, None).unwrap();

        let result = registry.validate_profile(&resolved);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("[ui]"),
            "expected mention of [ui] section: {msg}"
        );
    }

    #[test]
    fn registry_allows_common_generators_in_any_section() {
        let registry = CapabilityRegistry::new();

        let toml = r#"
[profiles.c.meta]
name = "c"
version = "1.0.0"
description = ""

[profiles.c.api]
generators = ["openapi"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["c"];
        let resolved = resolve_profile(def, None).unwrap();

        // openapi has target=Common, so it should be valid in [api] section
        assert!(registry.validate_profile(&resolved).is_ok());
    }

    // ── Build Plan Tests ─────────────────────────────────────────────

    #[test]
    fn build_plan_from_default_profile() {
        let registry = CapabilityRegistry::new();

        let toml = r#"
[profiles.default.meta]
name = "default"
version = "1.0.0"
description = ""

[profiles.default.api]
generators = ["ddl", "dto", "handler", "openapi", "scaffold"]

[profiles.default.ui]
generators = ["ui_page", "ui_form", "ui_scaffold"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["default"];
        let resolved = resolve_profile(def, None).unwrap();

        let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

        assert!(plan.has_entity_gen("ddl"));
        assert!(plan.has_entity_gen("dto"));
        assert!(plan.has_entity_gen("handler"));
        assert!(plan.has_entity_gen("ui_page"));
        assert!(plan.has_entity_gen("ui_form"));
        assert!(plan.has_global_gen("openapi"));
        assert!(plan.has_global_gen("scaffold"));
        assert!(plan.has_global_gen("ui_scaffold"));

        // Domain generators not listed → absent
        assert!(!plan.has_domain_gen("router"));
    }

    #[test]
    fn build_plan_includes_post_gen_scripts() {
        let registry = CapabilityRegistry::new();

        let toml = r#"
[profiles.ci.meta]
name = "ci"
version = "1.0.0"
description = ""

[profiles.ci.api]
generators = ["ddl"]
scripts.post_gen = ["cargo check"]

[profiles.ci.ui]
generators = ["ui_page"]
scripts.post_gen = ["pnpm run check", "pnpm run build"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["ci"];
        let resolved = resolve_profile(def, None).unwrap();

        let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

        assert_eq!(plan.post_gen_scripts.len(), 2);
        // Iteration is deterministic (sorted by section name), so verify both sections.
        let api_scripts: Vec<String> = plan
            .post_gen_scripts
            .iter()
            .filter(|(s, _)| s == "api")
            .flat_map(|(_, scripts)| scripts.clone())
            .collect();
        assert_eq!(api_scripts, vec!["cargo check".to_string()]);

        let ui_scripts: Vec<String> = plan
            .post_gen_scripts
            .iter()
            .filter(|(s, _)| s == "ui")
            .flat_map(|(_, scripts)| scripts.clone())
            .collect();
        assert_eq!(
            ui_scripts,
            vec!["pnpm run check".to_string(), "pnpm run build".to_string()]
        );
    }

    #[test]
    fn build_plan_empty_sections_are_skipped() {
        let registry = CapabilityRegistry::new();

        let toml = r#"
[profiles.only_api.meta]
name = "only_api"
version = "1.0.0"
description = ""

# No [ui] section — it's not declared
[profiles.only_api.api]
generators = ["ddl"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        let def = &config.profiles["only_api"];
        let resolved = resolve_profile(def, None).unwrap();

        let plan = BuildPlan::from_profile(&resolved, &registry).unwrap();

        assert!(plan.has_entity_gen("ddl"));
        assert!(!plan.has_global_gen("ui_scaffold"));
        assert_eq!(plan.post_gen_scripts.len(), 0);
    }
}
