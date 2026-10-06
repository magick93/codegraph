use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::GenerationEntry;
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
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
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::WebhookEndpointApi
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Subscription vocabulary validation (.evt, issues #454/#455):
        // only a non-empty architecture turns the gate on and names the
        // declared events; an empty architecture keeps the EXACT pre-#455
        // empty context (byte-identical rendering).
        let architecture = crate::evt_events::architecture_for(db, project, config).await?;
        let ctx = if architecture.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({
                "evt_events": true,
                "evt_declared_events": architecture
                    .declared_event_names()
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<String>>(),
            })
        };

        let endpoints =
            render_template_with_project(tera, "webhook/api_endpoints.tera", &ctx, project)?;
        let router = render_template_with_project(tera, "webhook/api_router.tera", &ctx, project)?;

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
