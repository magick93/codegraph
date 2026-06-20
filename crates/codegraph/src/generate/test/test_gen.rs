use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct TestContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub table_name: String,
    pub schema_name: String,
    pub has_create: bool,
}

pub struct TestGenerator {
    output_dir: PathBuf,
}

impl TestGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for TestGenerator {
    fn name(&self) -> &str {
        "test"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = db
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();
        let schema_name = domain.clone();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let operations = entity_cfg
            .and_then(|ec| ec.operations.clone())
            .unwrap_or_else(|| config.defaults.operations.clone());

        let has_create = operations.contains(&"create".to_string());

        let ctx = TestContext {
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            table_name: module_name.clone(),
            schema_name,
            has_create,
        };

        let mut files = Vec::new();

        let entity_test = render_template_with_project(tera, "test/entity_test.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("tests")
                .join(&domain)
                .join(format!("{}_test.rs", module_name)),
            content: entity_test,
        });

        let dto_test = render_template_with_project(tera, "test/dto_test.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("tests")
                .join(&domain)
                .join(format!("{}_dto_test.rs", module_name)),
            content: dto_test,
        });

        Ok(files)
    }
}
