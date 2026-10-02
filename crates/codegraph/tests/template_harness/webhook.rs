use crate::harness::{
    setup_mock, test_domain_config, test_generation_order, test_project_config, test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};

// ── Webhook Global Generators ────────────────────────────────────────────────

#[tokio::test]
async fn webhook_dispatch_generator_produces_dispatch_module() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-webhook-dispatch");

    let generator = generate::webhook::dispatch::WebhookDispatchGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let path = files[0].path.to_string_lossy();
    assert!(path.ends_with("webhook_dispatch.rs"), "path: {path}");

    let content = &files[0].content;
    assert!(content.contains("WebhookDispatcher"));
    assert!(content.contains("fn new("));
    assert!(content.contains("dispatch_pending"));
    assert!(content.contains("pgmq.list_queues"));
    assert!(content.contains("X-Webhook-Signature"));
    assert!(content.contains("delay_for_attempt"));
}

#[tokio::test]
async fn webhook_endpoint_api_generator_produces_api_and_router() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-webhook-endpoint-api");

    let generator = generate::webhook::endpoint_api::WebhookEndpointApiGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 2);

    let api_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("webhook_api.rs"))
        .expect("should produce webhook_api.rs");
    assert!(api_file.content.contains("list_endpoints"));
    assert!(api_file.content.contains("create_endpoint"));
    assert!(api_file.content.contains("rotate_secret"));
    assert!(api_file.content.contains("list_deliveries"));

    let router_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("webhook_router.rs"))
        .expect("should produce webhook_router.rs");
    assert!(router_file.content.contains("webhook_routes"));
    assert!(router_file.content.contains("webhook-endpoints"));
    assert!(router_file.content.contains("list_endpoints"));
    assert!(router_file.content.contains("delete_subscription"));
}
