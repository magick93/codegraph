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

/// Generator for hand-written-extension service tables (issues #43, #44).
///
/// Emits a single platform migration containing the storage half of the
/// trust-transitivity (community_graph.trust_path + refresh_trust_paths()
/// WITH RECURSIVE recompute) and starter-pack join analytics
/// (onboarding.starter_pack_join). These tables back hand-written services in
/// `cosmos-extensions` (trust.rs, onboarding.rs) and are not domain entities —
/// they get no REST CRUD codegen.
///
/// Gated on `has_atproto` (the services only exist on AT Protocol builds).
pub struct ServiceTablesGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl ServiceTablesGenerator {
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
impl GlobalGenerator for ServiceTablesGenerator {
    fn name(&self) -> &str {
        "service_tables"
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
        if !self.dialect.has_schemas() || !self.dialect.has_rls() {
            return Ok(vec![]);
        }

        let empty_ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "service_tables"),
            &empty_ctx,
            project,
        )?;
        Ok(vec![GeneratedFile {
            // Sorts after 0100_atproto_label, before entity migrations (500+),
            // and survives the stale-migration sweep by being re-emitted on
            // every fullstack run.
            path: crate::generate::db::migrations_root(&self.output_dir)
                .join("9800_service_tables.sql"),
            content,
        }])
    }
}
