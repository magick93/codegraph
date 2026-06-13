//! Codelist enum generator targeting the domain-types crate.
//! Generated enums live in `{crate_root}/src/codelist/`.

use std::path::PathBuf;

use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::codelist::rust_enum::RustCodelistGenerator;
use crate::generate::traits::GeneratedFile;
use crate::generate::ProjectConfig;

/// Thin wrapper around [`RustCodelistGenerator`] that targets the domain-types crate.
pub struct DomainTypesCodelistGenerator {
    inner: RustCodelistGenerator,
}

impl DomainTypesCodelistGenerator {
    /// Creates a generator that writes output under `base_dir` (crate root).
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self {
            inner: RustCodelistGenerator::new(&base_dir),
        }
    }

    /// Generate all codelist enum files, delegating to the inner generator.
    pub async fn generate_all(
        &self,
        db: &dyn GraphQuerier,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        self.inner.generate_all(db, tera, project).await
    }
}
