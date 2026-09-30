use std::path::{Path, PathBuf};

use codegraph_config::{DomainConfig, UiDomainConfig, UiOverrideConfig};
use codegraph_core::caching_querier::CachingQuerier;
use codegraph_core::traits::GraphQuerier;
use tera::Tera;

use crate::db;
use crate::db::dialect::{dialect_for_target, DatabaseTarget, SqlDialect};
use crate::error::{Error, Result};
use crate::output::reports_config_dir;
use crate::playwright;
use crate::project_config::{GeneratorOpts, ProjectConfig};
use crate::type_registry;

/// Shared, immutable setup for one generator run.
///
/// Built once by [`build_generator_context`] and threaded explicitly through
/// the generator factory helpers and phase runners (previously captured by
/// large closures inside [`run_generators_with_opts`]).
pub(crate) struct GeneratorContext<'a> {
    /// Cache-wrapping view over the graph querier, pre-warmed at build time.
    pub(crate) cached_db: CachingQuerier<'a>,
    pub(crate) config: &'a DomainConfig,
    pub(crate) output_dir: &'a Path,
    pub(crate) tera: &'a Tera,
    pub(crate) ui_overrides: &'a UiOverrideConfig,
    pub(crate) ui_domains: &'a UiDomainConfig,
    /// Resolved project config: `project_config.unwrap_or(&default)`.
    pub(crate) project: &'a ProjectConfig,
    /// The original optional project config. Kept alongside `project` because
    /// `None` and `Some(default)` genuinely differ for the emdash site bases
    /// (None ⇒ empty base vs Some ⇒ profile-provided path).
    pub(crate) project_config: Option<&'a ProjectConfig>,
    pub(crate) current_target: DatabaseTarget,
    pub(crate) schema_base_dir: &'a Path,
    pub(crate) seed_config: Option<&'a Path>,
    /// Override target dir for domain-types crate generators (`None` defaults
    /// to the main output directory).
    pub(crate) domain_types_base: Option<&'a Path>,
    /// Override target dir for hooks generators.
    pub(crate) hooks_base: Option<&'a Path>,
    /// Extension points config for integration infrastructure generators.
    pub(crate) ext_points: Option<&'a codegraph_ext_points::ExtensionPointsConfig>,
    /// Build profile plan controlling which generators to run.
    pub(crate) build_plan: Option<&'a crate::profile::BuildPlan>,
    /// Directory of the `domains.toml` config used for this run; sibling
    /// configs (`reports.toml`) are discovered relative to it.
    pub(crate) domain_config_dir: Option<&'a Path>,
    /// IFML framework targets (e.g. "svelte", "react"); empty defaults to
    /// `["svelte"]` when building the global generator set.
    pub(crate) ifml_frameworks: Vec<String>,
    /// Optional IFML component mappings (`ifml-components.toml`).
    pub(crate) ifml_components: Option<&'a codegraph_config::IfmlComponentMappings>,
    pub(crate) has_emdash: bool,
    pub(crate) emdash_plugins: Option<crate::emdash::EmdashPluginsConfig>,
    pub(crate) has_seed: bool,
    pub(crate) has_webhooks: bool,
    pub(crate) has_reports: bool,
    pub(crate) has_atproto: bool,
    pub(crate) has_fern: bool,
    pub(crate) has_grpc: bool,
    pub(crate) has_ui: bool,
    pub(crate) has_admin_cli: bool,
    pub(crate) has_auth_rate_limit: bool,
    pub(crate) has_labels: bool,
    pub(crate) migration_strategy: String,
    pub(crate) has_cli: bool,
    pub(crate) has_test_gen: bool,
    /// Whether generated backend output routes into per-domain worker crates
    /// under `workers/{domain}/`. The build plan is authoritative; falls back
    /// to the project config (default: monolith) when no plan is provided.
    pub(crate) workers_topology: bool,
    pub(crate) capability_registry: crate::profile::CapabilityRegistry,
}

impl GeneratorContext<'_> {
    /// The (cached) graph querier handed to every generator.
    pub(crate) fn db(&self) -> &dyn GraphQuerier {
        &self.cached_db
    }

    /// A fresh dialect instance for the configured database target.
    pub(crate) fn make_dialect(&self) -> Box<dyn SqlDialect> {
        dialect_for_target(self.current_target)
    }

    /// Whether the entity generator `name` runs for this build plan.
    ///
    /// Normalizes generator name for build_plan comparison: some name()
    /// methods return hyphens (e.g. "ui-page") while the build plan stores
    /// underscores (e.g. "ui_page") from profile generator lists.
    ///
    /// Without a BuildPlan (plan-less runs), feature-gated generators (gRPC,
    /// AT Protocol, Fern) are skipped: their output depends on dependencies
    /// the scaffolded Cargo.toml only enables via profile features, so
    /// emitting it unconditionally produced non-compiling apps.
    pub(crate) fn plan_has_entity(&self, name: &str) -> bool {
        match self.build_plan {
            Some(bp) => bp.has_entity_gen(&name.replace('-', "_")),
            None => !self
                .capability_registry
                .requires_build_plan(&name.replace('-', "_")),
        }
    }

    /// Whether the domain generator `name` runs for this build plan.
    pub(crate) fn plan_has_domain(&self, name: &str) -> bool {
        match self.build_plan {
            Some(bp) => bp.has_domain_gen(&name.replace('-', "_")),
            None => !self
                .capability_registry
                .requires_build_plan(&name.replace('-', "_")),
        }
    }

    /// Whether the global generator `name` runs for this build plan.
    pub(crate) fn plan_has_global(&self, name: &str) -> bool {
        match self.build_plan {
            Some(bp) => bp.has_global_gen(&name.replace('-', "_")),
            None => !self
                .capability_registry
                .requires_build_plan(&name.replace('-', "_")),
        }
    }
}

/// Build the shared [`GeneratorContext`]: wrap the querier in the caching
/// layer, pre-warm the cache, register types, and derive the per-run feature
/// flags from the build plan.
pub(crate) async fn build_generator_context<'a>(
    opts: GeneratorOpts<'a>,
    project: &'a ProjectConfig,
) -> Result<GeneratorContext<'a>> {
    let GeneratorOpts {
        db,
        config,
        output_dir,
        tera,
        ui_overrides,
        ui_domains,
        schema_base_dir,
        seed_config,
        domain_types_base,
        hooks_base,
        ext_points,
        build_plan, // used for has_webhooks / profile-based filter
        ifml_frameworks,
        ifml_components,
        ux_rules: _, // consumed by generators in later #293 phases
        project_config,
        emdash_plugins,
        domain_config_dir,
    } = opts;

    type_registry::init_type_registry();

    // Create the database dialect based on project config.
    let current_target = DatabaseTarget::from_config(&project.database_target);

    // Wrap the querier in a caching layer to avoid redundant graph queries
    // across the 15+ generators that each independently query the same schemas.
    let cached_db = CachingQuerier::new(db);

    // Pre-warm the cache with bulk queries to avoid hundreds of individual
    // graph queries during generation.
    cached_db.warm().await.map_err(Error::Graph)?;

    // Register framework types so generators can resolve them without hard-coded paths.
    type_registry::register_framework_types();

    // Pre-register all expected entity types so types from entities later in
    // the generation order (e.g. CertificationResponse referenced by Person's
    // include DTOs) are resolvable when earlier entities process their imports.
    register_entity_types(config);

    let has_emdash = build_plan.map(|bp| bp.has_emdash).unwrap_or(false);
    // Seed-provisioning (hr-seed sink + CLI) is strictly opt-in: unlike
    // webhooks it introduces a workspace-relative path dependency
    // (`../../hr-seed`), so plan-less runs and profiles that do not list
    // `seed_provision` must not emit the seed module or `[[bin]]` entry.
    let has_seed = build_plan
        .map(|bp| bp.has_global_gen("seed_provision"))
        .unwrap_or(false);

    // Whether webhook generators are active.  Derived from build_plan when available;
    // defaults to true for backward compatibility (all existing profiles include
    // webhook_dispatch and webhook_endpoint_api).
    let has_webhooks = build_plan
        .map(|bp| bp.has_global_gen("webhook_dispatch"))
        .unwrap_or(true);
    let has_reports = build_plan
        .map(|bp| bp.has_global_gen("report_views"))
        .unwrap_or(true)
        && reports_config_dir(domain_config_dir)
            .join("reports.toml")
            .exists();
    let has_atproto = build_plan
        .map(|bp| {
            bp.has_global_gen("atproto_identity")
                || bp.has_entity_gen("lexicon")
                || bp.has_global_gen("lexicon_scaffold")
                || bp.has_entity_gen("atproto_client")
                || bp.has_global_gen("atproto_client_scaffold")
        })
        .unwrap_or(false);
    let has_fern = build_plan
        .map(|bp| bp.has_global_gen("fern_config"))
        .unwrap_or(false);
    let has_grpc = build_plan
        .map(|bp| bp.has_global_gen("grpc_scaffold"))
        .unwrap_or(false);
    let has_ui = build_plan
        .map(|bp| bp.has_global_gen("ui_scaffold"))
        .unwrap_or(true);
    let has_admin_cli = build_plan
        .and_then(|bp| bp.features.get("has_admin_cli"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_auth_rate_limit = build_plan
        .and_then(|bp| bp.features.get("has_auth_rate_limit"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_labels = build_plan
        .and_then(|bp| bp.features.get("has_labels"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let migration_strategy = build_plan
        .and_then(|bp| bp.features.get("migration_strategy"))
        .and_then(|v| v.as_str())
        .unwrap_or("sea-orm")
        .to_string();
    let has_cli = build_plan
        .map(|bp| {
            bp.has_global_gen("cli_scaffold")
                || bp.has_entity_gen("cli_command")
                || bp.has_domain_gen("cli_domain")
        })
        .unwrap_or(true);
    let has_test_gen = build_plan
        .map(|bp| bp.has_entity_gen("test"))
        .unwrap_or(true);

    // Whether generated backend output routes into per-domain worker crates
    // under `workers/{domain}/`.  The build plan is authoritative; fall back
    // to the project config (default: monolith) when no plan is provided.
    let workers_topology = build_plan
        .map(|bp| bp.deployment_topology() == crate::profile::DeploymentTopology::Workers)
        .unwrap_or_else(|| project.is_workers_topology());

    let capability_registry = crate::profile::CapabilityRegistry::new();

    Ok(GeneratorContext {
        cached_db,
        config,
        output_dir,
        tera,
        ui_overrides,
        ui_domains,
        project,
        project_config,
        has_emdash,
        emdash_plugins,
        current_target,
        schema_base_dir,
        seed_config,
        domain_types_base,
        hooks_base,
        ext_points,
        build_plan,
        domain_config_dir,
        ifml_frameworks,
        ifml_components,
        has_seed,
        has_webhooks,
        has_reports,
        has_atproto,
        has_fern,
        has_grpc,
        has_ui,
        has_admin_cli,
        has_auth_rate_limit,
        has_labels,
        migration_strategy,
        has_cli,
        has_test_gen,
        workers_topology,
        capability_registry,
    })
}

/// Pre-register all expected entity types so types from entities later in
/// the generation order (e.g. CertificationResponse referenced by Person's
/// include DTOs) are resolvable when earlier entities process their imports.
fn register_entity_types(config: &DomainConfig) {
    let suffix = &config.defaults.type_suffix;
    for (domain_name, domain_entry) in &config.domains {
        for entity_title in &domain_entry.entities {
            // Match the naming used by the DTO generators: strip the configured
            // suffix, then PascalCase what remains (titles may contain spaces,
            // e.g. "Validation Issue" -> "ValidationIssue").
            let entity_name = codegraph_naming::to_pascal_case(&codegraph_naming::strip_suffix(
                entity_title,
                suffix,
            ));
            let module_name = codegraph_naming::to_snake_case(&entity_name);
            let base = || -> Vec<String> {
                vec![
                    "crate".into(),
                    "domain".into(),
                    domain_name.clone(),
                    module_name.clone(),
                ]
            };
            type_registry::register_type(
                &format!("{}Response", entity_name),
                [base(), vec!["dto_response".into()]].concat(),
            );
            type_registry::register_type(
                &format!("{}LinkedResponse", entity_name),
                [base(), vec!["dto_response".into()]].concat(),
            );
            type_registry::register_type(
                &format!("{}Repository", entity_name),
                [base(), vec!["repository".into()]].concat(),
            );
            type_registry::register_type(
                &format!("Create{}Request", entity_name),
                [base(), vec!["dto_create".into()]].concat(),
            );
            type_registry::register_type(
                &format!("Update{}Request", entity_name),
                [base(), vec!["dto_update".into()]].concat(),
            );
            type_registry::register_type(
                &format!("{}WithIncludeResponse", entity_name),
                [base(), vec!["dto_included".into()]].concat(),
            );
            type_registry::register_type(
                &format!("{}IncludedData", entity_name),
                [base(), vec!["dto_included".into()]].concat(),
            );
        }
    }
}

/// Compute the output roots for `.codegraph-manifest.json` emission: the main
/// output dir plus any domain-types/hooks-api bases. The repo-level
/// `e2e-tests` root (home of the TypeScript Playwright harness) and the
/// repo-level `migrations` root (hand-extended 0000–0009 + generated 0010+)
/// get their own manifests so the guard can prove generated files are
/// regenerated while the hand-written files stay excepted.
pub(crate) fn build_manifest_roots(ctx: &GeneratorContext<'_>) -> Vec<PathBuf> {
    let output_dir = ctx.output_dir;
    let e2e_manifest_root = playwright::e2e_tests_root(output_dir);
    let migrations_manifest_root = db::migrations_root(output_dir);

    let mut roots: Vec<PathBuf> = vec![output_dir.to_path_buf()];
    roots.extend(ctx.domain_types_base.map(Path::to_path_buf));
    roots.extend(ctx.hooks_base.map(Path::to_path_buf));
    roots.push(e2e_manifest_root);
    roots.push(migrations_manifest_root);

    // EmDash plugin packages: per-package roots (only for domains the
    // plugins config declares) plus the site pages/e2e roots, so
    // `emit_manifests` writes per-package + per-site `.codegraph-manifest.json`
    // files the guard can consume.
    if ctx.has_emdash {
        if let Some(ref plugins) = ctx.emdash_plugins {
            for domain_key in plugins.plugins.keys() {
                roots.push(crate::emdash::emdash_package_root(output_dir, domain_key));
            }
            if !plugins.plugins.is_empty() {
                roots.push(crate::emdash::emdash_site_pages_root_with_base(
                    output_dir,
                    &ctx.project_config
                        .map(|p| p.emdash_site_pages_base.clone())
                        .unwrap_or_default(),
                ));
                roots.push(crate::emdash::emdash_site_e2e_root_with_base(
                    output_dir,
                    &ctx.project_config
                        .map(|p| p.emdash_site_e2e_base.clone())
                        .unwrap_or_default(),
                ));
            }
        }
    }

    roots
}
