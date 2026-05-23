#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Graph error: {0}")]
    Graph(#[from] codegraph_core::error::GraphError),
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
