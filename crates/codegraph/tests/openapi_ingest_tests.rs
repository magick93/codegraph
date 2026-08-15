use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphQuerier;

mod helpers;

#[tokio::test]
async fn test_openapi_ingest_stats_and_roundtrip() {
    let engine = MockEngine::new();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/openapi/customers.json");

    let stats = codegraph::ingest::openapi_ingest::ingest_openapi_file(&engine, &path)
        .await
        .expect("OpenAPI ingestion should succeed");

    assert_eq!(stats.resources, 1, "one resource (customers)");
    assert_eq!(stats.operations, 3, "list + create + read");
    assert_eq!(stats.interactions, 3);
    assert_eq!(stats.endpoints, 3);

    let resources = engine.get_api_resources().await.unwrap();
    let customers = resources
        .iter()
        .find(|r| r.name == "customers")
        .expect("customers resource should exist");
    assert_eq!(customers.schema_title, "Customer");
    assert_eq!(customers.domain, "external");
    assert_eq!(customers.path_segment, "/v1/customers");

    let ops = engine.get_api_operations("customers").await.unwrap();
    assert_eq!(ops.len(), 3);

    let list = ops
        .iter()
        .find(|o| o.name == "list_customers")
        .expect("list_customers operation should exist");
    assert_eq!(list.kind, "list");
    assert!(list.paging);
    assert!(list.sorting);
    assert!(list.filtering);
    assert_eq!(list.output_schema, "Customer");

    let create = ops
        .iter()
        .find(|o| o.name == "create_customers")
        .expect("create_customers operation should exist");
    assert_eq!(create.kind, "create");
    assert_eq!(create.input_schema.as_deref(), Some("Customer"));
    assert!(!create.paging);

    let read = ops
        .iter()
        .find(|o| o.name == "read_customers")
        .expect("read_customers operation should exist");
    assert_eq!(read.kind, "read");

    let endpoint = engine
        .get_http_endpoint_for_operation("list_customers")
        .await
        .unwrap()
        .expect("endpoint for list_customers should exist");
    assert_eq!(endpoint.method, "GET");
    assert_eq!(endpoint.path_template, "/v1/customers");

    let read_endpoint = engine
        .get_http_endpoint_for_operation("read_customers")
        .await
        .unwrap()
        .expect("endpoint for read_customers should exist");
    assert_eq!(read_endpoint.method, "GET");
    assert_eq!(read_endpoint.path_template, "/v1/customers/{customerId}");
}

#[tokio::test]
async fn test_openapi_duplicate_operation_id_is_skipped() {
    let engine = MockEngine::new();
    let doc = serde_json::json!({
        "openapi": "3.0.3",
        "info": { "title": "dup", "version": "1.0.0" },
        "paths": {
            "/v1/widgets": {
                "get": {
                    "operationId": "list_widgets",
                    "responses": { "200": { "description": "ok" } }
                }
            },
            "/v1/gadgets": {
                "get": {
                    "operationId": "list_widgets",
                    "responses": { "200": { "description": "ok" } }
                }
            }
        }
    });

    let stats = codegraph::ingest::openapi_ingest::ingest_openapi_spec(&engine, &doc)
        .await
        .expect("ingestion should succeed");

    assert_eq!(stats.operations, 1, "duplicate operationId must be skipped");
    assert_eq!(stats.resources, 2);
}

/// Grafeo round-trip: `ingest_api_model` (domain config) creates
/// ApiOperation + Interaction + HttpEndpoint chains resolvable through
/// `get_http_endpoint_for_operation`. Operation names follow api_ingest's
/// `{kind}_{resource}` convention with the PascalCase resource name
/// (e.g. `list_Candidate` for entity `CandidateType`).
#[tokio::test]
async fn test_grafeo_http_endpoint_for_operation() {
    let engine = codegraph_grafeo::GrafeoEngine::in_memory().expect("in-memory Grafeo engine");
    let config = crate::helpers::domain_config();
    codegraph::ingest::api_ingest::ingest_api_model(&engine, &config)
        .await
        .expect("API model ingestion should succeed");

    let endpoint = engine
        .get_http_endpoint_for_operation("list_Candidate")
        .await
        .unwrap()
        .expect("endpoint for list_Candidate should exist");
    assert_eq!(endpoint.method, "GET");
    assert_eq!(
        endpoint.path_template,
        "/api/v1/recruiting/CandidateType",
        "list path template from api_ingest"
    );

    let op = engine
        .get_api_operation("read_Candidate")
        .await
        .unwrap()
        .expect("read_Candidate operation should exist");
    assert_eq!(op.kind, "read");
}
