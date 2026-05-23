use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

pub struct UiOrgchartGenerator {
    output_dir: PathBuf,
}

impl UiOrgchartGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for UiOrgchartGenerator {
    fn name(&self) -> &str {
        "ui-orgchart"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        // Only generate if any entity in the build order has has_orgchart = true
        let has_orgchart_entity = generation_order.iter().any(|entry| {
            config
                .domains
                .get(&entry.domain)
                .and_then(|d| d.get_entity_config(&entry.schema_title))
                .map(|ec| ec.has_orgchart)
                .unwrap_or(false)
        });

        if !has_orgchart_entity {
            return Ok(Vec::new());
        }

        let src = self.output_dir.join("ui").join("src").join("routes").join("(app)").join("org-chart");

        let mut files = Vec::new();

        // +page.server.ts
        let load_content = render_template(tera, "ui/orgchart_load.tera", &serde_json::json!({}))?;
        files.push(GeneratedFile {
            path: src.join("+page.server.ts"),
            content: load_content,
        });

        // +page.svelte
        let page_content = render_template(tera, "ui/orgchart_page.tera", &serde_json::json!({}))?;
        files.push(GeneratedFile {
            path: src.join("+page.svelte"),
            content: page_content,
        });

        Ok(files)
    }
}
