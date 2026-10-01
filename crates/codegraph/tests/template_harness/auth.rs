use crate::harness::{
    setup_mock, test_domain_config, test_generation_order, test_project_config, test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};

#[tokio::test]
async fn scaffold_middleware_supports_dual_auth() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-dual-auth");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(
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
    let files = gen
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let middleware_file = files
        .iter()
        .find(|f| f.path.ends_with("middleware/mod.rs"))
        .expect("Should generate middleware/mod.rs");
    let content = &middleware_file.content;

    assert!(
        content.contains("verify_jwt"),
        "Middleware should have verify_jwt function"
    );
    assert!(
        content.contains("verify_api_key"),
        "Middleware should have verify_api_key function"
    );
    assert!(
        content.contains("AuthMode"),
        "Middleware should define AuthMode enum"
    );
    assert!(
        content.contains("auth_middleware"),
        "Middleware should define auth_middleware function"
    );
    assert!(
        content.contains("sk_"),
        "Middleware should check for sk_ prefix to route API key auth"
    );
    assert!(
        content.contains("DecodingKey::from_secret"),
        "Middleware should verify JWT with HMAC-SHA256 via jsonwebtoken crate"
    );
    assert!(
        content.contains("Algorithm::HS256"),
        "Middleware should use HS256 algorithm"
    );
    assert!(
        content.contains("set_audience"),
        "Middleware should validate JWT audience"
    );
    assert!(
        content.contains("validate_exp"),
        "Middleware should validate JWT expiration"
    );
    assert!(
        content.contains("State(state): State<AppState>"),
        "Middleware should take AppState (not raw DatabaseConnection)"
    );
    assert!(
        content.contains("jwt_secret"),
        "Middleware should use jwt_secret from AppState"
    );
    assert!(
        !content.contains("base64::engine"),
        "Middleware must NOT use raw base64 decode — use jsonwebtoken crate"
    );

    // Verify cargo_toml has jsonwebtoken dependency
    let cargo_file = files
        .iter()
        .find(|f| f.path.ends_with("Cargo.toml"))
        .expect("Should generate Cargo.toml");
    assert!(
        cargo_file.content.contains("jsonwebtoken"),
        "Cargo.toml should include jsonwebtoken dependency"
    );

    // Verify app_state has jwt_secret field
    let app_state_file = files
        .iter()
        .find(|f| f.path.ends_with("app_state.rs"))
        .expect("Should generate app_state.rs");
    assert!(
        app_state_file.content.contains("jwt_secret: String"),
        "AppState should have jwt_secret: String field"
    );
}

// === app_user pool plumbing (#169 Phase 2) ===

#[tokio::test]
async fn scaffold_wires_app_user_pool_and_mode() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-app-user-pool");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(
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
    let files = gen
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    // server.rs: the app pool connects from APP_DATABASE_URL and falls back
    // to the owner DATABASE_URL in legacy mode; boot migrations still run on
    // the owner connection.
    let server_file = files
        .iter()
        .find(|f| f.path.ends_with("src/server.rs"))
        .expect("Should generate src/server.rs");
    let server = &server_file.content;
    assert!(
        server.contains("APP_DATABASE_URL"),
        "server.rs should read APP_DATABASE_URL for the app_user pool"
    );
    assert!(
        server.contains("DbPoolMode::AppUser"),
        "server.rs should select DbPoolMode::AppUser when APP_DATABASE_URL is set"
    );
    assert!(
        server.contains("DbPoolMode::Legacy"),
        "server.rs should fall back to DbPoolMode::Legacy without APP_DATABASE_URL"
    );
    assert!(
        server.contains("sea_orm::Database::connect(&url)"),
        "the app pool must connect from the APP_DATABASE_URL value"
    );
    assert!(
        server.contains("pool_mode,"),
        "AppState must be constructed with the pool mode"
    );

    // app_state.rs: DbPoolMode enum + AppState field.
    let app_state_file = files
        .iter()
        .find(|f| f.path.ends_with("src/app_state.rs"))
        .expect("Should generate src/app_state.rs");
    let app_state = &app_state_file.content;
    assert!(
        app_state.contains("pub enum DbPoolMode"),
        "app_state.rs should define DbPoolMode"
    );
    assert!(
        app_state.contains("pub pool_mode: DbPoolMode"),
        "AppState should expose the pool mode to the data path"
    );

    // doctor.rs (admin-CLI builds only): warn when APP_DATABASE_URL is unset
    // on a postgres target.
    let gen_admin = generate::scaffold::gen::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        true, // has_admin_cli — emits config.rs / doctor.rs / migration.rs
        false,
        "sea-orm",
    );
    let admin_files = gen_admin
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();
    let doctor_file = admin_files
        .iter()
        .find(|f| f.path.ends_with("src/doctor.rs"))
        .expect("Should generate src/doctor.rs");
    let doctor = &doctor_file.content;
    assert!(
        doctor.contains("APP_DATABASE_URL"),
        "doctor.rs should check APP_DATABASE_URL"
    );
    assert!(
        doctor.contains("check_app_pool_mode"),
        "doctor.rs should run the app_pool_mode check"
    );
    assert!(
        doctor.contains("CheckStatus::Warn"),
        "the unset APP_DATABASE_URL case must be a warning, not a failure"
    );
}

#[tokio::test]
async fn scaffold_generates_middleware() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-mw");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(
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
    let files = gen
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let middleware_file = files.iter().find(|f| f.path.ends_with("middleware/mod.rs"));
    assert!(
        middleware_file.is_some(),
        "Scaffold should generate middleware/mod.rs"
    );
    let content = &middleware_file.unwrap().content;
    assert!(
        content.contains("verify_api_key"),
        "Middleware should call verify_api_key"
    );
    assert!(
        content.contains("ApiKeyInfo"),
        "Middleware should define ApiKeyInfo"
    );
}

#[tokio::test]
async fn test_permission_middleware_generated() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-permission-mw");

    // has_atproto = true: the permission middleware (with extract_uuid_from_path)
    // lives in the atproto branch of the template.
    let gen = generate::scaffold::gen::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
    let files = gen
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let permission_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("permission.rs"))
        .expect("Scaffold should generate middleware/permission.rs");

    assert!(
        permission_file.content.contains("require_permission"),
        "Permission middleware should contain require_permission function. Got:\n{}",
        permission_file.content
    );
    assert!(
        permission_file.content.contains("RequiredPermission"),
        "Permission middleware should contain RequiredPermission struct. Got:\n{}",
        permission_file.content
    );
    assert!(
        permission_file.content.contains("extract_uuid_from_path"),
        "Permission middleware should contain extract_uuid_from_path helper. Got:\n{}",
        permission_file.content
    );
    assert!(
        permission_file.content.contains("AuthorizationService"),
        "Permission middleware should delegate to the registered AuthorizationService. Got:\n{}",
        permission_file.content
    );
    assert!(
        !permission_file.content.contains("has_permission"),
        "Permission middleware should not contain the old has_permission stub. Got:\n{}",
        permission_file.content
    );
}

/// The TEST_MODE persona DID validation must use rsky_syntax when the build has
/// AT Protocol enabled, and a lightweight prefix check otherwise.
#[tokio::test]
async fn middleware_test_mode_did_validation_uses_rsky_when_atproto() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-middleware-atproto");

    // has_atproto = true → rsky_syntax branch
    let gen = generate::scaffold::gen::ScaffoldGenerator::new(
        &output_dir,
        false,
        false,
        false,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        "sea-orm",
    );
    let files = gen
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let middleware_file = files
        .iter()
        .find(|f| f.path.ends_with("middleware/mod.rs"))
        .expect("Should generate middleware/mod.rs");
    let content = &middleware_file.content;

    // With atproto enabled the generated module re-exports the shared
    // vocabulary from cosmos-extensions; DID validation (rsky_syntax) and the
    // test-mode persona tokens live in that hand-written crate, not here.
    assert!(
        content.contains("cosmos_extensions::http::middleware"),
        "With atproto enabled, middleware should re-export cosmos-extensions auth types. Got:\n{content}"
    );
}

/// Entities with `permissions.scope` set must get the permission layers + a
/// per-module permission helper in the generated router.
#[tokio::test]
async fn router_permission_gated_emits_layers_and_helper() {
    generate::type_registry::register_framework_types();
    let mock = setup_mock().await;
    let mut config = test_domain_config();
    let recruiting = config
        .domains
        .get_mut("recruiting")
        .expect("recruiting domain exists");
    let candidate_cfg = recruiting
        .entity_config
        .get_mut("CandidateType")
        .expect("CandidateType entity config exists");
    candidate_cfg.permissions.scope = Some("support:support-plan".to_string());
    candidate_cfg.permissions.record_scoped = true;

    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-permission-router");

    let gen = generate::api::router::RouterGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "recruiting",
            &["CandidateType".to_string()],
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let content = files
        .iter()
        .map(|f| f.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        content.contains(
            ".layer(axum::middleware::from_fn(crate::middleware::permission::require_permission))"
        ),
        "Router should add require_permission layer. Got:\n{content}"
    );
    assert!(
        content.contains(".layer(axum::middleware::from_fn(candidate_permission))"),
        "Router should add candidate_permission layer as the outer layer. Got:\n{content}"
    );
    assert!(
        content.contains("async fn candidate_permission("),
        "Router should emit the per-module permission helper. Got:\n{content}"
    );
    assert!(
        content.contains("support:support-plan"),
        "Permission helper should embed the configured scope. Got:\n{content}"
    );
    assert!(
        content.contains("resource: scope.to_string()"),
        "Permission helper must build a RequiredPermission from the configured scope. Got:\n{content}"
    );
    // Backward compat: an entity WITHOUT permissions must NOT get the layers.
    let mut plain_config = test_domain_config();
    if let Some(recruiting) = plain_config.domains.get_mut("recruiting") {
        if let Some(cfg) = recruiting.entity_config.get_mut("CandidateType") {
            cfg.permissions.scope = None;
            cfg.permissions.record_scoped = false;
        }
    }
    let plain = generate::api::router::RouterGenerator::new(&output_dir);
    let files = plain
        .generate(
            &mock,
            "recruiting",
            &["CandidateType".to_string()],
            &plain_config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();
    let plain_content = files
        .iter()
        .map(|f| f.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !plain_content.contains("candidate_permission"),
        "Non-gated entities must not emit permission layers. Got:\n{plain_content}"
    );
}

// === Security: HTTP Middleware Tests ===

#[tokio::test]
async fn scaffold_main_has_security_middleware() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-security");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(
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
    let files = gen
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

    // CORS
    assert!(
        server_file.content.contains("CorsLayer"),
        "server.rs should use CorsLayer. Got:\n{}",
        server_file.content
    );
    // Request body limit
    assert!(
        server_file.content.contains("RequestBodyLimitLayer"),
        "server.rs should use RequestBodyLimitLayer"
    );
    // Security headers (lowercase in HeaderName::from_static)
    assert!(
        server_file.content.contains("x-content-type-options"),
        "server.rs should set X-Content-Type-Options header"
    );
    assert!(
        server_file.content.contains("x-frame-options"),
        "server.rs should set X-Frame-Options header"
    );
    // HSTS (conditionally enabled)
    assert!(
        server_file.content.contains("strict-transport-security"),
        "server.rs should support HSTS header"
    );
    assert!(
        server_file.content.contains("HSTS_ENABLED"),
        "HSTS should be gated on HSTS_ENABLED env var"
    );
    // DATABASE_URL must be required (no fallback)
    assert!(
        server_file.content.contains("DATABASE_URL")
            && !server_file
                .content
                .contains("unwrap_or_else(|_| \"postgres://localhost"),
        "server.rs should require DATABASE_URL (no fallback). Got:\n{}",
        server_file.content
    );
    assert!(
        !server_file
            .content
            .contains("unwrap_or_else(|_| \"postgres://localhost"),
        "server.rs must NOT have insecure DATABASE_URL fallback"
    );

    // Cargo.toml should have tower-http
    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("Should have Cargo.toml");
    assert!(
        cargo_file.content.contains("tower-http"),
        "Cargo.toml should include tower-http dependency. Got:\n{}",
        cargo_file.content
    );
}
