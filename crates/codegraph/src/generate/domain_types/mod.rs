pub mod codelist;
pub mod dto;
pub mod query_service;
pub mod scaffold;

use std::path::PathBuf;

/// Returns the `hr-domain-types/src/` directory, resolved from the compiled-in workspace root.
pub(crate) fn domain_types_src_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .expect("hr-graph should be inside workspace root")
        .join("crates")
        .join("hr-domain-types")
        .join("src")
}
