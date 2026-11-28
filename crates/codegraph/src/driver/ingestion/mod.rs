//! The ingestion orchestration layer: which plane ingests when. Each
//! submodule owns one plane's passes; the ordering contract lives in the
//! callers (`workflows::run` for the full pipeline):
//!
//! 1. [`models`] — mox, then rosetta, then JSON Schema, then alias wiring
//!    (the domain plane; every later plane resolves against it).
//! 2. [`designs`] — IFML ([`ifml`]), `.ddd`, `.evt`, `.deploy`, the API
//!    model, charts, OpenAPI.
//! 3. [`classification`] — entity classification, then the dependency
//!    faces (which must land AFTER classification), then AT Protocol.

pub(super) mod classification;
pub(super) mod designs;
pub(super) mod ifml;
pub(super) mod models;

/// Whether an ingest group reports its passes on stdout. The `classify`
/// report runs the model passes [`PassLog::Quiet`] so its stdout stays the
/// report alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PassLog {
    Report,
    Quiet,
}

impl PassLog {
    fn reports(self) -> bool {
        self == PassLog::Report
    }
}
