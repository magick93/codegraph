use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{DomainGenerator, GeneratedFile};
use crate::generate::ProjectConfig;
use codegraph_config::DomainConfig;
use codegraph_naming;

#[derive(Debug, Serialize)]
pub struct ErrorDef {
    pub code: String,
    pub description: String,
    pub http_status: i32,
    pub variant_name: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorContext {
    pub domain: String,
    pub errors: Vec<ErrorDef>,
}

pub struct ErrorGenerator {
    output_dir: PathBuf,
}

impl ErrorGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl DomainGenerator for ErrorGenerator {
    fn name(&self) -> &str {
        "errors"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        _entity_titles: &[String],
        _config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let all_errors = db.get_error_definitions().await.map_err(|e| {
            crate::error::Error::Config(format!("failed to query error definitions: {e}"))
        })?;

        let domain_errors: Vec<ErrorDef> = all_errors
            .iter()
            .filter(|e| {
                e.domain.as_deref() == Some(domain)
                    || e.domain.as_deref() == Some("common")
            })
            .map(|e| {
                let normalized = e.code.replace('-', "_").replace(' ', "_");
                let variant_name = codegraph_naming::to_pascal_case(&normalized);
                ErrorDef {
                    code: e.code.clone(),
                    description: e.description.clone(),
                    http_status: e.http_status,
                    variant_name,
                }
            })
            .collect();

        if domain_errors.is_empty() {
            return Ok(Vec::new());
        }

        let ctx = ErrorContext {
            domain: domain.to_string(),
            errors: domain_errors,
        };

        let content = render_template_with_project(tera, "ddd/errors.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join("errors.rs"),
            content,
        }])
    }
}
