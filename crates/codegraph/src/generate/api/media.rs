use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_type_contracts::RefClassificationKind;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct MediaRouteContext {
    entity_name: String,
    entity_snake: String,
    entity_module: String,
    media_field_name: String,
    media_accept: Vec<String>,
}

pub struct MediaRouteGenerator {
    output_dir: PathBuf,
    default_accept: Vec<String>,
}

impl MediaRouteGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            default_accept: vec!["image/*".into(), "application/pdf".into()],
        }
    }
}

#[async_trait]
impl EntityGenerator for MediaRouteGenerator {
    fn name(&self) -> &str {
        "media_route"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        _config: &DomainConfig,
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = match db.get_schema(schema_title).await? {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let entity_name = schema.rust_type_name.clone();
        let entity_snake = schema.pg_table_name.clone();
        let entity_module = format!("{}_{}", domain, entity_snake);

        if entity_snake.is_empty() {
            return Ok(Vec::new());
        }

        let all_props = db.get_properties(schema_title).await?;
        let mut files = Vec::new();

        for prop in &all_props {
            if prop.effective_kind() != Some(RefClassificationKind::MediaWrapper) {
                continue;
            }

            let media_field_name = prop.pg_column_name.clone();

            let ctx = MediaRouteContext {
                entity_name: entity_name.clone(),
                entity_snake: entity_snake.clone(),
                entity_module: entity_module.clone(),
                media_field_name: media_field_name.clone(),
                media_accept: self.default_accept.clone(),
            };

            let content = render_template(tera, "api/media_route.tera", &ctx)?;
            files.push(GeneratedFile {
                path: self
                    .output_dir
                    .join("src")
                    .join("api")
                    .join(domain)
                    .join(format!("media_{}_{}.rs", entity_snake, media_field_name)),
                content,
            });
        }

        Ok(files)
    }
}
