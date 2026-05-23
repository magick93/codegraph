//! Codelist enum generator targeting the `hr-domain-types` crate.
//! Generated enums live in `crates/hr-domain-types/src/codelist/`.

use std::path::PathBuf;

use codegraph_core::traits::GraphQuerier;

use crate::error::Result;
use crate::generate::codelist::rust_enum::RustCodelistGenerator;
use crate::generate::traits::GeneratedFile;

use super::domain_types_src_dir;

/// Thin wrapper around [`RustCodelistGenerator`] that targets the `hr-domain-types` crate.
pub struct DomainTypesCodelistGenerator {
    inner: RustCodelistGenerator,
}

impl Default for DomainTypesCodelistGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainTypesCodelistGenerator {
    /// Production constructor — resolves the workspace `hr-domain-types` crate directory.
    pub fn new() -> Self {
        let base_dir = domain_types_src_dir()
            .parent()
            .expect("domain_types_src_dir() should have a parent")
            .to_path_buf();
        Self {
            inner: RustCodelistGenerator::new(&base_dir),
        }
    }

    /// Test constructor — allows injecting a custom base directory.
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
    ) -> Result<Vec<GeneratedFile>> {
        self.inner.generate_all(db, tera).await
    }
}
