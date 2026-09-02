use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::db::dialect::{
    db_template_for, dialect_for_target, DatabaseTarget, SqlDialect,
};
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

/// Migration that grants the basejump / api-key roles and functions the
/// hand-written platform handlers (`src/api/platform/handwritten_routes.rs`)
/// execute. Only emitted when at least one entity-less `custom_routes` domain
/// is configured, so projects without one get no new migration.
pub struct PlatformGrantsGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl PlatformGrantsGenerator {
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
impl GlobalGenerator for PlatformGrantsGenerator {
    fn name(&self) -> &str {
        "platform_grants"
    }

    fn supported_targets(&self) -> Option<Vec<DatabaseTarget>> {
        Some(vec![DatabaseTarget::Postgres])
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if !self.dialect.has_schemas() {
            return Ok(vec![]);
        }

        // Only projects that declare an entity-less custom-routes domain need
        // the platform grants — the grants reference basejump/api-key objects
        // that only those handlers use.
        let has_custom_routes_domain = config.domains.values().any(|d| d.custom_routes);
        if !has_custom_routes_domain {
            return Ok(vec![]);
        }

        let empty_ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "platform_grants"),
            &empty_ctx,
            project,
        )?;
        Ok(vec![GeneratedFile {
            path: crate::generate::db::migrations_root(&self.output_dir)
                .join("0007_platform_grants.sql"),
            content,
        }])
    }
}
