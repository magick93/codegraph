use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::filter_fields::{
    resolve_filter_fields, resolve_nested_filter_fields, FilterFieldInfo, NestedFilterFieldInfo,
};
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct CliCommandContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub has_workflow: bool,
    pub has_approval_status: bool,
    pub has_fts: bool,
    pub fields: Vec<CliFieldInfo>,
    pub filter_fields: Vec<FilterFieldInfo>,
    pub nested_filter_fields: Vec<NestedFilterFieldInfo>,
}

#[derive(Debug, Serialize)]
pub struct CliFieldInfo {
    pub name: String,
    pub rust_type: String,
    pub is_required: bool,
    pub description: Option<String>,
}

pub struct CliCommandGenerator {
    output_dir: PathBuf,
}

impl CliCommandGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for CliCommandGenerator {
    fn name(&self) -> &str {
        "cli_command"
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
        let path_segment = schema.api_path_segment.clone();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

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
        let has_approval_status = workflow
            .and_then(|wf| wf.approval_status_field.as_ref())
            .is_some();

        let has_fts = entity_cfg
            .and_then(|ec| ec.search.fts_columns.as_ref())
            .map(|cols| !cols.is_empty())
            .unwrap_or(false);

        // Get properties for field-level CLI args
        let properties = db.get_properties(schema_title).await?;
        let fields: Vec<CliFieldInfo> = properties
            .iter()
            .filter(|p| p.render_strategy == "direct_column")
            .map(|p| CliFieldInfo {
                name: p.rust_field_name.clone(),
                rust_type: p.rust_field_type.clone(),
                is_required: p.is_required,
                description: p.description.clone(),
            })
            .collect();

        let filter_fields = resolve_filter_fields(
            db,
            schema_title,
            entity_cfg
                .and_then(|ec| ec.filter_fields.as_ref())
                .map(|v| v.as_slice()),
        )
        .await?;

        let nested_filter_fields =
            resolve_nested_filter_fields(db, schema_title, &module_name, domain, config).await?;

        let ctx = CliCommandContext {
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_list: operations.contains(&"list".to_string()),
            entity_name,
            module_name: module_name.clone(),
            domain: domain.to_string(),
            path_segment,
            has_workflow,
            has_approval_status,
            has_fts,
            fields,
            filter_fields,
            nested_filter_fields,
        };

        let content = render_template_with_project(tera, "cli/entity_command.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("cli")
                .join("src")
                .join("commands")
                .join(domain)
                .join(format!("{}.rs", module_name)),
            content,
        }])
    }
}
