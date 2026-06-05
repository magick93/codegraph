use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{DomainGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct UiDomainLayoutContext {
    pub domain: String,
    pub domain_label: String,
    pub entities: Vec<UiDomainEntity>,
}

#[derive(Debug, Serialize)]
pub struct UiDomainEntity {
    pub name: String,
    pub module_name: String,
    pub path_segment: String,
    pub label: String,
}

pub struct UiDomainLayoutGenerator {
    output_dir: PathBuf,
}

impl UiDomainLayoutGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for UiDomainLayoutGenerator {
    fn name(&self) -> &str {
        "ui-domain-layout"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let domain_label = config
            .domains
            .get(domain)
            .map(|d| d.label.clone())
            .unwrap_or_else(|| domain.to_string());

        let mut entities = Vec::new();
        for title in entity_titles {
            if let Ok(Some(schema)) = db.get_schema(title).await {
                if !schema.pg_table_name.is_empty() {
                    let name = schema.rust_type_name.clone();
                    let label = codegraph_naming::to_display_name(&name);
                    entities.push(UiDomainEntity {
                        name: name.clone(),
                        module_name: schema.pg_table_name.clone(),
                        path_segment: schema.api_path_segment.clone(),
                        label,
                    });
                }
            }
        }

        let ctx = UiDomainLayoutContext {
            domain: domain.to_string(),
            domain_label,
            entities,
        };

        let content = render_template_with_project(tera, "ui/domain_layout.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("ui")
                .join("src")
                .join("routes")
                .join("(app)")
                .join(domain)
                .join("+layout.svelte"),
            content,
        }])
    }
}
