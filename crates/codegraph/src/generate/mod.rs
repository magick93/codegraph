pub mod codelist;
pub mod filter_fields;
pub mod ifml;
pub mod report;
pub mod template_engine;
pub mod traits;
pub mod type_registry;

pub mod api;
pub mod cli;
pub mod db;
pub mod ddd;
pub mod domain_types;
pub mod hooks;
pub mod integration;
pub mod playwright;
pub mod scaffold;
pub mod test;
pub mod ui;
pub mod grpc;
pub mod webhook;

/// Returns the lowercased PG cast string for range/geometry types, or `None` for standard types.
/// Used by entity and repository generators to emit explicit casts (e.g. `$N::tstzrange`,
/// `column_type = "custom(\"geometry\")"` with `select_as = "text"`).
pub fn pg_cast_for_type(pg_column_type: &str) -> Option<String> {
    let upper = pg_column_type.to_uppercase();
    if upper.starts_with("GEOMETRY") || upper.starts_with("GEOGRAPHY") {
        return Some("geometry".to_string());
    }
    match upper.as_str() {
        "TSTZRANGE" | "DATERANGE" | "INT4RANGE" | "INT8RANGE" => {
            Some(pg_column_type.to_lowercase())
        }
        _ => None,
    }
}

/// Returns true if the pg_cast value represents a geometry/geography type
/// that needs ST_AsGeoJSON/ST_GeomFromGeoJSON in queries.
pub fn is_geometry_cast(cast: &str) -> bool {
    cast == "geometry"
}

/// Compute the FK column name for a child entity given a `ParentCandidate`.
///
/// This is the single source of truth for FK naming; entity, DDL, repository,
/// command, query, handler, and router generators must all call this helper
/// (or the bulk wrapper `resolve_parent_fk_column`) to stay in sync.
pub fn fk_column_for_candidate(pc: &codegraph_core::types::ParentCandidate, suffix: &str) -> String {
    let parent_name = api::router::strip_suffix(&pc.parent_title, suffix);
    match pc.source {
        codegraph_core::types::DetectionSource::ArrayItems => {
            codegraph_naming::to_snake_case(parent_name) + "_id"
        }
        _ => codegraph_naming::to_snake_case(&pc.field_name) + "_id",
    }
}

/// Find the first `ParentCandidate` whose child matches `schema_title` and
/// return the FK column name.  Falls back to `entity_cfg.parent_ref` if set.
///
/// This is the sync version — it does NOT check whether the parent is in the
/// same domain.  Use it for DB-level generators (entity, DDL) where the FK
/// column should exist regardless of routing/nesting.
pub fn resolve_parent_fk_column(
    schema_title: &str,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    suffix: &str,
) -> Option<String> {
    // 1. Manual config always wins
    if let Some(fk) = entity_cfg.and_then(|ec| ec.parent_ref.clone()) {
        return Some(fk);
    }
    // If manual config says role=child with a parent, derive FK from parent name
    if let Some(ec) = entity_cfg {
        if ec.role.as_deref() == Some("child") {
            if let Some(ref parent_title) = ec.parent {
                let parent_name = api::router::strip_suffix(parent_title, suffix);
                return Some(format!("{}_id", codegraph_naming::to_snake_case(parent_name)));
            }
        }
    }
    // 2. Graph fallback (no domain check — FK column always needed)
    let stripped = api::router::strip_suffix(schema_title, suffix);
    parent_candidates.iter().find_map(|pc| {
        let child_name = api::router::strip_suffix(&pc.child_title, suffix);
        if child_name == stripped {
            Some(fk_column_for_candidate(pc, suffix))
        } else {
            None
        }
    })
}

/// Async version of [`resolve_parent_fk_column`] that checks whether the
/// parent is in the same domain before treating the entity as a child.
/// Use this for API-level generators (handler, command, query, repository)
/// where `parent_ref` drives route nesting and parent_id parameters.
pub async fn resolve_parent_fk_column_same_domain(
    schema_title: &str,
    parent_candidates: &[codegraph_core::types::ParentCandidate],
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    domain: &str,
    config: &codegraph_config::DomainConfig,
    db: &dyn codegraph_core::traits::GraphQuerier,
) -> Option<String> {
    // 1. Manual config always wins
    if let Some(fk) = entity_cfg.and_then(|ec| ec.parent_ref.clone()) {
        return Some(fk);
    }
    if let Some(ec) = entity_cfg {
        if ec.role.as_deref() == Some("child") {
            if let Some(ref parent_title) = ec.parent {
                let parent_name = api::router::strip_suffix(parent_title, &config.defaults.type_suffix);
                return Some(format!("{}_id", codegraph_naming::to_snake_case(parent_name)));
            }
        }
    }
    // 2. Graph fallback with same-domain check
    let stripped = api::router::strip_suffix(schema_title, &config.defaults.type_suffix);
    for pc in parent_candidates {
        let child_name = api::router::strip_suffix(&pc.child_title, &config.defaults.type_suffix);
        if child_name == stripped {
            // Check if parent is in same domain: explicitly listed OR schema domain matches
            let in_explicit_list = config
                .domains
                .get(domain)
                .map(|d| d.entities.contains(&pc.parent_title))
                .unwrap_or(false);
            let in_same_domain = in_explicit_list
                || db
                    .get_schema(&pc.parent_title)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|s| s.domain.as_ref().map(|d| d == domain))
                    .unwrap_or(false);
            if in_same_domain {
                return Some(fk_column_for_candidate(pc, &config.defaults.type_suffix));
            }
            return None; // Parent in different domain — no parent_ref for nesting
        }
    }
    None
}

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

use codegraph_core::caching_querier::CachingQuerier;
use codegraph_core::traits::GraphQuerier;
use tera::Tera;

use crate::error::{Error, Result};
use crate::generate::db::dialect::{dialect_for_target, DatabaseTarget};
use codegraph_config::{DomainConfig, UiDomainConfig, UiOverrideConfig};

use std::sync::OnceLock;

use self::traits::{DomainGenerator, EntityGenerator, GeneratedFile, GlobalGenerator};

// =============================================================================
// Project-level configuration for template rendering.
// Set once at the start of the generation pipeline, then accessible from
// any generator via `get_project_config()` — no need to thread through
// every helper function.
// =============================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectConfig {
    pub app_name: String,
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
    /// Import prefix for structured wrapper types in generated re-exports.
    /// Default: "codegraph_type_contracts".
    /// Domain crates should set this to their own crate or module path (e.g. "crate").
    pub types_import_prefix: String,
    /// Git revision SHA used for fallback path dependencies in generated Cargo.toml.
    /// When domain_types_base is empty, the domain types Cargo.toml uses this rev
    /// to reference codegraph-type-contracts as a git dependency.
    #[serde(default)]
    pub codegraph_rev: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            app_name: "app".into(),
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
            types_import_prefix: "codegraph_type_contracts".into(),
        }
    }
}

static PROJECT_CONFIG: OnceLock<ProjectConfig> = OnceLock::new();

/// Initialize the global project config. Must be called before any generator runs.
pub fn init_project_config(config: ProjectConfig) {
    PROJECT_CONFIG.set(config).ok();
}

/// Get the current project config. Falls back to a box-leaked default if not initialized.
pub fn get_project_config() -> &'static ProjectConfig {
    PROJECT_CONFIG.get().unwrap_or_else(|| {
        // Leak a default on first call as permanent fallback
        let default: &'static ProjectConfig = Box::leak(Box::new(ProjectConfig::default()));
        PROJECT_CONFIG.set(default.clone()).ok();
        PROJECT_CONFIG.get().unwrap()
    })
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
    /// Project-level config injected into all template contexts.
    pub project_config: Option<&'a ProjectConfig>,
}

/// Run all generators for all entities in topological order.
pub async fn run_generators(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    output_dir: &Path,
    tera: &Tera,
    ui_overrides: &UiOverrideConfig,
    ui_domains: &UiDomainConfig,
    schema_base_dir: &Path,
) -> Result<report::GenerationReport> {
        run_generators_with_opts(GeneratorOpts {
        db,
        config,
        output_dir,
        tera,
        ui_overrides,
        ui_domains,
        schema_base_dir,
        seed_config: None,
        domain_types_base: None,
        hooks_base: None,
        ext_points: None,
        build_plan: None,
        ifml_frameworks: vec![],
        project_config: None,
    })
    .await
}

/// Like [`run_generators`] but redirects `hr-domain-types` and `hr-hooks-api`
/// output to temp directories instead of the compiled-in workspace paths.
#[allow(clippy::too_many_arguments)]
pub async fn run_generators_with_domain_types_base(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    output_dir: &Path,
    tera: &Tera,
    ui_overrides: &UiOverrideConfig,
    ui_domains: &UiDomainConfig,
    schema_base_dir: &Path,
    domain_types_base: &Path,
    hooks_base: &Path,
) -> Result<report::GenerationReport> {
    run_generators_with_opts(GeneratorOpts {
        db,
        config,
        output_dir,
        tera,
        ui_overrides,
        ui_domains,
        schema_base_dir,
        seed_config: None,
        domain_types_base: Some(domain_types_base),
        hooks_base: Some(hooks_base),
        ext_points: None,
        build_plan: None,
        ifml_frameworks: vec![],
        project_config: None,
    })
    .await
}

/// Run generators with full configuration via [`GeneratorOpts`].
pub async fn run_generators_with_opts(opts: GeneratorOpts<'_>) -> Result<report::GenerationReport> {
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
        project_config,
    } = opts;
    // Initialize global project config so generator helpers can access it.
    let default_project = ProjectConfig::default();
    let project = project_config.unwrap_or(&default_project);
    init_project_config(project.clone());
    type_registry::init_type_registry();

    // Create the database dialect based on project config.
    let make_dialect = || dialect_for_target(DatabaseTarget::from_config(&project.database_target));

    // Wrap the querier in a caching layer to avoid redundant graph queries
    // across the 15+ generators that each independently query the same schemas.
    let cached_db = CachingQuerier::new(db);
    let db: &dyn GraphQuerier = &cached_db;

    // Pre-warm the cache with bulk queries to avoid hundreds of individual
    // graph queries during generation.
    cached_db.warm().await.map_err(Error::Graph)?;

    // Register framework types so generators can resolve them without hard-coded paths.
    type_registry::register_framework_types();

    // Pre-register all expected entity types so types from entities later in
    // the generation order (e.g. CertificationResponse referenced by Person's
    // include DTOs) are resolvable when earlier entities process their imports.
    let suffix = &config.defaults.type_suffix;
    for (domain_name, domain_entry) in &config.domains {
        for entity_title in &domain_entry.entities {
            let entity_name = codegraph_naming::strip_suffix(entity_title, suffix);
            let module_name = codegraph_naming::to_snake_case(&entity_name);
            let base = || -> Vec<String> {
                vec!["crate".into(), "domain".into(), domain_name.clone(), module_name.clone()]
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

    // Whether webhook generators are active.  Derived from build_plan when available;
    // defaults to true for backward compatibility (all existing profiles include
    // webhook_dispatch and webhook_endpoint_api).
    let has_webhooks = build_plan
        .map(|bp| bp.has_global_gen("webhook_dispatch"))
        .unwrap_or(true);
    let has_reports = build_plan
        .map(|bp| bp.has_global_gen("report_views"))
        .unwrap_or(true)
        && std::env::current_dir().unwrap_or_default().join("reports.toml").exists();
    let has_grpc = build_plan
        .map(|bp| bp.has_global_gen("grpc_scaffold"))
        .unwrap_or(false);

    let order = compute_generation_order(db, config).await?;
    let mut report = report::GenerationReport::new();

    // Clean stale generated files from the output directory before generators
    // run.  Previous pipeline runs may have produced files for entities that
    // are no longer in the generation order (e.g. reclassified as VOs).
    // The filesystem-scanning `generate_mod_files` pass would otherwise pick
    // them up and emit broken `pub mod` declarations.
    clean_generated_output(output_dir, &order, &config.defaults.type_suffix);

    // Clean stale IFML route directories from previous runs.
    clean_stale_ifml_routes(output_dir, &[]);

    // Clean generated migration files (seq >= 10) from previous runs.  New runs
    // may generate a different set of files (e.g. duplicates removed),
    // and stale numbered SQL files must not linger in the migrations directory.
    // Hand-written bootstrap migrations (0000–0009) are preserved.
    let migrations_dir = output_dir.join("migrations");
    if migrations_dir.is_dir() {
        let mut removed = 0usize;
        for entry in fs::read_dir(&migrations_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sql") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Parse leading numeric prefix.
                let prefix_end = stem
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(stem.len());
                if let Ok(seq) = stem[..prefix_end].parse::<usize>() {
                    // Generated migrations start at seq 10 (codelist, integration, entity).
                    if seq >= 10 {
                        let _ = fs::remove_file(&path);
                        removed += 1;
                    }
                }
            }
        }
        if removed > 0 {
            tracing::debug!("Removed {removed} stale generated migration files from migrations/");
        }
    }

    // Fetch parent-child relationship candidates from the graph for
    // router and handler generators to populate nested route information.
    let parent_candidates = db
        .get_parent_candidates()
        .await
        .map_err(|e| Error::Config(e.to_string()))?;

    // Normalize generator name for build_plan comparison: some name() methods
    // return hyphens (e.g. "ui-page") while the build plan stores underscores
    // (e.g. "ui_page") from profile generator lists.
    let plan_has_entity =
        |name: &str| build_plan.is_none_or(|bp| bp.has_entity_gen(&name.replace('-', "_")));
    let plan_has_domain =
        |name: &str| build_plan.is_none_or(|bp| bp.has_domain_gen(&name.replace('-', "_")));
    let plan_has_global =
        |name: &str| build_plan.is_none_or(|bp| bp.has_global_gen(&name.replace('-', "_")));

    let entity_gens: Vec<Box<dyn EntityGenerator>> = vec![
        Box::new(
            db::ddl::DdlGenerator::new(output_dir)
                .with_dialect(make_dialect())
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(
            db::entity::SeaOrmEntityGenerator::new(output_dir)
                .with_dialect(make_dialect())
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(
            ddd::repository::RepositoryTraitGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(
            ddd::command::CommandGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(
            ddd::query::QueryGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(ddd::event::EventGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        Box::new(ddd::dto::DtoGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        Box::new(
            api::handler::HandlerGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(
            api::workflow_action::WorkflowActionGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(api::media::MediaRouteGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        Box::new(test::test_gen::TestGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        Box::new(
            ui::page::UiPageGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(ui::form::UiFormGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        Box::new(
            ui::store::UiStoreGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(
            ui::e2e_test::UiE2eTestGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn EntityGenerator>,
        Box::new(playwright::entity_gen::PlaywrightEntityGenerator::new(
            output_dir,
        )) as Box<dyn EntityGenerator>,
        Box::new(ui::descriptor::UiDescriptorGenerator::new(
            output_dir,
            ui_overrides.clone(),
            ui_domains.clone(),
        )) as Box<dyn EntityGenerator>,
        Box::new(ui::shell::UiShellGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        Box::new(
            hooks::lifecycle_trait::LifecycleTraitGenerator::new_with_base(
                hooks_base.map(|b| b.to_path_buf()).unwrap_or_else(|| output_dir.to_path_buf()),
            ),
        ) as Box<dyn EntityGenerator>,
        // domain_types generators: use the provided base override, defaulting to output_dir.
        Box::new(
            domain_types::dto::DomainTypesDtoGenerator::new_with_base(
                domain_types_base.map(|b| b.to_path_buf()).unwrap_or_else(|| output_dir.to_path_buf()),
            ),
        ) as Box<dyn EntityGenerator>,
        Box::new(
            domain_types::query_service::QueryServiceGenerator::new_with_base(
                domain_types_base.map(|b| b.to_path_buf()).unwrap_or_else(|| output_dir.to_path_buf()),
            ),
        ) as Box<dyn EntityGenerator>,
        Box::new(cli::command::CliCommandGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        // gRPC entity generators
        Box::new(grpc::proto::GrpcProtoGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
        Box::new(grpc::service::GrpcServiceGenerator::new(output_dir)) as Box<dyn EntityGenerator>,
    ]
    .into_iter()
    .filter(|gen| plan_has_entity(gen.name()))
    .collect::<Vec<_>>();

    let domain_gens: Vec<Box<dyn DomainGenerator>> = vec![
        Box::new(
            api::router::RouterGenerator::new(output_dir)
                .with_parent_candidates(parent_candidates.clone()),
        ) as Box<dyn DomainGenerator>,
        Box::new(api::links::LinksGenerator::new(output_dir)) as Box<dyn DomainGenerator>,
        Box::new(ui::domain_layout::UiDomainLayoutGenerator::new(output_dir))
            as Box<dyn DomainGenerator>,
        Box::new(cli::domain::CliDomainGenerator::new(output_dir)) as Box<dyn DomainGenerator>,
        // gRPC domain generator
        Box::new(grpc::router::GrpcRouterGenerator::new(output_dir)) as Box<dyn DomainGenerator>,
    ]
    .into_iter()
    .filter(|gen| plan_has_domain(gen.name()))
    .collect::<Vec<_>>();

    let global_gens: Vec<Box<dyn GlobalGenerator>> = vec![
        Box::new(
            db::basejump_setup::BasejumpSetupGenerator::new(output_dir)
                .with_dialect(make_dialect()),
        ) as Box<dyn GlobalGenerator>,
        Box::new(
            db::event_trigger::PgmqSetupGenerator::new(output_dir)
                .with_dialect(make_dialect()),
        ) as Box<dyn GlobalGenerator>,
        Box::new(
            db::platform_schema::PlatformSchemaGenerator::new(output_dir)
                .with_dialect(make_dialect()),
        ) as Box<dyn GlobalGenerator>,
        Box::new(
            db::workflow_seed::WorkflowSeedGenerator::new(output_dir)
                .with_dialect(make_dialect()),
        ) as Box<dyn GlobalGenerator>,
        Box::new(api::openapi::OpenApiGenerator::new(output_dir)) as Box<dyn GlobalGenerator>,
        Box::new(scaffold::gen::ScaffoldGenerator::new(
            output_dir,
            has_webhooks,
            has_reports,
            has_grpc,
        )) as Box<dyn GlobalGenerator>,
        Box::new(ui::scaffold::UiScaffoldGenerator::new(
            output_dir,
            ext_points.is_some(),
            has_webhooks,
        )) as Box<dyn GlobalGenerator>,
        Box::new(ui::types::UiTypeGenerator::new(output_dir)) as Box<dyn GlobalGenerator>,
        Box::new(ui::codelist::UiCodelistGenerator::new(
            output_dir,
            schema_base_dir,
        )) as Box<dyn GlobalGenerator>,
        Box::new(hooks::registry::HookRegistryGenerator::new_with_base(
            hooks_base.map(|b| b.to_path_buf()).unwrap_or_else(|| output_dir.to_path_buf()),
        )) as Box<dyn GlobalGenerator>,
        Box::new(
            domain_types::scaffold::DomainTypesScaffoldGenerator::new_with_base(
                domain_types_base.map(|b| b.to_path_buf()).unwrap_or_else(|| output_dir.to_path_buf()),
            ),
        ) as Box<dyn GlobalGenerator>,
        Box::new(cli::scaffold::CliScaffoldGenerator::new(output_dir)) as Box<dyn GlobalGenerator>,
        Box::new(
            db::report_view::ReportViewGenerator::new(output_dir)
                .with_dialect(make_dialect()),
        ) as Box<dyn GlobalGenerator>,
        Box::new(
            db::seed::SeedDataGenerator::new(
                output_dir,
                seed_config.map(|p| p.to_path_buf()),
            )
            .with_dialect(make_dialect()),
        ) as Box<dyn GlobalGenerator>,
        Box::new(playwright::global_gen::PlaywrightGlobalGenerator::new(
            output_dir,
        )) as Box<dyn GlobalGenerator>,
        Box::new(webhook::dispatch::WebhookDispatchGenerator::new(output_dir))
            as Box<dyn GlobalGenerator>,
        Box::new(webhook::endpoint_api::WebhookEndpointApiGenerator::new(
            output_dir,
        )) as Box<dyn GlobalGenerator>,
        // gRPC global generator
        Box::new(grpc::scaffold::GrpcScaffoldGenerator::new(output_dir))
            as Box<dyn GlobalGenerator>,
    ]
    .into_iter()
    .filter(|gen| plan_has_global(gen.name()))
    .collect::<Vec<_>>();

    let mut global_gens = global_gens;

    // Add IFML generators per framework
    let ifml_frameworks = if ifml_frameworks.is_empty() {
        vec!["svelte".to_string()]
    } else {
        ifml_frameworks.clone()
    };
    for fw in &ifml_frameworks {
        let fw_output = output_dir.join(fw);
        if build_plan.is_none() || plan_has_global(&format!("ifml_route_{}", fw)) {
            global_gens.push(
                Box::new(ifml::route_generator::IfmlRouteGenerator::new(&fw_output, fw))
                    as Box<dyn GlobalGenerator>,
            );
        }
        if build_plan.is_none() || plan_has_global(&format!("ifml_navigation_{}", fw)) {
            global_gens.push(
                Box::new(ifml::navigation_generator::IfmlNavigationGenerator::new(
                    &fw_output, fw,
                )) as Box<dyn GlobalGenerator>,
            );
        }
    }

    if let Some(ext) = ext_points {
        let integration_gens: [Box<dyn GlobalGenerator>; 4] = [
            Box::new(integration::tables::IntegrationTablesGenerator::new(
                output_dir,
                ext.clone(),
            )),
            Box::new(integration::config::IntegrationConfigGenerator::new(
                output_dir,
                ext.clone(),
            )),
            Box::new(integration::dispatch::IntegrationDispatchGenerator::new(
                output_dir,
            )),
            Box::new(integration::catalog::IntegrationCatalogGenerator::new(
                output_dir,
            )),
        ];
        for gen in integration_gens {
            if plan_has_global(gen.name()) {
                global_gens.push(gen);
            }
        }
    }

    // Per-entity generators — run entities sequentially to ensure TypeRegistry
    // is populated for earlier entities before later entities reference their types.
    // Within each entity, generators run sequentially.
    let mut entity_results: Vec<(Vec<GeneratedFile>, Vec<report::GenerationError>)> = Vec::new();
    for entry in &order {
        let mut entity_files = Vec::new();
        let mut errors = Vec::new();
        for gen in entity_gens.iter() {
            match gen
                .generate(db, &entry.schema_title, &entry.domain, config, tera, project)
                .await
            {
                Ok(files) => entity_files.extend(files),
                Err(e) => {
                    errors.push(report::GenerationError {
                        entity: entry.schema_title.clone(),
                        generator: gen.name().to_string(),
                        source: e,
                    });
                }
            }
        }
        entity_results.push((entity_files, errors));
    }

    // Entity migrations start at 500 to avoid overlap with codelist range (10..200).
    // Deduplicate migration files by their unprefixed base name: two different schema
    // titles can produce the same pg_table_name (e.g. "AssessmentAccessType" and a
    // cross-domain ref "AssessmentAccess" both → assessments_assessment_access.sql).
    // Keep only the first occurrence; skip subsequent duplicates.
    let mut seen_migration_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut entity_seq = 500;
    for (entity_files, errors) in entity_results.into_iter() {
        if errors.is_empty() {
            for file in entity_files {
                // Check for duplicate migration base names before assigning seq number.
                let is_migration = file
                    .path
                    .parent()
                    .and_then(|p| p.file_name())
                    .is_some_and(|d| d == "migrations");
                if is_migration {
                    if let Some(name) = file.path.file_name().and_then(|n| n.to_str()) {
                        let base = name
                            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '_')
                            .to_string();
                        if !seen_migration_names.insert(base.clone()) {
                            // Two different schema titles produced the same pg_table_name
                            // (e.g. "AssessmentAccessType" and a cross-domain ref
                            // "AssessmentAccess" both produce assessments_assessment_access.sql).
                            // Keep the first occurrence; skip subsequent ones.
                            tracing::warn!(
                                migration = %name,
                                base = %base,
                                "skipping duplicate migration — same pg_table_name produced by \
                                 multiple schema titles; first occurrence wins"
                            );
                            continue;
                        }
                    }
                }
                let file = prefix_migration_path(file, entity_seq);
                entity_seq += 1;
                write_output(&file)?;
                report.files.push(file);
            }
        } else {
            report.errors.extend(errors);
        }
    }

    // Codelist SQL migration generators (codelists are not entities, run separately)
    {
        let codelists = db
            .list_codelists()
            .await
            .map_err(|e| Error::Config(e.to_string()))?;
        let codelist_sql_gen = db::codelist::CodelistGenerator::new(output_dir)
            .with_dialect(make_dialect());
        for (idx, cl) in codelists.iter().enumerate() {
            if let Ok(files) = codelist_sql_gen
                .generate(db, &cl.name, "common", config, tera, project)
                .await
            {
                for file in files {
                    let file = prefix_migration_path(file, idx + 10);
                    write_output(&file)?;
                    report.files.push(file);
                }
            }
        }
    }

    // Codelist Rust enums into domain-types crate (source-of-truth for DTOs)
    let codelist_gen = domain_types::codelist::DomainTypesCodelistGenerator::new_with_base(
        domain_types_base.map(|b| b.to_path_buf()).unwrap_or_else(|| output_dir.to_path_buf()),
    );
    match codelist_gen
        .generate_all(db, tera, project)
        .await
    {
        Ok(files) => {
            for file in &files {
                write_output(file)?;
            }
            report.files.extend(files);
        }
        Err(e) => {
            report.errors.push(report::GenerationError {
                entity: "(domain-types-codelists)".into(),
                generator: "domain_types_codelist".into(),
                source: e,
            });
        }
    }

    // Codelist Rust enum re-exports (generated app re-exports from hr_domain_types)
    match codelist::rust_enum::RustCodelistGenerator::new(output_dir)
        .generate_reexport_mod(db)
        .await
    {
        Ok(files) => {
            for file in &files {
                write_output(file)?;
            }
            report.files.extend(files);
        }
        Err(e) => {
            report.errors.push(report::GenerationError {
                entity: "(codelists)".into(),
                generator: "rust_enum".into(),
                source: e,
            });
        }
    }

    // Per-domain generators — run all (domain, generator) pairs in parallel
    let domains_with_entities = group_by_domain(&order);
    let domain_results: Vec<_> = futures::future::join_all(domains_with_entities.iter().flat_map(
        |(domain, entity_titles)| {
            domain_gens.iter().map(move |gen| {
                let domain = domain.clone();
                async move {
                    let result = gen.generate(db, &domain, entity_titles, config, tera, project).await;
                    (domain, gen.name().to_string(), result)
                }
            })
        },
    ))
    .await;

    for (domain, gen_name, result) in domain_results {
        match result {
            Ok(files) => {
                for file in &files {
                    write_output(file)?;
                }
                report.files.extend(files);
            }
            Err(e) => {
                report.errors.push(report::GenerationError {
                    entity: domain,
                    generator: gen_name,
                    source: e,
                });
            }
        }
    }

    // Global generators — run in parallel, fatal on failure
    let global_results: Vec<_> = futures::future::join_all(
        global_gens
            .iter()
            .map(|gen| gen.generate(db, config, &order, tera, project)),
    )
    .await;

    for result in global_results {
        let files = result?;
        for file in &files {
            write_output(file)?;
        }
        report.files.extend(files);
    }

    // Validate that every entity in the generation order has entity-specific files
    report.validate_consistency(&order, &config.defaults.type_suffix);

    // Generate mod.rs files for all directories under src/.
    // Collects all .rs files and subdirs, writes `pub mod` declarations.
    // Always overwrites existing mod.rs UNLESS it contains `pub use`
    // (indicating it was written by a specialised generator like codelist).
    let mod_files = generate_mod_files(&output_dir.join("src"))?;
    for file in &mod_files {
        write_output(file)?;
    }
    report.files.extend(mod_files);

    // Prune entity/mod.rs to only declare modules that are actually
    // referenced by repository code, eliminating thousands of dead-code
    // warnings from unused SeaORM entities.
    let entity_mod = prune_entity_mod(&output_dir.join("src"))?;
    if let Some(file) = entity_mod {
        write_output(&file)?;
        report.files.push(file);
    }

    Ok(report)
}

/// Group generation entries by domain, preserving order within each domain.
fn group_by_domain(entries: &[GenerationEntry]) -> Vec<(String, Vec<String>)> {
    let mut seen = HashSet::new();
    let mut domain_entities: HashMap<String, Vec<String>> = HashMap::new();
    let mut domain_order = Vec::new();

    let mut seen_entity_per_domain: HashSet<(String, String)> = HashSet::new();
    for entry in entries {
        if seen.insert(entry.domain.clone()) {
            domain_order.push(entry.domain.clone());
        }
        // Deduplicate entity titles within each domain
        if seen_entity_per_domain.insert((entry.domain.clone(), entry.schema_title.clone())) {
            domain_entities
                .entry(entry.domain.clone())
                .or_default()
                .push(entry.schema_title.clone());
        }
    }

    domain_order
        .into_iter()
        .map(|d| {
            let entities = domain_entities.remove(&d).unwrap_or_default();
            (d, entities)
        })
        .collect()
}

/// Compute the generation order using per-domain schema listing + domain order.
///
/// Previous approach used `get_entity_names()` (which deduplicates titles) then
/// `get_schema(title)` (which returns only one domain).  This lost entities whose
/// title appears in multiple domains (e.g. "OrderType" in both screening and
/// assessments).  We now use `list_schemas` per-domain so each domain gets its
/// own entry for shared titles.
pub async fn compute_generation_order(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<GenerationEntry>> {
    // Build domain order from config
    let domain_config_registry = codegraph_config::DomainRegistry::from_config(config.clone())
        .map_err(|e| Error::Config(e.to_string()))?;
    let domain_order = domain_config_registry
        .topological_order()
        .map_err(|e| Error::Config(e.to_string()))?;

    let domain_rank: HashMap<String, usize> = domain_order
        .iter()
        .enumerate()
        .map(|(i, d)| (d.clone(), i))
        .collect();

    // Get all schemas from the graph, grouped by domain.
    let all_schemas = db
        .list_schemas(None)
        .await
        .map_err(|e| Error::Config(e.to_string()))?;

    // Build a set of entity titles present in each domain's graph data.
    // We include all schemas that have a pg_table_name (meaning they produce
    // entity .rs files), not just is_entity=true schemas. This ensures that
    // ValueObject, CompositeWrapper, and other non-root types referenced by
    // DTO/repository code as crate::entity::<module>:: actually have files.
    // Inline/local definitions (parent_schema.is_some()) are excluded since
    // they are generated recursively as child entities from their parent.
    let mut graph_entities_by_domain: HashMap<String, HashSet<String>> = HashMap::new();
    for schema in &all_schemas {
        if schema.pg_table_name.is_empty() {
            continue;
        }
        if schema.parent_schema.is_some() {
            continue;
        }
        let domain = schema.domain.as_deref().unwrap_or("");
        if domain.is_empty() || !domain_rank.contains_key(domain) {
            continue;
        }
        graph_entities_by_domain
            .entry(domain.to_string())
            .or_default()
            .insert(schema.title.clone());
    }

    let mut entries = Vec::new();
    let mut seen_entries = HashSet::new();

    for domain_name in &domain_order {
        let domain_entry = match config.domains.get(domain_name.as_str()) {
            Some(e) => e,
            None => continue,
        };

        let exclude: HashSet<&str> = domain_entry.exclude.iter().map(|s| s.as_str()).collect();
        let force_vo: HashSet<&str> = domain_entry
            .force_value_objects
            .iter()
            .map(|s| s.as_str())
            .collect();

        let graph_entities = graph_entities_by_domain
            .get(domain_name.as_str())
            .cloned()
            .unwrap_or_default();

        // Collect titles: explicitly configured entities + graph-discovered entities
        let mut domain_titles: BTreeSet<String> = BTreeSet::new();
        for title in &domain_entry.entities {
            if graph_entities.contains(title.as_str()) {
                domain_titles.insert(title.clone());
            }
        }
        for title in &graph_entities {
            domain_titles.insert(title.clone());
        }

        for title in &domain_titles {
            // Skip excluded or force-VO types for this domain
            if exclude.contains(title.as_str()) || force_vo.contains(title.as_str()) {
                continue;
            }
            if !seen_entries.insert((title.clone(), domain_name.clone())) {
                continue;
            }

            entries.push(GenerationEntry {
                schema_title: title.clone(),
                domain: domain_name.clone(),
                pg_schema: domain_name.clone(),
                is_cyclic: false,
            });
        }
    }

    // Group entries by domain for intra-domain topological sorting
    let mut domain_groups: HashMap<String, Vec<GenerationEntry>> = HashMap::new();
    for entry in entries {
        domain_groups
            .entry(entry.domain.clone())
            .or_default()
            .push(entry);
    }

    // Bulk-fetch all schema→schema reference edges once, instead of N individual
    // get_referenced_schemas() calls. Build a lookup map for intra-domain sorting.
    let all_refs = db
        .list_all_schema_references()
        .await
        .map_err(|e| Error::Config(e.to_string()))?;
    let mut all_refs_map: HashMap<String, Vec<String>> = HashMap::new();
    for (src, tgt) in &all_refs {
        all_refs_map
            .entry(src.clone())
            .or_default()
            .push(tgt.clone());
    }

    // Build a set of all entity titles for transitive dependency resolution.
    let entity_titles: HashSet<String> = all_schemas
        .iter()
        .filter(|s| s.is_entity)
        .map(|s| s.title.clone())
        .collect();

    // Compute transitive entity dependencies through value-object schemas.
    // When entity A references VO V, and V references entity B, the DDL
    // generator emits a FK column on A's child table pointing to B. This
    // means B's migration must precede A's. Discover these transitive deps
    // by following reference chains through non-entity (VO) schemas.
    let mut transitive_entity_deps: HashMap<String, HashSet<String>> = HashMap::new();
    for entity_title in &entity_titles {
        let mut visited: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();
        // Seed with the entity's direct references
        if let Some(direct_refs) = all_refs_map.get(entity_title) {
            for r in direct_refs {
                stack.push(r.clone());
            }
        }
        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if entity_titles.contains(&current) && current != *entity_title {
                // Found an entity dependency (direct or transitive)
                transitive_entity_deps
                    .entry(entity_title.clone())
                    .or_default()
                    .insert(current.clone());
            } else if !entity_titles.contains(&current) {
                // Non-entity (value object): follow its references further
                if let Some(vo_refs) = all_refs_map.get(&current) {
                    for r in vo_refs {
                        if !visited.contains(r) {
                            stack.push(r.clone());
                        }
                    }
                }
            }
        }
    }

    // Merge transitive deps into all_refs_map so the topological sort picks them up.
    for (entity, deps) in &transitive_entity_deps {
        let entry = all_refs_map.entry(entity.clone()).or_default();
        for dep in deps {
            if !entry.contains(dep) {
                entry.push(dep.clone());
            }
        }
    }

    // For each domain, build an entity-level dependency graph and sort topologically.
    // This ensures that FK-target entities appear before entities that reference them.
    let mut sorted_entries = Vec::new();
    for domain_name in &domain_order {
        let group = match domain_groups.remove(domain_name) {
            Some(g) => g,
            None => continue,
        };

        // Build a title→index map for entities in this domain
        let title_to_idx: HashMap<String, usize> = group
            .iter()
            .enumerate()
            .map(|(i, e)| (e.schema_title.clone(), i))
            .collect();

        // Build adjacency (dependency edges) using pre-fetched reference map
        let mut in_degree = vec![0usize; group.len()];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); group.len()];

        for (idx, entry) in group.iter().enumerate() {
            let empty = Vec::new();
            let refs = all_refs_map.get(&entry.schema_title).unwrap_or(&empty);
            for ref_title in refs {
                // Only consider intra-domain dependencies; cross-domain deps
                // are already handled by domain-level topological ordering.
                if let Some(&dep_idx) = title_to_idx.get(ref_title) {
                    if dep_idx != idx {
                        dependents[dep_idx].push(idx);
                        in_degree[idx] += 1;
                    }
                }
            }
        }

        // Kahn's algorithm — seed queue with zero-in-degree entities, sorted
        // alphabetically for deterministic output.
        let mut queue: VecDeque<usize> = {
            let mut zeros: Vec<usize> = in_degree
                .iter()
                .enumerate()
                .filter(|(_, &d)| d == 0)
                .map(|(i, _)| i)
                .collect();
            zeros.sort_by(|a, b| group[*a].schema_title.cmp(&group[*b].schema_title));
            zeros.into_iter().collect()
        };

        let mut topo_order: Vec<usize> = Vec::with_capacity(group.len());
        while let Some(current) = queue.pop_front() {
            topo_order.push(current);
            // Sort dependents alphabetically before enqueuing for determinism
            let mut next: Vec<usize> = Vec::new();
            for &dep in &dependents[current] {
                in_degree[dep] -= 1;
                if in_degree[dep] == 0 {
                    next.push(dep);
                }
            }
            next.sort_by(|a, b| group[*a].schema_title.cmp(&group[*b].schema_title));
            queue.extend(next);
        }

        // Append any remaining entities involved in cycles (alphabetical fallback)
        if topo_order.len() < group.len() {
            let in_topo: HashSet<usize> = topo_order.iter().copied().collect();
            let mut remaining: Vec<usize> =
                (0..group.len()).filter(|i| !in_topo.contains(i)).collect();
            remaining.sort_by(|a, b| group[*a].schema_title.cmp(&group[*b].schema_title));
            topo_order.extend(remaining);
        }

        // Emit entries in topological order
        for idx in topo_order {
            sorted_entries.push(group[idx].clone());
        }
    }

    Ok(sorted_entries)
}

/// Render a serializable context through a Tera template.
/// Injects `project` config into the template context when provided.
pub fn render_template<C: serde::Serialize>(
    tera: &Tera,
    template_name: &str,
    ctx: &C,
) -> Result<String> {
    let context = tera::Context::from_serialize(ctx)
        .map_err(|e| Error::Template(format!("Serialize context: {}", e)))?;
    tera.render(template_name, &context)
        .map_err(|e| Error::Template(format!("'{}': {}", template_name, e)))
}

/// Like [`render_template`] but injects `project` into the template context.
/// Templates can use `{{ project.app_name }}`, `{{ project.domain_types_crate }}`, etc.
pub fn render_template_with_project<C: serde::Serialize>(
    tera: &Tera,
    template_name: &str,
    ctx: &C,
    project: &ProjectConfig,
) -> Result<String> {
    let mut context = tera::Context::from_serialize(ctx)
        .map_err(|e| Error::Template(format!("Serialize context: {}", e)))?;
    context.insert("project", project);
    tera.render(template_name, &context)
        .map_err(|e| Error::Template(format!("'{}': {}", template_name, e)))
}

/// Add a numeric prefix to migration file paths so alphabetical order matches
/// dependency order. Non-migration files are returned unchanged.
fn prefix_migration_path(mut file: GeneratedFile, seq: usize) -> GeneratedFile {
    let is_migration = file
        .path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|d| d == "migrations");
    if is_migration {
        if let Some(name) = file.path.file_name().and_then(|n| n.to_str()) {
            // Skip files that already have a numeric prefix (e.g. 0005_pgmq_setup.sql)
            if !name.starts_with(|c: char| c.is_ascii_digit()) {
                let prefixed = format!("{:04}_{}", seq, name);
                file.path = file.path.with_file_name(prefixed);
            }
        }
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_migration(name: &str) -> GeneratedFile {
        GeneratedFile {
            path: PathBuf::from("output/migrations").join(name),
            content: String::new(),
        }
    }

    #[test]
    fn test_prefix_migration_path_codelist_range() {
        // Codelists use seq = idx + 10, so range is 10..99
        for idx in 0..20u16 {
            let file = make_migration("common_some_codelist.sql");
            let result = prefix_migration_path(file, (idx + 10) as usize);
            let name = result.path.file_name().unwrap().to_str().unwrap();
            // Must start with 4-digit zero-padded prefix
            let prefix: String = name.chars().take(4).collect();
            assert!(
                prefix.chars().all(|c| c.is_ascii_digit()),
                "codelist prefix must be 4 digits, got: {name}"
            );
            let num: u16 = prefix.parse().unwrap();
            assert!(
                (10..100).contains(&num),
                "codelist seq {num} out of range 10..99 for idx {idx}"
            );
            assert_eq!(&name[4..5], "_", "5th char must be underscore: {name}");
        }
    }

    #[test]
    fn test_prefix_migration_path_entity_range() {
        // Entities start at seq 500 to avoid codelist overlap (codelists use 10..200)
        for idx in 0..20u16 {
            let file = make_migration("recruiting_candidate.sql");
            let result = prefix_migration_path(file, (idx + 500) as usize);
            let name = result.path.file_name().unwrap().to_str().unwrap();
            let prefix: String = name.chars().take(4).collect();
            assert!(
                prefix.chars().all(|c| c.is_ascii_digit()),
                "entity prefix must be 4 digits, got: {name}"
            );
            let num: u16 = prefix.parse().unwrap();
            assert!(
                num >= 500,
                "entity seq {num} should be >= 500 for idx {idx}"
            );
            assert_eq!(&name[4..5], "_", "5th char must be underscore: {name}");
        }
    }

    #[test]
    fn test_platform_files_use_four_digit_prefix() {
        let platform_files = [
            "0000_extensions.sql",
            "0001_basejump_install.sql",
            "0002_api_key_management.sql",
            "0003_pgmq_setup.sql",
            "0005_platform_schema.sql",
            "0006_workflow_seed.sql",
        ];
        for name in &platform_files {
            let prefix: String = name.chars().take(4).collect();
            assert!(
                prefix.chars().all(|c| c.is_ascii_digit()),
                "platform file must have 4-digit prefix: {name}"
            );
            let num: u16 = prefix.parse().unwrap();
            assert!(
                num < 10,
                "platform file seq {num} should be in 0..9: {name}"
            );
        }
    }

    #[test]
    fn test_migration_sort_order() {
        let mut files = vec![
            // Platform band (0-9)
            "0000_extensions.sql",
            "0001_basejump_install.sql",
            "0002_api_key_management.sql",
            "0003_pgmq_setup.sql",
            "0005_platform_schema.sql",
            "0006_workflow_seed.sql",
            // Codelist band (10-99)
            "0010_common_some_codelist.sql",
            "0011_common_other_codelist.sql",
            // Entity band (100+)
            "0100_recruiting_candidate.sql",
            "0101_recruiting_application.sql",
        ];
        let expected = files.clone();
        files.sort();
        assert_eq!(
            files, expected,
            "migration files must sort in platform < codelist < entity order"
        );
    }

    #[test]
    fn test_prefix_skips_already_numbered_files() {
        // Files that already start with a digit should not be double-prefixed
        let file = make_migration("0003_pgmq_setup.sql");
        let result = prefix_migration_path(file, 42);
        let name = result.path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            name, "0003_pgmq_setup.sql",
            "already-prefixed file should not be modified"
        );
    }

    #[test]
    fn test_non_migration_file_unchanged() {
        let file = GeneratedFile {
            path: PathBuf::from("output/src/model.rs"),
            content: String::new(),
        };
        let result = prefix_migration_path(file, 10);
        let name = result.path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            name, "model.rs",
            "non-migration file should not be prefixed"
        );
    }
}

/// Clean stale generated files from the output directory.
///
/// Removes entity-specific subdirectories under `src/domain/{domain}/` that
/// are NOT in the current generation order.  Stale modules from a previous run
/// (e.g. entities reclassified as value objects, or sample schemas removed from
/// the schema loader) must not linger and cause broken `pub mod` declarations
/// in mod.rs.
///
/// Directories that ARE in the generation order are left intact so that, if any
/// generator fails or is skipped partway through, the previous working state is
/// preserved.  Generators always overwrite files they produce, so leaving the
/// directory in place is safe.
///
/// The `mod.rs` file in each domain directory is always preserved; it is
/// regenerated by `generate_mod_files` after the generators run.
fn clean_generated_output(output_dir: &Path, generation_order: &[GenerationEntry], suffix: &str) {
    // Build the set of (domain, module_name) pairs that SHOULD exist after
    // this run.
    let mut expected: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for entry in generation_order {
        let stripped = codegraph_naming::strip_suffix(&entry.schema_title, suffix);
        let module_name = codegraph_naming::to_snake_case(&stripped);
        expected.insert((entry.domain.clone(), module_name));
    }

    // Build the set of (domain, path_segment) pairs for UI route/test directories.
    // path_segment is kebab-case of the stripped title.
    let mut expected_paths: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for entry in generation_order {
        let stripped = codegraph_naming::strip_suffix(&entry.schema_title, suffix);
        let path_segment = codegraph_naming::to_kebab_case(&stripped);
        expected_paths.insert((entry.domain.clone(), path_segment));
    }

    // Collect the set of domain names that appear in the generation order so
    // we only scan domains we actually care about.
    let domains: std::collections::HashSet<&str> =
        generation_order.iter().map(|e| e.domain.as_str()).collect();

    let src_domain_dir = output_dir.join("src").join("domain");

    for domain in &domains {
        let domain_dir = src_domain_dir.join(domain);
        if !domain_dir.is_dir() {
            continue;
        }

        // Walk the immediate children of src/domain/{domain}/.
        // Each subdirectory is an entity module; mod.rs is preserved.
        let entries = match fs::read_dir(&domain_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for child in entries.flatten() {
            let path = child.path();
            if !path.is_dir() {
                continue; // skip mod.rs and other files
            }
            let module_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            // Only remove directories that are NOT in the expected set.
            // Directories in the expected set are left for generators to overwrite —
            // if a generator fails partway through, the previous working state is preserved.
            let key = (domain.to_string(), module_name.clone());
            if expected.contains(&key) {
                continue;
            }
            tracing::debug!(
                domain = %domain,
                entity = %module_name,
                path = %path.display(),
                "removing stale entity directory"
            );
            let _ = fs::remove_dir_all(&path);
        }

        // Clean stale API handler files: src/api/{domain}/*_handler.rs
        let api_domain_dir = output_dir.join("src").join("api").join(domain);
        if api_domain_dir.is_dir() {
            if let Ok(api_entries) = fs::read_dir(&api_domain_dir) {
                for child in api_entries.flatten() {
                    let path = child.path();
                    let name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(n) => n.to_string(),
                        None => continue,
                    };
                    // Only clean *_handler.rs files (not mod.rs, router.rs, etc.)
                    if let Some(module) = name.strip_suffix("_handler.rs") {
                        let key = (domain.to_string(), module.to_string());
                        if !expected.contains(&key) {
                            tracing::debug!(
                                domain = %domain,
                                handler = %name,
                                path = %path.display(),
                                "removing stale API handler file"
                            );
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        // Also clean stale UI route directories: ui/src/routes/(app)/{domain}/{path_segment}/
        let ui_route_dir = output_dir
            .join("ui")
            .join("src")
            .join("routes")
            .join("(app)")
            .join(domain);
        if ui_route_dir.is_dir() {
            if let Ok(route_entries) = fs::read_dir(&ui_route_dir) {
                for child in route_entries.flatten() {
                    let path = child.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let seg = match path.file_name().and_then(|n| n.to_str()) {
                        Some(n) => n.to_string(),
                        None => continue,
                    };
                    // Keep special SvelteKit files like +layout.svelte's directory
                    if seg.starts_with('+') {
                        continue;
                    }
                    let key = (domain.to_string(), seg.clone());
                    if !expected_paths.contains(&key) {
                        tracing::debug!(
                            domain = %domain,
                            path_segment = %seg,
                            path = %path.display(),
                            "removing stale UI route directory"
                        );
                        let _ = fs::remove_dir_all(&path);
                    }
                }
            }
        }

        // Clean stale UI e2e test files: ui/tests/generated/{domain}/{path_segment}.*.test.ts
        let ui_tests_dir = output_dir
            .join("ui")
            .join("tests")
            .join("generated")
            .join(domain);
        if ui_tests_dir.is_dir() {
            if let Ok(test_entries) = fs::read_dir(&ui_tests_dir) {
                for child in test_entries.flatten() {
                    let path = child.path();
                    if path.is_dir() {
                        continue;
                    }
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        // Test file names are like "{path_segment}.api.crud.test.ts"
                        let path_seg = name.split('.').next().unwrap_or("").to_string();
                        let key = (domain.to_string(), path_seg);
                        if !expected_paths.contains(&key) {
                            tracing::debug!(
                                domain = %domain,
                                file = %name,
                                "removing stale UI test file"
                            );
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }
}

fn write_output(file: &GeneratedFile) -> Result<()> {
    if let Some(parent) = file.path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&file.path, &file.content)?;
    Ok(())
}

/// Clean stale IFML-generated route files that are no longer in the IFML model.
/// IFML routes are generated at src/routes/{view_name}/+page.svelte and +page.ts.
/// This removes routes for views that no longer exist in the current model.
fn clean_stale_ifml_routes(output_dir: &Path, active_views: &[String]) {
    let routes_dir = output_dir.join("src").join("routes");
    if !routes_dir.exists() {
        return;
    }

    let entries = match std::fs::read_dir(&routes_dir) {
        Ok(e) => e,
        _ => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip non-IFML directories (they start with _, (, or contain other chars)
        if dir_name.starts_with('_') || dir_name.starts_with('(') || dir_name.starts_with('.') {
            continue;
        }

        // If this directory name doesn't match any active view, it's stale
        let is_active = active_views.iter().any(|v| v.to_lowercase() == dir_name);
        if !is_active {
            // Check if the directory contains IFML-generated files
            let has_ifml_files = path.join("+page.svelte").exists();
            if has_ifml_files {
                tracing::debug!("Removing stale IFML route: {}", path.display());
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

/// Remove stale `mod.rs` files from a previous generation run.
/// This ensures `generate_mod_files` always creates fresh module declarations
/// that include all `.rs` files in the directory.
#[allow(dead_code)]
fn clean_stale_mod_files(src_dir: &Path) {
    if !src_dir.exists() {
        return;
    }
    for entry in walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_name() == "mod.rs" {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Generate `mod.rs` files for all directories under `src_dir` that contain `.rs` files.
///
/// Scans the directory tree and creates a `mod.rs` in each directory with
/// `pub mod <name>;` declarations for every `.rs` file and subdirectory.
/// Skips `mod.rs`, `main.rs`, and `lib.rs` (these are not submodules).
fn generate_mod_files(src_dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut files = Vec::new();
    generate_mod_files_recursive(src_dir, &mut files)?;
    Ok(files)
}

/// Returns `true` if the directory has any content (`.rs` files or subdirectories
/// with content), avoiding a redundant second `read_dir` call.
fn generate_mod_files_recursive(dir: &Path, out: &mut Vec<GeneratedFile>) -> Result<bool> {
    if !dir.is_dir() {
        return Ok(false);
    }

    let mut modules = BTreeSet::new();
    let mut has_content = false;

    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            let subdir_has_content = generate_mod_files_recursive(&path, out)?;
            if subdir_has_content {
                modules.insert(name);
            }
        } else if let Some(ext) = path.extension() {
            if ext == "rs" {
                has_content = true;
                // Skip special files that aren't submodules
                if matches!(name.as_str(), "mod.rs" | "main.rs" | "lib.rs") {
                    continue;
                }
                let module_name = name.strip_suffix(".rs").unwrap_or(&name);
                modules.insert(module_name.to_string());
            }
        }
    }

    if !modules.is_empty() {
        has_content = true;
        let mod_path = dir.join("mod.rs");
        // Skip if a mod.rs with pub-use re-exports exists (written by a specialised
        // generator like codelist). Always overwrite plain `pub mod` declarations.
        let skip = mod_path.exists()
            && fs::read_to_string(&mod_path)
                .map(|c| c.contains("pub use "))
                .unwrap_or(false);
        if !skip {
            let content = modules
                .iter()
                .map(|m| format!("pub mod {};", m))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            out.push(GeneratedFile {
                path: mod_path,
                content,
            });
        }
    }

    Ok(has_content)
}

/// Scan `src/domain/` for `crate::entity::<module>::` references and rewrite
/// `src/entity/mod.rs` to only declare the modules that are actually used.
/// This eliminates thousands of dead-code warnings from unused SeaORM entities.
fn prune_entity_mod(src_dir: &Path) -> Result<Option<GeneratedFile>> {
    let entity_mod_path = src_dir.join("entity").join("mod.rs");
    if !entity_mod_path.exists() {
        return Ok(None);
    }

    let domain_dir = src_dir.join("domain");
    if !domain_dir.is_dir() {
        return Ok(None);
    }

    // Collect all entity module names referenced via `crate::entity::<name>::`
    let prefix = "crate::entity::";
    let mut used = BTreeSet::new();

    fn scan_dir(dir: &Path, prefix: &str, used: &mut BTreeSet<String>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, prefix, used)?;
            } else if path.extension().is_some_and(|e| e == "rs") {
                let content = fs::read_to_string(&path)?;
                for (idx, _) in content.match_indices(prefix) {
                    let rest = &content[idx + prefix.len()..];
                    let module: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !module.is_empty() {
                        used.insert(module);
                    }
                }
            }
        }
        Ok(())
    }

    scan_dir(&domain_dir, prefix, &mut used)?;
    // Also scan api/ for entity references (e.g., media upload handlers).
    let api_dir = src_dir.join("api");
    if api_dir.is_dir() {
        scan_dir(&api_dir, prefix, &mut used)?;
    }

    if used.is_empty() {
        return Ok(None);
    }

    let content = used
        .iter()
        .map(|m| format!("pub mod {};", m))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    Ok(Some(GeneratedFile {
        path: entity_mod_path,
        content,
    }))
}
