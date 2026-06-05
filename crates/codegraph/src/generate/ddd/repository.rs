use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::filter_fields::{resolve_filter_fields, FilterFieldInfo};
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

use super::repository_emitter::RepositoryImplEmitter;

#[derive(Debug, Serialize)]
pub struct RepositoryContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub operations: Vec<String>,
    pub has_create: bool,
    pub has_read: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub has_list: bool,
    pub has_fts: bool,
    pub has_embeddings: bool,
    pub filter_fields: Vec<FilterFieldInfo>,
    /// FK column for parent-scoped lookups (child entities only).
    pub parent_ref: Option<String>,
    /// Self-referential FK column for tree/hierarchy queries (e.g. "parent_id").
    pub hierarchy_field: Option<String>,
    /// Whether tree_include is configured (changes find_tree return type).
    #[serde(default)]
    pub tree_include: bool,
}

pub struct RepositoryTraitGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
}

impl RepositoryTraitGenerator {
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
impl EntityGenerator for RepositoryTraitGenerator {
    fn name(&self) -> &str {
        "repository"
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

        // Resolve parent_ref for child entities (graph-detected or manual config)
        let parent_ref = crate::generate::resolve_parent_fk_column_same_domain(
            schema_title,
            &self.parent_candidates,
            entity_cfg,
            &domain,
            config,
            db,
        )
        .await;

        let hierarchy_field = entity_cfg
            .and_then(|ec| ec.hierarchy_field.as_ref())
            .cloned();

        let tree_include = entity_cfg
            .and_then(|ec| ec.tree_include.as_ref())
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        let ctx = RepositoryContext {
            has_create: operations.contains(&"create".to_string()),
            has_read: operations.contains(&"read".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            has_list: operations.contains(&"list".to_string()),
            has_fts,
            has_embeddings,
            filter_fields,
            parent_ref: parent_ref.clone(),
            hierarchy_field,
            tree_include,
            entity_name,
            module_name: module_name.clone(),
            domain: domain.clone(),
            operations,
        };

        let base_dir = self
            .output_dir
            .join("src")
            .join("domain")
            .join(&domain)
            .join(&module_name);

        let mut files = Vec::new();

        // Repository trait (Tera template)
        let trait_content = render_template_with_project(tera, "ddd/repository.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: base_dir.join("repository.rs"),
            content: trait_content,
        });

        // Repository implementation (Rust emitter)
        let emitter = RepositoryImplEmitter;
        let impl_content = emitter
            .emit(db, schema_title, &domain, config, parent_ref.as_deref())
            .await?;
        files.push(GeneratedFile {
            path: base_dir.join("repository_impl.rs"),
            content: impl_content,
        });

        Ok(files)
    }
}
