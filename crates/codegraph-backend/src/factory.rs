use codegraph_core::error::GraphError;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_grafeo::GrafeoEngine;

use crate::config::BackendConfig;

pub struct Backend {
    engine: GrafeoEngine,
}

impl Backend {
    pub fn ingestor(&self) -> &dyn GraphIngestor {
        &self.engine
    }

    pub fn querier(&self) -> &dyn GraphQuerier {
        &self.engine
    }
}

pub async fn create_backend(_config: &BackendConfig) -> Result<Backend, GraphError> {
    let engine = GrafeoEngine::in_memory()?;
    Ok(Backend { engine })
}
