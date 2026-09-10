pub mod classify;
pub mod driver;
pub mod error;
pub mod ingest;
pub mod init;
pub mod lsp;
pub mod rev;
pub mod validate;

/// Compatibility re-export: the generation engine now lives in the
/// `codegraph-generate` crate. `crate::generate::…` and
/// `codegraph::generate::…` paths resolve unchanged through this alias.
pub use codegraph_generate as generate;

/// Compatibility re-export: profile loading / BuildPlan now lives in the
/// `codegraph-generate` crate.
pub use codegraph_generate::profile;
