use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_ext_points::{ConfigFieldType, ExtensionPointsConfig};
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct ConfigContext {
    extension_points: Vec<PointConfigCtx>,
}

#[derive(Debug, Serialize)]
struct PointConfigCtx {
    struct_name: String,
    name: String,
    config: Vec<FieldCtx>,
}

#[derive(Debug, Serialize)]
struct FieldCtx {
    name: String,
    rust_type: String,
    required: bool,
}

pub struct IntegrationConfigGenerator {
    output_dir: PathBuf,
    ext_config: ExtensionPointsConfig,
}

impl IntegrationConfigGenerator {
    pub fn new(output_dir: &Path, ext_config: ExtensionPointsConfig) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            ext_config,
        }
    }
}

fn rust_type_for(field_type: ConfigFieldType) -> &'static str {
    match field_type {
        ConfigFieldType::Text | ConfigFieldType::Select | ConfigFieldType::Secret => "String",
        ConfigFieldType::Toggle => "bool",
    }
}

#[async_trait]
impl GlobalGenerator for IntegrationConfigGenerator {
    fn name(&self) -> &str {
        "integration_config"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut points: Vec<PointConfigCtx> = self
            .ext_config
            .points
            .iter()
            .map(|(id, def)| {
                let fields: Vec<FieldCtx> = def
                    .config
                    .iter()
                    .map(|(name, field)| FieldCtx {
                        name: name.clone(),
                        rust_type: rust_type_for(field.field_type).to_string(),
                        required: field.required,
                    })
                    .collect();

                PointConfigCtx {
                    struct_name: codegraph_naming::to_pascal_case(&id.replace('-', "_")),
                    name: def.name.clone(),
                    config: fields,
                }
            })
            .collect();
        points.sort_by(|a, b| a.struct_name.cmp(&b.struct_name));

        let ctx = ConfigContext {
            extension_points: points,
        };
        let content = render_template_with_project(tera, "integration/config_struct.tera", &ctx, project)?;

        Ok(vec![GeneratedFile {
            path: self.output_dir.join("src").join("integration_config.rs"),
            content,
        }])
    }
}
