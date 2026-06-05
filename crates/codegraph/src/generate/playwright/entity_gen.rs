use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use crate::generate::ui::common::collect_ui_fields;

use super::PlaywrightEntityContext;

pub struct PlaywrightEntityGenerator {
    output_dir: PathBuf,
}

impl PlaywrightEntityGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for PlaywrightEntityGenerator {
    fn name(&self) -> &str {
        "playwright-entity"
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
        let schema = db
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let module_name = schema.pg_table_name.clone();
        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_name = schema.rust_type_name.clone();
        let path_segment = schema.api_path_segment.clone();

        let entity_cfg = config
            .domains
            .get(domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let operations = entity_cfg
            .and_then(|ec| ec.operations.clone())
            .unwrap_or_else(|| config.defaults.operations.clone());

        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let has_workflow = workflow
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);
        let workflow_states = workflow.map(|wf| wf.states.clone()).unwrap_or_default();
        let initial_state = workflow
            .map(|wf| wf.initial_state.clone())
            .unwrap_or_default();

        let immutable_fields: Vec<String> = entity_cfg
            .map(|ec| ec.dto.immutable_fields.clone())
            .unwrap_or_default();
        let mut excluded: Vec<String> = immutable_fields.clone();
        if let Some(wf) = workflow {
            excluded.push(wf.status_field.clone());
            if let Some(ref af) = wf.approval_status_field {
                excluded.push(af.clone());
            }
        }

        let all_fields =
            collect_ui_fields(db, schema_title, &immutable_fields, Some(domain)).await?;
        let create_fields = all_fields
            .iter()
            .filter(|f| !excluded.contains(&f.name))
            .cloned()
            .collect();

        let ctx = PlaywrightEntityContext {
            entity_name: entity_name.clone(),
            module_name: module_name.clone(),
            domain: domain.to_string(),
            path_segment: path_segment.clone(),
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_workflow,
            workflow_states,
            initial_state,
            create_fields,
        };

        let pages_dir = self
            .output_dir
            .join("e2e")
            .join("src")
            .join("pages")
            .join(domain);
        let factories_dir = self
            .output_dir
            .join("e2e")
            .join("src")
            .join("factories")
            .join(domain);

        let page_content = render_template_with_project(tera, "playwright/entity_page.tera", &ctx, project)?;
        let factory_content = render_template_with_project(tera, "playwright/test_data_factory.tera", &ctx, project)?;

        Ok(vec![
            GeneratedFile {
                path: pages_dir.join(format!("{module_name}.rs")),
                content: page_content,
            },
            GeneratedFile {
                path: factories_dir.join(format!("{module_name}_factory.rs")),
                content: factory_content,
            },
        ])
    }
}
