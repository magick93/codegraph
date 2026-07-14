use thiserror::Error;

#[derive(Debug, Error)]
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
