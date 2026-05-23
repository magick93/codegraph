use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
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
    pub hr_config_path: String,
    pub compliance_path: String,
    pub has_webhooks: bool,
    pub has_reports: bool,
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
}

impl ScaffoldGenerator {
    pub fn new(output_dir: &Path, has_webhooks: bool, has_reports: bool) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            has_webhooks,
            has_reports,
        }
    }
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

        // Compute workspace root and absolute output dir (shared by all path calculations)
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("CARGO_MANIFEST_DIR should have a parent");
        let abs_output = if self.output_dir.is_absolute() {
            self.output_dir.clone()
        } else {
            std::env::current_dir()
                .expect("current_dir should be accessible")
                .join(&self.output_dir)
        };

        let codegraph_workflow_path = {
            let platform_crate = workspace_root.join("crates").join("platform-workflow");
            pathdiff::diff_paths(&platform_crate, &abs_output)
                .unwrap_or_else(|| PathBuf::from("../crates/platform-workflow"))
                .to_string_lossy()
                .into_owned()
        };

        let hooks_api_path = {
            let hooks_crate = workspace_root.join("crates").join("hr-hooks-api");
            pathdiff::diff_paths(&hooks_crate, &abs_output)
                .unwrap_or_else(|| PathBuf::from("../crates/hr-hooks-api"))
                .to_string_lossy()
                .into_owned()
        };

        let extensions_path = {
            let ext_crate = workspace_root.join("crates").join("hr-extensions");
            pathdiff::diff_paths(&ext_crate, &abs_output)
                .unwrap_or_else(|| PathBuf::from("../crates/hr-extensions"))
                .to_string_lossy()
                .into_owned()
        };

        let hr_config_path = {
            let config_crate = workspace_root.join("crates").join("hr-config");
            pathdiff::diff_paths(&config_crate, &abs_output)
                .unwrap_or_else(|| PathBuf::from("../crates/hr-config"))
                .to_string_lossy()
                .into_owned()
        };

        let type_contracts_path = {
            let crate_path = workspace_root.join("crates").join("hr-type-contracts");
            pathdiff::diff_paths(&crate_path, &abs_output)
                .unwrap_or_else(|| PathBuf::from("../crates/hr-type-contracts"))
                .to_string_lossy()
                .into_owned()
        };

        let domain_types_path = {
            let crate_path = workspace_root.join("crates").join("hr-domain-types");
            pathdiff::diff_paths(&crate_path, &abs_output)
                .unwrap_or_else(|| PathBuf::from("../crates/hr-domain-types"))
                .to_string_lossy()
                .into_owned()
        };

        let compliance_path = {
            let crate_path = workspace_root.join("crates").join("hr-compliance");
            pathdiff::diff_paths(&crate_path, &abs_output)
                .unwrap_or_else(|| PathBuf::from("../crates/hr-compliance"))
                .to_string_lossy()
                .into_owned()
        };

        let ctx = ScaffoldContext {
            app_name: "hr-app".to_string(),
            domains,
            codegraph_workflow_path,
            type_contracts_path,
            domain_types_path,
            hooks_api_path,
            extensions_path,
            hr_config_path,
            compliance_path,
            has_webhooks: self.has_webhooks,
            has_reports: self.has_reports,
        };

        let mut files = Vec::new();

        let main_rs = render_template(tera, "scaffold/main.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("main.rs"),
            content: main_rs,
        });

        let app_state = render_template(tera, "scaffold/app_state.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("app_state.rs"),
            content: app_state,
        });

        let cargo_toml = render_template(tera, "scaffold/cargo_toml.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("Cargo.toml"),
            content: cargo_toml,
        });

        let build_rs = render_template(tera, "scaffold/build_rs.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("build.rs"),
            content: build_rs,
        });

        let lib_rs = render_template(tera, "scaffold/lib.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("lib.rs"),
            content: lib_rs,
        });

        let error_rs = render_template(tera, "scaffold/error.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("error.rs"),
            content: error_rs,
        });

        let middleware_rs = render_template(tera, "scaffold/middleware.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("middleware.rs"),
            content: middleware_rs,
        });

        let metrics_middleware_rs =
            render_template(tera, "scaffold/metrics_middleware.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("metrics_middleware.rs"),
            content: metrics_middleware_rs,
        });

        let qs_query_rs = render_template(tera, "scaffold/qs_query.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("qs_query.rs"),
            content: qs_query_rs,
        });

        let integrations_rs = render_template(tera, "scaffold/integrations_handler.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("integrations.rs"),
            content: integrations_rs,
        });

        let api_key_migration = render_template(tera, "db/api_key_migration.tera", &ctx)?;
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
