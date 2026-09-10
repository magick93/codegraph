use crate::scaffold::gen::{build_scaffold_domains, ScaffoldDomain};
use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;
use codegraph_config::DomainConfig;

/// Template context for the seed CLI binary. The AppState construction loop
/// mirrors `scaffold/server.tera`, so it needs the same domain grouping.
#[derive(Debug, Serialize)]
struct SeedCliContext {
    app_name: String,
    domains: Vec<ScaffoldDomain>,
}

/// Emits the demo-data provisioning module (`src/seed/`) and its CLI binary
/// (`src/bin/seed.rs`). Implements `hr_seed::SeedSink` over the generated
/// command handlers so `SeedService::seed_into_org` can persist through the
/// same write path (hooks, validation, RLS) as the HTTP API.
pub struct SeedProvisionGenerator {
    output_dir: PathBuf,
}

impl SeedProvisionGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for SeedProvisionGenerator {
    fn name(&self) -> &str {
        "seed_provision"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let domains = build_scaffold_domains(db, config, generation_order).await;
        let ctx = SeedCliContext {
            app_name: project.app_name.clone(),
            domains,
        };

        let seed_mod = render_template_with_project(tera, "seed/mod_rs.tera", &ctx, project)?;
        let sink = render_template_with_project(tera, "seed/sink_rs.tera", &ctx, project)?;
        let cli = render_template_with_project(tera, "seed/cli_rs.tera", &ctx, project)?;

        Ok(vec![
            GeneratedFile {
                path: self.output_dir.join("src").join("seed").join("mod.rs"),
                content: seed_mod,
            },
            GeneratedFile {
                path: self.output_dir.join("src").join("seed").join("sink.rs"),
                content: sink,
            },
            GeneratedFile {
                path: self.output_dir.join("src").join("bin").join("seed.rs"),
                content: cli,
            },
        ])
    }
}
