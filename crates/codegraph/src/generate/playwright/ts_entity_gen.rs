use crate::generate::domain_model::{build_entity_model, EntityField};
use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use super::{TsEntityContext, TsFieldDef};

pub struct TsEntityGenerator {
    output_dir: PathBuf,
}

impl TsEntityGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for TsEntityGenerator {
    fn name(&self) -> &str {
        "playwright_ts_entity"
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
        let model = build_entity_model(db, schema_title, domain, config, &project.atproto_authority).await?;

        if model.table_name.is_empty() {
            return Ok(Vec::new());
        }

        let create_fields: Vec<TsFieldDef> = model
            .fields
            .iter()
            .map(|f| TsFieldDef {
                name: f.name.clone(),
                label: f.label.clone(),
                ts_type: f.ts_type.clone(),
                required: f.required,
                example_value: f.example_value.clone(),
            })
            .collect();

        let ctx = TsEntityContext {
            entity_name: model.name.clone(),
            module_name: model.entity_module.clone(),
            domain: model.domain.clone(),
            path_segment: model.api_path.clone(),
            nsid: model.nsid.clone(),
            has_create: model.operations.create,
            has_read: model.operations.read,
            has_update: model.operations.update,
            has_delete: model.operations.delete,
            has_list: model.operations.list,
            create_fields,
            schema_name: model.table_name.clone(),
        };

        let e2e_dir = self.output_dir.join("e2e-tests");
        let spec_dir = e2e_dir.join("specs").join(domain);
        let fixture_dir = e2e_dir.join("fixtures").join(domain);
        let api_dir = e2e_dir.join("api").join(domain);

        Ok(vec![
            GeneratedFile {
                path: spec_dir.join(format!("{}.spec.ts", model.table_name)),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_spec.tera",
                    &ctx,
                    project,
                )?,
            },
            GeneratedFile {
                path: fixture_dir.join(format!("{}.ts", model.table_name)),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_fixture.tera",
                    &ctx,
                    project,
                )?,
            },
            GeneratedFile {
                path: api_dir.join(format!("{}.ts", model.table_name)),
                content: render_template_with_project(
                    tera,
                    "playwright/ts_api_client.tera",
                    &ctx,
                    project,
                )?,
            },
        ])
    }
}
