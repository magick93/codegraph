use codegraph_backend::{create_backend, BackendConfig, BackendKind};
use codegraph_core::traits::{GraphIngestor, GraphQuerier};

#[tokio::test]
async fn create_grafeo_backend() {
    let config = BackendConfig {
        kind: BackendKind::Grafeo,
        connection_url: None,
        data_dir: None,
    };
    let backend = create_backend(&config).await.unwrap();
    let _ingestor: &dyn GraphIngestor = backend.ingestor();
    let _querier: &dyn GraphQuerier = backend.querier();
}

#[tokio::test]
async fn grafeo_backend_passes_conformance() {
    use codegraph_core::test_fixtures;

    let config = BackendConfig::default();
    let backend = create_backend(&config).await.unwrap();

    let stats = test_fixtures::ingest_fixtures(backend.ingestor())
        .await
        .unwrap();
    assert_eq!(stats.schema_count, 2);

    test_fixtures::assert_conformance_queries(backend.querier()).await;
}

#[tokio::test]
async fn default_backend_is_grafeo() {
    let config = BackendConfig::default();
    assert_eq!(config.kind, BackendKind::Grafeo);
}
