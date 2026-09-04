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

/// Generator for the AT Protocol label storage table.
///
/// Emits a single platform migration that creates the `atproto` schema and the
/// `atproto.label` table — the appview's materialized index of applied labels
/// written by the `SubscribeLabels` firehose consumer (issue #33). Labels are
/// free-standing data objects, not domain entities, so they get no REST CRUD
/// codegen; this table is the storage half of the label primitive.
///
/// Feature-gated on `has_labels` (like the `has_atproto` gate for the lexicon
/// generators), so profiles that don't opt into the labels substrate get no
/// migration and no consumers.
pub struct LabelSetupGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl LabelSetupGenerator {
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
impl GlobalGenerator for LabelSetupGenerator {
    fn name(&self) -> &str {
        "label_setup"
    }

    fn supported_targets(&self) -> Option<Vec<DatabaseTarget>> {
        Some(vec![DatabaseTarget::Postgres])
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Labels are a PostgreSQL-only platform table (RLS, schemas, JSONB).
        if !self.dialect.has_schemas() || !self.dialect.has_rls() {
            return Ok(vec![]);
        }

        let empty_ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "label_setup"),
            &empty_ctx,
            project,
        )?;
        Ok(vec![GeneratedFile {
            // Numbered in the generated codelist/entity gap (100..499) so it
            // sorts after the hand-written 0000–0009 platform band and before
            // entity migrations (500+), and survives the stale-migration
            // sweep (seq >= 10) by being re-emitted on every fullstack run.
            path: crate::generate::db::migrations_root(&self.output_dir)
                .join("0100_atproto_label.sql"),
            content,
        }])
    }
}
