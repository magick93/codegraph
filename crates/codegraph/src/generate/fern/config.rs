use std::path::Path;

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct FernConfigContext {
    organization: String,
    languages: Vec<String>,
}

pub struct FernConfigGenerator {
    output_dir: std::path::PathBuf,
}

impl FernConfigGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for FernConfigGenerator {
    fn name(&self) -> &str {
        "fern_config"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let languages = project
            .fern_sdk_languages
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let ctx = FernConfigContext {
            organization: project.app_name.clone(),
            languages,
        };

        let fern_json = render_template_with_project(tera, "fern/fern_config.tera", &ctx, project)?;
        let generators_yml =
            render_template_with_project(tera, "fern/generators.tera", &ctx, project)?;

        Ok(vec![
            GeneratedFile {
                path: self.output_dir.join("fern").join("fern.config.json"),
                content: fern_json,
            },
            GeneratedFile {
                path: self.output_dir.join("fern").join("generators.yml"),
                content: generators_yml,
            },
        ])
    }
}
