use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct ScaffoldContext {
    pub app_name: String,
    pub domains: Vec<ScaffoldDomain>,
    pub codegraph_workflow_path: String,
    pub type_contracts_path: String,
    pub domain_types_path: String,
    pub hooks_api_path: String,
    pub extensions_path: String,
    pub app_config_path: String,
    pub decision_engine_path: String,
    pub has_webhooks: bool,
    pub has_reports: bool,
    pub has_grpc: bool,
}

#[derive(Debug, Serialize)]
pub struct ScaffoldEntity {
    pub name: String,
    pub module_name: String,
    pub domain: String,
}

#[derive(Debug, Serialize)]
pub struct ScaffoldDomain {
    pub name: String,
    pub label: String,
    pub postgres_schema: String,
    pub entities: Vec<ScaffoldEntity>,
}

pub struct ScaffoldGenerator {
    output_dir: PathBuf,
    has_webhooks: bool,
    has_reports: bool,
    has_grpc: bool,
}

impl ScaffoldGenerator {
    pub fn new(output_dir: &Path, has_webhooks: bool, has_reports: bool, has_grpc: bool) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            has_webhooks,
            has_reports,
            has_grpc,
        }
    }
}

/// Resolve a base path (relative to CWD or absolute) to a path relative
/// to the output directory. Returns empty string if `base` is empty.
fn resolve_path(base: &str, abs_output: &Path) -> String {
    if base.is_empty() {
        return String::new();
    }
    let abs_base = std::env::current_dir()
        .unwrap_or_default()
        .join(base);
    pathdiff::diff_paths(&abs_base, abs_output)
        .unwrap_or_else(|| PathBuf::from(base))
        .to_string_lossy()
        .into_owned()
}

#[async_trait]
impl GlobalGenerator for ScaffoldGenerator {
    fn name(&self) -> &str {
        "scaffold"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Group generation_order entries by domain
        let mut domain_entity_map: std::collections::HashMap<String, Vec<ScaffoldEntity>> =
            std::collections::HashMap::new();
        let mut seen_scaffold_entities = std::collections::HashSet::new();
        for entry in generation_order {
            let stripped = config.defaults.strip_suffix(&entry.schema_title);
            let module_name = codegraph_naming::to_snake_case(&stripped);
            // Dedup by (domain, module_name) to prevent cross-domain name collisions
            if !seen_scaffold_entities.insert((entry.domain.clone(), module_name.clone())) {
                continue;
            }
            domain_entity_map
                .entry(entry.domain.clone())
                .or_default()
                .push(ScaffoldEntity {
                    module_name,
                    name: stripped,
                    domain: entry.domain.clone(),
                });
        }

        let mut domains: Vec<ScaffoldDomain> = config
            .domains
            .iter()
            .filter_map(|(name, entry)| {
                let entities = domain_entity_map.remove(name.as_str())?;
                Some(ScaffoldDomain {
                    name: name.clone(),
                    label: entry.label.clone(),
                    postgres_schema: entry.postgres_schema.clone(),
                    entities,
                })
            })
            .collect();
        domains.sort_by(|a, b| a.name.cmp(&b.name));

        // Compute absolute output dir (shared by all path calculations)
        let abs_output = if self.output_dir.is_absolute() {
            self.output_dir.clone()
        } else {
            std::env::current_dir()
                .expect("current_dir should be accessible")
                .join(&self.output_dir)
        };

        let codegraph_workflow_path = resolve_path(&project.codegraph_workflow_base, &abs_output);
        let type_contracts_path = resolve_path(&project.type_contracts_base, &abs_output);
        let domain_types_path = resolve_path(&project.domain_types_base, &abs_output);
        let hooks_api_path = resolve_path(&project.hooks_api_base, &abs_output);
        let extensions_path = resolve_path(&project.extensions_base, &abs_output);
        let app_config_path = resolve_path(&project.app_config_base, &abs_output);
        let decision_engine_path = resolve_path(&project.decision_engine_base, &abs_output);

        let ctx = ScaffoldContext {
            app_name: crate::generate::get_project_config().app_name.clone(),
            domains,
            codegraph_workflow_path,
            type_contracts_path,
            domain_types_path,
            hooks_api_path,
            extensions_path,
            app_config_path,
            decision_engine_path,
            has_webhooks: self.has_webhooks,
            has_reports: self.has_reports,
            has_grpc: self.has_grpc,
        };

        let mut files = Vec::new();

        let main_rs = render_template_with_project(tera, "scaffold/main.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("main.rs"),
            content: main_rs,
        });

        let app_state = render_template_with_project(tera, "scaffold/app_state.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("app_state.rs"),
            content: app_state,
        });

        let cargo_toml = render_template_with_project(tera, "scaffold/cargo_toml.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("Cargo.toml"),
            content: cargo_toml,
        });

        let build_rs = render_template_with_project(tera, "scaffold/build_rs.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("build.rs"),
            content: build_rs,
        });

        let lib_rs = render_template_with_project(tera, "scaffold/lib.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("lib.rs"),
            content: lib_rs,
        });

        let error_rs = render_template_with_project(tera, "scaffold/error.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("error.rs"),
            content: error_rs,
        });

        let middleware_rs = render_template_with_project(tera, "scaffold/middleware.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("middleware.rs"),
            content: middleware_rs,
        });

        let metrics_middleware_rs =
            render_template_with_project(tera, "scaffold/metrics_middleware.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("metrics_middleware.rs"),
            content: metrics_middleware_rs,
        });

        let qs_query_rs = render_template_with_project(tera, "scaffold/qs_query.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("qs_query.rs"),
            content: qs_query_rs,
        });

        let integrations_rs = render_template_with_project(tera, "scaffold/integrations_handler.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("integrations.rs"),
            content: integrations_rs,
        });

        let api_key_migration = render_template_with_project(tera, "db/api_key_migration.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0002_api_key_management.sql"),
            content: api_key_migration,
        });

        Ok(files)
    }
}
