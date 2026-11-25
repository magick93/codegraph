use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Shared metadata included in all API JSON responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Meta {
    /// Correlation ID for request tracing.
    pub correlation_id: String,
}
