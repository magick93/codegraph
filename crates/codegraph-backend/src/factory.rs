use codegraph_core::error::GraphError;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_grafeo::GrafeoEngine;

use crate::config::BackendConfig;

pub struct Backend {
    engine: GrafeoEngine,
}

impl Backend {
    pub fn from_engine(engine: GrafeoEngine) -> Backend {
        Backend { engine }
    }

    pub fn engine(&self) -> &GrafeoEngine {
        &self.engine
    }

    pub fn ingestor(&self) -> &dyn GraphIngestor {
        &self.engine
    }

    pub fn querier(&self) -> &dyn GraphQuerier {
        &self.engine
    }
}

/// Create the backend the pipeline runs on. With `data_dir` set the engine
/// persists to a single-file Grafeo database at that path (issue #275);
/// the default stays in-memory.
pub async fn create_backend(config: &BackendConfig) -> Result<Backend, GraphError> {
    let engine = match &config.data_dir {
        Some(dir) => GrafeoEngine::persistent(dir)?,
        None => GrafeoEngine::in_memory()?,
    };
    Ok(Backend::from_engine(engine))
}
