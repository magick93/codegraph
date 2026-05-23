use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_ext_points::ExtensionPointsConfig;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
struct TablesContext {
    extension_points: Vec<ExtensionPointCtx>,
}

#[derive(Debug, Serialize)]
struct ExtensionPointCtx {
    id: String,
    name: String,
    description: String,
    cardinality: String,
    entities: Vec<String>,
    directions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RlsContext {
    tenant_tables: Vec<String>,
}

pub struct IntegrationTablesGenerator {
    output_dir: PathBuf,
    ext_config: ExtensionPointsConfig,
}

impl IntegrationTablesGenerator {
    pub fn new(output_dir: &Path, ext_config: ExtensionPointsConfig) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            ext_config,
        }
    }

    fn build_context(&self) -> TablesContext {
        let mut points: Vec<ExtensionPointCtx> = self
            .ext_config
            .points
            .iter()
            .map(|(id, def)| ExtensionPointCtx {
                id: id.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                cardinality: format!("{:?}", def.cardinality).to_lowercase(),
                entities: def.entities.clone(),
                directions: def
                    .directions
                    .iter()
                    .map(|d| format!("{:?}", d).to_lowercase())
                    .collect(),
            })
            .collect();
        points.sort_by(|a, b| a.id.cmp(&b.id));
        TablesContext {
            extension_points: points,
        }
    }
}

#[async_trait]
impl GlobalGenerator for IntegrationTablesGenerator {
    fn name(&self) -> &str {
        "integration_tables"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx = self.build_context();
        let mut files = Vec::new();

        let ddl = render_template(tera, "integration/tables.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0007_integration_tables.sql"),
            content: ddl,
        });

        let rls_ctx = RlsContext {
            tenant_tables: vec![
                "installations".into(),
                "installation_settings".into(),
                "audit_log".into(),
            ],
        };
        let rls = render_template(tera, "integration/rls.tera", &rls_ctx)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0008_integration_rls.sql"),
            content: rls,
        });

        let seed = render_template(tera, "integration/seed.tera", &ctx)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0009_integration_seed.sql"),
            content: seed,
        });

        Ok(files)
    }
}
