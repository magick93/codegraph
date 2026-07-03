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

/// Context for the pgmq setup migration (global, once per project).
#[derive(Debug, Serialize)]
pub struct PgmqSetupContext {
    pub domains: Vec<String>,
}

pub struct PgmqSetupGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl PgmqSetupGenerator {
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
impl GlobalGenerator for PgmqSetupGenerator {
    fn name(&self) -> &str {
        "pgmq_setup"
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
        // PGMQ is PostgreSQL-only — skip for dialects without plpgsql support
        if !self.dialect.has_plpgsql() {
            return Ok(vec![]);
        }

        let mut domains: Vec<String> = config.domains.keys().cloned().collect();
        domains.sort();

        let ctx = PgmqSetupContext { domains };

        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "pgmq_setup"),
            &ctx,
            project,
        )?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0003_pgmq_setup.sql"),
            content,
        }])
    }
}
