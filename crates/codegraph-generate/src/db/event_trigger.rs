use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::GenerationEntry;
use crate::db::dialect::{DatabaseTarget, SqlDialect, db_template_for, dialect_for_target};
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::DomainConfig;

/// Context for the pgmq setup migration (global, once per project).
#[derive(Debug, Serialize)]
pub struct PgmqSetupContext {
    pub domains: Vec<String>,
    /// DSL channel queues from the semantic event architecture (`.evt`,
    /// issue #455): sorted, deduped, and with domain-owned colliding
    /// queues excluded (`pgmq.create` is not idempotent — the domain loop
    /// above already creates those). Empty when the graph carries no
    /// `.evt` models, keeping the rendering byte-identical.
    pub channels: Vec<String>,
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
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::PgmqSetup
    }

    fn supported_targets(&self) -> Option<Vec<DatabaseTarget>> {
        Some(vec![DatabaseTarget::Postgres])
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // PGMQ is PostgreSQL-only — skip for dialects without plpgsql support
        if !self.dialect.has_plpgsql() {
            return Ok(vec![]);
        }

        // One pgmq queue per domain with generated domain-event triggers.
        // Entity-less custom-routes domains (entities = [] + custom_routes)
        // have no event triggers, so they get no queue.
        let mut domains: Vec<String> = config
            .domains
            .iter()
            .filter(|(_, entry)| !(entry.custom_routes && entry.entities.is_empty()))
            .map(|(name, _)| name.clone())
            .collect();
        domains.sort();

        // Semantic event channels (.evt, issue #455): DSL channel queues
        // join the per-domain queues in this migration. With no `.evt`
        // models the architecture is empty (short-circuiting before its
        // trigger enumeration), so `channels` is empty and the rendering
        // stays byte-identical.
        let architecture =
            crate::events::EventArchitecture::from_graph(db, &*self.dialect, config).await?;
        let channels = architecture.channel_queues();

        let ctx = PgmqSetupContext { domains, channels };

        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "pgmq_setup"),
            &ctx,
            project,
        )?;
        Ok(vec![GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir).join("0003_pgmq_setup.sql"),
            content,
        }])
    }
}
