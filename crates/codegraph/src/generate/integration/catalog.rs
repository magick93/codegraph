use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

pub struct IntegrationCatalogGenerator {
    output_dir: PathBuf,
}

impl IntegrationCatalogGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for IntegrationCatalogGenerator {
    fn name(&self) -> &str {
        "integration_catalog"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        let handler = render_template(tera, "integration/catalog_handler.tera", &ctx)?;
        let router = render_template(tera, "integration/catalog_router.tera", &ctx)?;

        Ok(vec![
            GeneratedFile {
                path: self.output_dir.join("src").join("integration_catalog.rs"),
                content: handler,
            },
            GeneratedFile {
                path: self.output_dir.join("src").join("integration_router.rs"),
                content: router,
            },
        ])
    }
}
