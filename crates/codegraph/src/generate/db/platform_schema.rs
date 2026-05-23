use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

/// Context for the platform schema migration.
#[derive(Debug, Serialize)]
pub struct PlatformSchemaContext {
    pub is_tenant_scoped: bool,
}

pub struct PlatformSchemaGenerator {
    output_dir: PathBuf,
}

impl PlatformSchemaGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for PlatformSchemaGenerator {
    fn name(&self) -> &str {
        "platform_schema"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx = PlatformSchemaContext {
            is_tenant_scoped: true,
        };

        let content = render_template(tera, "db/platform_schema.tera", &ctx)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0005_platform_schema.sql"),
            content,
        }])
    }
}
