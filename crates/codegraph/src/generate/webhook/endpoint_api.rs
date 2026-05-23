use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

pub struct WebhookEndpointApiGenerator {
    output_dir: PathBuf,
}

impl WebhookEndpointApiGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for WebhookEndpointApiGenerator {
    fn name(&self) -> &str {
        "webhook_endpoint_api"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        let endpoints = render_template(tera, "webhook/api_endpoints.tera", &ctx)?;
        let router = render_template(tera, "webhook/api_router.tera", &ctx)?;

        Ok(vec![
            GeneratedFile {
                path: self.output_dir.join("src").join("webhook_api.rs"),
                content: endpoints,
            },
            GeneratedFile {
                path: self.output_dir.join("src").join("webhook_router.rs"),
                content: router,
            },
        ])
    }
}
