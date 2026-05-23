pub mod caching_querier;
pub mod error;
pub mod mock;
pub mod traits;
pub mod types;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;

// Re-export for consumers (used by GraphQuerier::get_entity_schema_map)
pub use std::collections::HashMap;
