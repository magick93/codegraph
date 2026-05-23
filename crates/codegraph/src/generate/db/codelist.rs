use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct CodelistContext {
    pub schema_name: String,
    pub table_name: String,
    pub display_name: String,
    pub values: Vec<CodelistValue>,
    pub render_as: String,
}

#[derive(Debug, Serialize)]
pub struct CodelistValue {
    pub code: String,
    pub display_name: String,
    pub sort_order: usize,
}

pub struct CodelistGenerator {
    output_dir: PathBuf,
}

impl CodelistGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for CodelistGenerator {
    fn name(&self) -> &str {
        "codelist"
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

        // Only generate for codelist schemas
        if schema.classification != "codelist" && schema.classification != "codelist_check" {
            return Ok(Vec::new());
        }

        let table_name = &schema.pg_table_name;
        let schema_name = if domain.is_empty() { "common" } else { domain };
        let display_name = &schema.rust_type_name;

        // Query enum values
        let enum_values = db.get_enum_values(schema_title).await.unwrap_or_default();
        let values: Vec<CodelistValue> = enum_values
            .iter()
            .enumerate()
            .map(|(i, v)| CodelistValue {
                code: v.value.clone(),
                display_name: v.display_name.as_deref().unwrap_or("").to_string(),
                sort_order: i,
            })
            .collect();

        let ctx = CodelistContext {
            schema_name: schema_name.to_string(),
            table_name: table_name.to_string(),
            display_name: display_name.to_string(),
            values,
            render_as: schema.classification.clone(),
        };

        let content = render_template(tera, "db/codelist.tera", &ctx)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join(format!("{}_{}_codelist.sql", schema_name, table_name)),
            content,
        }])
    }
}
