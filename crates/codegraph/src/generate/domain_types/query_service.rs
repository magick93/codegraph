use crate::generate::ProjectConfig;
use std::path::PathBuf;

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct QueryServiceContext {
    entity_name: String,
    module_name: String,
    domain: String,
}

/// Generates query service trait files into the domain-types crate per entity.
pub struct QueryServiceGenerator {
    /// Base `src/` directory for domain-types output.
    ///
    /// In production this is `{workspace_root}/crates/domain-types/src`.
    /// In tests this should be a temp directory to avoid corrupting the real workspace source.
    src_dir: PathBuf,
}

impl QueryServiceGenerator {
    /// Creates a generator that writes output under `base_dir` (crate root), appending `src/` internally.
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self { src_dir: base_dir.join("src") }
    }
}

#[async_trait]
impl EntityGenerator for QueryServiceGenerator {
    fn name(&self) -> &str {
        "domain_types_query_service"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        _config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = db
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let ctx = QueryServiceContext {
            entity_name,
            module_name: module_name.clone(),
            domain: domain.to_string(),
        };

        let content = render_template_with_project(tera, "domain_types/query_service.tera", &ctx, project)?;

        let output_path = self
            .src_dir
            .join(domain)
            .join(&module_name)
            .join("query_service.rs");

        Ok(vec![GeneratedFile {
            path: output_path,
            content,
        }])
    }
}
