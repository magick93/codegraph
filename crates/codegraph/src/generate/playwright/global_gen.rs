use crate::generate::ProjectConfig;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;

use super::{PlaywrightCrateContext, PlaywrightDomainSummary, PlaywrightEntitySummary};

pub struct PlaywrightGlobalGenerator {
    output_dir: PathBuf,
}

impl PlaywrightGlobalGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for PlaywrightGlobalGenerator {
    fn name(&self) -> &str {
        "playwright-global"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Group entities by domain, preserving stable order via BTreeMap.
        // Use pg_table_name from the schema (same as PlaywrightEntityGenerator) so that
        // the module names declared in lib.rs match the filenames on disk.
        let mut domain_map: BTreeMap<String, Vec<PlaywrightEntitySummary>> = BTreeMap::new();
        for entry in generation_order {
            let schema = db.get_schema_in_domain(&entry.schema_title, &entry.domain).await?;
            let module_name = schema
                .as_ref()
                .map(|s| s.pg_table_name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    // Fallback: strip type suffix then snake_case (should rarely trigger)
                    let stripped =
                        crate::generate::api::router::strip_suffix(&entry.schema_title, &config.defaults.type_suffix);
                    codegraph_naming::to_snake_case(stripped)
                });
            if module_name.is_empty() {
                continue;
            }
            domain_map
                .entry(entry.domain.clone())
                .or_default()
                .push(PlaywrightEntitySummary {
                    module_name,
                    domain: entry.domain.clone(),
                });
        }

        let domains: Vec<PlaywrightDomainSummary> = domain_map
            .into_iter()
            .map(|(name, entities)| PlaywrightDomainSummary { name, entities })
            .collect();

        let ctx = PlaywrightCrateContext { domains };

        let e2e_dir = self.output_dir.join("e2e");

        let lib_content = render_template_with_project(tera, "playwright/crate_lib.tera", &ctx, project)?;
        let cargo_content = render_template_with_project(tera, "playwright/crate_cargo.tera", &ctx, project)?;

        Ok(vec![
            GeneratedFile {
                path: e2e_dir.join("src").join("lib.rs"),
                content: lib_content,
            },
            GeneratedFile {
                path: e2e_dir.join("Cargo.toml"),
                content: cargo_content,
            },
        ])
    }
}
