use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::generate::render_template_with_project;
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
    dialect: Box<dyn SqlDialect>,
}

impl PlatformSchemaGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            dialect: dialect_for_target(DatabaseTarget::Postgres),
        }
    }

    pub fn with_dialect(mut self, dialect: Box<dyn SqlDialect>) -> Self {
        self.dialect = dialect;
        self
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
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Platform schema is PostgreSQL-only (uses schemas) — skip for dialects without schema support
        if !self.dialect.has_schemas() {
            return Ok(vec![]);
        }

        let ctx = PlatformSchemaContext {
            is_tenant_scoped: true,
        };

        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "platform_schema"),
            &ctx,
            project,
        )?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0005_platform_schema.sql"),
            content,
        }])
    }
}
