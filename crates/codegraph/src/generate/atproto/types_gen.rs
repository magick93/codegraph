use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile, GlobalGenerator};
use crate::generate::{GenerationEntry, ProjectConfig};
use codegraph_config::DomainConfig;

use super::types_context::TypesContext;

pub struct AtprotoTypesEmitter {
    output_dir: PathBuf,
}

impl AtprotoTypesEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for AtprotoTypesEmitter {
    fn name(&self) -> &str {
        "atproto_types"
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
        let lexicon = match db.get_lexicon_by_schema(schema_title).await? {
            Some(l) => l,
            None => return Ok(Vec::new()),
        };

        let schema = match db.get_schema_in_domain(schema_title, domain).await? {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let properties = db.get_properties_in_domain(schema_title, domain).await?;

        let context = TypesContext::build(db, &lexicon, &schema, &properties, project).await?;

        let mut content =
            render_template_with_project(tera, "atproto/rust_type.tera", &context, project)?;

        if context.is_record {
            let record_impl = render_template_with_project(
                tera,
                "atproto/rust_record_impl.tera",
                &context,
                project,
            )?;
            content.push('\n');
            content.push_str(&record_impl);
        }

        for enum_def in &context.enum_defs {
            let enum_content =
                render_template_with_project(tera, "atproto/rust_enum.tera", enum_def, project)?;
            content.push('\n');
            content.push_str(&enum_content);
        }

        let entity_name = codegraph_naming::to_snake_case(&schema.rust_type_name);
        let path = self
            .output_dir
            .join("src")
            .join("atproto")
            .join(domain)
            .join(format!("{}.rs", entity_name));

        Ok(vec![GeneratedFile { path, content }])
    }
}

pub struct GeneratedTypesEmitter {
    output_dir: PathBuf,
}

impl GeneratedTypesEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for GeneratedTypesEmitter {
    fn name(&self) -> &str {
        "atproto_generated_types"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if project.atproto_authority.is_empty() {
            return Ok(Vec::new());
        }

        let context = serde_json::json!({
            "value_object_stubs": [],
            "composite_stubs": [],
            "codelist_names": <&[String]>::default(),
        });

        let content =
            render_template_with_project(tera, "atproto/generated_types.tera", &context, project)?;

        let path = self
            .output_dir
            .join("src")
            .join("atproto")
            .join("generated_types.rs");

        Ok(vec![GeneratedFile { path, content }])
    }
}
