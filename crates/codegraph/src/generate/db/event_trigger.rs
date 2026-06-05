use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
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
}

impl PgmqSetupGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for PgmqSetupGenerator {
    fn name(&self) -> &str {
        "pgmq_setup"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut domains: Vec<String> = config.domains.keys().cloned().collect();
        domains.sort();

        let ctx = PgmqSetupContext { domains };

        let content = render_template_with_project(tera, "db/pgmq_setup.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0003_pgmq_setup.sql"),
            content,
        }])
    }
}
