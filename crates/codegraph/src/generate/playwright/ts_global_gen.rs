use crate::generate::ProjectConfig;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use super::{e2e_tests_root, TsDomainSummary, TsEntitySummary, TsGlobalContext};
use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;

pub struct TsGlobalGenerator {
    output_dir: PathBuf,
}

impl TsGlobalGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for TsGlobalGenerator {
    fn name(&self) -> &str {
        "playwright_ts_global"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut domain_map: BTreeMap<String, Vec<TsEntitySummary>> = BTreeMap::new();
        for entry in generation_order {
            let schema = db
                .get_schema_in_domain(&entry.schema_title, &entry.domain)
                .await?;
            let module_name = schema
                .as_ref()
                .map(|s| s.pg_table_name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    let stripped = crate::generate::api::router::strip_suffix(
                        &entry.schema_title,
                        &config.defaults.type_suffix,
                    );
                    codegraph_naming::to_snake_case(stripped)
                });
            if module_name.is_empty() {
                continue;
            }

            let entity_name = schema
                .as_ref()
                .map(|s| s.rust_type_name.clone())
                .unwrap_or_else(|| entry.schema_title.clone());
            let path_segment = schema
                .as_ref()
                .map(|s| s.api_path_segment.clone())
                .unwrap_or_else(|| module_name.clone());

            domain_map
                .entry(entry.domain.clone())
                .or_default()
                .push(TsEntitySummary {
                    module_name,
                    domain: entry.domain.clone(),
                    path_segment,
                    entity_name,
                });
        }

        let domains: Vec<TsDomainSummary> = domain_map
            .into_iter()
            .map(|(name, entities)| TsDomainSummary { name, entities })
            .collect();

        let ctx = TsGlobalContext {
            domains,
            project_name: "community-os".to_string(),
            api_base_url: "http://localhost:3000".to_string(),
        };

        // The harness lives at the repo root (hand-extended under
        // specs/manual/), not inside the generated tree.
        let e2e_dir = e2e_tests_root(&self.output_dir);

        Ok(vec![
            GeneratedFile {
                path: e2e_dir.join("playwright.config.ts"),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_playwright_config.tera",
                    &ctx,
                    project,
                )?,
            },
            GeneratedFile {
                path: e2e_dir.join("auth.setup.ts"),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_auth.tera",
                    &ctx,
                    project,
                )?,
            },
            GeneratedFile {
                path: e2e_dir.join("docker-compose.test.yml"),
                content: render_template_with_project(
                    tera,
                    "playwright/docker_compose_test.tera",
                    &ctx,
                    project,
                )?,
            },
            GeneratedFile {
                path: e2e_dir.join("tsconfig.json"),
                content: r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "lib": ["ES2022", "DOM"],
    "types": ["node"],
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "outDir": "./dist",
    "rootDir": "."
  },
  "include": ["**/*.ts"],
  "exclude": ["node_modules", "dist"]
}"#
                .to_string(),
            },
            GeneratedFile {
                path: e2e_dir.join("package.json"),
                content: r#"{
  "name": "community-os-e2e",
  "private": true,
  "scripts": {
    "test": "npx playwright test --reporter=list",
    "test:headed": "npx playwright test --headed"
  },
  "devDependencies": {
    "@axe-core/playwright": "^4.13.0",
    "@playwright/test": "^1.48.0",
    "@types/node": "^22.0.0",
    "typescript": "^5.6.0"
  }
}"#
                .to_string(),
            },
        ])
    }
}
