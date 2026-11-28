//! The persisted-graph cache (issue #275): the inputs hash, reopening a
//! reusable graph, wiping a stale one (issue #538), and persisting a
//! freshly ingested graph with its node-count-stamped marker.

use std::path::Path;

use codegraph_backend::{Backend, BackendConfig, create_backend};

use super::sources::ModelSources;
use crate::error::Result;

/// Compute the persisted-graph cache inputs hash (issue #275): every model
/// config input plus the pinned dependency faces (#276). `None` when no
/// cache dir was requested (hashing is skipped entirely).
pub(super) fn compute_inputs_hash(
    graph_cache: Option<&Path>,
    sources: &ModelSources<'_>,
    config_path: &Path,
    domain_config: &codegraph_config::config::DomainConfig,
    config_dir: Option<&Path>,
) -> Result<Option<String>> {
    let inputs_hash: Option<String> = match graph_cache {
        Some(_cache_dir) => {
            let mut inputs = crate::artifact::collect_run_inputs(
                sources.schemas,
                sources.classifier,
                config_path,
                sources.mox_files,
                sources.rosetta_files,
                sources.ddd_files,
                sources.evt_files,
                sources.deploy_files,
                sources.ifml_files,
                sources.openapi_files,
            )
            .map_err(|e| crate::error::Error::Config(e.to_string()))?;
            // Pinned dependency faces shape the graph too (issue #276):
            // editing one must invalidate the persisted graph cache.
            inputs.extend(
                crate::ingest::dependencies::collect_dependency_inputs(domain_config, config_dir)
                    .map_err(|e| crate::error::Error::Config(e.to_string()))?,
            );
            Some(crate::artifact::inputs_hash(&inputs))
        }
        None => None,
    };
    Ok(inputs_hash)
}

/// Graph cache (issue #275): reopen the persisted graph when the stored
/// inputs hash matches; otherwise ingest fresh into a persistent engine
/// (checkpointed after ingestion). Without a cache dir this is the
/// in-memory default. Returns the backend and whether the persisted graph
/// was reused (ingestion passes are skipped on reuse).
pub(super) async fn open_graph_backend(
    graph_cache: Option<&Path>,
    inputs_hash: Option<&str>,
) -> Result<(Backend, bool)> {
    let (be, graph_cache_reused) = match (graph_cache, inputs_hash) {
        (Some(cache_dir), Some(hash)) => {
            if let Some(engine) = crate::artifact::open_reusable_engine(cache_dir, hash)
                .map_err(|e| crate::error::Error::Config(e.to_string()))?
            {
                eprintln!(
                    "INFO: reusing persisted graph cache at {} (inputs hash {} unchanged) — skipping re-ingestion",
                    cache_dir.display(),
                    &hash[..16]
                );
                (Backend::from_engine(engine), true)
            } else {
                // Cache miss (inputs changed, or no/poisoned marker): the
                // stale graph must be rebuilt from scratch. Grafeo 0.6
                // `Config::persistent` REOPENS an existing single-file
                // database, so pointing the ingest target at the old file
                // appended every node a second time (issue #538) — wipe it
                // first. The marker is only written after the successful
                // checkpoint below, so an aborted re-ingest leaves the cache
                // absent (the next run misses and wipes again).
                crate::artifact::reset_cache_graph(cache_dir);
                let cached_config = BackendConfig {
                    data_dir: Some(cache_dir.join(crate::artifact::CACHE_GRAPH_FILE)),
                    ..BackendConfig::default()
                };
                let backend = create_backend(&cached_config)
                    .await
                    .map_err(|e| crate::error::Error::Config(e.to_string()))?;
                (backend, false)
            }
        }
        _ => (in_memory_backend().await?, false),
    };
    Ok((be, graph_cache_reused))
}

/// Persist the freshly ingested graph to the cache dir (issue #275): the
/// engine checkpoint plus the node-count-stamped marker.
pub(super) fn persist_fresh_graph(be: &Backend, cache_dir: &Path, hash: &str) -> Result<()> {
    let node_count = be
        .engine()
        .node_count()
        .map_err(|e| crate::error::Error::Config(e.to_string()))?;
    be.engine()
        .checkpoint()
        .map_err(|e| crate::error::Error::Config(e.to_string()))?;
    crate::artifact::persist_cache_marker(cache_dir, hash, node_count)
        .map_err(|e| crate::error::Error::Config(e.to_string()))?;
    println!(
        "Graph cache: persisted fresh graph (inputs hash {})",
        &hash[..16]
    );
    Ok(())
}

/// The in-memory default backend — every workflow without a graph cache.
pub(super) async fn in_memory_backend() -> Result<Backend> {
    create_backend(&BackendConfig::default())
        .await
        .map_err(|e| crate::error::Error::Config(e.to_string()))
}
