use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

#[derive(Debug, Deserialize)]
struct ReportsConfig {
    reports: Vec<ReportDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ReportDef {
    name: String,
    label: String,
    domain: String,
    description: String,
    filters: Vec<String>,
}

pub struct ReportViewGenerator {
    output_dir: PathBuf,
}

impl ReportViewGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for ReportViewGenerator {
    fn name(&self) -> &str {
        "report_views"
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let reports_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("reports.toml");

        if !reports_path.exists() {
            return Ok(vec![]);
        }

        let reports_toml = std::fs::read_to_string(&reports_path)
            .map_err(|e| crate::error::Error::Config(format!("Failed to read reports.toml: {e}")))?;
        let config: ReportsConfig = toml::from_str(&reports_toml)
            .map_err(|e| crate::error::Error::Config(format!("Invalid reports.toml: {e}")))?;

        let mut ctx = tera::Context::new();
        ctx.insert("reports", &config.reports);

        let mut files = Vec::new();

        let view_sql = tera
            .render("db/report_view.tera", &ctx)
            .map_err(|e| crate::error::Error::Template(e.to_string()))?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0850_report_views.sql"),
            content: view_sql,
        });

        let handler_rs = tera
            .render("api/report_handler.tera", &ctx)
            .map_err(|e| crate::error::Error::Template(e.to_string()))?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("api")
                .join("report_handler.rs"),
            content: handler_rs,
        });

        let router_rs = tera
            .render("api/report_router.tera", &ctx)
            .map_err(|e| crate::error::Error::Template(e.to_string()))?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("api")
                .join("report_router.rs"),
            content: router_rs,
        });

        Ok(files)
    }
}
