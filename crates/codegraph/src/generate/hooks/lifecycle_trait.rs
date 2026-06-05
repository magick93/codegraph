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
    _output_dir: PathBuf,
    /// Base directory for generated hooks output.
    ///
    /// In production this is `{workspace_root}/crates/hr-hooks-api/src/generated`.
    /// In tests this should be a temp directory to avoid corrupting the real workspace source.
    generated_dir: PathBuf,
}

impl LifecycleTraitGenerator {
    /// Production constructor: derives the output path from the compiled-in workspace root.
    pub fn new(output_dir: &Path) -> Self {
        Self {
            _output_dir: output_dir.to_path_buf(),
            generated_dir: Self::workspace_root()
                .join("crates")
                .join("hr-hooks-api")
                .join("src")
                .join("generated"),
        }
    }

    /// Test / override constructor: writes output under `base_dir` instead of the
    /// compiled-in workspace root.  Pass a `tempfile::tempdir()` path to avoid
    /// corrupting the real `crates/hr-hooks-api/src/generated` when running with a mock graph.
    pub fn new_with_base(output_dir: &Path, base_dir: PathBuf) -> Self {
        Self {
            _output_dir: output_dir.to_path_buf(),
            generated_dir: base_dir,
        }
    }

    /// Compute the workspace root from `CARGO_MANIFEST_DIR` (which points to `hr-graph/`).
    fn workspace_root() -> PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir)
            .parent()
            .expect("hr-graph should be inside workspace root")
            .to_path_buf()
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
            .get_schema(schema_title)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();

        if module_name.is_empty() {
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
