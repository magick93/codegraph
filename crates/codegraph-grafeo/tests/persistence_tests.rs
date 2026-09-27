//! L1 graph persistence (issue #275): the engine can be reopened from a
//! path and sees the same graph. This is the substrate for the driver's
//! skip-re-ingestion path — ingestion writes must survive process exit.

use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::SchemaNode;
use codegraph_grafeo::GrafeoEngine;

fn fixture_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: "common/PersonType".to_string(),
        title: "PersonType".to_string(),
        description: Some("A person".to_string()),
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/PersonType.json".to_string(),
        pg_type: "TABLE".to_string(),
        rust_type: "PersonType".to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: "PersonType".to_string(),
        pg_table_name: "person_type".to_string(),
        api_path_segment: "person-type".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

#[tokio::test]
async fn persistent_engine_round_trips_schema_data() {
    let dir = tempfile::tempdir().unwrap();
    let graph_path = dir.path().join("graph.grafeo");

    {
        let engine = GrafeoEngine::persistent(&graph_path).unwrap();
        engine
            .ingest_schema(&fixture_schema())
            .await
            .expect("ingest into persistent engine");
        engine.checkpoint().expect("checkpoint");
    }

    let reopened = GrafeoEngine::persistent(&graph_path).unwrap();
    let schema = reopened
        .get_schema("PersonType")
        .await
        .expect("query reopened engine")
        .expect("schema survives reopen");
    assert_eq!(schema.schema_id, "common/PersonType");
    assert_eq!(schema.domain.as_deref(), Some("common"));
}

#[tokio::test]
async fn persistent_engine_on_fresh_path_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let graph_path = dir.path().join("nested").join("graph.grafeo");

    let engine = GrafeoEngine::persistent(&graph_path).unwrap();
    let schemas = engine.list_schemas(None).await.expect("list schemas");
    assert!(schemas.is_empty(), "fresh persistent engine must be empty");
    assert!(graph_path.exists(), "graph file must exist after open");
}

#[tokio::test]
async fn read_only_reopen_sees_checkpointed_graph_and_rejects_writes() {
    let dir = tempfile::tempdir().unwrap();
    let graph_path = dir.path().join("graph.grafeo");

    {
        let engine = GrafeoEngine::persistent(&graph_path).unwrap();
        engine
            .ingest_schema(&fixture_schema())
            .await
            .expect("ingest");
        engine.checkpoint().expect("checkpoint");
    }

    let read_only = GrafeoEngine::open_read_only(&graph_path).unwrap();
    let schema = read_only
        .get_schema("PersonType")
        .await
        .expect("query read-only engine")
        .expect("checkpointed schema visible");
    assert_eq!(schema.schema_id, "common/PersonType");

    let session = read_only.db().session();
    let mutation = session.execute("INSERT (:Schema {title: 'X'})");
    assert!(mutation.is_err(), "read-only open must reject DB mutations");
}
