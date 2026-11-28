//! The model-source inputs a pipeline run ingests.

use std::path::{Path, PathBuf};

/// The CLI model-source inputs (the `RunArgs` model fields), threaded
/// through the inputs-hash computation and the ingest pass groups.
pub(super) struct ModelSources<'a> {
    pub(super) schemas: Option<&'a Path>,
    pub(super) classifier: Option<&'a Path>,
    pub(super) mox_files: &'a [PathBuf],
    pub(super) rosetta_files: &'a [PathBuf],
    pub(super) ddd_files: &'a [PathBuf],
    pub(super) evt_files: &'a [PathBuf],
    pub(super) deploy_files: &'a [PathBuf],
    pub(super) ifml_files: &'a [PathBuf],
    pub(super) openapi_files: &'a [PathBuf],
}
