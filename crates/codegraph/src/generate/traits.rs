use std::path::PathBuf;

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use codegraph_config::DomainConfig;

use super::{GenerationEntry, ProjectConfig};

/// A target file to render from a template.
#[derive(Debug)]
pub struct RenderTarget {
    /// Template name (e.g. "db/table.tera")
    pub template: String,
    /// Output file path
    pub output: PathBuf,
    /// Whether to actually render this target (enables conditional generation)
    pub condition: bool,
}

/// Result of a single generator run — files to write.
#[derive(Debug)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub content: String,
}

/// Per-entity generator: runs once for each entity in generation order.
#[async_trait]
pub trait EntityGenerator: Send + Sync {
    fn name(&self) -> &str;

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>>;
}

/// Per-domain generator: runs once for each domain.
#[async_trait]
pub trait DomainGenerator: Send + Sync {
    fn name(&self) -> &str;

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>>;
}

/// Global generator: runs once for the entire project.
///
/// Receives `generation_order` — the graph-derived list of entities that were
/// actually ingested and will have per-entity files generated. Implementations
/// should use this instead of `config.domains[*].entities` to stay consistent
/// with the entity generators.
#[async_trait]
pub trait GlobalGenerator: Send + Sync {
    fn name(&self) -> &str;

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>>;
}
