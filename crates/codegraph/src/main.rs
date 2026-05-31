use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::Parser;
use codegraph_backend::{create_backend, BackendConfig};

mod cli;

/// Arguments for the `run` subcommand.
struct RunArgs<'a> {
    schemas: &'a Path,
    classifier: &'a Path,
    config_path: &'a Path,
    output: &'a Path,
    extension_points_path: Option<&'a Path>,
    profile_name: &'a str,
    variant: Option<&'a str>,
    profiles_config_path: Option<PathBuf>,
    no_post_gen: bool,
    ifml_files: &'a [PathBuf],
}

#[tokio::main]
async fn main() -> codegraph::error::Result<()> {
    let cli = cli::Cli::parse();

    match cli.command {
        cli::Commands::Generate {
            config,
            output,
            extension_points,
        } => cmd_generate(&config, &output, extension_points.as_deref()).await,
        cli::Commands::Classify {
            schemas,
            classifier,
            config,
            domain,
            format,
        } => cmd_classify(&schemas, &classifier, &config, domain.as_deref(), format).await,
        cli::Commands::Run {
            schemas,
            classifier,
            config,
            output,
            extension_points,
            profile,
            variant,
            profiles_config,
            no_post_gen,
            ifml_files,
        } => {
            cmd_run(RunArgs {
                schemas: &schemas,
                classifier: &classifier,
                config_path: &config,
                output: &output,
                extension_points_path: extension_points.as_deref(),
                profile_name: &profile,
                variant: variant.as_deref(),
                profiles_config_path: profiles_config,
                no_post_gen,
                ifml_files: &ifml_files,
            })
            .await
        }
        cli::Commands::Lsp { schemas, classifier, config } => {
            cmd_lsp(&schemas, classifier.as_deref(), config.as_deref()).await
        }
    }
}

async fn run_validation(
    querier: &dyn codegraph_core::traits::GraphQuerier,
    config: &codegraph_config::DomainConfig,
) -> codegraph::error::Result<()> {
    let issues = codegraph::validate::ValidationPass::run(querier, config).await;
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.severity, codegraph::validate::Severity::Error))
        .collect();
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("  ERR  {} [{}]: {}", e.entity, e.check, e.message);
        }
        return Err(codegraph::error::Error::Validation(format!(
            "{} validation errors found",
            errors.len()
        )));
    }
    for w in issues
        .iter()
        .filter(|i| matches!(i.severity, codegraph::validate::Severity::Warning))
    {
        eprintln!("  WARN {} [{}]: {}", w.entity, w.check, w.message);
    }
    Ok(())
}

fn load_ui_overrides(
    config_path: &Path,
) -> codegraph::error::Result<codegraph_config::UiOverrideConfig> {
    let parent = config_path.parent().ok_or_else(|| {
        codegraph::error::Error::Config(format!(
            "Config path '{}' has no parent directory",
            config_path.display()
        ))
    })?;
    let path = parent.join("ui-overrides.toml");
    if path.exists() {
        codegraph_config::parse_ui_overrides_config(&path)
            .map_err(|e| codegraph::error::Error::Config(e.to_string()))
    } else {
        Ok(codegraph_config::UiOverrideConfig::default())
    }
}

fn load_seed_config(config_path: &Path) -> Option<PathBuf> {
    let path = config_path.parent()?.join("seed.toml");
    if path.exists() { Some(path) } else { None }
}

fn load_ui_domains(
    config_path: &Path,
) -> codegraph::error::Result<codegraph_config::UiDomainConfig> {
    let parent = config_path.parent().ok_or_else(|| {
        codegraph::error::Error::Config(format!(
            "Config path '{}' has no parent directory",
            config_path.display()
        ))
    })?;
    let path = parent.join("ui-domains.toml");
    if path.exists() {
        codegraph_config::parse_ui_domains_config(&path)
            .map_err(|e| codegraph::error::Error::Config(e.to_string()))
    } else {
        Ok(codegraph_config::UiDomainConfig::default())
    }
}

async fn cmd_generate(
    config_path: &Path,
    output: &Path,
    extension_points_path: Option<&Path>,
) -> codegraph::error::Result<()> {
    let config = codegraph_config::config::parse_domain_config(config_path)
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    let backend_config = BackendConfig::default();
    let be = create_backend(&backend_config)
        .await
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    let ui_overrides = load_ui_overrides(config_path)?;
    let ui_domains = load_ui_domains(config_path)?;

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir)?;

    let ext_config = match extension_points_path {
        Some(path) => Some(
            codegraph_ext_points::parse_extension_points(path)
                .map_err(|e| codegraph::error::Error::Config(e.to_string()))?,
        ),
        None => None,
    };

    // cmd_generate uses a pre-populated backend; schema base dir is unknown here.
    // Pass an empty path so UiCodelistGenerator skips gracefully.
    run_validation(be.querier(), &config).await?;
    let report = codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
        db: be.querier(),
        config: &config,
        output_dir: output,
        tera: &tera,
        ui_overrides: &ui_overrides,
        ui_domains: &ui_domains,
        schema_base_dir: Path::new(""),
        seed_config: load_seed_config(config_path).as_deref(),
        domain_types_base: None,
        hooks_base: None,
        ext_points: ext_config.as_ref(),
        build_plan: None,
    })
    .await?;
    print!("{}", report.summary());
    if report.has_errors() {
        eprintln!("Generation completed with errors. Some entities were skipped.");
    }
    Ok(())
}

async fn cmd_run(args: RunArgs<'_>) -> codegraph::error::Result<()> {
    let RunArgs {
        schemas,
        classifier,
        config_path,
        output,
        extension_points_path,
        profile_name,
        variant,
        profiles_config_path,
        no_post_gen,
        ifml_files,
    } = args;

    let backend_config = BackendConfig::default();
    let be = create_backend(&backend_config)
        .await
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    let domain_config = codegraph_config::config::parse_domain_config(config_path)
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;
    let classifier_config = codegraph_classifier::config::parse_classifier_config(classifier)
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    let ui_overrides = load_ui_overrides(config_path)?;
    let ui_domains = load_ui_domains(config_path)?;

    // Load and resolve the profile.
    let profiles_path = profiles_config_path.unwrap_or_else(|| PathBuf::from("profiles.toml"));
    let registry = codegraph::profile::CapabilityRegistry::new();
    let build_plan = if profiles_path.exists() || profile_name != "default" {
        let resolved =
            codegraph::profile::load_and_resolve_profile(&profiles_path, profile_name, variant)?;
        let plan = codegraph::profile::BuildPlan::from_profile(&resolved, &registry)?;
        println!(
            "Using profile \"{}\" (variant: {:?}) — {} entity, {} domain, {} global generators",
            resolved.meta.name,
            variant,
            plan.entity_generators.len(),
            plan.domain_generators.len(),
            plan.global_generators.len(),
        );
        Some(plan)
    } else {
        // profiles.toml doesn't exist and profile is "default" —
        // backward compat: run all generators without a plan.
        eprintln!(
            "Warning: profiles.toml not found at {} — running all generators (no profile filtering)",
            profiles_path.display()
        );
        None
    };

    // Pass 1: Ingest all schemas (no entity classification)
    let empty_entities = HashSet::new();
    let ingest_result = codegraph::ingest::async_ingest::ingest_schemas(
        be.ingestor(),
        schemas,
        &classifier_config,
        &empty_entities,
        &ui_overrides,
        &domain_config.defaults.type_suffix,
    )
    .await?;
    println!(
        "Pass 1 complete: {} schemas ingested",
        ingest_result.schemas_created
    );

    // Pass 1b: Ingest IFML DSL files (if provided)
    if !ifml_files.is_empty() {
        println!("Pass 1b: {} IFML files to ingest", ifml_files.len());
        let mut total_stats = codegraph::ingest::ifml_ingest::IfmlIngestStats::default();
        for ifml_path in ifml_files {
            let model = codegraph_ifml_dsl::parse_ifml_file(ifml_path)
                .map_err(|e| codegraph::error::Error::Config(format!(
                    "Failed to parse IFML file '{}': {}", ifml_path.display(), e
                )))?;
            let stats = codegraph::ingest::ifml_ingest::ingest_ifml_model(be.ingestor(), &model).await?;
            total_stats.view_containers += stats.view_containers;
            total_stats.containers += stats.containers;
            total_stats.components += stats.components;
            total_stats.events += stats.events;
            total_stats.parameters += stats.parameters;
            total_stats.actions += stats.actions;
        }
        println!("Pass 1b complete: {total_stats}");
    }

    // Auto-classify
    let classifier_types: HashSet<String> = classifier_config
        .primitive_wrappers
        .keys()
        .cloned()
        .chain(classifier_config.array_wrappers.keys().cloned())
        .chain(classifier_config.range_wrappers.keys().cloned())
        .chain(
            classifier_config
                .composite_wrappers
                .iter()
                .map(|cw| cw.schema.clone()),
        )
        .collect();

    let all_data = be
        .querier()
        .get_classification_data()
        .await
        .map_err(codegraph::error::Error::Graph)?;

    let naming_rules = classifier_config.naming_rules.clone();
    let auto_classifier = codegraph::classify::AutoClassifier::new(classifier_types, naming_rules);
    let mut entity_names = HashSet::new();

    let mut sorted_domain_names: Vec<&String> = domain_config.domains.keys().collect();
    sorted_domain_names.sort();
    for domain_name in &sorted_domain_names {
        let domain_entry = &domain_config.domains[domain_name.as_str()];
        let domain_schemas: Vec<_> = all_data
            .iter()
            .filter(|d| d.domain.as_deref() == Some(domain_name.as_str()))
            .cloned()
            .collect();
        let result = auto_classifier.classify_domain(domain_name, domain_entry, &domain_schemas);
        for score in &result.entities {
            entity_names.insert(score.title.clone());
        }
    }

    // Also include legacy entities[] for backward compat during migration
    for domain_entry in domain_config.domains.values() {
        for entity in &domain_entry.entities {
            entity_names.insert(entity.clone());
        }
    }

    println!("Auto-classified {} entities", entity_names.len());

    // Pass 2: Update graph with entity flags
    codegraph::ingest::async_ingest::reclassify_with_entities(
        be.ingestor(),
        be.querier(),
        &entity_names,
    )
    .await?;

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = codegraph::generate::template_engine::create_tera(&template_dir)?;

    let ext_config = match extension_points_path {
        Some(path) => Some(
            codegraph_ext_points::parse_extension_points(path)
                .map_err(|e| codegraph::error::Error::Config(e.to_string()))?,
        ),
        None => None,
    };

    run_validation(be.querier(), &domain_config).await?;

    let report = codegraph::generate::run_generators_with_opts(codegraph::generate::GeneratorOpts {
        db: be.querier(),
        config: &domain_config,
        output_dir: output,
        tera: &tera,
        ui_overrides: &ui_overrides,
        ui_domains: &ui_domains,
        schema_base_dir: schemas,
        seed_config: load_seed_config(config_path).as_deref(),
        domain_types_base: None,
        hooks_base: None,
        ext_points: ext_config.as_ref(),
        build_plan: build_plan.as_ref(),
    })
    .await?;

    print!("{}", report.summary());
    if report.has_errors() {
        eprintln!("Generation completed with errors. Some entities were skipped.");
    }

    // Run post-generation scripts from the profile plan.
    if let Some(ref plan) = build_plan {
        if !no_post_gen && !plan.post_gen_scripts.is_empty() {
            println!("\nPost-generation scripts:");
            for (section, scripts) in &plan.post_gen_scripts {
                for cmd in scripts {
                    println!("  [{section}] {cmd}");
                    let status = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(cmd)
                        .status()
                        .map_err(|e| {
                            codegraph::error::Error::Config(format!(
                                "failed to run post_gen script [{section}] {cmd}: {e}"
                            ))
                        })?;
                    if !status.success() {
                        let code = status.code().unwrap_or(-1);
                        return Err(codegraph::error::Error::Config(format!(
                            "post_gen script [{section}] {cmd} failed (exit {code})"
                        )));
                    }
                }
            }
        }
    }

    println!("Done.");
    Ok(())
}

async fn cmd_classify(
    schemas: &Path,
    classifier_path: &Path,
    config_path: &Path,
    domain_filter: Option<&str>,
    format: cli::ClassifyFormat,
) -> codegraph::error::Result<()> {
    let domain_config = codegraph_config::config::parse_domain_config(config_path)
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;
    let classifier_config = codegraph_classifier::config::parse_classifier_config(classifier_path)
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    let classifier_types: HashSet<String> = classifier_config
        .primitive_wrappers
        .keys()
        .cloned()
        .chain(classifier_config.array_wrappers.keys().cloned())
        .chain(classifier_config.range_wrappers.keys().cloned())
        .chain(
            classifier_config
                .composite_wrappers
                .iter()
                .map(|cw| cw.schema.clone()),
        )
        .collect();

    let backend_config = BackendConfig::default();
    let be = create_backend(&backend_config)
        .await
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    let ui_overrides = load_ui_overrides(config_path)?;

    let empty_entities = HashSet::new();
    codegraph::ingest::async_ingest::ingest_schemas(
        be.ingestor(),
        schemas,
        &classifier_config,
        &empty_entities,
        &ui_overrides,
        &domain_config.defaults.type_suffix,
    )
    .await?;

    let all_data = be
        .querier()
        .get_classification_data()
        .await
        .map_err(codegraph::error::Error::Graph)?;

    let naming_rules = classifier_config.naming_rules.clone();
    let auto_classifier = codegraph::classify::AutoClassifier::new(classifier_types, naming_rules);
    let mut results = Vec::new();

    let mut sorted_domain_names: Vec<&String> = domain_config.domains.keys().collect();
    sorted_domain_names.sort();
    for domain_name in &sorted_domain_names {
        let domain_entry = &domain_config.domains[domain_name.as_str()];
        let domain_schemas: Vec<_> = all_data
            .iter()
            .filter(|d| d.domain.as_deref() == Some(domain_name.as_str()))
            .cloned()
            .collect();

        let result = auto_classifier.classify_domain(domain_name, domain_entry, &domain_schemas);
        results.push(result);
    }

    results.sort_by(|a, b| a.domain.cmp(&b.domain));

    match format {
        cli::ClassifyFormat::Table => {
            codegraph::classify::output::format_table(&results, domain_filter)
        }
        cli::ClassifyFormat::Json => codegraph::classify::output::format_json(&results),
    }

    Ok(())
}

async fn cmd_lsp(
    schema_dirs: &[PathBuf],
    classifier: Option<&Path>,
    config: Option<&Path>,
) -> codegraph::error::Result<()> {
    use codegraph::lsp::{run_lsp_server, LspBackend};
    use codegraph_backend::{create_backend, BackendConfig};
    use lsp_server::Connection;

    let _backend_config = BackendConfig::default();
    let _be = create_backend(&_backend_config)
        .await
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    // Load JSON Schema files into the graph (future: pass to LspBackend)
    for dir in schema_dirs {
        if dir.exists() {
            let empty_entities = std::collections::HashSet::new();
            let default_ui = codegraph_config::UiOverrideConfig::default();
            let default_suffix = "Type".to_string();
            let classifier_config = if let Some(classifier_path) = classifier {
                codegraph_classifier::config::parse_classifier_config(classifier_path)
                    .map_err(|e| codegraph::error::Error::Config(e.to_string()))?
            } else {
                codegraph_classifier::config::parse_classifier_config_str("{}")
                    .map_err(|e| codegraph::error::Error::Config(e.to_string()))?
            };

            codegraph::ingest::async_ingest::ingest_schemas(
                _be.ingestor(),
                dir,
                &classifier_config,
                &empty_entities,
                &default_ui,
                &default_suffix,
            )
            .await?;
        }
    }

    eprintln!("codegraph LSP server starting (IFML language)...");
    let (connection, io_threads) = Connection::stdio();
    let backend = LspBackend::new();

    run_lsp_server(connection, backend)?;

    io_threads
        .join()
        .map_err(|e| codegraph::error::Error::Config(format!("IO thread error: {e}")))?;

    Ok(())
}
