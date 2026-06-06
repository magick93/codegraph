pub mod file_presence;
pub mod proto_compile;
pub mod snapshot;
pub mod string_pattern;

use std::path::Path;

use codegraph::generate::traits::GeneratedFile;

/// Trait for validating generated output files.
pub trait OutputValidator: Send + Sync {
    fn name(&self) -> &str;

    /// Validate the set of generated files.
    /// Returns `Ok(())` on success, or `Err` with a list of error messages.
    fn validate(
        &self,
        files: &[GeneratedFile],
        work_dir: &Path,
    ) -> Result<(), Vec<String>>;
}
