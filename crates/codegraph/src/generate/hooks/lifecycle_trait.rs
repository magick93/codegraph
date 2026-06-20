use crate::generate::ProjectConfig;
use std::path::PathBuf;

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct LifecycleTraitContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub has_create: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_workflow: bool,
}

pub struct LifecycleTraitGenerator {
    /// Base directory for generated hooks output.
    generated_dir: PathBuf,
}

impl LifecycleTraitGenerator {
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self {
            generated_dir: base_dir,
        }
    }
}

#[async_trait]
impl EntityGenerator for LifecycleTraitGenerator {
    fn name(&self) -> &str {
        "lifecycle_trait"
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
        // Skip child/inline definition schemas — they don't have standalone entity
        // files, so the hook_registry can't reference their lifecycle traits.
        if schema.parent_schema.is_some() {
            return Ok(Vec::new());
        }

        let operations = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name))
            .and_then(|ec| ec.operations.clone())
            .unwrap_or_else(|| config.defaults.operations.clone());

        let has_workflow = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name))
            .and_then(|ec| ec.workflow.as_ref())
            .is_some();

        let ctx = LifecycleTraitContext {
            has_create: operations.contains(&"create".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_workflow,
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
        };

        let content = render_template_with_project(tera, "hooks/lifecycle_trait.tera", &ctx, project)?;

        let output_path = self
            .generated_dir
            .join(&domain)
            .join(format!("{}.rs", module_name));

        Ok(vec![GeneratedFile {
            path: output_path,
            content,
        }])
    }
}
