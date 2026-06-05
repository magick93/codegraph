use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct CommandContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub operations: Vec<String>,
    pub has_create: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub parent_ref: Option<String>,
}

pub struct CommandGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
}

impl CommandGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
        }
    }

    pub fn with_parent_candidates(mut self, candidates: Vec<ParentCandidate>) -> Self {
        self.parent_candidates = candidates;
        self
    }
}

#[async_trait]
impl EntityGenerator for CommandGenerator {
    fn name(&self) -> &str {
        "command"
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

        // Resolve parent_ref for child entities
        let parent_ref = crate::generate::resolve_parent_fk_column_same_domain(
            schema_title,
            &self.parent_candidates,
            entity_cfg,
            &domain,
            config,
            db,
        )
        .await;

        let ctx = CommandContext {
            has_create: operations.contains(&"create".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            parent_ref,
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            operations,
        };

        let content = render_template_with_project(tera, "ddd/command.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(&domain)
                .join(&module_name)
                .join("command.rs"),
            content,
        }])
    }
}
