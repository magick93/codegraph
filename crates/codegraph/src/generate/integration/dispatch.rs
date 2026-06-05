use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

pub struct IntegrationDispatchGenerator {
    output_dir: PathBuf,
}

impl IntegrationDispatchGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for IntegrationDispatchGenerator {
    fn name(&self) -> &str {
        "integration_dispatch"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let content = render_template_with_project(tera, "integration/dispatcher.tera", &ctx, project)?;

        Ok(vec![GeneratedFile {
            path: self.output_dir.join("src").join("integration_dispatch.rs"),
            content,
        }])
    }
}
