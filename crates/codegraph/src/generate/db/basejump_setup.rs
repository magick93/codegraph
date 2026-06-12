use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

const EXTENSIONS_SQL: &str = "\
-- Bootstrap: required PostgreSQL extensions for basejump / pg_tle
CREATE EXTENSION IF NOT EXISTS http WITH SCHEMA extensions;
CREATE EXTENSION IF NOT EXISTS pg_tle;
";

const BASEJUMP_INSTALL_SQL: &str = include_str!("../../../static/basejump_core_2.0.1_install.sql");

pub struct BasejumpSetupGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl BasejumpSetupGenerator {
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
impl GlobalGenerator for BasejumpSetupGenerator {
    fn name(&self) -> &str {
        "basejump_setup"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Basejump is PostgreSQL-only — skip for dialects without extensions support
        if !self.dialect.has_extensions() {
            return Ok(vec![]);
        }

        let empty_ctx: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let rbac_roles = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "rbac_roles"),
            &empty_ctx,
            project,
        )?;

        Ok(vec![
            GeneratedFile {
                path: self
                    .output_dir
                    .join("migrations")
                    .join("0000_extensions.sql"),
                content: EXTENSIONS_SQL.to_string(),
            },
            GeneratedFile {
                path: self
                    .output_dir
                    .join("migrations")
                    .join("0001_basejump_install.sql"),
                content: BASEJUMP_INSTALL_SQL.to_string(),
            },
            GeneratedFile {
                path: self
                    .output_dir
                    .join("migrations")
                    .join("0004_rbac_roles.sql"),
                content: rbac_roles,
            },
        ])
    }
}
