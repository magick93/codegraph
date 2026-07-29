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

#[cfg(feature = "axum-integration")]
impl axum::response::IntoResponse for AtprotoError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match &self {
            AtprotoError::Network(e) => (axum::http::StatusCode::BAD_GATEWAY, e.to_string()),
            AtprotoError::Serialization(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AtprotoError::Repo(msg) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AtprotoError::Validation(msg) => (axum::http::StatusCode::BAD_REQUEST, msg.clone()),
            AtprotoError::Auth(msg) => (axum::http::StatusCode::UNAUTHORIZED, msg.clone()),
            AtprotoError::NotFound(msg) => (axum::http::StatusCode::NOT_FOUND, msg.clone()),
            AtprotoError::RateLimited => (axum::http::StatusCode::TOO_MANY_REQUESTS, "Rate limited".to_string()),
        };
        (status, msg).into_response()
    }
}
