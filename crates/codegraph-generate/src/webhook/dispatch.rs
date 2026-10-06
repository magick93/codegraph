use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::GenerationEntry;
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::DomainConfig;

pub struct WebhookDispatchGenerator {
    output_dir: PathBuf,
}

impl WebhookDispatchGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for WebhookDispatchGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::WebhookDispatch
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Typed-envelope drain branch (.evt, issues #454/#455): only when
        // the architecture is non-empty does the context carry `evt_events`
        // — an empty architecture keeps the EXACT pre-#455 empty context,
        // so the rendering is byte-identical.
        let architecture = crate::evt_events::architecture_for(db, project, config).await?;
        let ctx = if architecture.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "evt_events": true })
        };
        let content = render_template_with_project(tera, "webhook/dispatch.tera", &ctx, project)?;

        Ok(vec![GeneratedFile {
            path: self.output_dir.join("src").join("webhook_dispatch.rs"),
            content,
        }])
    }
}
