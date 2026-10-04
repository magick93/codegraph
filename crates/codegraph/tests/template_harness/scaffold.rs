use crate::harness::{
    setup_mock, sqlite_project_config, test_domain_config, test_generation_order,
    test_project_config, test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};

// === Scaffold Template Tests (Global) ===

#[tokio::test]
async fn scaffold_main() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
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

    assert!(
        files.len() >= 3,
        "Should have main.rs, app_state.rs, Cargo.toml"
    );

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("main.rs"))
        .expect("Should have main.rs");
    assert!(!main_file.content.is_empty());
    assert!(
        main_file.content.contains("server::run_server()"),
        "main.rs should delegate to server::run_server(). Got:\n{}",
        main_file.content
    );

    let server_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("server.rs"))
        .expect("Should have server.rs");
    assert!(!server_file.content.is_empty());
    // Swagger UI should be mounted with per-domain URLs
    assert!(
        server_file
            .content
            .contains("SwaggerUi::new(\"/swagger-ui\")"),
        "server.rs should mount SwaggerUi"
    );
    assert!(
        server_file.content.contains(".urls(vec!["),
        "server.rs should use .urls() for multi-spec dropdown"
    );
    assert!(
        server_file.content.contains("use utoipa::OpenApi"),
        "server.rs should import utoipa::OpenApi"
    );
    assert!(
        server_file
            .content
            .contains("api::openapi::all::AllApiDoc::openapi()"),
        "server.rs should reference AllApiDoc"
    );
    assert!(
        server_file
            .content
            .contains("api::openapi::recruiting::RecruitingApiDoc::openapi()"),
        "server.rs should reference per-domain RecruitingApiDoc"
    );
    assert!(
        server_file.content.contains("/api-catalog.json"),
        "server.rs should mount API catalog endpoint"
    );
    assert!(
        server_file.content.contains("init_tracing"),
        "server.rs should init tracing"
    );
    assert!(
        server_file.content.contains("/health"),
        "server.rs should have health endpoint"
    );
    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("Should have lib.rs");
    assert!(
        lib_file.content.contains("mod error"),
        "lib.rs should include error module"
    );

    assert!(
        server_file.content.contains("codegraph_workflow"),
        "server.rs should reference codegraph_workflow. Got:\n{}",
        server_file.content
    );

    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("Should have Cargo.toml");
    assert!(!cargo_file.content.is_empty());
}

#[tokio::test]
async fn scaffold_error_module() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-error");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
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

    let error_file = files.iter().find(|f| f.path.ends_with("error.rs"));
    assert!(error_file.is_some(), "Should generate error.rs");

    let content = &error_file.unwrap().content;
    assert!(content.contains("pub struct AppError"));
    assert!(content.contains("impl IntoResponse for AppError"));
    assert!(content.contains("fn not_found"));
    assert!(content.contains("fn internal"));
    assert!(
        content.contains("fn unauthorized"),
        "Should have unauthorized() method"
    );
    assert!(
        content.contains("fn forbidden"),
        "Should have forbidden() method"
    );
    assert!(
        content.contains("correlation_id"),
        "Should have correlation_id in response"
    );
    assert!(
        content.contains("FieldError"),
        "Should have FieldError struct for validation details"
    );
}

// === Pgmq Setup Tests (Global) ===

#[tokio::test]
async fn pgmq_setup_global() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-pgmq");

    let generator = generate::db::event_trigger::PgmqSetupGenerator::new(&output_dir);
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
    let content = &files[0].content;
    assert!(
        content.contains("CREATE EXTENSION IF NOT EXISTS pgmq"),
        "Should create pgmq extension. Got:\n{}",
        content
    );
    assert!(
        content.contains("emit_domain_event"),
        "Should contain emit_domain_event function. Got:\n{}",
        content
    );
    assert!(
        content.contains("pgmq.send"),
        "Should enqueue events via pgmq.send. Got:\n{}",
        content
    );
}

// === Platform Schema Tests (Global) ===

#[tokio::test]
async fn platform_schema_global() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-platform");

    let generator = generate::db::platform_schema::PlatformSchemaGenerator::new(&output_dir);
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
    let content = &files[0].content;
    assert!(
        content.contains("CREATE SCHEMA IF NOT EXISTS platform"),
        "Should create platform schema"
    );
    assert!(
        content.contains("platform.workflow_definition"),
        "Should contain workflow_definition table"
    );
    assert!(
        content.contains("platform.workflow_instance"),
        "Should contain workflow_instance table"
    );
    assert!(
        content.contains("platform.workflow_transition"),
        "Should contain workflow_transition table"
    );
    assert!(
        content.contains("platform.workflow_timer"),
        "Should contain workflow_timer table"
    );
    assert!(
        content.contains("platform.event_subscription"),
        "Should contain event_subscription table"
    );
    assert!(
        content.contains("platform.approval_step"),
        "Should contain approval_step table"
    );
    assert!(
        content.contains("platform.approval_decision"),
        "Should contain approval_decision table"
    );
    // RLS policies
    assert!(
        content.contains("ENABLE ROW LEVEL SECURITY"),
        "Should enable RLS"
    );
}

#[tokio::test]
async fn platform_schema_rls_consistency() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-platform-rls");

    let generator = generate::db::platform_schema::PlatformSchemaGenerator::new(&output_dir);
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
    let content = &files[0].content;

    // Verify RLS policies use get_current_org_id() instead of app.tenant_id
    assert!(
        content.contains("get_current_org_id()"),
        "RLS policies should use get_current_org_id(). Got:\n{}",
        content
    );

    assert!(
        !content.contains("app.tenant_id"),
        "RLS policies should NOT contain app.tenant_id. Got:\n{}",
        content
    );

    // Additional consistency checks for RLS policies
    assert!(
        content.contains("CREATE POLICY tenant_isolation_workflow_definition"),
        "Should have RLS policy for workflow_definition"
    );
    assert!(
        content.contains("CREATE POLICY tenant_isolation_workflow_instance"),
        "Should have RLS policy for workflow_instance"
    );
    assert!(
        content.contains("CREATE POLICY tenant_isolation_approval_step"),
        "Should have RLS policy for approval_step"
    );
}

// === Graceful Shutdown + OTel Flush Tests ===

#[tokio::test]
async fn scaffold_main_has_graceful_shutdown() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-shutdown");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
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

    let server_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("server.rs"))
        .expect("Should have server.rs");

    assert!(
        server_file.content.contains("with_graceful_shutdown"),
        "server.rs should use with_graceful_shutdown. Got:\n{}",
        server_file.content
    );
    assert!(
        server_file.content.contains("provider.shutdown()"),
        "server.rs should flush OTel provider on shutdown"
    );
}

// === Health Ready Endpoint Tests ===

#[tokio::test]
async fn scaffold_main_has_health_ready() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-health-ready");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
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

    let server_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("server.rs"))
        .expect("Should have server.rs");

    assert!(
        server_file.content.contains("/health/ready"),
        "server.rs should have /health/ready route. Got:\n{}",
        server_file.content
    );
    assert!(
        server_file.content.contains("health_ready"),
        "server.rs should have health_ready handler function"
    );
}

// === Full Generation Integration Test ===

#[tokio::test]
async fn full_generation_run() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = tempfile::TempDir::new().unwrap();

    // Use dedicated temp dirs for domain-types and hooks output to avoid
    // overwriting real workspace files with mock data.
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();
    let report = generate::run_generators_with_domain_types_base(
        &mock,
        &config,
        output_dir.path(),
        &tera,
        &Default::default(),
        &Default::default(),
        std::path::Path::new(""),
        domain_types_tmp.path(),
        hooks_tmp.path(),
    )
    .await
    .unwrap();

    assert!(
        !report.files.is_empty(),
        "Should write at least 1 file, got {}",
        report.files.len()
    );
    assert!(
        !report.has_errors(),
        "Should have no errors: {:?}",
        report
            .errors
            .iter()
            .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
            .collect::<Vec<_>>()
    );
}

// === Version Information Tests ===

#[tokio::test]
async fn scaffold_cargo_toml_has_shadow_rs() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-shadow");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
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

    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("Should have Cargo.toml");

    assert!(
        cargo_file.content.contains("shadow-rs"),
        "Cargo.toml should have shadow-rs dependency"
    );
    assert!(
        cargo_file.content.contains("[build-dependencies]"),
        "Cargo.toml should have [build-dependencies] section"
    );
}

#[tokio::test]
async fn scaffold_generates_build_rs() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-build-rs");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
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

    let build_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("build.rs"))
        .expect("Should generate build.rs");

    assert!(
        build_rs.content.contains("ShadowBuilder::builder()"),
        "build.rs should invoke ShadowBuilder::builder()"
    );
}

#[tokio::test]
async fn scaffold_main_has_version_endpoint() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-version");

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
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

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("main.rs"))
        .expect("Should have main.rs");

    // shadow-rs macro invocation — still in main.rs
    assert!(
        main_file.content.contains("shadow!(build)"),
        "main.rs should invoke shadow!(build) macro"
    );

    let server_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("server.rs"))
        .expect("Should have server.rs");

    // VersionInfo struct with ToSchema for OpenAPI
    assert!(
        server_file.content.contains("pub struct VersionInfo"),
        "server.rs should define VersionInfo struct"
    );
    assert!(
        server_file.content.contains("utoipa::ToSchema"),
        "VersionInfo should derive ToSchema for OpenAPI"
    );

    // Version handler with utoipa path annotation
    assert!(
        server_file.content.contains("async fn version()"),
        "server.rs should have version handler"
    );
    assert!(
        server_file.content.contains("tag = \"System\""),
        "version endpoint should be tagged under System"
    );

    // Route registration
    assert!(
        server_file.content.contains("\"/version\""),
        "server.rs should register /version route"
    );

    // Key shadow-rs constants used
    assert!(
        server_file.content.contains("build::SHORT_COMMIT"),
        "version handler should use SHORT_COMMIT"
    );
    assert!(
        server_file.content.contains("build::BUILD_TIME_3339"),
        "version handler should use BUILD_TIME_3339"
    );
}

/// Verify that scaffold Cargo.toml uses sqlx-sqlite when database_target is "sqlite".
#[tokio::test]
async fn scaffold_cargo_toml_with_sqlite_dialect() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-sqlite-scaffold");
    let project = sqlite_project_config();

    let generator = generate::scaffold::generator::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
    let files = generator
        .generate(&mock, &config, &test_generation_order(), &tera, &project)
        .await
        .unwrap();

    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("Should generate Cargo.toml");

    // With SQLite dialect, should use sqlx-sqlite feature
    assert!(
        cargo_file.content.contains("sqlx-sqlite"),
        "SQLite scaffold Cargo.toml should use sqlx-sqlite feature. Got:\n{}",
        cargo_file.content
    );

    // Must NOT use sqlx-postgres
    assert!(
        !cargo_file.content.contains("sqlx-postgres"),
        "SQLite scaffold Cargo.toml must NOT use sqlx-postgres. Got:\n{}",
        cargo_file.content
    );
}
