//! Test harness for template output correctness.
//!
//! Uses MockEngine from hr-graph-core to ingest schema data, then runs
//! generators against it and asserts template output correctness
//! per-schema, per-template.

use codegraph::generate;
use codegraph::generate::db::basejump_setup::BasejumpSetupGenerator;
use codegraph::generate::db::codelist::CodelistGenerator;
use codegraph::generate::db::dialect::{dialect_for_target, DatabaseTarget};
use codegraph::generate::template_engine;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph::generate::GenerationEntry;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{EnumValue, PropertyNode, SchemaNode};
use std::path::Path;

fn test_domain_config() -> codegraph_config::DomainConfig {
    codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml")).unwrap()
}

fn test_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    template_engine::create_tera(&template_dir).unwrap()
}

fn test_generation_order() -> Vec<GenerationEntry> {
    vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }]
}

fn test_project_config() -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig::default()
}

fn sqlite_project_config() -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig {
        database_target: "sqlite".to_string(),
        ..Default::default()
    }
}

fn gender_codelist_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "common/json/codelist/GenderCodeList.json".to_string(),
        title: "GenderCodeList".to_string(),
        description: Some("Gender codes".to_string()),
        schema_type: "object".to_string(),
        classification: "codelist".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/codelist/GenderCodeList.json".to_string(),
        pg_type: "TEXT".to_string(),
        rust_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        rust_type_name: "GenderCode".to_string(),
        pg_table_name: "gender_code".to_string(),
        api_path_segment: String::new(),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn candidate_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "recruiting/json/CandidateType.json".to_string(),
        title: "CandidateType".to_string(),
        description: Some("A person requesting consideration for a position".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("recruiting".to_string()),
        rel_path: "recruiting/json/CandidateType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Candidate".to_string(),
        pg_table_name: "candidate".to_string(),
        api_path_segment: "candidates".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: true,
        has_one_of: false,
        has_any_of: false,
        has_definitions: true,
    }
}

fn candidate_properties() -> Vec<PropertyNode> {
    vec![
        PropertyNode {
            name: "givenName".to_string(),
            prop_type: "string".to_string(),
            description: Some("The person's given name".to_string()),
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "given_name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "given_name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "familyName".to_string(),
            prop_type: "string".to_string(),
            description: Some("The person's family name".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "family_name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "family_name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
    ]
}

async fn setup_mock() -> MockEngine {
    let engine = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties("CandidateType", candidate_properties())
        .build();
    engine
}

// === DDL Template Tests ===

#[tokio::test]
async fn candidate_ddl_table() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-ddl");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "DDL generator should produce files");

    let table_file = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("recruiting_candidate.sql")
        })
        .expect("Should have a table SQL file");

    assert!(
        table_file
            .content
            .contains("CREATE TABLE IF NOT EXISTS recruiting.candidate"),
        "Should contain CREATE TABLE. Got:\n{}",
        table_file.content
    );
    assert!(
        table_file.content.contains("given_name TEXT NOT NULL"),
        "Should contain given_name column. Got:\n{}",
        table_file.content
    );
    assert!(
        table_file.content.contains("family_name TEXT"),
        "Should contain family_name column. Got:\n{}",
        table_file.content
    );
    // Tenant column should be present (non-global entity)
    assert!(
        table_file
            .content
            .contains("platform_organization_id UUID NOT NULL DEFAULT"),
        "Should contain platform_organization_id column. Got:\n{}",
        table_file.content
    );
    // Timestamps
    assert!(
        table_file.content.contains("created_at TIMESTAMPTZ"),
        "Should contain created_at"
    );
    assert!(
        table_file.content.contains("updated_at TIMESTAMPTZ"),
        "Should contain updated_at"
    );
}

#[tokio::test]
async fn candidate_ddl_trigger() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-trigger");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let trigger_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("trigger"))
        .expect("Should have a trigger SQL file");

    assert!(
        !trigger_file.content.is_empty(),
        "Trigger file should not be empty"
    );
}

#[tokio::test]
async fn candidate_ddl_rls() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-rls");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let rls_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("_rls.sql"))
        .expect("Should have an RLS SQL file");

    assert!(!rls_file.content.is_empty(), "RLS file should not be empty");
    assert!(
        rls_file.content.contains("get_current_org_id()"),
        "RLS should use unified get_current_org_id(). Got:\n{}",
        rls_file.content
    );
    assert!(
        rls_file.content.contains("FORCE ROW LEVEL SECURITY"),
        "RLS should force even for table owner"
    );
    assert!(
        rls_file.content.contains("org_isolation_select"),
        "RLS should have org_isolation_select policy"
    );
}

#[tokio::test]
async fn candidate_ddl_rls_has_authenticated_policies() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-rls-auth");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let rls_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("_rls.sql"))
        .expect("Should have an RLS SQL file");

    // Unified RLS uses get_current_org_id() for all auth modes (no role-specific policies)
    assert!(
        rls_file.content.contains("get_current_org_id()"),
        "RLS should use unified get_current_org_id() for all auth modes. Got:\n{}",
        rls_file.content
    );
    assert!(
        !rls_file.content.contains("TO authenticated"),
        "Unified RLS should NOT have role-specific policies"
    );
    // Unified policies don't use auth.uid() for org isolation
    assert!(
        !rls_file.content.contains("auth.uid()"),
        "Unified RLS should NOT use auth.uid() (uses get_current_org_id instead)"
    );
    // Auditable entities should have API key scope-aware policies
    assert!(
        rls_file.content.contains("TO api_key"),
        "Auditable entities should have api_key scope-aware policies"
    );
    assert!(
        rls_file.content.contains("check_api_key_scope"),
        "API key policies should use check_api_key_scope()"
    );
    assert!(
        rls_file.content.contains("org_isolation_delete"),
        "RLS should have all four CRUD policies"
    );
}

/// This test verifies that DdlGenerator with SQLite dialect uses SQLite-compatible
/// types (TEXT for UUID, TEXT for TIMESTAMPTZ) instead of PostgreSQL types.
/// Before the dialect wiring fix, this test FAILS because the generator ignores the
/// dialect and emits PG types (UUID, TIMESTAMPTZ, gen_random_uuid()).
/// After the fix, it should PASS with SQLite types.
#[tokio::test]
async fn ddl_with_sqlite_dialect_uses_sqlite_types() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-sqlite-ddl");

    // Create DdlGenerator with SQLite dialect
    let gen = generate::db::ddl::DdlGenerator::new(&output_dir)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));

    let project = test_project_config();

    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &project)
        .await
        .unwrap();

    let table_file = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("recruiting_candidate.sql")
        })
        .expect("Should have a table SQL file");

    // With SQLite dialect, the id column should be TEXT not UUID
    assert!(
        table_file.content.contains("id TEXT"),
        "SQLite DDL should use TEXT for id column. Got:\n{}",
        table_file.content
    );
    // With SQLite dialect, timestamps should be TEXT not TIMESTAMPTZ
    assert!(
        table_file.content.contains("created_at TEXT"),
        "SQLite DDL should use TEXT for created_at. Got:\n{}",
        table_file.content
    );
    // With SQLite dialect, gen_random_uuid() should not appear (UUIDs are client-generated)
    assert!(
        !table_file.content.contains("gen_random_uuid"),
        "SQLite DDL should not contain gen_random_uuid(). Got:\n{}",
        table_file.content
    );
}

#[tokio::test]
async fn scaffold_middleware_supports_dual_auth() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-dual-auth");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    let middleware_file = files
        .iter()
        .find(|f| f.path.ends_with("middleware.rs"))
        .expect("Should generate middleware.rs");
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

// === Entity Model Template Tests ===

#[tokio::test]
async fn candidate_entity_model() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-entity");

    let gen = generate::db::entity::SeaOrmEntityGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("DeriveEntityModel"));
    assert!(files[0].content.contains("given_name"));
    assert!(files[0].content.contains("family_name"));
}

// === DTO Template Tests ===

#[tokio::test]
async fn candidate_dto_create() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();

    // App DTOs are now re-exports; verify the re-export references the correct type
    let app_output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-dto");
    let app_gen = generate::ddd::dto::DtoGenerator::new(&app_output_dir);
    let app_files = app_gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let app_create = app_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("Should have a create DTO file");
    assert!(
        app_create
            .content
            .contains("pub use hr_domain_types::recruiting::candidate::CreateCandidateRequest"),
        "App DTO should re-export from hr_domain_types. Got:\n{}",
        app_create.content
    );

    // Verify struct content in domain_types output
    let tmp = std::env::temp_dir().join("hr-graph-test-harness-dto-domain");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dt_gen = generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dt_files = dt_gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let dt_create = dt_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("Should have a domain_types create DTO file");
    assert!(
        dt_create.content.contains("CreateCandidateRequest"),
        "Domain types DTO should contain CreateCandidateRequest struct"
    );
    assert!(
        dt_create.content.contains("garde::Validate"),
        "Domain types DTO should derive garde::Validate. Got:\n{}",
        dt_create.content
    );
    assert!(
        dt_create.content.contains("given_name"),
        "Should contain given_name field"
    );
}

#[tokio::test]
async fn dto_create_template_omits_garde_when_disabled() {
    let tera = test_tera();
    let project = codegraph::generate::ProjectConfig::default();

    let ctx = codegraph::generate::ddd::dto::DtoContext {
        module_name: "test_entity".to_string(),
        entity_name: "TestEntity".to_string(),
        domain: "test".to_string(),
        fields: vec![
            codegraph::generate::ddd::dto::DtoField {
                name: "name".to_string(),
                rust_type: "String".to_string(),
                is_required: true,
                is_array: false,
                description: String::new(),
                render_strategy: "direct_column".to_string(),
                is_entity_ref: false,
                is_hierarchy_field: false,
                min_length: Some(2),
                max_length: Some(100),
                minimum: None,
                maximum: None,
                pattern: None,
                format: None,
            },
            codegraph::generate::ddd::dto::DtoField {
                name: "email".to_string(),
                rust_type: "String".to_string(),
                is_required: false,
                is_array: false,
                description: String::new(),
                render_strategy: "direct_column".to_string(),
                is_entity_ref: false,
                is_hierarchy_field: false,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pattern: None,
                format: Some("email".to_string()),
            },
            codegraph::generate::ddd::dto::DtoField {
                name: "parent".to_string(),
                rust_type: "Uuid".to_string(),
                is_required: false,
                is_array: false,
                description: String::new(),
                render_strategy: "entity_ref".to_string(),
                is_entity_ref: true,
                is_hierarchy_field: false,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pattern: None,
                format: None,
            },
        ],
        immutable_fields: vec![],
        workflow_excluded_fields: vec![],
        list_exclude: vec![],
        list_include: vec![],
        has_list_fields: false,
        operations: vec!["create".to_string(), "update".to_string()],
        child_dtos: vec![],
        all_child_dtos: vec![],
        codelist_imports: vec![],
        codelist_imports_update: vec![],
        has_workflow: false,
        has_approval_status: false,
        structured_imports: vec![],
        has_validate: false,
    };

    let create = codegraph::generate::render_template_with_project(
        &tera,
        "domain_types/dto_create.tera",
        &ctx,
        &project,
    )
    .unwrap();
    assert!(
        !create.contains("garde::Validate"),
        "Should NOT derive garde::Validate when disabled"
    );
    assert!(
        !create.contains("#[garde("),
        "Should NOT have garde attributes when disabled"
    );

    let update = codegraph::generate::render_template_with_project(
        &tera,
        "domain_types/dto_update.tera",
        &ctx,
        &project,
    )
    .unwrap();
    assert!(
        !update.contains("garde::Validate"),
        "Update DTO should NOT derive garde::Validate when disabled"
    );
    assert!(
        !update.contains("#[garde("),
        "Update DTO should NOT have garde attributes when disabled"
    );
}

#[tokio::test]
async fn candidate_dto_response() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();

    // App DTOs are re-exports; verify the re-export
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-dto-resp");
    let gen = generate::ddd::dto::DtoGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let response_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .expect("Should have a response DTO file");
    assert!(
        response_file.content.contains("CandidateResponse"),
        "Should contain CandidateResponse re-export"
    );

    // Verify struct content in domain_types output
    let tmp = std::env::temp_dir().join("hr-graph-test-harness-dto-resp-domain");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dt_gen = generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dt_files = dt_gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let dt_response = dt_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .expect("Should have a domain_types response DTO file");
    assert!(
        dt_response.content.contains("pub struct CandidateResponse"),
        "Domain types should contain CandidateResponse struct"
    );
}

// === Command Template Tests ===

#[tokio::test]
async fn candidate_command() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd");

    let gen = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("Candidate"),
        "Should reference Candidate entity"
    );
}

// === Query Template Tests ===

#[tokio::test]
async fn candidate_query() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-query");

    let gen = generate::ddd::query::QueryGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("Candidate"),
        "Should reference Candidate entity"
    );
}

// === Event Template Tests ===

#[tokio::test]
async fn candidate_event() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-event");

    let gen = generate::ddd::event::EventGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("Candidate"),
        "Should reference Candidate entity"
    );
}

// === Repository Template Tests ===

#[tokio::test]
async fn candidate_repository() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-repo");

    let gen = generate::ddd::repository::RepositoryTraitGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(files.len() >= 2, "Should have trait + impl files");

    let trait_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository.rs"))
        .expect("Should have repository trait file");
    assert!(trait_file.content.contains("CandidateRepository"));

    let impl_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository_impl.rs"))
        .expect("Should have repository impl file");
    assert!(impl_file.content.contains("CandidateRepositoryImpl"));
    assert!(impl_file.content.contains("async fn create"));
    assert!(impl_file.content.contains("async fn find_by_id"));
    assert!(impl_file.content.contains("async fn update"));
    // CandidateType operations = ["create", "read", "update", "list"] — no delete
    assert!(!impl_file.content.contains("async fn delete"));
    assert!(impl_file.content.contains("async fn list"));
}

// === Handler Template Tests ===

#[tokio::test]
async fn candidate_handler() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-handler");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("Candidate"),
        "Should reference Candidate entity"
    );
    // Check file path is correct
    assert!(
        files[0]
            .path
            .to_string_lossy()
            .contains("candidate_handler.rs"),
        "File should be named candidate_handler.rs"
    );
    // ListParams should derive IntoParams for Swagger UI
    assert!(
        content.contains("utoipa::IntoParams"),
        "ListParams should derive utoipa::IntoParams"
    );
    // list handler should reference ListParams in utoipa params
    assert!(
        content.contains("params(ListParams)"),
        "list handler should use params(ListParams)"
    );
    // Handler should use AppError, not StatusCode errors
    assert!(
        content.contains("AppError"),
        "Handler should use AppError, not StatusCode errors"
    );
    assert!(
        content.contains("#[tracing::instrument"),
        "Handler should have instrument attribute"
    );
    assert!(
        content.contains("use crate::error::AppError"),
        "Handler should import AppError"
    );
    // Bulk create: untagged enum dispatch
    assert!(
        content.contains("#[serde(untagged)]"),
        "Handler should use untagged enum for single/bulk dispatch"
    );
    assert!(
        content.contains("StatusCode::MULTI_STATUS"),
        "Bulk create should return 207 Multi-Status"
    );
    // Bulk create: entity-namespaced OpenAPI schema to avoid collision across entities
    assert!(
        content.contains("CandidateBulkCreateResponse"),
        "BulkCreateResponse should be renamed with entity prefix to avoid schema collision"
    );
    // Bulk create: 207 response includes correlation_id for tracing
    assert!(
        content.contains("correlation_id: correlation_id.to_string()"),
        "207 response should include correlation_id"
    );
    // Bulk create: uses crate BulkItemError, not a local duplicate type
    assert!(
        content.contains("use crate::error::BulkItemError"),
        "Handler should import BulkItemError from crate::error, not define a local duplicate"
    );
    // Bulk create: max_bulk_size rejection must not use format! with no args (clippy::useless_format)
    assert!(
        !content.contains("format!(\"Bulk request exceeds"),
        "Bulk size rejection message should be a string literal, not format!()"
    );
}

// === Router Template Tests (Domain-level) ===

#[tokio::test]
async fn recruiting_router() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-router");

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

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("candidates"),
        "Router should include candidates path segment"
    );
}

/// Test that child entities are nested under their parent in the router.
#[tokio::test]
async fn router_nests_child_under_parent() {
    // Create a parent entity (Compensation) and child entity (Reward) in the mock
    let parent_schema = SchemaNode {
        schema_id: "compensation/json/CompensationType.json".to_string(),
        title: "CompensationType".to_string(),
        description: Some("Compensation package".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/CompensationType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Compensation".to_string(),
        pg_table_name: "compensation".to_string(),
        api_path_segment: "compensation".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };
    let child_schema = SchemaNode {
        schema_id: "compensation/json/RewardType.json".to_string(),
        title: "RewardType".to_string(),
        description: Some("A reward within compensation".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/RewardType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Reward".to_string(),
        pg_table_name: "reward".to_string(),
        api_path_segment: "reward".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let mock = MockEngine::builder()
        .with_schema(parent_schema)
        .with_schema(child_schema)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-nested-router");

    let parent_candidates = vec![codegraph_core::types::ParentCandidate {
        child_title: "RewardType".to_string(),
        parent_title: "CompensationType".to_string(),
        field_name: "compensation_type_id".to_string(),
        source: codegraph_core::types::DetectionSource::ScalarRef,
    }];

    let gen = generate::api::router::RouterGenerator::new(&output_dir)
        .with_parent_candidates(parent_candidates);
    let files = gen
        .generate(
            &mock,
            "compensation",
            &["CompensationType".to_string(), "RewardType".to_string()],
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;

    // Root entity should be at top level
    assert!(
        content.contains(".nest(\"/compensation\", compensation_routes())"),
        "Root entity should mount at top level. Got:\n{content}"
    );

    // Child should NOT be at top level
    assert!(
        !content.contains(".nest(\"/reward\", reward_routes())"),
        "Child entity should NOT mount at top level. Got:\n{content}"
    );

    // Child should be nested under parent
    assert!(
        content.contains("/{compensation_id}/reward"),
        "Child should be nested under parent with /{{compensation_id}}/reward. Got:\n{content}"
    );
}

/// Test that entities with no relationships render as root (backwards compatible).
#[tokio::test]
async fn router_no_relationships_renders_flat() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-flat-router");

    // No parent_candidates — all entities should render as root
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

    let content = &files[0].content;
    assert!(
        content.contains(".nest(\"/candidates\", candidate_routes())"),
        "Entity with no relationships should mount at top level"
    );
}

// === Handler Template Tests (Child Entity) ===

/// Build a mock with Compensation (parent) + Reward (child) for handler tests.
fn parent_child_mock() -> (MockEngine, Vec<codegraph_core::types::ParentCandidate>) {
    let parent_schema = SchemaNode {
        schema_id: "compensation/json/CompensationType.json".to_string(),
        title: "CompensationType".to_string(),
        description: Some("Compensation package".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/CompensationType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Compensation".to_string(),
        pg_table_name: "compensation".to_string(),
        api_path_segment: "compensation".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };
    let child_schema = SchemaNode {
        schema_id: "compensation/json/RewardType.json".to_string(),
        title: "RewardType".to_string(),
        description: Some("A reward within compensation".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/RewardType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Reward".to_string(),
        pg_table_name: "reward".to_string(),
        api_path_segment: "reward".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let candidates = vec![codegraph_core::types::ParentCandidate {
        child_title: "RewardType".to_string(),
        parent_title: "CompensationType".to_string(),
        field_name: "compensationType".to_string(),
        source: codegraph_core::types::DetectionSource::ScalarRef,
    }];

    // The child entity needs an EntityReference property whose rust_field_name
    // generates a FK column matching the inferred parent_ref (compensation_type_id).
    let child_fk_property = PropertyNode {
        name: "compensationType".to_string(),
        prop_type: "object".to_string(),
        description: Some("FK to parent compensation".to_string()),
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "compensation_type_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "compensation_type".to_string(),
        rust_field_type: "Option<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("CompensationType".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: Some(codegraph_type_contracts::RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    };

    let mock = MockEngine::builder()
        .with_schema(parent_schema)
        .with_schema(child_schema)
        .with_properties("RewardType", vec![child_fk_property])
        .build();

    (mock, candidates)
}

/// Child handler must use list_filtered for ownership checks, not DTO field access.
/// Regression test: Response DTOs don't expose FK columns, so `response.{fk}` causes E0609.
#[tokio::test]
async fn child_handler_uses_find_by_id_scoped_for_ownership() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-handler-ownership");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "Handler generator should produce a file");
    let content = &files[0].content;

    // Must NOT access FK on response DTO (would cause compile error)
    assert!(
        !content.contains("response.compensation"),
        "Child handler must not access FK field on Response DTO. Got:\n{content}"
    );
    assert!(
        !content.contains("_existing.compensation"),
        "Child handler must not access FK field on _existing DTO. Got:\n{content}"
    );

    // Must NOT set FK on Create DTO (would cause compile error — FK only exists on SeaORM entity)
    assert!(
        !content.contains("item.compensation_type_id"),
        "Child handler must not set FK field on Create DTO. Got:\n{content}"
    );

    // Must use find_by_id_scoped for ownership checks on get/update/delete
    assert!(
        content.contains("find_by_id_scoped"),
        "Child handler must use find_by_id_scoped for ownership checks. Got:\n{content}"
    );

    // Must pass parent_id to command.create for child entities
    assert!(
        content.contains("commands.create(item, parent_id,"),
        "Child handler must pass parent_id to command.create. Got:\n{content}"
    );
}

/// Child handler must derive parent_ref from ParentCandidate.field_name (not leave it blank).
/// Regression test: blank parent_ref emitted `item. = Some(parent_id)` — invalid Rust.
#[tokio::test]
async fn child_handler_derives_parent_ref_from_graph() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-handler-parent-ref");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let content = &files[0].content;

    // The handler should pass parent_id to the command layer — not set FK on DTO
    assert!(
        content.contains("parent_id"),
        "Handler should reference parent_id for child entity. Got:\n{content}"
    );

    // Should contain find_by_id_scoped (uses derived FK column internally at repo layer)
    assert!(
        content.contains("find_by_id_scoped"),
        "Handler should use find_by_id_scoped for child entity ownership. Got:\n{content}"
    );
}

/// Child handler must retain tag annotation in all utoipa path blocks.
/// Regression test: restructuring for role=="child" dropped tag from get/update/delete/list.
#[tokio::test]
async fn child_handler_retains_utoipa_tags() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-handler-tags");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let content = &files[0].content;

    // Count tag annotations — should match the number of utoipa::path blocks
    let tag_count = content.matches("tag = \"").count();
    let utoipa_count = content.matches("#[utoipa::path(").count();
    assert!(
        tag_count == utoipa_count,
        "Every utoipa::path block must have a tag annotation. Found {tag_count} tags for {utoipa_count} endpoints."
    );
}

/// ArrayItems detection should derive FK from parent type name, not array property name.
/// Regression test: `rewards` (array prop) → `rewards_id` instead of `compensation_id`.
#[tokio::test]
async fn array_items_fk_uses_parent_type_name() {
    let parent_schema = SchemaNode {
        schema_id: "compensation/json/CompensationType.json".to_string(),
        title: "CompensationType".to_string(),
        description: Some("Compensation package".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/CompensationType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Compensation".to_string(),
        pg_table_name: "compensation".to_string(),
        api_path_segment: "compensation".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };
    let child_schema = SchemaNode {
        schema_id: "compensation/json/RewardType.json".to_string(),
        title: "RewardType".to_string(),
        description: Some("A reward within compensation".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/RewardType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Reward".to_string(),
        pg_table_name: "reward".to_string(),
        api_path_segment: "reward".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    // ArrayItems: field_name is the array property on the parent (e.g., "rewards")
    let candidates = vec![codegraph_core::types::ParentCandidate {
        child_title: "RewardType".to_string(),
        parent_title: "CompensationType".to_string(),
        field_name: "rewards".to_string(),
        source: codegraph_core::types::DetectionSource::ArrayItems,
    }];

    let mock = MockEngine::builder()
        .with_schema(parent_schema)
        .with_schema(child_schema)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-array-items-fk");

    let gen = generate::api::router::RouterGenerator::new(&output_dir)
        .with_parent_candidates(candidates.clone());
    let files = gen
        .generate(
            &mock,
            "compensation",
            &["CompensationType".to_string(), "RewardType".to_string()],
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let content = &files[0].content;

    // Should NOT contain rewards_id (derived from array property name)
    assert!(
        !content.contains("rewards_id"),
        "ArrayItems FK should not use array property name 'rewards_id'. Got:\n{content}"
    );
}

/// Handler for ArrayItems child should derive FK from parent type, not array property.
#[tokio::test]
async fn array_items_handler_fk_uses_parent_type_name() {
    let parent_schema = SchemaNode {
        schema_id: "compensation/json/CompensationType.json".to_string(),
        title: "CompensationType".to_string(),
        description: Some("Compensation package".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/CompensationType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Compensation".to_string(),
        pg_table_name: "compensation".to_string(),
        api_path_segment: "compensation".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };
    let child_schema = SchemaNode {
        schema_id: "compensation/json/RewardType.json".to_string(),
        title: "RewardType".to_string(),
        description: Some("A reward within compensation".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/RewardType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Reward".to_string(),
        pg_table_name: "reward".to_string(),
        api_path_segment: "reward".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let candidates = vec![codegraph_core::types::ParentCandidate {
        child_title: "RewardType".to_string(),
        parent_title: "CompensationType".to_string(),
        field_name: "rewards".to_string(),
        source: codegraph_core::types::DetectionSource::ArrayItems,
    }];

    // Add EntityReference property so validate_parent_ref finds the FK column
    let child_fk_property = PropertyNode {
        name: "compensation".to_string(),
        prop_type: "object".to_string(),
        description: Some("FK to parent compensation".to_string()),
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "compensation_id".to_string(),
        pg_column_type: "UUID".to_string(),
        rust_field_name: "compensation".to_string(),
        rust_field_type: "Option<Uuid>".to_string(),
        sea_orm_type: "Uuid".to_string(),
        render_strategy: "entity_reference".to_string(),
        ref_target: Some("CompensationType".to_string()),
        classification: Some("entity_reference".to_string()),
        projection: None,
        classification_kind: Some(codegraph_type_contracts::RefClassificationKind::EntityReference),
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    };

    let mock = MockEngine::builder()
        .with_schema(parent_schema)
        .with_schema(child_schema)
        .with_properties("RewardType", vec![child_fk_property])
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-array-items-handler-fk");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let content = &files[0].content;

    // FK should be compensation_id (from parent type), not rewards_id (from array property)
    assert!(
        content.contains("compensation_id"),
        "ArrayItems handler should derive FK 'compensation_id' from parent type name. Got:\n{content}"
    );
    assert!(
        !content.contains("rewards_id"),
        "ArrayItems handler should NOT use array property name for FK. Got:\n{content}"
    );
}

/// LinksGenerator should produce a links.rs file with Links struct.
#[tokio::test]
async fn links_generator_produces_output() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-links-gen");

    let gen = generate::api::links::LinksGenerator::new(&output_dir);
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

    assert_eq!(
        files.len(),
        1,
        "LinksGenerator should produce exactly one file"
    );
    assert!(
        files[0].path.to_string_lossy().ends_with("links.rs"),
        "Output file should be links.rs"
    );
    assert!(
        files[0].content.contains("pub struct Links"),
        "links.rs should contain Links struct"
    );
    assert!(
        files[0].content.contains("pub struct NamedLink"),
        "links.rs should contain NamedLink struct"
    );
}

/// Child entity nested path must include parent domain in utoipa annotations.
#[tokio::test]
async fn child_handler_nested_path_includes_parent() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-nested-path");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let content = &files[0].content;

    // Path should be nested: /api/{domain}/{parent_path}/{parent_id}/{child_path}
    assert!(
        content.contains("/compensation/{compensation_id}/reward"),
        "Child handler path should nest under parent. Got:\n{content}"
    );

    // Must NOT have double-slash in path annotations (regression: empty parent_path_segment)
    assert!(
        !content.contains("/api//"),
        "Handler path must not contain double slash in URL. Got:\n{content}"
    );
}

// === OpenAPI Template Tests (Global) ===

#[tokio::test]
async fn openapi_spec() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-openapi");

    let gen = generate::api::openapi::OpenApiGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    // Should produce: mod.rs, security.rs, all.rs, recruiting.rs (per-domain), catalog.rs
    assert!(
        files.len() >= 5,
        "Should produce at least 5 files (mod, security, all, per-domain, catalog), got {}",
        files.len()
    );

    // --- security.rs ---
    let security_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("security.rs"))
        .expect("Should produce security.rs");
    assert!(
        security_file.content.contains("ApiKeySecurity"),
        "security.rs should define ApiKeySecurity"
    );

    // --- all.rs (combined spec) ---
    let all_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("all.rs"))
        .expect("Should produce all.rs");
    assert!(
        all_file.content.contains("AllApiDoc"),
        "all.rs should define AllApiDoc struct"
    );
    assert!(
        all_file.content.contains("HR Open API"),
        "all.rs should contain API title"
    );
    assert!(
        all_file.content.contains("components(schemas("),
        "all.rs should include components(schemas(...))"
    );
    assert!(
        all_file
            .content
            .contains("dto_create::CreateCandidateRequest"),
        "all.rs should register Create DTO"
    );
    assert!(
        all_file.content.contains("dto_response::CandidateResponse"),
        "all.rs should register Response DTO"
    );
    assert!(
        !all_file.content.contains("candidate_handler::delete"),
        "CandidateType should not have delete path (not in operations)"
    );
    assert!(
        all_file
            .content
            .contains("use super::security::ApiKeySecurity"),
        "all.rs should import shared security modifier"
    );

    // --- recruiting.rs (per-domain spec) ---
    let recruiting_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("recruiting.rs"))
        .expect("Should produce per-domain recruiting.rs");
    assert!(
        recruiting_file.content.contains("RecruitingApiDoc"),
        "recruiting.rs should define RecruitingApiDoc struct"
    );
    assert!(
        recruiting_file
            .content
            .contains("dto_create::CreateCandidateRequest"),
        "recruiting.rs should register Create DTO"
    );
    assert!(
        recruiting_file
            .content
            .contains("use super::security::ApiKeySecurity"),
        "recruiting.rs should import shared security modifier"
    );

    // --- catalog.rs ---
    let catalog_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("catalog.rs"))
        .expect("Should produce catalog.rs");
    assert!(
        catalog_file.content.contains("api_catalog"),
        "catalog.rs should define api_catalog handler"
    );
    assert!(
        catalog_file.content.contains("ApiCatalogEntry"),
        "catalog.rs should define ApiCatalogEntry struct"
    );
    assert!(
        catalog_file.content.contains("recruiting"),
        "catalog.rs should list recruiting domain"
    );

    // --- mod.rs ---
    let mod_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("openapi/mod.rs"))
        .expect("Should produce openapi/mod.rs");
    assert!(
        mod_file.content.contains("pub mod all;"),
        "mod.rs should declare pub mod all"
    );
    assert!(
        mod_file.content.contains("pub mod catalog;"),
        "mod.rs should declare pub mod catalog"
    );
    assert!(
        mod_file.content.contains("pub mod security;"),
        "mod.rs should declare pub mod security"
    );
    assert!(
        mod_file.content.contains("pub mod recruiting;"),
        "mod.rs should declare pub mod recruiting"
    );

    // All files should be under src/api/openapi/ directory
    for file in &files {
        assert!(
            file.path.to_string_lossy().contains("api/openapi/"),
            "All openapi files should be under src/api/openapi/, got: {}",
            file.path.display()
        );
    }
}

// === Scaffold Template Tests (Global) ===

#[tokio::test]
async fn scaffold_main() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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
    // Swagger UI should be mounted with per-domain URLs
    assert!(
        main_file
            .content
            .contains("SwaggerUi::new(\"/swagger-ui\")"),
        "main.rs should mount SwaggerUi"
    );
    assert!(
        main_file.content.contains(".urls(vec!["),
        "main.rs should use .urls() for multi-spec dropdown"
    );
    assert!(
        main_file.content.contains("use utoipa::OpenApi"),
        "main.rs should import utoipa::OpenApi"
    );
    assert!(
        main_file
            .content
            .contains("api::openapi::all::AllApiDoc::openapi()"),
        "main.rs should reference AllApiDoc"
    );
    assert!(
        main_file
            .content
            .contains("api::openapi::recruiting::RecruitingApiDoc::openapi()"),
        "main.rs should reference per-domain RecruitingApiDoc"
    );
    assert!(
        main_file.content.contains("/api-catalog.json"),
        "main.rs should mount API catalog endpoint"
    );
    assert!(
        main_file.content.contains("init_tracing"),
        "Main should init tracing"
    );
    assert!(
        main_file.content.contains("/health"),
        "Main should have health endpoint"
    );
    assert!(
        main_file.content.contains("mod error"),
        "Main should include error module"
    );

    assert!(
        main_file.content.contains("codegraph_workflow"),
        "main.rs should reference codegraph_workflow. Got:\n{}",
        main_file.content
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

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

#[tokio::test]
async fn scaffold_generates_middleware() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-mw");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    let middleware_file = files.iter().find(|f| f.path.ends_with("middleware.rs"));
    assert!(
        middleware_file.is_some(),
        "Scaffold should generate middleware.rs"
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

// === Test Generator Template Tests ===

#[tokio::test]
async fn candidate_test_gen() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-testgen");

    let gen = generate::test::test_gen::TestGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 2, "Should have entity_test and dto_test");
}

// === Cross-Layer Type Consistency Tests ===

/// Assert that DDL output contains a column with the expected Postgres type.
fn assert_ddl_column_type(ddl: &str, column_name: &str, expected_pg: &str) {
    let pattern = format!("{} {}", column_name, expected_pg);
    assert!(
        ddl.contains(&pattern),
        "Expected column '{}' with type '{}' in DDL:\n{}",
        column_name,
        expected_pg,
        ddl
    );
}

fn candidate_with_fk_properties() -> Vec<PropertyNode> {
    vec![
        PropertyNode {
            name: "givenName".to_string(),
            prop_type: "string".to_string(),
            description: Some("Given name".to_string()),
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "given_name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "given_name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "employer".to_string(),
            prop_type: "object".to_string(),
            description: Some("The employer organization".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "employer".to_string(),
            pg_column_type: "UUID".to_string(),
            rust_field_name: "employer".to_string(),
            rust_field_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            render_strategy: "fk_column".to_string(),
            ref_target: Some("recruiting/json/EmployerType.json".to_string()),
            classification: Some("entity_reference".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
    ]
}

fn candidate_with_codelist_properties() -> Vec<PropertyNode> {
    vec![
        PropertyNode {
            name: "givenName".to_string(),
            prop_type: "string".to_string(),
            description: None,
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "given_name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "given_name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "gender".to_string(),
            prop_type: "string".to_string(),
            description: Some("Gender code".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "gender".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "gender".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "fk_lookup".to_string(),
            ref_target: Some("common/json/codelist/GenderCodeList.json".to_string()),
            classification: Some("codelist".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
    ]
}

#[tokio::test]
async fn fk_field_consistent_across_ddl_and_entity() {
    let engine = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties("CandidateType", candidate_with_fk_properties())
        .build();
    let config = test_domain_config();
    let tera = test_tera();

    // DDL: should have employer_id UUID column
    let ddl_gen = generate::db::ddl::DdlGenerator::new(Path::new("/tmp/test-fk-ddl"));
    let ddl_files = ddl_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let ddl_content = &ddl_files[0].content;
    assert!(
        ddl_content.contains("employer_id UUID"),
        "DDL should have employer_id UUID column. Got:\n{}",
        ddl_content
    );
    assert_ddl_column_type(ddl_content, "employer_id", "UUID");

    // Entity: should include given_name column (FK columns are excluded from entity since they use fk_column strategy)
    let entity_gen =
        generate::db::entity::SeaOrmEntityGenerator::new(Path::new("/tmp/test-fk-entity"));
    let entity_files = entity_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let entity_content = &entity_files[0].content;
    assert!(
        entity_content.contains("given_name"),
        "Entity should have given_name. Got:\n{}",
        entity_content
    );
}

#[tokio::test]
async fn codelist_field_produces_text_fk_in_ddl() {
    let engine = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties("CandidateType", candidate_with_codelist_properties())
        .build();
    let config = test_domain_config();
    let tera = test_tera();

    let ddl_gen = generate::db::ddl::DdlGenerator::new(Path::new("/tmp/test-codelist-ddl"));
    let ddl_files = ddl_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let ddl_content = &ddl_files[0].content;

    // Codelist should produce gender TEXT column with FK (pg_column_name used as-is, no _code suffix added)
    assert!(
        ddl_content.contains("gender TEXT"),
        "DDL should have gender TEXT column. Got:\n{}",
        ddl_content
    );
    assert_ddl_column_type(ddl_content, "gender", "TEXT");
    assert!(
        ddl_content.contains("REFERENCES"),
        "DDL should have FK constraint for codelist. Got:\n{}",
        ddl_content
    );
}

#[tokio::test]
async fn codelist_field_with_code_suffix_no_double_code() {
    // Regression: workerTypeCode -> pg_column_name "worker_type_code" classified as codelist
    // must NOT produce "worker_type_code_code" — the _code suffix must not be appended again.
    let engine = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties(
            "CandidateType",
            vec![PropertyNode {
                name: "workerTypeCode".to_string(),
                prop_type: "string".to_string(),
                description: Some("Worker type code".to_string()),
                format: None,
                is_required: false,
                is_nullable: true,
                is_array: false,
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                pg_column_name: "worker_type_code".to_string(),
                pg_column_type: "TEXT".to_string(),
                rust_field_name: "worker_type_code".to_string(),
                rust_field_type: "String".to_string(),
                sea_orm_type: "Text".to_string(),
                render_strategy: "fk_lookup".to_string(),
                ref_target: Some("common/json/codelist/WorkerTypeCodeList.json".to_string()),
                classification: Some("codelist".to_string()),
                projection: None,
                classification_kind: None,
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
            }],
        )
        .build();
    let config = test_domain_config();
    let tera = test_tera();

    let ddl_gen =
        generate::db::ddl::DdlGenerator::new(Path::new("/tmp/test-codelist-no-double-code"));
    let ddl_files = ddl_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let ddl_content = &ddl_files[0].content;

    assert!(
        ddl_content.contains("worker_type_code TEXT"),
        "DDL should have worker_type_code TEXT column (no double _code). Got:\n{}",
        ddl_content
    );
    assert!(
        !ddl_content.contains("worker_type_code_code"),
        "DDL must NOT contain worker_type_code_code double suffix. Got:\n{}",
        ddl_content
    );

    // Entity: should also use worker_type_code as-is (no double _code suffix)
    let entity_gen = generate::db::entity::SeaOrmEntityGenerator::new(Path::new(
        "/tmp/test-codelist-no-double-code-entity",
    ));
    let entity_files = entity_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();
    let entity_content = &entity_files[0].content;
    assert!(
        entity_content.contains("worker_type_code"),
        "Entity should have worker_type_code field. Got:\n{}",
        entity_content
    );
    assert!(
        !entity_content.contains("worker_type_code_code"),
        "Entity must NOT contain worker_type_code_code double suffix. Got:\n{}",
        entity_content
    );
}

// === Entity Reference DTO Tests ===

#[tokio::test]
async fn candidate_create_dto_renders_entity_ref_as_id_field() {
    let engine = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties(
            "CandidateType",
            vec![
                PropertyNode {
                    name: "givenName".into(),
                    prop_type: "string".into(),
                    description: None,
                    format: None,
                    is_required: true,
                    is_nullable: false,
                    is_array: false,
                    pattern: None,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pg_column_name: "given_name".into(),
                    pg_column_type: "TEXT".into(),
                    rust_field_name: "given_name".into(),
                    rust_field_type: "String".into(),
                    sea_orm_type: "Text".into(),
                    render_strategy: "primitive_wrapper".into(),
                    ref_target: None,
                    classification: None,
                    projection: None,
                    classification_kind: None,
                    ui_override_detail: None,
                    ui_override_list_cell: None,
                    ui_override_form: None,
                    ui_override_inline: None,
                },
                PropertyNode {
                    name: "referredByApplication".into(),
                    prop_type: "object".into(),
                    description: None,
                    format: None,
                    is_required: false,
                    is_nullable: true,
                    is_array: false,
                    pattern: None,
                    min_length: None,
                    max_length: None,
                    minimum: None,
                    maximum: None,
                    pg_column_name: "referred_by_application".into(),
                    pg_column_type: "UUID".into(),
                    rust_field_name: "referred_by_application".into(),
                    rust_field_type: "Uuid".into(),
                    sea_orm_type: "Uuid".into(),
                    render_strategy: "entity_reference".into(),
                    ref_target: Some("ApplicationType".into()),
                    classification: None,
                    projection: None,
                    classification_kind: None,
                    ui_override_detail: None,
                    ui_override_list_cell: None,
                    ui_override_form: None,
                    ui_override_inline: None,
                },
            ],
        )
        .build();

    let config = test_domain_config();
    let tera = test_tera();

    // App DTOs are re-exports; check struct content in domain_types output
    let tmp = std::env::temp_dir().join("hr-graph-test-entity-ref-dto");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file");

    // Entity reference should render as _id field, not full object
    assert!(
        create_file.content.contains("referred_by_application_id"),
        "entity ref should render as _id field. Got:\n{}",
        create_file.content,
    );
    assert!(
        create_file.content.contains("Option<uuid::Uuid>"),
        "entity ref should be Option<uuid::Uuid>. Got:\n{}",
        create_file.content,
    );
    // Should NOT contain the raw field name without _id suffix
    assert!(
        !create_file.content.contains("pub referred_by_application:"),
        "should not contain full entity object field. Got:\n{}",
        create_file.content,
    );
}

// === Codelist Rust Enum Template Tests ===

#[tokio::test]
async fn codelist_enum_template_renders_correctly() {
    let tera = test_tera();
    let ctx = codegraph::generate::codelist::rust_enum::RustEnumContext {
        enum_name: "GenderCodeList".to_string(),
        description: "Gender code list.".to_string(),
        variants: vec![
            codegraph::generate::codelist::rust_enum::RustEnumVariant {
                name: "Male".to_string(),
                code: "Male".to_string(),
                serde_rename: None,
            },
            codegraph::generate::codelist::rust_enum::RustEnumVariant {
                name: "Female".to_string(),
                code: "Female".to_string(),
                serde_rename: None,
            },
            codegraph::generate::codelist::rust_enum::RustEnumVariant {
                name: "X".to_string(),
                code: "X".to_string(),
                serde_rename: None,
            },
        ],
    };

    let content = generate::render_template_with_project(&tera, "codelist/enum.tera", &ctx, &test_project_config()).unwrap();

    assert!(
        content.contains("pub enum GenderCodeList"),
        "Should contain enum declaration. Got:\n{}",
        content
    );
    assert!(
        content.contains("Serialize, Deserialize, ToSchema"),
        "Should have serde + utoipa derives"
    );
    assert!(
        content.contains("impl std::fmt::Display for GenderCodeList"),
        "Should implement Display"
    );
    assert!(
        content.contains("Self::Male => write!(f, \"Male\")"),
        "Display should write original code"
    );
    // No serde rename needed for PascalCase values
    assert!(
        !content.contains("#[serde(rename"),
        "PascalCase values should NOT have serde rename"
    );
    // Default derive (first variant marked with #[default]) — issue #9
    assert!(
        content.contains("Default,"),
        "Codelist enum should derive Default (issue #9)"
    );
    assert!(content.contains("#[default]"), "First variant should be #[default]");
    assert!(
        content.contains("Male"),
        "Default should be first variant (Male)"
    );
    // FromStr impl
    assert!(
        content.contains("impl std::str::FromStr for GenderCodeList"),
        "Should implement FromStr"
    );
}

#[tokio::test]
async fn codelist_enum_template_renders_serde_rename() {
    let tera = test_tera();
    let ctx = codegraph::generate::codelist::rust_enum::RustEnumContext {
        enum_name: "CurrencyCodeList".to_string(),
        description: "ISO 4217 currency codes.".to_string(),
        variants: vec![
            codegraph::generate::codelist::rust_enum::RustEnumVariant {
                name: "Usd".to_string(),
                code: "USD".to_string(),
                serde_rename: Some("USD".to_string()),
            },
            codegraph::generate::codelist::rust_enum::RustEnumVariant {
                name: "Eur".to_string(),
                code: "EUR".to_string(),
                serde_rename: Some("EUR".to_string()),
            },
        ],
    };

    let content = generate::render_template_with_project(&tera, "codelist/enum.tera", &ctx, &test_project_config()).unwrap();

    assert!(
        content.contains("#[serde(rename = \"USD\")]"),
        "Should have serde rename for USD"
    );
    assert!(content.contains("    Usd,"), "Should have Usd variant");
    assert!(
        content.contains("Self::Usd => write!(f, \"USD\")"),
        "Display should write USD for Usd variant"
    );
}

// === Sanitize Variant Name Tests ===

#[test]
fn sanitize_variant_name_rules() {
    use codegraph::generate::codelist::rust_enum::sanitize_variant_name;

    // PascalCase pass-through
    assert_eq!(sanitize_variant_name("Male"), "Male");
    assert_eq!(sanitize_variant_name("FullTime"), "FullTime");

    // ALL-CAPS → PascalCase
    assert_eq!(sanitize_variant_name("USD"), "Usd");
    assert_eq!(sanitize_variant_name("EUR"), "Eur");

    // Leading digit → prefix with _
    assert_eq!(sanitize_variant_name("3rdParty"), "_3rdParty");

    // Rust keyword → prefix with R (only when PascalCase result is still a keyword)
    assert_eq!(sanitize_variant_name("type"), "Type"); // "type" → PascalCase "Type" (not a keyword)
    assert_eq!(sanitize_variant_name("Self"), "RSelf"); // "Self" stays "Self" (keyword)

    // Special characters
    assert_eq!(sanitize_variant_name("full-time"), "FullTime");
    assert_eq!(sanitize_variant_name("a/b"), "AB");
    assert_eq!(sanitize_variant_name("a.b"), "AB");
}

// === Domain Event Trigger Tests ===

#[tokio::test]
async fn candidate_ddl_event_trigger() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-event-trigger");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let event_trigger_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("event_trigger"))
        .expect("Should have an event trigger SQL file");

    assert!(
        event_trigger_file
            .content
            .contains("AFTER INSERT OR UPDATE OR DELETE"),
        "Event trigger should fire on INSERT/UPDATE/DELETE. Got:\n{}",
        event_trigger_file.content
    );
    assert!(
        event_trigger_file.content.contains("emit_domain_event"),
        "Event trigger should call emit_domain_event. Got:\n{}",
        event_trigger_file.content
    );
    assert!(
        event_trigger_file
            .content
            .contains("trg_candidate_domain_event"),
        "Event trigger should have correct name. Got:\n{}",
        event_trigger_file.content
    );
}

// === Pgmq Setup Tests (Global) ===

#[tokio::test]
async fn pgmq_setup_global() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-pgmq");

    let gen = generate::db::event_trigger::PgmqSetupGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

    let gen = generate::db::platform_schema::PlatformSchemaGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

    let gen = generate::db::platform_schema::PlatformSchemaGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

// === Enriched Event Tests ===

#[tokio::test]
async fn candidate_enriched_event() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-enriched-event");

    let gen = generate::ddd::event::EventGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("CandidateEventPayload"),
        "Should contain enriched event payload struct. Got:\n{}",
        content
    );
    assert!(
        content.contains("correlation_id: Uuid"),
        "Event payload should include correlation_id. Got:\n{}",
        content
    );
    assert!(
        content.contains("occurred_at: DateTime<Utc>"),
        "Event payload should include occurred_at. Got:\n{}",
        content
    );
    assert!(
        content.contains("platform_organization_id: Uuid"),
        "Event payload should include platform_organization_id. Got:\n{}",
        content
    );
    assert!(
        content.contains("changed_fields: Vec<String>"),
        "Updated variant should include changed_fields. Got:\n{}",
        content
    );
    assert!(
        content.contains("DelegationChanged"),
        "Should contain DelegationChanged variant for workflow entities. Got:\n{}",
        content
    );
}

// === Command with correlation_id Tests ===

#[tokio::test]
async fn candidate_command_correlation_id() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd-corr");

    let gen = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("correlation_id: Uuid"),
        "Command handler should accept correlation_id. Got:\n{}",
        content
    );
}

// === Workflow Action Tests ===

#[tokio::test]
async fn workflow_action_calls_service() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-wf-action");

    let gen = generate::api::workflow_action::WorkflowActionGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(
        content.contains("workflow_service"),
        "Should call workflow_service. Got:\n{}",
        content
    );
    assert!(
        content.contains("TransitionContext"),
        "Should build TransitionContext. Got:\n{}",
        content
    );
    // transition handler should not return NOT_IMPLEMENTED
    assert!(
        content.contains("pub async fn transition("),
        "Should have transition handler. Got:\n{}",
        content
    );
    assert!(
        content.contains("AppError"),
        "Should use AppError for workflow error handling. Got:\n{}",
        content
    );
}

/// Regression test: workflow_action template must render for child entities.
/// ScreeningReportType (child of OrderType with workflow) was silently dropped
/// because workflow_action.tera referenced parent_path_segment/parent_domain
/// which were missing from WorkflowActionContext.
#[tokio::test]
async fn workflow_action_child_entity_renders() {
    let parent_schema = SchemaNode {
        schema_id: "compensation/json/CompensationType.json".to_string(),
        title: "CompensationType".to_string(),
        description: Some("Compensation package".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/CompensationType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Compensation".to_string(),
        pg_table_name: "compensation".to_string(),
        api_path_segment: "compensation".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };
    let child_schema = SchemaNode {
        schema_id: "compensation/json/RewardType.json".to_string(),
        title: "RewardType".to_string(),
        description: Some("A reward within compensation".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("compensation".to_string()),
        rel_path: "compensation/json/RewardType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Reward".to_string(),
        pg_table_name: "reward".to_string(),
        api_path_segment: "reward".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let mock = MockEngine::builder()
        .with_schema(parent_schema)
        .with_schema(child_schema)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-wf-child");

    let parent_candidates = vec![codegraph_core::types::ParentCandidate {
        child_title: "RewardType".to_string(),
        parent_title: "CompensationType".to_string(),
        field_name: "compensation_type_id".to_string(),
        source: codegraph_core::types::DetectionSource::ScalarRef,
    }];

    let gen = generate::api::workflow_action::WorkflowActionGenerator::new(&output_dir)
        .with_parent_candidates(parent_candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .expect("workflow_action template should render for child entity with workflow");

    assert_eq!(files.len(), 1, "Should produce one workflow file");
    let content = &files[0].content;

    // Verify utoipa paths include parent path segment
    assert!(
        content.contains("/compensation/{compensation_id}/reward/{reward_id}/actions/transition"),
        "Child workflow utoipa path should include parent segment. Got:\n{content}"
    );
    // Verify parent param is in utoipa params
    assert!(
        content.contains("\"compensation_id\""),
        "Should reference parent param name in utoipa params. Got:\n{content}"
    );
}

// === Workflow Seed Tests ===

#[tokio::test]
async fn workflow_seed_global() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-wf-seed");

    let gen = generate::db::workflow_seed::WorkflowSeedGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1, "Should generate one workflow seed file");
    let content = &files[0].content;
    assert!(
        content.contains("INSERT INTO platform.workflow_definition"),
        "Should insert into workflow_definition. Got:\n{}",
        content
    );
    assert!(
        content.contains("ON CONFLICT"),
        "Should have upsert clause. Got:\n{}",
        content
    );
    // CandidateType has a workflow config in test fixture
    assert!(
        content.contains("recruiting.candidate"),
        "Should contain recruiting.candidate workflow entry. Got:\n{}",
        content
    );
    // Should contain transition data in state_machine JSON
    assert!(
        content.contains("transitions"),
        "Should contain transitions in state_machine JSON. Got:\n{}",
        content
    );
}

// === Security: Parameterized set_config Tests ===

#[tokio::test]
async fn command_uses_parameterized_set_config() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-cmd-security");

    let gen = generate::ddd::command::CommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let cmd_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("command"))
        .expect("Should have a command file");

    // Must use parameterized set_config with $1, $2, $3
    assert!(
        cmd_file
            .content
            .contains("set_config('app.current_api_key', $1, true)"),
        "Command should use parameterized set_config for api_key. Got:\n{}",
        cmd_file.content
    );
    assert!(
        cmd_file
            .content
            .contains("set_config('app.organization_id', $2, true)"),
        "Command should use parameterized set_config for org_id"
    );
    assert!(
        cmd_file
            .content
            .contains("set_config('app.correlation_id', $3, true)"),
        "Command should use parameterized set_config for correlation_id"
    );
    // Must NOT use format!() string interpolation for set_config
    assert!(
        !cmd_file.content.contains("format!(\"SELECT set_config"),
        "Command must not use format!() for set_config (SQL injection risk)"
    );
    // Must use Statement::from_sql_and_values
    assert!(
        cmd_file.content.contains("Statement::from_sql_and_values"),
        "Command should use Statement::from_sql_and_values for parameterized query"
    );
}

#[tokio::test]
async fn query_uses_parameterized_set_config() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-query-security");

    let gen = generate::ddd::query::QueryGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let query_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("query"))
        .expect("Should have a query file");

    // Query sets 2 vars (no correlation_id on reads)
    assert!(
        query_file
            .content
            .contains("set_config('app.current_api_key', $1, true)"),
        "Query should use parameterized set_config for api_key. Got:\n{}",
        query_file.content
    );
    assert!(
        query_file
            .content
            .contains("set_config('app.organization_id', $2, true)"),
        "Query should use parameterized set_config for org_id"
    );
    // Must NOT have $3 (queries don't set correlation_id)
    assert!(
        !query_file.content.contains("$3"),
        "Query should only set 2 vars (no correlation_id)"
    );
    // Must NOT use format!()
    assert!(
        !query_file.content.contains("format!(\"SELECT set_config"),
        "Query must not use format!() for set_config"
    );
    assert!(
        query_file
            .content
            .contains("Statement::from_sql_and_values"),
        "Query should use Statement::from_sql_and_values"
    );
}

// === Security: HTTP Middleware Tests ===

#[tokio::test]
async fn scaffold_main_has_security_middleware() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-security");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("main.rs"))
        .expect("Should have main.rs");

    // CORS
    assert!(
        main_file.content.contains("CorsLayer"),
        "main.rs should use CorsLayer. Got:\n{}",
        main_file.content
    );
    // Request body limit
    assert!(
        main_file.content.contains("RequestBodyLimitLayer"),
        "main.rs should use RequestBodyLimitLayer"
    );
    // Security headers (lowercase in HeaderName::from_static)
    assert!(
        main_file.content.contains("x-content-type-options"),
        "main.rs should set X-Content-Type-Options header"
    );
    assert!(
        main_file.content.contains("x-frame-options"),
        "main.rs should set X-Frame-Options header"
    );
    // HSTS (conditionally enabled)
    assert!(
        main_file.content.contains("strict-transport-security"),
        "main.rs should support HSTS header"
    );
    assert!(
        main_file.content.contains("HSTS_ENABLED"),
        "HSTS should be gated on HSTS_ENABLED env var"
    );
    // DATABASE_URL must be required (no fallback)
    assert!(
        main_file.content.contains("DATABASE_URL")
            && !main_file
                .content
                .contains("unwrap_or_else(|_| \"postgres://localhost"),
        "main.rs should require DATABASE_URL (no fallback). Got:\n{}",
        main_file.content
    );
    assert!(
        !main_file
            .content
            .contains("unwrap_or_else(|_| \"postgres://localhost"),
        "main.rs must NOT have insecure DATABASE_URL fallback"
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

// === Graceful Shutdown + OTel Flush Tests ===

#[tokio::test]
async fn scaffold_main_has_graceful_shutdown() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-shutdown");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("main.rs"))
        .expect("Should have main.rs");

    assert!(
        main_file.content.contains("with_graceful_shutdown"),
        "main.rs should use with_graceful_shutdown. Got:\n{}",
        main_file.content
    );
    assert!(
        main_file.content.contains("provider.shutdown()"),
        "main.rs should flush OTel provider on shutdown"
    );
}

// === Health Ready Endpoint Tests ===

#[tokio::test]
async fn scaffold_main_has_health_ready() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-health-ready");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("main.rs"))
        .expect("Should have main.rs");

    assert!(
        main_file.content.contains("/health/ready"),
        "main.rs should have /health/ready route. Got:\n{}",
        main_file.content
    );
    assert!(
        main_file.content.contains("health_ready"),
        "main.rs should have health_ready handler function"
    );
}

// === Workflow Identity Tests ===

#[tokio::test]
async fn workflow_action_uses_real_identity() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-workflow-identity");

    let gen = generate::api::workflow_action::WorkflowActionGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    // Find the workflow action file
    let wf_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("workflow_action"));
    if let Some(wf_file) = wf_file {
        assert!(
            !wf_file.content.contains("Uuid::nil()"),
            "Workflow handlers must not use Uuid::nil() for identity. Got:\n{}",
            wf_file.content
        );
        assert!(
            !wf_file.content.contains("actor_id: None"),
            "Transition handler must not use actor_id: None"
        );
        assert!(
            wf_file.content.contains("api_key_info.api_key_id"),
            "Workflow handlers should use api_key_info.api_key_id for identity"
        );
    }
}

// === DDL No tenant_id Tests ===

#[tokio::test]
async fn candidate_ddl_no_tenant_id() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-no-tenant-id");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let table_file = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("recruiting_candidate.sql")
        })
        .expect("Should have table SQL");

    // Must have platform_organization_id column
    assert!(
        table_file.content.contains("platform_organization_id"),
        "Tenant-scoped entity should have platform_organization_id column. Got:\n{}",
        table_file.content
    );
    // Must NOT use tenant_id
    assert!(
        !table_file.content.contains(" tenant_id "),
        "DDL should use platform_organization_id, NOT tenant_id. Got:\n{}",
        table_file.content
    );
}

// === Composite Range Collapsing Tests ===

#[tokio::test]
async fn composite_range_collapses_start_end_into_daterange() {
    use codegraph_core::types::CompositeRange;

    let start_prop = PropertyNode {
        name: "start".to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: Some("date-time".to_string()),
        is_required: false,
        is_nullable: true,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "start".to_string(),
        pg_column_type: "TIMESTAMPTZ".to_string(),
        rust_field_name: "start".to_string(),
        rust_field_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    };

    let end_prop = PropertyNode {
        name: "end".to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: Some("date-time".to_string()),
        is_required: false,
        is_nullable: true,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "end".to_string(),
        pg_column_type: "TIMESTAMPTZ".to_string(),
        rust_field_name: "end".to_string(),
        rust_field_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    };

    let title_prop = PropertyNode {
        name: "title".to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "title".to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: "title".to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    };

    let composite_range = CompositeRange {
        pg_column_name: "history_period".to_string(),
        pg_type: "DATERANGE".to_string(),
        rust_type: "std::ops::Range<chrono::NaiveDate>".to_string(),
        start_field: "start".to_string(),
        end_field: "end".to_string(),
        open_end: false,
    };

    let consumed_fields = vec![
        (start_prop.clone(), "start".to_string()),
        (end_prop.clone(), "end".to_string()),
    ];

    let schema = SchemaNode {
        schema_id: "common/json/PositionHistoryType.json".to_string(),
        title: "PositionHistoryType".to_string(),
        description: Some("A record of position history".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/PositionHistoryType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PositionHistory".to_string(),
        pg_table_name: "position_history".to_string(),
        api_path_segment: "position-histories".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let mock = MockEngine::builder()
        .with_schema(schema)
        .with_properties(
            "PositionHistoryType",
            vec![start_prop.clone(), end_prop.clone(), title_prop.clone()],
        )
        .with_composite_range("PositionHistoryType", composite_range)
        .with_consumed_fields("PositionHistoryType", consumed_fields)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-composite-range");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "PositionHistoryType", "common", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let table_file = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("common_position_history.sql")
        })
        .expect("Should have a table SQL file");

    let content = &table_file.content;

    assert!(
        content.contains("history_period DATERANGE"),
        "DDL should contain history_period DATERANGE column. Got:\n{}",
        content
    );
    assert!(
        !content.contains("    start TIMESTAMPTZ"),
        "DDL should NOT contain start TIMESTAMPTZ (consumed by composite range). Got:\n{}",
        content
    );
    assert!(
        !content.contains("    end TIMESTAMPTZ"),
        "DDL should NOT contain end TIMESTAMPTZ (consumed by composite range). Got:\n{}",
        content
    );
    assert!(
        content.contains("title TEXT"),
        "DDL should still contain title TEXT (non-consumed field). Got:\n{}",
        content
    );
}

// === Recursive Child Table Tests ===

#[tokio::test]
async fn recursive_child_tables_with_full_classification() {
    // 3-level hierarchy:
    // PersonType (entity)
    //   └─ communication (ValueObject → CommunicationType)
    //        └─ address (ValueObject → AddressType)
    //             ├─ city (PrimitiveWrapper TEXT)
    //             └─ countryCode (CodelistReference)

    let person_schema = SchemaNode {
        schema_id: "common/json/PersonType.json".to_string(),
        title: "PersonType".to_string(),
        description: Some("A person".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/PersonType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Person".to_string(),
        pg_table_name: "person".to_string(),
        api_path_segment: "persons".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let communication_schema = SchemaNode {
        schema_id: "common/json/CommunicationType.json".to_string(),
        title: "CommunicationType".to_string(),
        description: Some("Communication details".to_string()),
        schema_type: "object".to_string(),
        classification: "value_object".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/CommunicationType.json".to_string(),
        pg_type: "JSONB".to_string(),
        rust_type: "Communication".to_string(),
        sea_orm_type: "JsonBinary".to_string(),
        rust_type_name: "Communication".to_string(),
        pg_table_name: "communication".to_string(),
        api_path_segment: "".to_string(),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let address_schema = SchemaNode {
        schema_id: "common/json/AddressType.json".to_string(),
        title: "AddressType".to_string(),
        description: Some("Address details".to_string()),
        schema_type: "object".to_string(),
        classification: "value_object".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/AddressType.json".to_string(),
        pg_type: "JSONB".to_string(),
        rust_type: "Address".to_string(),
        sea_orm_type: "JsonBinary".to_string(),
        rust_type_name: "Address".to_string(),
        pg_table_name: "address".to_string(),
        api_path_segment: "".to_string(),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let country_codelist_schema = SchemaNode {
        schema_id: "common/json/codelist/CountryCodeList.json".to_string(),
        title: "CountryCodeList".to_string(),
        description: Some("Country codes".to_string()),
        schema_type: "object".to_string(),
        classification: "codelist".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/codelist/CountryCodeList.json".to_string(),
        pg_type: "TEXT".to_string(),
        rust_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        rust_type_name: "CountryCode".to_string(),
        pg_table_name: "country_code".to_string(),
        api_path_segment: "".to_string(),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    // Person has a "name" (PrimitiveWrapper) and "communication" (ValueObject)
    let person_props = vec![
        PropertyNode {
            name: "name".to_string(),
            prop_type: "string".to_string(),
            description: Some("Person name".to_string()),
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: Some("common/json/CommunicationType.json".to_string()),
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "communication".to_string(),
            prop_type: "object".to_string(),
            description: Some("Communication details".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "communication".to_string(),
            pg_column_type: "JSONB".to_string(),
            rust_field_name: "communication".to_string(),
            rust_field_type: "Communication".to_string(),
            sea_orm_type: "JsonBinary".to_string(),
            render_strategy: "value_object".to_string(),
            ref_target: Some("common/json/CommunicationType.json".to_string()),
            classification: Some("value_object".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
    ];

    // CommunicationType has "email" (PrimitiveWrapper) and "address" (ValueObject)
    let communication_props = vec![
        PropertyNode {
            name: "email".to_string(),
            prop_type: "string".to_string(),
            description: Some("Email address".to_string()),
            format: Some("email".to_string()),
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "email".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "email".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "address".to_string(),
            prop_type: "object".to_string(),
            description: Some("Physical address".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "address".to_string(),
            pg_column_type: "JSONB".to_string(),
            rust_field_name: "address".to_string(),
            rust_field_type: "Address".to_string(),
            sea_orm_type: "JsonBinary".to_string(),
            render_strategy: "value_object".to_string(),
            ref_target: Some("common/json/AddressType.json".to_string()),
            classification: Some("value_object".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
    ];

    // AddressType has "city" (PrimitiveWrapper) and "countryCode" (CodelistReference)
    let address_props = vec![
        PropertyNode {
            name: "city".to_string(),
            prop_type: "string".to_string(),
            description: Some("City name".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "city".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "city".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "countryCode".to_string(),
            prop_type: "string".to_string(),
            description: Some("Country code".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "country_code".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "country_code".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "fk_lookup".to_string(),
            ref_target: Some("common/json/codelist/CountryCodeList.json".to_string()),
            classification: Some("codelist_reference".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
    ];

    let engine = MockEngine::builder()
        .with_schema(person_schema)
        .with_schema(communication_schema.clone())
        .with_schema(address_schema.clone())
        .with_schema(country_codelist_schema)
        .with_properties("PersonType", person_props)
        .with_properties("CommunicationType", communication_props)
        .with_properties("AddressType", address_props)
        // Wire up $ref resolution for ValueObject properties
        .with_ref_target("communication", "PersonType", communication_schema)
        .with_ref_target("address", "CommunicationType", address_schema)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-recursive-child");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&engine, "PersonType", "common", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let table_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("common_person.sql"))
        .expect("Should have a table SQL file");

    let content = &table_file.content;

    // First-level child table: person_communication
    assert!(
        content.contains("CREATE TABLE IF NOT EXISTS common.person_communication"),
        "Should contain first-level child table person_communication. Got:\n{}",
        content
    );

    // First-level child should have email column
    assert!(
        content.contains("email TEXT"),
        "person_communication should have email TEXT column. Got:\n{}",
        content
    );

    // First-level child should have parent FK column
    assert!(
        content.contains("person_id UUID NOT NULL"),
        "person_communication should have person_id UUID NOT NULL column. Got:\n{}",
        content
    );

    // First-level child parent FK is now ALTER TABLE ADD CONSTRAINT (not inline REFERENCES)
    assert!(
        content.contains("FOREIGN KEY (person_id) REFERENCES common.person(id) ON DELETE CASCADE"),
        "person_communication should have ALTER TABLE FK to parent person. Got:\n{}",
        content
    );

    // Nested child table: person_communication_address
    assert!(
        content.contains("CREATE TABLE IF NOT EXISTS common.person_communication_address"),
        "Should contain nested child table person_communication_address. Got:\n{}",
        content
    );

    // Nested child should have city TEXT (properly typed, not empty)
    assert!(
        content.contains("city TEXT"),
        "person_communication_address should have city TEXT column. Got:\n{}",
        content
    );

    // Nested child should have country_code TEXT column
    assert!(
        content.contains("country_code TEXT"),
        "person_communication_address should have country_code TEXT column. Got:\n{}",
        content
    );

    // Nested child should have parent FK column
    assert!(
        content.contains("person_communication_id UUID NOT NULL"),
        "person_communication_address should have person_communication_id column. Got:\n{}",
        content
    );

    // Nested child parent FK is ALTER TABLE ADD CONSTRAINT (not inline REFERENCES)
    assert!(
        content.contains("FOREIGN KEY (person_communication_id) REFERENCES common.person_communication(id) ON DELETE CASCADE"),
        "person_communication_address should have ALTER TABLE FK to person_communication. Got:\n{}",
        content
    );

    // Codelist FK constraint for country_code in nested child
    assert!(
        content.contains("fk_person_communication_address_country_code"),
        "Should have FK constraint name containing person_communication_address_country_code. Got:\n{}",
        content
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

/// Child entity command template must accept parent_id and pass it to repository.
#[tokio::test]
async fn child_command_accepts_parent_id() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-command");

    let gen = generate::ddd::command::CommandGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "Command generator should produce a file");
    let content = &files[0].content;

    // create() must accept parent_id parameter
    assert!(
        content.contains("parent_id: Uuid"),
        "Child command create must accept parent_id parameter. Got:\n{content}"
    );

    // Must pass parent_id to repo.create
    assert!(
        content.contains("repo.create(&tx, cmd, parent_id"),
        "Child command must pass parent_id to repo.create. Got:\n{content}"
    );
}

/// Child entity repository trait must include parent_id in create signature.
#[tokio::test]
async fn child_repository_trait_create_has_parent_id() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-repo-trait");

    let gen = generate::ddd::repository::RepositoryTraitGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(
        !files.is_empty(),
        "Repository trait generator should produce a file"
    );
    let content = &files[0].content;

    // create() trait method must include parent_id
    assert!(
        content.contains("parent_id: Uuid"),
        "Child repository create must include parent_id parameter. Got:\n{content}"
    );

    // find_by_id_scoped must be present
    assert!(
        content.contains("find_by_id_scoped"),
        "Child repository must include find_by_id_scoped method. Got:\n{content}"
    );
}

/// Child entity query handler must include find_by_id_scoped.
#[tokio::test]
async fn child_query_has_find_by_id_scoped() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-query");

    let gen =
        generate::ddd::query::QueryGenerator::new(&output_dir).with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "Query generator should produce a file");
    let content = &files[0].content;

    assert!(
        content.contains("find_by_id_scoped"),
        "Child query must include find_by_id_scoped method. Got:\n{content}"
    );
}

/// Child handler utoipa path annotations must include the parent prefix in the URL.
/// This verifies the generated API exposes nested routes like /api/compensation/compensation/{parent_id}/reward/{id}.
#[tokio::test]
async fn child_handler_has_nested_utoipa_path() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-child-handler-utoipa-path");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "Handler generator should produce a file");
    let content = &files[0].content;

    // The get_by_id path should include the parent path segment with {parent_id}
    assert!(
        content.contains("/api/compensation/compensation/{compensation_id}/reward/{reward_id}"),
        "Child handler utoipa path must include nested parent path. Got:\n{content}"
    );

    // The create path should include only {parent_id}
    assert!(
        content.contains("/api/compensation/compensation/{compensation_id}/reward"),
        "Child handler create path must include parent prefix. Got:\n{content}"
    );

    // Path extractor should destructure (parent_id, id) for get_by_id
    assert!(
        content.contains("Path((parent_id, id)): Path<(Uuid, Uuid)>"),
        "Child handler should destructure (parent_id, id) from path. Got:\n{content}"
    );
}

/// Entity generator must inject FK column for ParentCandidate relationships.
/// Without this, the SeaORM ActiveModel has no field for the parent FK.
#[tokio::test]
async fn entity_generator_injects_fk_for_parent_candidate() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-entity-fk-injection");

    let gen = generate::db::entity::SeaOrmEntityGenerator::new(&output_dir)
        .with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "Entity generator should produce a file");
    let content = &files[0].content;

    // Should have the FK column as a field on the entity model
    assert!(
        content.contains("compensation_type_id"),
        "Entity model must include compensation_type_id FK column. Got:\n{content}"
    );

    // Should be a Uuid type
    assert!(
        content.contains("compensation_type_id") && content.contains("Uuid"),
        "FK column should be Uuid type. Got:\n{content}"
    );
}

/// DDL generator must inject FK column + foreign key constraint for ParentCandidate relationships.
#[tokio::test]
async fn ddl_generator_injects_fk_for_parent_candidate() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-ddl-fk-injection");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir).with_parent_candidates(candidates);
    let files = gen
        .generate(&mock, "RewardType", "compensation", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "DDL generator should produce a file");
    let content = &files[0].content;

    // Should have the FK column in the CREATE TABLE
    assert!(
        content.contains("compensation_type_id"),
        "DDL must include compensation_type_id FK column. Got:\n{content}"
    );

    // Should have UUID type
    assert!(
        content.contains("compensation_type_id UUID"),
        "FK column should be UUID type in DDL. Got:\n{content}"
    );

    // Should have REFERENCES constraint
    assert!(
        content.contains("REFERENCES compensation.compensation(id)"),
        "DDL must include FK constraint referencing parent table. Got:\n{content}"
    );
}

// === Version Information Tests ===

#[tokio::test]
async fn scaffold_cargo_toml_has_shadow_rs() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-shadow");

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
        .await
        .unwrap();

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("main.rs"))
        .expect("Should have main.rs");

    // shadow-rs macro invocation
    assert!(
        main_file.content.contains("shadow!(build)"),
        "main.rs should invoke shadow!(build) macro"
    );

    // VersionInfo struct with ToSchema for OpenAPI
    assert!(
        main_file.content.contains("pub struct VersionInfo"),
        "main.rs should define VersionInfo struct"
    );
    assert!(
        main_file.content.contains("utoipa::ToSchema"),
        "VersionInfo should derive ToSchema for OpenAPI"
    );

    // Version handler with utoipa path annotation
    assert!(
        main_file.content.contains("async fn version()"),
        "main.rs should have version handler"
    );
    assert!(
        main_file.content.contains("tag = \"System\""),
        "version endpoint should be tagged under System"
    );

    // Route registration
    assert!(
        main_file.content.contains("\"/version\""),
        "main.rs should register /version route"
    );

    // Key shadow-rs constants used
    assert!(
        main_file.content.contains("build::SHORT_COMMIT"),
        "version handler should use SHORT_COMMIT"
    );
    assert!(
        main_file.content.contains("build::BUILD_TIME_3339"),
        "version handler should use BUILD_TIME_3339"
    );
}

// ── Webhook Global Generators ────────────────────────────────────────────────

#[tokio::test]
async fn webhook_dispatch_generator_produces_dispatch_module() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-webhook-dispatch");

    let gen = generate::webhook::dispatch::WebhookDispatchGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

    let gen = generate::webhook::endpoint_api::WebhookEndpointApiGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &test_generation_order(), &tera, &test_project_config())
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

// === SQLite Dialect Integration Tests ===

/// Verify that SeaOrmEntityGenerator with SQLite dialect uses the SQLite-specific
/// entity template: no schema_name attribute, primary key without auto_increment = false.
#[tokio::test]
async fn entity_with_sqlite_dialect_uses_sqlite_template() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-sqlite-entity");

    let gen = generate::db::entity::SeaOrmEntityGenerator::new(&output_dir)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1, "Should produce exactly one entity file");
    let content = &files[0].content;

    // SQLite entity template has a distinctive header comment
    assert!(
        content.contains("SQLite SeaORM entity"),
        "Should use SQLite entity template. Got:\n{content}"
    );

    // SQLite entity must NOT have schema_name in the sea_orm table annotation
    assert!(
        !content.contains(r#"schema_name =""#),
        "SQLite entity must not have schema_name in sea_orm attribute. Got:\n{content}"
    );

    // SQLite primary key should NOT have auto_increment = false
    assert!(
        content.contains("#[sea_orm(primary_key)]"),
        "SQLite entity should have primary_key annotation. Got:\n{content}"
    );
    assert!(
        !content.contains("auto_increment = false"),
        "SQLite entity should NOT have auto_increment = false. Got:\n{content}"
    );

    // Should contain the tenant-scoped column
    assert!(
        content.contains("platform_organization_id"),
        "Entity should include platform_organization_id. Got:\n{content}"
    );
}

/// Verify that CodelistGenerator with SQLite dialect produces INSERT OR IGNORE
/// instead of the PostgreSQL ON CONFLICT DO NOTHING pattern.
#[tokio::test]
async fn codelist_with_sqlite_dialect_uses_insert_or_ignore() {
    let engine = MockEngine::builder()
        .with_schema(gender_codelist_schema())
        .with_enum_values(
            "GenderCodeList",
            vec![
                EnumValue {
                    value: "Male".to_string(),
                    display_name: Some("Male".to_string()),
                    sort_order: 0,
                },
                EnumValue {
                    value: "Female".to_string(),
                    display_name: Some("Female".to_string()),
                    sort_order: 1,
                },
                EnumValue {
                    value: "X".to_string(),
                    display_name: Some("Non-binary".to_string()),
                    sort_order: 2,
                },
            ],
        )
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-sqlite-codelist");

    let gen = CodelistGenerator::new(&output_dir)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));
    let files = gen
        .generate(&engine, "GenderCodeList", "common", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(!files.is_empty(), "Codelist generator should produce a file");
    let content = &files[0].content;

    // SQLite codelist template uses INSERT OR IGNORE
    assert!(
        content.contains("INSERT OR IGNORE"),
        "SQLite codelist should use INSERT OR IGNORE. Got:\n{content}"
    );

    // Should contain codelist values
    assert!(
        content.contains("'Male'"),
        "Should contain Male codelist value. Got:\n{content}"
    );
    assert!(
        content.contains("'Female'"),
        "Should contain Female codelist value. Got:\n{content}"
    );

    // Should use STRICT table mode
    assert!(
        content.contains("STRICT"),
        "SQLite codelist table should use STRICT mode. Got:\n{content}"
    );
}

/// Verify that PG-only generators are skipped when using SQLite dialect.
/// These generators check dialect feature flags and return empty results.
#[tokio::test]
async fn pg_only_generators_skipped_for_sqlite_dialect() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let generation_order = test_generation_order();

    // BasejumpSetupGenerator is PG-only (requires extensions)
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-sqlite-basejump");
    let gen = BasejumpSetupGenerator::new(&output_dir)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();
    assert!(
        files.is_empty(),
        "BasejumpSetupGenerator should return empty for SQLite. Got {} files",
        files.len()
    );

    // PgmqSetupGenerator is PG-only (requires plpgsql)
    let output_dir2 = std::path::PathBuf::from("/tmp/hr-graph-test-sqlite-pgmq");
    let gen2 = generate::db::event_trigger::PgmqSetupGenerator::new(&output_dir2)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));
    let files2 = gen2
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();
    assert!(
        files2.is_empty(),
        "PgmqSetupGenerator should return empty for SQLite. Got {} files",
        files2.len()
    );

    // PlatformSchemaGenerator is PG-only (requires schemas)
    let output_dir3 = std::path::PathBuf::from("/tmp/hr-graph-test-sqlite-platform");
    let gen3 = generate::db::platform_schema::PlatformSchemaGenerator::new(&output_dir3)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));
    let files3 = gen3
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();
    assert!(
        files3.is_empty(),
        "PlatformSchemaGenerator should return empty for SQLite. Got {} files",
        files3.len()
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

    let gen = generate::scaffold::gen::ScaffoldGenerator::new(&output_dir, false, false, false);
    let files = gen
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
