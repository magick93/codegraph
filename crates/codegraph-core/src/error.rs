/// The unified error type for ingest and generation pipelines.
///
/// Lives in `codegraph-core` so both the `codegraph` binary crate and the
/// `codegraph-generate` crate can share one `Result` alias; each crate
/// re-exports it as `crate::error`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Graph error: {0}")]
    Graph(#[from] GraphError),
    #[error("Schema not found: {0}")]
    SchemaNotFound(String),
    #[error("Ref resolution failed: {0}")]
    RefResolution(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Config error: {0}")]
    Config(String),
    #[error("Template error: {0}")]
    Template(String),
    #[error("Validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Schema not found: {0}")]
    NotFound(String),
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Query failed: {0}")]
    Query(String),
    #[error("Ingest failed: {0}")]
    Ingest(String),
    #[error("Internal error: {0}")]
    Internal(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("Not implemented")]
    NotImplemented,
}
