use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::ddd::dto::build_dto_context;
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

/// Generates DTO files into the `hr-domain-types` crate instead of the generated app.
///
/// Reuses the same `DtoContext` and context-building logic as the app-level `DtoGenerator`,
/// but outputs to `crates/hr-domain-types/src/{domain}/{entity}/` using dedicated templates.
pub struct DomainTypesDtoGenerator {
    /// Base `src/` directory for `hr-domain-types` output.
    ///
    /// In production this is `{workspace_root}/crates/hr-domain-types/src`.
    /// In tests this should be a temp directory to avoid corrupting the real workspace source.
    src_dir: PathBuf,
}

impl DomainTypesDtoGenerator {
    /// Production constructor: derives the output path from the compiled-in workspace root.
    pub fn new(_output_dir: &Path) -> Self {
        Self {
            src_dir: super::domain_types_src_dir(),
        }
    }

    /// Test / override constructor: writes output under `base_dir` instead of the
    /// compiled-in workspace root.
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self { src_dir: base_dir }
    }
}

#[async_trait]
impl EntityGenerator for DomainTypesDtoGenerator {
    fn name(&self) -> &str {
        "domain_types_dto"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let ctx = build_dto_context(db, schema_title, domain, config).await?;

        if ctx.module_name.is_empty() {
            return Ok(Vec::new());
        }

        let base_dir = self.src_dir.join(&ctx.domain).join(&ctx.module_name);

        let mut files = Vec::new();

        if ctx.operations.contains(&"create".to_string()) {
            let content = render_template(tera, "domain_types/dto_create.tera", &ctx)?;
            files.push(GeneratedFile {
                path: base_dir.join("dto_create.rs"),
                content,
            });
        }

        if ctx.operations.contains(&"update".to_string()) {
            let content = render_template(tera, "domain_types/dto_update.tera", &ctx)?;
            files.push(GeneratedFile {
                path: base_dir.join("dto_update.rs"),
                content,
            });
        }

        let response = render_template(tera, "domain_types/dto_response.tera", &ctx)?;
        files.push(GeneratedFile {
            path: base_dir.join("dto_response.rs"),
            content: response,
        });

        Ok(files)
    }
}
