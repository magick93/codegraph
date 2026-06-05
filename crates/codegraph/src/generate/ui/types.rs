use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

use super::common::collect_ui_fields;

#[derive(Debug, Serialize)]
pub struct UiTypesContext {
    pub entities: Vec<UiEntityType>,
}

#[derive(Debug, Serialize)]
pub struct UiEntityType {
    pub name: String,
    pub module_name: String,
    pub domain: String,
    pub response_fields: Vec<UiTypeField>,
    pub create_fields: Vec<UiTypeField>,
    pub update_fields: Vec<UiTypeField>,
    pub has_create: bool,
    pub has_update: bool,
    pub has_workflow: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiTypeField {
    pub name: String,
    pub ts_type: String,
    pub is_required: bool,
    pub is_array: bool,
    pub description: String,
}

pub struct UiTypeGenerator {
    output_dir: PathBuf,
}

impl UiTypeGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for UiTypeGenerator {
    fn name(&self) -> &str {
        "ui-types"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut entities = Vec::new();

        for entry in generation_order {
            let schema = match db.get_schema(&entry.schema_title).await? {
                Some(s) => s,
                None => continue,
            };

            let entity_name = schema.rust_type_name.clone();
            let module_name = schema.pg_table_name.clone();
            let domain = entry.domain.clone();

            if module_name.is_empty() {
                continue;
            }

            let entity_cfg = config
                .domains
                .get(&domain)
                .and_then(|d| d.get_entity_config(&entity_name));

            let operations = entity_cfg
                .and_then(|ec| ec.operations.clone())
                .unwrap_or_else(|| config.defaults.operations.clone());

            let dto_config = entity_cfg.map(|ec| &ec.dto);
            let immutable_fields: Vec<String> = dto_config
                .map(|d| d.immutable_fields.clone())
                .unwrap_or_default();

            let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
            let has_workflow = workflow
                .map(|wf| wf.generate_action_endpoints)
                .unwrap_or(false);

            let mut all_excluded: Vec<String> = immutable_fields.clone();
            if let Some(wf) = workflow {
                all_excluded.push(wf.status_field.clone());
                if let Some(ref approval_field) = wf.approval_status_field {
                    all_excluded.push(approval_field.clone());
                }
            }

            let ui_fields =
                collect_ui_fields(db, &entry.schema_title, &immutable_fields, Some(&domain))
                    .await?;
            let mut response_fields = Vec::new();
            let mut create_fields = Vec::new();
            let mut update_fields = Vec::new();

            for ui_field in &ui_fields {
                let type_field = UiTypeField {
                    name: ui_field.name.clone(),
                    ts_type: ui_field.ts_type.clone(),
                    is_required: ui_field.is_required,
                    is_array: ui_field.is_array,
                    description: ui_field.description.clone(),
                };

                response_fields.push(type_field.clone());

                if !all_excluded.contains(&ui_field.name) {
                    create_fields.push(type_field.clone());
                }

                if !ui_field.is_immutable && !all_excluded.contains(&ui_field.name) {
                    update_fields.push(type_field);
                }
            }

            entities.push(UiEntityType {
                name: entity_name,
                module_name,
                domain,
                response_fields,
                create_fields,
                update_fields,
                has_create: operations.contains(&"create".to_string()),
                has_update: operations.contains(&"update".to_string()),
                has_workflow,
            });
        }

        entities.sort_by(|a, b| a.name.cmp(&b.name));

        let ctx = UiTypesContext { entities };
        let content = render_template_with_project(tera, "ui/scaffold/types.tera", &ctx, project)?;

        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("ui")
                .join("src")
                .join("lib")
                .join("api")
                .join("types.ts"),
            content,
        }])
    }
}
