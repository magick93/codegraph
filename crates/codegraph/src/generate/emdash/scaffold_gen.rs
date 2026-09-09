//! `emdash_plugin_scaffold` — global scaffolding for the EmDash plugin
//! package family.
//!
//! v1 emits a single README at the repo `packages/` root documenting the
//! generated plugin packages — and only when at least one plugin is
//! configured. With no configured plugins it emits nothing at all.

use std::path::PathBuf;

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::emdash::config::EmdashPluginsConfig;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::{GenerationEntry, ProjectConfig};

#[derive(Debug, serde::Serialize)]
struct ScaffoldEntry {
    domain: String,
    plugin_id: String,
    label: String,
    description: String,
}

#[derive(Debug, serde::Serialize)]
struct ScaffoldContext {
    packages: Vec<ScaffoldEntry>,
}

pub struct EmdashPluginScaffoldGenerator {
    output_dir: PathBuf,
    plugins: EmdashPluginsConfig,
}

impl EmdashPluginScaffoldGenerator {
    pub fn new(output_dir: PathBuf, plugins: EmdashPluginsConfig) -> Self {
        Self {
            output_dir,
            plugins,
        }
    }
}

#[async_trait]
impl GlobalGenerator for EmdashPluginScaffoldGenerator {
    fn name(&self) -> &str {
        "emdash_plugin_scaffold"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if self.plugins.plugins.is_empty() {
            return Ok(Vec::new());
        }

        let mut packages: Vec<ScaffoldEntry> = Vec::new();
        for (domain, cfg) in &self.plugins.plugins {
            let kebab = super::kebab_domain(domain);
            packages.push(ScaffoldEntry {
                domain: domain.clone(),
                plugin_id: format!("community-{kebab}"),
                label: cfg.label.clone(),
                description: cfg
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("{} domain plugin", cfg.label)),
            });
        }
        packages.sort_by(|a, b| a.domain.cmp(&b.domain));

        let ctx = ScaffoldContext { packages };
        let content =
            render_template_with_project(tera, "emdash/scaffold_readme.tera", &ctx, project)?;

        Ok(vec![GeneratedFile {
            path: super::emdash_packages_root(&self.output_dir).join("README.md"),
            content,
        }])
    }
}
