use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct QueryServiceContext {
    entity_name: String,
    module_name: String,
    domain: String,
}

/// Generates query service trait files into `hr-domain-types` per entity.
pub struct QueryServiceGenerator {
    /// Base `src/` directory for `hr-domain-types` output.
    ///
    /// In production this is `{workspace_root}/crates/hr-domain-types/src`.
    /// In tests this should be a temp directory to avoid corrupting the real workspace source.
    src_dir: PathBuf,
}

impl QueryServiceGenerator {
    /// Production constructor: derives the output path from the compiled-in workspace root.
    pub fn new(_output_dir: &Path) -> Self {
        Self {
            src_dir: super::domain_types_src_dir(),
        }
    }

    /// Test / override constructor: writes output under `base_dir` instead of the
    /// compiled-in workspace root.
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self { src_dir: base_dir }
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

        let content = render_template(tera, "domain_types/query_service.tera", &ctx)?;

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
