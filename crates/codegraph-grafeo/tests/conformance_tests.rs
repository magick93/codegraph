use codegraph_core::test_fixtures;
use codegraph_grafeo::GrafeoEngine;

#[tokio::test]
async fn grafeo_conformance() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let stats = test_fixtures::ingest_fixtures(&engine).await.unwrap();
    assert_eq!(stats.schema_count, 2);
    assert_eq!(stats.property_count, 3);
    assert_eq!(stats.codelist_count, 1);
    assert_eq!(stats.enum_value_count, 3);

    test_fixtures::assert_conformance_queries(&engine).await;
}
