use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct EventContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub operations: Vec<String>,
    /// Whether this entity has a workflow (generates StateTransitioned variant).
    pub has_workflow: bool,
    /// Whether this entity has approval status (generates ApprovalChanged variant).
    pub has_approval_status: bool,
}

pub struct EventGenerator {
    output_dir: PathBuf,
}

impl EventGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for EventGenerator {
    fn name(&self) -> &str {
        "event"
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
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let operations = entity_cfg
            .and_then(|ec| ec.operations.clone())
            .unwrap_or_else(|| config.defaults.operations.clone());

        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let has_workflow = workflow
            .map(|wf| wf.generate_action_endpoints)
            .unwrap_or(false);
        let has_approval_status = workflow
            .and_then(|wf| wf.approval_status_field.as_ref())
            .is_some();

        let ctx = EventContext {
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            operations,
            has_workflow,
            has_approval_status,
        };

        let content = render_template_with_project(tera, "ddd/event.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(&domain)
                .join(&module_name)
                .join("event.rs"),
            content,
        }])
    }
}
