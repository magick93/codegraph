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
pub struct CliScaffoldContext {
    pub app_name: String,
    pub domains: Vec<CliScaffoldDomain>,
}

#[derive(Debug, Serialize)]
pub struct CliScaffoldDomain {
    pub name: String,
    pub label: String,
    pub entities: Vec<CliScaffoldEntity>,
}

#[derive(Debug, Serialize)]
pub struct CliScaffoldEntity {
    pub name: String,
    pub module_name: String,
}

pub struct CliScaffoldGenerator {
    output_dir: PathBuf,
}

impl CliScaffoldGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for CliScaffoldGenerator {
    fn name(&self) -> &str {
        "cli_scaffold"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let mut domain_entity_map: std::collections::HashMap<String, Vec<CliScaffoldEntity>> =
            std::collections::HashMap::new();
        let mut seen = std::collections::HashSet::new();

        for entry in generation_order {
            let stripped = config.defaults.strip_suffix(&entry.schema_title);
            let module_name = codegraph_naming::to_snake_case(&stripped);
            if !seen.insert((entry.domain.clone(), module_name.clone())) {
                continue;
            }
            domain_entity_map
                .entry(entry.domain.clone())
                .or_default()
                .push(CliScaffoldEntity {
                    module_name,
                    name: stripped,
                });
        }

        let mut domains: Vec<CliScaffoldDomain> = config
            .domains
            .iter()
            .filter_map(|(name, entry)| {
                let entities = domain_entity_map.remove(name.as_str())?;
                Some(CliScaffoldDomain {
                    name: name.clone(),
                    label: entry.label.clone(),
                    entities,
                })
            })
            .collect();
        domains.sort_by(|a, b| a.name.cmp(&b.name));

        let ctx = CliScaffoldContext {
            app_name: "hr".to_string(),
            domains,
        };

        let mut files = Vec::new();

        let main_rs = render_template(tera, "cli/main.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("cli").join("src").join("main.rs"),
            content: main_rs,
        });

        let cargo_toml = render_template(tera, "cli/cargo_toml.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("cli").join("Cargo.toml"),
            content: cargo_toml,
        });

        let build_rs = render_template(tera, "cli/build_rs.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("cli").join("build.rs"),
            content: build_rs,
        });

        let config_rs = render_template(tera, "cli/config.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("cli").join("src").join("config.rs"),
            content: config_rs,
        });

        let output_rs = render_template(tera, "cli/output.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("cli").join("src").join("output.rs"),
            content: output_rs,
        });

        let client_rs = render_template(tera, "cli/client.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("cli").join("src").join("client.rs"),
            content: client_rs,
        });

        let util_rs = render_template(tera, "cli/util.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("cli").join("src").join("util.rs"),
            content: util_rs,
        });

        let commands_mod = render_template(tera, "cli/commands_mod.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("cli")
                .join("src")
                .join("commands")
                .join("mod.rs"),
            content: commands_mod,
        });

        Ok(files)
    }
}
