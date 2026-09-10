use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::error::Result;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;
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
    /// Directory in which the optional `reports.toml` config is looked up.
    /// `None` falls back to the process current directory (legacy behavior).
    reports_dir: Option<PathBuf>,
    dialect: Box<dyn SqlDialect>,
}

impl ReportViewGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            reports_dir: None,
            dialect: dialect_for_target(DatabaseTarget::Postgres),
        }
    }

    /// Sets the directory in which `reports.toml` is discovered. Typically
    /// the directory of the `domains.toml` config used for the run, so
    /// generation output is independent of the invoking shell's cwd.
    pub fn with_reports_dir(mut self, dir: Option<&Path>) -> Self {
        self.reports_dir = dir.map(Path::to_path_buf);
        self
    }

    pub fn with_dialect(mut self, dialect: Box<dyn SqlDialect>) -> Self {
        self.dialect = dialect;
        self
    }
}

#[async_trait]
impl GlobalGenerator for ReportViewGenerator {
    fn name(&self) -> &str {
        "report_views"
    }

    fn supported_targets(&self) -> Option<Vec<DatabaseTarget>> {
        Some(vec![DatabaseTarget::Postgres, DatabaseTarget::Sqlite])
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let reports_path = self
            .reports_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("reports.toml");

        if !reports_path.exists() {
            return Ok(vec![]);
        }

        // Report views reference schema-qualified tables — PG-only
        if !self.dialect.has_schemas() {
            return Ok(vec![]);
        }

        let reports_toml = std::fs::read_to_string(&reports_path).map_err(|e| {
            crate::error::Error::Config(format!("Failed to read reports.toml: {e}"))
        })?;
        let config: ReportsConfig = toml::from_str(&reports_toml)
            .map_err(|e| crate::error::Error::Config(format!("Invalid reports.toml: {e}")))?;

        let mut ctx = tera::Context::new();
        ctx.insert("reports", &config.reports);
        ctx.insert("project", project);

        let mut files = Vec::new();

        let view_sql = tera
            .render(&db_template_for(&*self.dialect, "report_view"), &ctx)
            .map_err(|e| crate::error::Error::Template(e.to_string()))?;
        files.push(GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir).join("0850_report_views.sql"),
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

#[cfg(test)]
mod tests {
    use super::*;

    const REPORTS_TOML: &str = r#"
[[reports]]
name = "headcount"
label = "Headcount"
domain = "common"
description = "Headcount by department"
filters = ["department"]
"#;

    const DOMAIN_CONFIG: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []
"#;

    fn test_tera() -> tera::Tera {
        let mut tera = tera::Tera::default();
        tera.add_raw_template("db/report_view.tera", "views: {{ reports | length }}")
            .unwrap();
        tera.add_raw_template("api/report_handler.tera", "handler")
            .unwrap();
        tera.add_raw_template("api/report_router.tera", "router")
            .unwrap();
        tera
    }

    fn test_domain_config() -> DomainConfig {
        codegraph_config::config::parse_domain_config_str(DOMAIN_CONFIG).unwrap()
    }

    #[tokio::test]
    async fn reports_toml_is_discovered_relative_to_supplied_dir_not_cwd() {
        let config_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(config_dir.path().join("reports.toml"), REPORTS_TOML).unwrap();

        let engine = codegraph_core::mock::MockEngine::new();
        let output_dir = tempfile::TempDir::new().unwrap();
        let gen =
            ReportViewGenerator::new(output_dir.path()).with_reports_dir(Some(config_dir.path()));

        // The process cwd (the crate root during `cargo test`) contains no
        // reports.toml, so any generated files must have come from the
        // supplied directory — pinning the fix away from cwd discovery.
        let files = gen
            .generate(
                &engine,
                &test_domain_config(),
                &[],
                &test_tera(),
                &ProjectConfig::default(),
            )
            .await
            .unwrap();

        assert_eq!(
            files.len(),
            3,
            "reports.toml in the supplied dir must drive generation (view SQL + handler + router)"
        );
    }

    #[tokio::test]
    async fn missing_reports_toml_yields_no_files() {
        let config_dir = tempfile::TempDir::new().unwrap();
        let engine = codegraph_core::mock::MockEngine::new();
        let output_dir = tempfile::TempDir::new().unwrap();
        let gen =
            ReportViewGenerator::new(output_dir.path()).with_reports_dir(Some(config_dir.path()));

        let files = gen
            .generate(
                &engine,
                &test_domain_config(),
                &[],
                &test_tera(),
                &ProjectConfig::default(),
            )
            .await
            .unwrap();

        assert!(
            files.is_empty(),
            "no reports.toml in the config dir must yield no files"
        );
    }
}
