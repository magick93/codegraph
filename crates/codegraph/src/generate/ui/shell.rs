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
struct ShellContext {
    entity_name: String,
    domain: String,
    path_segment: String,
    has_create: bool,
    has_read: bool,
    has_update: bool,
}

pub struct UiShellGenerator {
    output_dir: PathBuf,
}

impl UiShellGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for UiShellGenerator {
    fn name(&self) -> &str {
        "ui-shell"
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
        let path_segment = schema.api_path_segment.clone();

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

        // Path segment override from entity config
        let path_segment = entity_cfg
            .and_then(|ec| ec.path_segment.clone())
            .unwrap_or(path_segment);

        let ctx = ShellContext {
            entity_name,
            domain: domain.clone(),
            path_segment: path_segment.clone(),
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_update: operations.contains(&"update".to_string()),
        };

        let base = self
            .output_dir
            .join("ui")
            .join("src")
            .join("routes")
            .join(&domain)
            .join(&path_segment);

        let mut files = Vec::new();

        // List page
        if operations.contains(&"list".to_string()) {
            let content = render_template_with_project(tera, "ui/shell_list.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: base.join("+page.svelte"),
                content,
            });
        }

        // Detail page
        if operations.contains(&"read".to_string()) {
            let content = render_template_with_project(tera, "ui/shell_detail.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: base.join("[id]").join("+page.svelte"),
                content,
            });
            let load = render_template_with_project(tera, "ui/shell_detail_load.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: base.join("[id]").join("+page.server.ts"),
                content: load,
            });
        }

        // Create page
        if operations.contains(&"create".to_string()) {
            let content = render_template_with_project(tera, "ui/shell_create.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: base.join("new").join("+page.svelte"),
                content,
            });
        }

        // Edit page
        if operations.contains(&"update".to_string()) {
            let content = render_template_with_project(tera, "ui/shell_edit.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: base.join("[id]").join("edit").join("+page.svelte"),
                content,
            });
        }

        Ok(files)
    }
}
