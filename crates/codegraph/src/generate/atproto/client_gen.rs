use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{EntityGenerator, GeneratedFile, GlobalGenerator};
use crate::generate::{GenerationEntry, ProjectConfig};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct ClientContext {
    pub entity_name: String,
    pub domain: String,
    pub collection_nsid: String,
    pub record_type: String,
    pub fields: Vec<QueryableField>,
    pub operations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct QueryableField {
    pub name: String,
    pub rust_type: String,
}

#[derive(Debug, Serialize)]
pub struct ClientScaffoldContext {
    pub entities: Vec<ClientEntityEntry>,
    pub authority: String,
}

#[derive(Debug, Serialize)]
pub struct ClientEntityEntry {
    pub entity_name: String,
    pub module_name: String,
    pub collection_nsid: String,
    pub domain: String,
}

pub struct AtprotoClientEmitter {
    output_dir: PathBuf,
}

impl AtprotoClientEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for AtprotoClientEmitter {
    fn name(&self) -> &str {
        "atproto_client"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        _config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let authority = &project.atproto_authority;
        if authority.is_empty() {
            return Ok(Vec::new());
        }

        let collections = db.get_collections(domain).await.unwrap_or_default();

        let entity_snake = codegraph_naming::to_snake_case(schema_title);
        let expected_nsid_prefix = format!("{}.{}.{}", authority, domain, entity_snake);

        let collection = collections
            .iter()
            .find(|c| c.nsid == expected_nsid_prefix || c.nsid.starts_with(&expected_nsid_prefix));

        let collection_nsid = match collection {
            Some(c) => c.nsid.clone(),
            None => return Ok(Vec::new()),
        };

        let entity_name = codegraph_naming::strip_suffix(
            schema_title,
            &_config.defaults.type_suffix,
        );

        let record_type_name = format!("{}Record", entity_name);

        // Fetch schema properties for queryable fields
        let properties = db
            .get_properties_in_domain(schema_title, domain)
            .await
            .unwrap_or_default();

        let fields: Vec<QueryableField> = properties
            .iter()
            .map(|p| QueryableField {
                name: p.name.clone(),
                rust_type: p.rust_field_type.clone(),
            })
            .collect();

        let operations = vec![
            "create".to_string(),
            "read".to_string(),
            "update".to_string(),
            "delete".to_string(),
            "list".to_string(),
        ];

        let context = ClientContext {
            entity_name,
            domain: domain.to_string(),
            collection_nsid,
            record_type: record_type_name,
            fields,
            operations,
        };

        let content =
            render_template_with_project(tera, "atproto/client.tera", &context, project)?;

        let module_name = codegraph_naming::to_snake_case(schema_title);
        let path = self
            .output_dir
            .join("src")
            .join("atproto_client")
            .join(format!("{}_client.rs", module_name));

        Ok(vec![GeneratedFile { path, content }])
    }
}

pub struct AtprotoClientScaffoldEmitter {
    output_dir: PathBuf,
}

impl AtprotoClientScaffoldEmitter {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for AtprotoClientScaffoldEmitter {
    fn name(&self) -> &str {
        "atproto_client_scaffold"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let authority = &project.atproto_authority;
        if authority.is_empty() {
            return Ok(Vec::new());
        }

        let collections = db.get_collections("").await.unwrap_or_default();

        let entities: Vec<ClientEntityEntry> = collections
            .iter()
            .map(|c| {
                let parts: Vec<&str> = c.nsid.strip_prefix(&format!("{}.", authority))
                    .unwrap_or(&c.nsid)
                    .split('.')
                    .collect();
                let last_part = parts.last().unwrap_or(&"unknown");
                let entity_name = codegraph_naming::to_pascal_case(last_part);
                let module_name = codegraph_naming::to_snake_case(last_part);
                ClientEntityEntry {
                    entity_name,
                    module_name,
                    collection_nsid: c.nsid.clone(),
                    domain: c.domain.clone(),
                }
            })
            .collect();

        let context = ClientScaffoldContext {
            entities,
            authority: authority.clone(),
        };

        let content = render_template_with_project(
            tera,
            "atproto/client_scaffold.tera",
            &context,
            project,
        )?;

        let path = self
            .output_dir
            .join("src")
            .join("atproto_client")
            .join("mod.rs");

        Ok(vec![GeneratedFile { path, content }])
    }
}
