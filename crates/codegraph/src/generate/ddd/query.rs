use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;
use serde::Serialize;

use crate::error::Result;
use crate::generate::filter_fields::{resolve_filter_fields, FilterFieldInfo};
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct QueryContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub has_read: bool,
    pub has_list: bool,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub filter_fields: Vec<FilterFieldInfo>,
    /// FK column for parent-scoped lookups (child entities only).
    pub parent_ref: Option<String>,
}

pub struct QueryGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
}

impl QueryGenerator {
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
impl EntityGenerator for QueryGenerator {
    fn name(&self) -> &str {
        "query"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
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

        let search = entity_cfg.map(|ec| &ec.search);
        let has_fts = search
            .and_then(|s| s.fts_columns.as_ref())
            .map(|cols| !cols.is_empty())
            .unwrap_or(false);
        let has_embeddings = search
            .map(|s| !s.embedding_columns.is_empty())
            .unwrap_or(false);

        let filter_fields = resolve_filter_fields(
            db,
            schema_title,
            entity_cfg
                .and_then(|ec| ec.filter_fields.as_ref())
                .map(|v| v.as_slice()),
        )
        .await?;

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

        let ctx = QueryContext {
            has_read: operations.contains(&"read".to_string()),
            has_list: operations.contains(&"list".to_string()),
            has_fts,
            has_embeddings,
            filter_fields,
            parent_ref,
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
        };

        let content = render_template(tera, "ddd/query.tera", &ctx)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(&domain)
                .join(&module_name)
                .join("query.rs"),
            content,
        }])
    }
}
