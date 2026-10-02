//! L1 backend factory persistence (issue #275): `create_backend` with a
//! `data_dir` reopens the persisted graph instead of starting in-memory.

use codegraph_backend::{BackendConfig, BackendKind, create_backend};
use codegraph_core::types::SchemaNode;

fn schema(title: &str) -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: format!("common/{title}"),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some("common".to_string()),
        rel_path: format!("common/{title}.json"),
        pg_type: "TABLE".to_string(),
        rust_type: title.to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: title.to_string(),
        pg_table_name: format!("t_{title}"),
        api_path_segment: title.to_string(),
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
async fn backend_with_data_dir_reopens_persisted_graph() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("graph.grafeo");

    {
        let config = BackendConfig {
            kind: BackendKind::Grafeo,
            connection_url: None,
            data_dir: Some(data_dir.clone()),
        };
        let be = create_backend(&config).await.unwrap();
        be.ingestor()
            .ingest_schema(&schema("WidgetType"))
            .await
            .unwrap();
        be.engine().checkpoint().unwrap();
    }

    let config = BackendConfig {
        kind: BackendKind::Grafeo,
        connection_url: None,
        data_dir: Some(data_dir),
    };
    let be = create_backend(&config).await.unwrap();
    let titles: Vec<String> = be
        .querier()
        .list_schemas(None)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.title)
        .collect();
    assert_eq!(titles, vec!["WidgetType".to_string()]);
}
