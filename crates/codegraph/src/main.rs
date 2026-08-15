use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use clap::Parser;
use codegraph_backend::{create_backend, BackendConfig};

mod cli;

#[tokio::main]
async fn main() -> codegraph::error::Result<()> {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init();
    }

    let cli = cli::Cli::parse();

    match cli.command {
        cli::Commands::Generate {
            config,
            output,
            extension_points,
            template_dir,
            ifml_framework,
        } => {
            codegraph::driver::generate(
                &config,
                &output,
                extension_points.as_deref(),
                &template_dir,
                &ifml_framework,
            )
            .await
        }
        cli::Commands::Migrate(args) => cmd_migrate(args).await,
        cli::Commands::Classify {
            schemas,
            classifier,
            config,
            domain,
            format,
        } => {
            let format = match format {
                cli::ClassifyFormat::Table => codegraph::driver::ClassifyFormat::Table,
                cli::ClassifyFormat::Json => codegraph::driver::ClassifyFormat::Json,
            };
            codegraph::driver::classify(
                &schemas,
                &classifier,
                &config,
                domain.as_deref(),
                format,
            )
            .await
        }
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
            template_dir,
            ifml_files,
            openapi_files,
            ifml_framework,
        } => {
            codegraph::driver::run(codegraph::driver::RunArgs {
                schemas: &schemas,
                classifier: &classifier,
                config_path: &config,
                output: &output,
                extension_points_path: extension_points.as_deref(),
                profile_name: &profile,
                variant: variant.as_deref(),
                profiles_config_path: profiles_config,
                no_post_gen,
                template_dir: &template_dir,
                ifml_files: &ifml_files,
                openapi_files: &openapi_files,
                ifml_framework: &ifml_framework,
                codegraph_rev: None,
            })
            .await
        }
        cli::Commands::Lsp {
            schemas,
            classifier,
            config,
        } => cmd_lsp(&schemas, classifier.as_deref(), config.as_deref()).await,
        cli::Commands::Init {
            name,
            output,
            domains,
            database_target,
            persistence_provider,
            deployment_topology,
            grpc,
            ifml,
            no_ops,
            rev,
            codegraph_path,
            force,
            template_dir,
        } => {
            let args = codegraph::init::commands::InitArgs {
                name,
                output_dir: output.unwrap_or_else(|| PathBuf::from(".")),
                domains: domains.unwrap_or_else(|| vec!["common".to_string()]),
                database_target,
                persistence_provider,
                deployment_topology,
                grpc,
                ifml,
                ops: !no_ops,
                rev,
                codegraph_path,
                force,
                template_dirs: template_dir,
            };
            codegraph::init::commands::cmd_init(&args)
        }
        cli::Commands::Doctor {
            config,
            schemas,
            classifier,
            profiles_config,
        } => {
            let args = codegraph::init::commands::DoctorArgs {
                config,
                schemas,
                classifier,
                profiles_config,
            };
            codegraph::init::commands::cmd_doctor(&args)
        }
        cli::Commands::Add { target } => match target {
            cli::AddTarget::Domain { name } => codegraph::init::commands::cmd_add_domain(
                &PathBuf::from("domains.toml"),
                &PathBuf::from("schemas"),
                &name,
            ),
        },
    }
}


async fn cmd_migrate(args: cli::MigrateArgs) -> codegraph::error::Result<()> {
    let domain_config = codegraph_config::config::parse_domain_config(&args.config)
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    let be = create_backend(&BackendConfig::default())
        .await
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    println!(
        "Ingesting API model from domain configuration '{}'...",
        args.config.display()
    );
    let stats =
        codegraph::ingest::api_ingest::ingest_api_model(be.ingestor(), &domain_config).await?;

    println!("Migration complete: {stats}");
    println!(
        "{} API resources, {} operations, {} endpoints, {} interactions created",
        stats.resources, stats.operations, stats.endpoints, stats.interactions
    );

    Ok(())
}

async fn cmd_lsp(
    schema_dirs: &[PathBuf],
    classifier: Option<&Path>,
    config: Option<&Path>,
) -> codegraph::error::Result<()> {
    use codegraph::lsp::{run_lsp_server, GrafeoState, SchemaInfo};
    use codegraph_backend::{create_backend, BackendConfig};

    let backend_config = BackendConfig::default();
    let be = create_backend(&backend_config)
        .await
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    // Load JSON Schema files into the graph
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
                be.ingestor(),
                dir,
                &classifier_config,
                &empty_entities,
                &default_ui,
                &default_suffix,
            )
            .await?;
        }
    }

    // Run entity classification using AutoClassifier if domain config is provided.
    // This replaces the naive suffix-stripping with structural scoring + naming rules.
    let classifier_config = if let Some(classifier_path) = classifier {
        codegraph_classifier::config::parse_classifier_config(classifier_path)
            .map_err(|e| codegraph::error::Error::Config(e.to_string()))?
    } else {
        codegraph_classifier::config::parse_classifier_config_str("{}")
            .map_err(|e| codegraph::error::Error::Config(e.to_string()))?
    };

    let all_data = be
        .querier()
        .get_classification_data()
        .await
        .map_err(codegraph::error::Error::Graph)?;

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

    let naming_rules = classifier_config.naming_rules.clone();
    let auto_classifier = codegraph::classify::AutoClassifier::new(classifier_types, naming_rules);

    // Build entity names by stripping the "Type" suffix from raw schema titles.
    // IFML references entities without the suffix (e.g. "Customer" not "CustomerType").
    // We ALSO try the AutoClassifier for more precise entity/VO classification,
    // but always include suffix-stripped names as a reliable fallback.
    let default_suffix = "Type";
    let mut entity_names_set: HashSet<String> = HashSet::new();

    // Always include suffix-stripped names for every loaded schema (reliable fallback)
    let schemas = be.querier().list_schemas(None).await?;
    for schema in &schemas {
        entity_names_set.insert(
            schema
                .title
                .strip_suffix(default_suffix)
                .unwrap_or(&schema.title)
                .to_string(),
        );
    }

    // Also try AutoClassifier if domain config is provided (more precise)
    if let Some(config_path) = config {
        if let Ok(domain_config) = codegraph_config::config::parse_domain_config(config_path) {
            for (domain_name, domain_entry) in &domain_config.domains {
                let domain_schemas: Vec<_> = all_data
                    .iter()
                    .filter(|d| d.domain.as_deref() == Some(domain_name.as_str()))
                    .cloned()
                    .collect();
                let result =
                    auto_classifier.classify_domain(domain_name, domain_entry, &domain_schemas);
                for score in &result.entities {
                    let name = score
                        .title
                        .strip_suffix(default_suffix)
                        .unwrap_or(&score.title)
                        .to_string();
                    entity_names_set.insert(name);
                }
                // Also include legacy explicit entities from domains.toml
                for entity in &domain_entry.entities {
                    entity_names_set.insert(entity.clone());
                }
            }
        }
    }

    let entity_names: Vec<String> = entity_names_set.into_iter().collect();

    // Build schema_infos keyed by entity name for synchronous LSP access
    let schemas = be.querier().list_schemas(None).await?;
    let mut schema_infos = HashMap::new();
    for schema in &schemas {
        let entity_name = schema
            .title
            .strip_suffix(default_suffix)
            .unwrap_or(&schema.title)
            .to_string();
        if let Ok(props) = be.querier().get_properties(&schema.title).await {
            schema_infos.insert(
                entity_name,
                SchemaInfo {
                    title: schema.title.clone(),
                    description: schema.description.clone(),
                    properties: props.iter().map(|p| p.name.clone()).collect(),
                    rel_path: schema.rel_path.clone(),
                },
            );
        }
    }

    let grafeo_state = GrafeoState {
        entity_names,
        schema_infos,
        schema_dirs: schema_dirs.to_vec(),
    };

    eprintln!("codegraph LSP server starting (IFML language)...");
    let (connection, _io_threads) = auto_lsp::lsp_server::Connection::stdio();
    run_lsp_server(connection, grafeo_state)
        .map_err(|e| codegraph::error::Error::Config(e.to_string()))?;

    Ok(())
}

