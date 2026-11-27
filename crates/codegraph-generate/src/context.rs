use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use codegraph_config::{DomainConfig, UiDomainConfig, UiOverrideConfig};
use codegraph_core::caching_querier::CachingQuerier;
use codegraph_core::traits::GraphQuerier;
use tera::Tera;

use crate::capabilities::{Capability, CapabilitySet};
use crate::db;
use crate::db::dialect::{DatabaseTarget, SqlDialect, dialect_for_target};
use crate::error::{Error, Result};
use crate::ordering::compute_generation_order;
use crate::output::reports_config_dir;
use crate::project_config::{GenerationEntry, GeneratorOpts, ProjectConfig};
use crate::type_registry::TypeRegistryHandle;

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
    /// Integration catalog manifests (issue #552) — ride with `ext_points`
    /// into the integration tables generator (catalog seed migration).
    pub(crate) integration_manifests: Option<&'a codegraph_ext_points::IntegrationManifestSet>,
    /// Build profile plan controlling which generators to run.
    pub(crate) build_plan: Option<&'a crate::profile::BuildPlan>,
    /// The resolved profile name (the ops `[workers]` plan seeds it).
    pub(crate) profile_name: &'a str,
    /// Directory of the `domains.toml` config used for this run; sibling
    /// configs (`reports.toml`) are discovered relative to it.
    pub(crate) domain_config_dir: Option<&'a Path>,
    /// IFML framework targets (e.g. "svelte", "react"); empty defaults to
    /// `["svelte"]` when building the global generator set.
    pub(crate) ifml_frameworks: Vec<String>,
    /// Optional IFML component mappings (`ifml-components.toml`).
    pub(crate) ifml_components: Option<&'a codegraph_config::IfmlComponentMappings>,
    /// Per-run feature flags derived from the build plan (each with the
    /// plan-absent fallback its former boolean flag carried).
    pub(crate) capabilities: CapabilitySet,
    pub(crate) emdash_plugins: Option<crate::emdash::EmdashPluginsConfig>,
    pub(crate) migration_strategy: String,
    /// Whether generated backend output routes into per-domain worker crates
    /// under `workers/{domain}/`. The build plan is authoritative; falls back
    /// to the project config (default: monolith) when no plan is provided.
    pub(crate) workers_topology: bool,
    pub(crate) capability_registry: crate::profile::CapabilityRegistry,
    /// This run's type registry (issue #746) — created once in
    /// [`build_generator_context`] and threaded into the generators that
    /// register types or resolve imports. Pipeline runs NEVER touch the
    /// ambient registry.
    pub(crate) type_registry: TypeRegistryHandle,
    /// This run's deterministic generation order (issue #332), computed once
    /// here so entity-type pre-registration follows the SAME title-claim
    /// precedence the generators emit against (issue #746 F1).
    pub(crate) generation_order: Vec<GenerationEntry>,
    /// This run's publishable producer plans (issue #820 slice b), computed
    /// once when `events_producer` is on and shared by the command + handler
    /// generators. `None` ⇒ no publishing output.
    pub(crate) producer_plans: Option<std::sync::Arc<crate::events::producer::ProducerPlanIndex>>,
    /// This run's producer report (issue #820 slice c) — the SAME planner run
    /// the index is derived from, handed to the events generator so the
    /// catalogue can mark each event's runtime publication. `None` ⇒ no
    /// marks (flag off, byte-identical).
    pub(crate) producer_report: Option<std::sync::Arc<crate::events::producer::ProducerReport>>,
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

    /// This run's type registry (issue #746).
    pub(crate) fn type_registry(&self) -> &TypeRegistryHandle {
        &self.type_registry
    }

    /// This run's producer plans (issue #820 slice b), when publishing is on.
    pub(crate) fn producer_plans(
        &self,
    ) -> Option<&std::sync::Arc<crate::events::producer::ProducerPlanIndex>> {
        self.producer_plans.as_ref()
    }

    /// This run's producer report (issue #820 slice c), when publishing is on.
    pub(crate) fn producer_report(
        &self,
    ) -> Option<&std::sync::Arc<crate::events::producer::ProducerReport>> {
        self.producer_report.as_ref()
    }

    /// This run's deterministic generation order.
    pub(crate) fn generation_order(&self) -> &[GenerationEntry] {
        &self.generation_order
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
        integration_manifests,
        build_plan, // used for has_webhooks / profile-based filter
        profile_name,
        ifml_frameworks,
        ifml_components,
        ux_rules: _, // consumed by generators in later #293 phases
        project_config,
        emdash_plugins,
        domain_config_dir,
    } = opts;

    // Create the database dialect based on project config.
    let current_target = project.database.database_target;

    // Wrap the querier in a caching layer to avoid redundant graph queries
    // across the 15+ generators that each independently query the same schemas.
    let cached_db = CachingQuerier::new(db);

    // Pre-warm the cache with bulk queries to avoid hundreds of individual
    // graph queries during generation.
    cached_db.warm().await.map_err(Error::Graph)?;

    // Deterministic generation order (issue #332), computed HERE because
    // the entity-type pre-registration below must follow the same title-claim
    // precedence the generators emit against (issue #746 F1). The pipeline
    // reuses this order rather than recomputing it.
    let generation_order = compute_generation_order(&cached_db, config).await?;

    // Issue #746: ONE registry for this run — seeded with the framework
    // types and the config's entity types, then threaded into the
    // registry-consuming generators via the context. Nothing here touches
    // the process-ambient registry, so parallel/sequential runs in one
    // process cannot resolve each other's module paths.
    let type_registry = TypeRegistryHandle::with_framework_types();
    // F4: a consumer that seeded the pre-#746 ambient registry must not
    // silently believe those registrations reach this run.
    if let Some(count) = crate::type_registry::ambient_registration_count()
        && count > 0
    {
        eprintln!(
            "warning: type-registry: {count} ambient registration(s) do not affect \
             pipeline runs (issue #746); construct generators directly or pass a \
             TypeRegistryHandle"
        );
    }
    // Pre-register all expected entity types so types from entities later in
    // the generation order (e.g. CertificationResponse referenced by Person's
    // include DTOs) are resolvable when earlier entities process their imports.
    register_entity_types(config, &type_registry, &generation_order);

    let capabilities = derive_capabilities(build_plan, domain_config_dir);

    let migration_strategy = build_plan
        .and_then(|bp| bp.features.get("migration_strategy"))
        .and_then(|v| v.as_str())
        .unwrap_or("sea-orm")
        .to_string();

    // Whether generated backend output routes into per-domain worker crates
    // under `workers/{domain}/`.  The build plan is authoritative; fall back
    // to the project config (default: monolith) when no plan is provided.
    let workers_topology = build_plan
        .map(|bp| bp.deployment_topology() == crate::profile::DeploymentTopology::Workers)
        .unwrap_or_else(|| project.is_workers_topology());

    let capability_registry = crate::profile::CapabilityRegistry::new();

    // Events producer plans (issue #820 slice b): computed ONCE here when the
    // profile enables publishing; validation has already fail-closed on any
    // refusal, so this only carries the successful plans. The SAME report
    // feeds the catalogue's runtime-publication marks (slice c).
    let producer_report = if project.codegen.events_producer {
        crate::events::producer::report_for_run(
            &cached_db,
            config,
            workers_topology,
            project.database.database_target,
        )
        .await
    } else {
        None
    };
    let producer_plans = producer_report.as_ref().map(|report| {
        std::sync::Arc::new(crate::events::producer::ProducerPlanIndex::from_report(
            report,
        ))
    });

    Ok(GeneratorContext {
        cached_db,
        config,
        output_dir,
        tera,
        ui_overrides,
        ui_domains,
        project,
        project_config,
        capabilities,
        emdash_plugins,
        current_target,
        schema_base_dir,
        seed_config,
        domain_types_base,
        hooks_base,
        ext_points,
        integration_manifests,
        build_plan,
        profile_name,
        domain_config_dir,
        ifml_frameworks,
        ifml_components,
        migration_strategy,
        workers_topology,
        capability_registry,
        type_registry,
        generation_order,
        producer_plans,
        producer_report,
    })
}

/// Derive the per-run [`CapabilitySet`] from the build plan. Each condition
/// mirrors its former `has_*` boolean verbatim, including the plan-absent
/// fallback.
fn derive_capabilities(
    build_plan: Option<&crate::profile::BuildPlan>,
    domain_config_dir: Option<&Path>,
) -> CapabilitySet {
    let mut caps = CapabilitySet::default();
    if build_plan.map(|bp| bp.has_emdash).unwrap_or(false) {
        caps.insert(Capability::EmDash);
    }
    // Seed-provisioning (hr-seed sink + CLI) is strictly opt-in: unlike
    // webhooks it introduces a workspace-relative path dependency
    // (`../../hr-seed`), so plan-less runs and profiles that do not list
    // `seed_provision` must not emit the seed module or `[[bin]]` entry.
    if build_plan
        .map(|bp| bp.has_global_gen("seed_provision"))
        .unwrap_or(false)
    {
        caps.insert(Capability::Seed);
    }
    // Whether webhook generators are active.  Derived from build_plan when available;
    // defaults to true for backward compatibility (all existing profiles include
    // webhook_dispatch and webhook_endpoint_api).
    if build_plan
        .map(|bp| bp.has_global_gen("webhook_dispatch"))
        .unwrap_or(true)
    {
        caps.insert(Capability::Webhooks);
    }
    if build_plan
        .map(|bp| bp.has_global_gen("report_views"))
        .unwrap_or(true)
        && reports_config_dir(domain_config_dir)
            .join("reports.toml")
            .exists()
    {
        caps.insert(Capability::Reports);
    }
    if build_plan
        .map(|bp| {
            bp.has_global_gen("atproto_identity")
                || bp.has_entity_gen("lexicon")
                || bp.has_global_gen("lexicon_scaffold")
                || bp.has_entity_gen("atproto_client")
                || bp.has_global_gen("atproto_client_scaffold")
        })
        .unwrap_or(false)
    {
        caps.insert(Capability::Atproto);
    }
    if build_plan
        .map(|bp| bp.has_global_gen("fern_config"))
        .unwrap_or(false)
    {
        caps.insert(Capability::Fern);
    }
    if build_plan
        .map(|bp| bp.has_global_gen("grpc_scaffold"))
        .unwrap_or(false)
    {
        caps.insert(Capability::Grpc);
    }
    if build_plan
        .map(|bp| bp.has_global_gen("ui_scaffold"))
        .unwrap_or(true)
    {
        caps.insert(Capability::Ui);
    }
    if build_plan
        .and_then(|bp| bp.features.get("has_admin_cli"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        caps.insert(Capability::AdminCli);
    }
    if build_plan
        .and_then(|bp| bp.features.get("has_auth_rate_limit"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        caps.insert(Capability::AuthRateLimit);
    }
    if build_plan
        .and_then(|bp| bp.features.get("has_labels"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        caps.insert(Capability::Labels);
    }
    if build_plan
        .map(|bp| {
            bp.has_global_gen("cli_scaffold")
                || bp.has_entity_gen("cli_command")
                || bp.has_domain_gen("cli_domain")
        })
        .unwrap_or(true)
    {
        caps.insert(Capability::Cli);
    }
    if build_plan
        .map(|bp| bp.has_entity_gen("test"))
        .unwrap_or(true)
    {
        caps.insert(Capability::TestGen);
    }
    caps
}

/// Pre-register all expected entity types so types from entities later in
/// the generation order (e.g. CertificationResponse referenced by Person's
/// include DTOs) are resolvable when earlier entities process their imports.
///
/// Determinism and ownership (issue #746 F1): the registry is keyed by type
/// NAME, and a title may legitimately be carried by more than one domain
/// (the documented `PositionType` common+screening shape). Registration
/// therefore follows [`compute_generation_order`]'s title claim — each title
/// is registered exactly ONCE, for the domain that actually emits the
/// entity. Letting `config.domains`' `HashMap` iteration pick the first
/// registrant was nondeterministic and could point generated imports at a
/// domain that is never emitted (E0433).
fn register_entity_types(
    config: &DomainConfig,
    registry: &TypeRegistryHandle,
    generation_order: &[GenerationEntry],
) {
    // Titles the config explicitly lists, plus a deterministic fallback
    // owner (first listing domain in sorted-name order) for titles the
    // order never emits — missing schema, excluded, or force-VO. Master
    // registered those too; dropping them would be an unrelated change.
    let mut configured_titles: HashSet<&str> = HashSet::new();
    let mut fallback_owner: HashMap<&str, &str> = HashMap::new();
    let mut domain_names: Vec<&String> = config.domains.keys().collect();
    domain_names.sort();
    for domain_name in &domain_names {
        for entity_title in &config.domains[*domain_name].entities {
            configured_titles.insert(entity_title.as_str());
            fallback_owner
                .entry(entity_title.as_str())
                .or_insert(domain_name.as_str());
        }
    }

    let mut registered: HashSet<&str> = HashSet::new();
    // 1) The EMITTED owner of every configured title, in generation order.
    for entry in generation_order {
        if !configured_titles.contains(entry.schema_title.as_str())
            || !registered.insert(entry.schema_title.as_str())
        {
            continue;
        }
        register_entity_type(
            registry,
            &config.defaults.type_suffix,
            &entry.schema_title,
            &entry.domain,
        );
    }
    // 2) Configured titles the order never emits, in sorted-domain/config
    // order (deterministic first-listing owner).
    for domain_name in &domain_names {
        for entity_title in &config.domains[*domain_name].entities {
            if !registered.insert(entity_title.as_str()) {
                continue;
            }
            let owner = fallback_owner
                .get(entity_title.as_str())
                .copied()
                .unwrap_or(domain_name.as_str());
            register_entity_type(registry, &config.defaults.type_suffix, entity_title, owner);
        }
    }
}

/// Register the seven cross-generator names for one entity title under
/// `domain_name`.
fn register_entity_type(
    registry: &TypeRegistryHandle,
    suffix: &str,
    entity_title: &str,
    domain_name: &str,
) {
    // Match the naming used by the DTO generators: strip the configured
    // suffix, then PascalCase what remains (titles may contain spaces,
    // e.g. "Validation Issue" -> "ValidationIssue").
    let entity_name =
        codegraph_naming::to_pascal_case(&codegraph_naming::strip_suffix(entity_title, suffix));
    let module_name = codegraph_naming::to_snake_case(&entity_name);
    let base = || -> Vec<String> {
        vec![
            "crate".into(),
            "domain".into(),
            domain_name.to_string(),
            module_name.clone(),
        ]
    };
    registry.register_type(
        &format!("{}Response", entity_name),
        [base(), vec!["dto_response".into()]].concat(),
    );
    registry.register_type(
        &format!("{}LinkedResponse", entity_name),
        [base(), vec!["dto_response".into()]].concat(),
    );
    registry.register_type(
        &format!("{}Repository", entity_name),
        [base(), vec!["repository".into()]].concat(),
    );
    registry.register_type(
        &format!("Create{}Request", entity_name),
        [base(), vec!["dto_create".into()]].concat(),
    );
    registry.register_type(
        &format!("Update{}Request", entity_name),
        [base(), vec!["dto_update".into()]].concat(),
    );
    registry.register_type(
        &format!("{}WithIncludeResponse", entity_name),
        [base(), vec!["dto_included".into()]].concat(),
    );
    registry.register_type(
        &format!("{}IncludedData", entity_name),
        [base(), vec!["dto_included".into()]].concat(),
    );
}

/// Compute the output roots for `.codegraph-manifest.json` emission: the main
/// output dir plus any domain-types/hooks-api bases. The repo-level
/// `migrations` root (hand-extended 0000–0009 + generated 0010+) gets its own
/// manifest so the guard can prove generated files are regenerated while the
/// hand-written files stay excepted. (The old `e2e-tests` root died with the
/// playwright-ts retirement — #463 phase 3.)
pub(crate) fn build_manifest_roots(ctx: &GeneratorContext<'_>) -> Vec<PathBuf> {
    let output_dir = ctx.output_dir;
    let migrations_manifest_root = db::migrations_root(output_dir);

    let mut roots: Vec<PathBuf> = vec![output_dir.to_path_buf()];
    roots.extend(ctx.domain_types_base.map(Path::to_path_buf));
    roots.extend(ctx.hooks_base.map(Path::to_path_buf));
    roots.push(migrations_manifest_root);

    // EmDash plugin packages: per-package roots (only for domains the
    // plugins config declares) plus the site pages/e2e roots, so
    // `emit_manifests` writes per-package + per-site `.codegraph-manifest.json`
    // files the guard can consume.
    if ctx.capabilities.has(Capability::EmDash)
        && let Some(ref plugins) = ctx.emdash_plugins
    {
        for domain_key in plugins.plugins.keys() {
            roots.push(crate::emdash::emdash_package_root(output_dir, domain_key));
        }
        if !plugins.plugins.is_empty() {
            roots.push(crate::emdash::emdash_site_pages_root_with_base(
                output_dir,
                &ctx.project_config
                    .map(|p| p.integration.emdash_site_pages_base.clone())
                    .unwrap_or_default(),
            ));
            roots.push(crate::emdash::emdash_site_e2e_root_with_base(
                output_dir,
                &ctx.project_config
                    .map(|p| p.integration.emdash_site_e2e_base.clone())
                    .unwrap_or_default(),
            ));
        }
    }

    roots
}

#[cfg(test)]
mod type_registry_tests {
    //! Issue #746 F1: entity-type pre-registration is DETERMINISTIC and
    //! winner-consistent with `compute_generation_order`. A title carried by
    //! two domains must register once — under the domain that actually emits
    //! it — so generated imports cannot flip run-to-run or point at a domain
    //! that is never emitted.

    use std::path::Path;

    use codegraph_config::{UiDomainConfig, UiOverrideConfig};
    use codegraph_core::mock::MockEngine;
    use codegraph_core::types::SchemaNode;

    use super::*;

    fn candidate_schema(domain: &str) -> SchemaNode {
        SchemaNode {
            schema_id: format!("{domain}/CandidateType"),
            title: "CandidateType".to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            namespace: None,
            rel_path: format!("{domain}/CandidateType.json"),
            pg_type: "TABLE".to_string(),
            rust_type: "Candidate".to_string(),
            sea_orm_type: "Entity".to_string(),
            rust_type_name: "Candidate".to_string(),
            pg_table_name: "candidate".to_string(),
            api_path_segment: "candidates".to_string(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        }
    }

    fn config(toml_text: &str) -> DomainConfig {
        toml::from_str(toml_text).expect("config parses")
    }

    /// The inputs `build_generator_context` needs, grouped so the helper
    /// stays a plain function rather than an eight-positional-argument call.
    struct CtxInputs<'a> {
        engine: &'a MockEngine,
        config: &'a DomainConfig,
        tera: &'a Tera,
        output_dir: &'a Path,
        schema_base: &'a Path,
        ui_overrides: &'a UiOverrideConfig,
        ui_domains: &'a UiDomainConfig,
        project: &'a ProjectConfig,
    }

    async fn build_ctx<'a>(inputs: CtxInputs<'a>) -> GeneratorContext<'a> {
        build_generator_context(
            GeneratorOpts {
                db: inputs.engine,
                config: inputs.config,
                output_dir: inputs.output_dir,
                tera: inputs.tera,
                ui_overrides: inputs.ui_overrides,
                ui_domains: inputs.ui_domains,
                schema_base_dir: inputs.schema_base,
                seed_config: None,
                domain_types_base: None,
                hooks_base: None,
                ext_points: None,
                integration_manifests: None,
                build_plan: None,
                profile_name: "default",
                ifml_frameworks: Vec::new(),
                ifml_components: None,
                ux_rules: None,
                project_config: None,
                emdash_plugins: None,
                domain_config_dir: None,
            },
            inputs.project,
        )
        .await
        .expect("context builds")
    }

    fn resolved_candidate(ctx: &GeneratorContext<'_>) -> Vec<String> {
        ctx.type_registry()
            .resolve_imports(&["CandidateResponse".to_string()], &[])
    }

    /// The documented duplicate-title shape: several domains explicitly list
    /// the same title. `compute_generation_order` claims it for the first
    /// domain in ITS order (`DomainRegistry::topological_order` reverses the
    /// sorted Kahn output, so the lexicographically LAST independent domain
    /// comes first); registration must agree, every time.
    ///
    /// Four claiming domains is deliberate: the pre-#746 code iterated
    /// `config.domains` (a `HashMap`) and let the first registrant win, so it
    /// would agree with the emitted domain only ~(1/4)^4 of 4-run sequences.
    #[tokio::test]
    async fn duplicate_title_registers_once_for_the_emitted_domain() {
        let engine = MockEngine::builder()
            .with_schema(candidate_schema("common"))
            .build();
        let config = config(
            r#"
[domains.common]
label = "Common"
schema_dir = "schemas/common"
postgres_schema = "common"
entities = ["CandidateType"]

[domains.dup]
label = "Dup"
schema_dir = "schemas/common"
postgres_schema = "dup"
entities = ["CandidateType"]

[domains.extra]
label = "Extra"
schema_dir = "schemas/common"
postgres_schema = "extra"
entities = ["CandidateType"]

[domains.final]
label = "Final"
schema_dir = "schemas/common"
postgres_schema = "final"
entities = ["CandidateType"]
"#,
        );
        let tera = crate::template_engine::create_tera_with_overrides(&[]).expect("tera");
        let out = tempfile::tempdir().expect("tempdir");
        let ui_overrides = UiOverrideConfig::default();
        let ui_domains = UiDomainConfig::default();
        let project = ProjectConfig::default();

        let expected = vec![
            "use crate::domain::final::candidate::dto_response::CandidateResponse;".to_string(),
        ];
        let mut seen: Vec<Vec<String>> = Vec::new();
        for _ in 0..4 {
            let ctx = build_ctx(CtxInputs {
                engine: &engine,
                config: &config,
                tera: &tera,
                output_dir: out.path(),
                schema_base: out.path(),
                ui_overrides: &ui_overrides,
                ui_domains: &ui_domains,
                project: &project,
            })
            .await;
            let emitted_domain = ctx
                .generation_order()
                .iter()
                .find(|e| e.schema_title == "CandidateType")
                .map(|e| e.domain.clone())
                .expect("the title is emitted");
            assert_eq!(
                emitted_domain, "final",
                "the first domain in compute_generation_order claims the title"
            );
            assert_eq!(
                resolved_candidate(&ctx),
                expected,
                "the registry must register the title for the EMITTED domain"
            );
            assert!(
                ctx.type_registry().conflict_messages().is_empty(),
                "one registration per title means no conflict diagnostics"
            );
            seen.push(resolved_candidate(&ctx));
        }
        assert!(
            seen.windows(2).all(|w| w[0] == w[1]),
            "registration must be identical across runs: {seen:?}"
        );
    }

    /// Explicit config beats graph discovery (the cross-domain allOf shape):
    /// the schema physically lives in `zeta` (the FIRST domain in
    /// `compute_generation_order`, so it claims the title by discovery), but
    /// `alpha` explicitly lists it — the order reassigns the claim to
    /// `alpha`, so that is what must be registered. A plain "sorted listing
    /// domain wins" shortcut would pick `alpha` here by luck only; with the
    /// discovery domain first this test proves registration follows the
    /// order's reassignment, not the listing order.
    #[tokio::test]
    async fn explicit_domain_claim_wins_over_graph_discovery() {
        let engine = MockEngine::builder()
            .with_schema(candidate_schema("zeta"))
            .build();
        let config = config(
            r#"
[domains.alpha]
label = "Alpha"
schema_dir = "schemas/alpha"
postgres_schema = "alpha"
entities = ["CandidateType"]

[domains.zeta]
label = "Zeta"
schema_dir = "schemas/zeta"
postgres_schema = "zeta"
"#,
        );
        let tera = crate::template_engine::create_tera_with_overrides(&[]).expect("tera");
        let out = tempfile::tempdir().expect("tempdir");
        let ui_overrides = UiOverrideConfig::default();
        let ui_domains = UiDomainConfig::default();
        let project = ProjectConfig::default();

        let ctx = build_ctx(CtxInputs {
            engine: &engine,
            config: &config,
            tera: &tera,
            output_dir: out.path(),
            schema_base: out.path(),
            ui_overrides: &ui_overrides,
            ui_domains: &ui_domains,
            project: &project,
        })
        .await;
        let emitted_domain = ctx
            .generation_order()
            .iter()
            .find(|e| e.schema_title == "CandidateType")
            .map(|e| e.domain.clone())
            .expect("the title is emitted");
        assert_eq!(emitted_domain, "alpha");
        assert_eq!(
            resolved_candidate(&ctx),
            vec![
                "use crate::domain::alpha::candidate::dto_response::CandidateResponse;".to_string()
            ]
        );
        assert!(ctx.type_registry().conflict_messages().is_empty());
    }
}
