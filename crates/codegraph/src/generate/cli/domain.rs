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
pub struct CliDomainContext {
    pub domain: String,
    pub domain_label: String,
    pub entities: Vec<CliDomainEntity>,
}

#[derive(Debug, Serialize)]
pub struct CliDomainEntity {
    pub entity_name: String,
    pub module_name: String,
    pub path_segment: String,
}

pub struct CliDomainGenerator {
    output_dir: PathBuf,
}

impl CliDomainGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for CliDomainGenerator {
    fn name(&self) -> &str {
        "cli_domain"
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
        let mut seen_modules = std::collections::HashSet::new();

        for title in entity_titles {
            if let Ok(Some(schema)) = db.get_schema_in_domain(title, domain).await {
                if !schema.pg_table_name.is_empty()
                    && seen_modules.insert(schema.pg_table_name.clone())
                {
                    entities.push(CliDomainEntity {
                        entity_name: schema.rust_type_name.clone(),
                        module_name: schema.pg_table_name.clone(),
                        path_segment: schema.api_path_segment.clone(),
                    });
                }
            }
        }

        let ctx = CliDomainContext {
            domain: domain.to_string(),
            domain_label,
            entities,
        };

        let content = render_template_with_project(tera, "cli/domain_command.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("cli")
                .join("src")
                .join("commands")
                .join(domain)
                .join("mod.rs"),
            content,
        }])
    }
}
