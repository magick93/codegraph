use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtprotoError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Repo operation failed: {0}")]
    Repo(String),
    #[error("Validation failed: {0}")]
    Validation(String),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Rate limited")]
    RateLimited,
}
