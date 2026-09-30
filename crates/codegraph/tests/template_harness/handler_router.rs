use crate::harness::{
    parent_child_mock, setup_mock, test_domain_config, test_generation_order, test_project_config,
    test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};

// === Handler Template Tests ===

#[tokio::test]
async fn candidate_handler() {
    generate::type_registry::register_framework_types();
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-handler");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
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

// === FTS REST surface mode tests ===

/// Entity with search.fts_rest_mode = "dedicated" must generate the
/// standalone GET /search endpoint with typed SearchParams and a
/// versioned utoipa path.
#[tokio::test]
async fn handler_fts_rest_dedicated_generates_search_endpoint() {
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
    candidate_cfg.search.fts_columns = Some(vec!["summary".to_string()]);
    candidate_cfg.search.fts_rest_mode = "dedicated".to_string();

    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-handler-fts-dedicated");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let content = &files[0].content;
    assert!(
        content.contains("pub struct SearchParams"),
        "Dedicated mode should generate SearchParams struct. Got:\n{content}"
    );
    assert!(
        content.contains("pub async fn search("),
        "Dedicated mode should generate search handler. Got:\n{content}"
    );
    assert!(
        content.contains("operation_id = \"recruiting_candidate_search\""),
        "Search handler should have dedicated operation_id"
    );
    assert!(
        content.contains("path = \"/api/v1/recruiting/candidates/search\""),
        "Search utoipa path should include api_version"
    );
    assert!(
        content.contains("/search\""),
        "Search utoipa path should end with /search"
    );
    assert!(
        content.contains("api_key_info.user_id"),
        "Search handler should pass user_id to queries.search"
    );
    assert!(
        content.contains("candidate_queries.search("),
        "Search handler should delegate to candidate_queries.search"
    );
    // The ?q= param should still be present on the list route
    assert!(
        content.contains("pub q: Option<String>"),
        "List route should still have ?q= param"
    );
}

/// Dedicated /search route registration in the router template.
#[tokio::test]
async fn router_fts_rest_dedicated_registers_search_route() {
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
    candidate_cfg.search.fts_columns = Some(vec!["summary".to_string()]);
    candidate_cfg.search.fts_rest_mode = "dedicated".to_string();

    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-router-fts-dedicated");

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

    let router_content = files
        .iter()
        .map(|f| f.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        router_content.contains(".route(\"/search\""),
        "Dedicated mode should register /search route. Got:\n{router_content}"
    );
    assert!(
        router_content.contains("candidate_handler::search"),
        "/search route should point to candidate_handler::search"
    );
}

/// Default mode (query_param) must NOT generate a dedicated /search endpoint
/// or register a /search route — backward compatibility.
#[tokio::test]
async fn handler_fts_rest_query_param_omits_search_endpoint() {
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
    candidate_cfg.search.fts_columns = Some(vec!["summary".to_string()]);
    candidate_cfg.search.fts_rest_mode = "query_param".to_string();

    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-handler-fts-query");

    let gen = generate::api::handler::HandlerGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let content = &files[0].content;
    assert!(
        !content.contains("pub struct SearchParams"),
        "query_param mode should NOT generate SearchParams. Got:\n{content}"
    );
    assert!(
        content.contains("pub q: Option<String>"),
        "query_param mode should keep ?q= on the list route"
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
        namespace: None,
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
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };
    let child_schema = SchemaNode {
        namespace: None,
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
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
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
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
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

/// Handler for ArrayItems child should derive FK from parent type, not array property.
#[tokio::test]
async fn array_items_handler_fk_uses_parent_type_name() {
    let parent_schema = SchemaNode {
        namespace: None,
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
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };
    let child_schema = SchemaNode {
        namespace: None,
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
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
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
        min_items: None,
        max_items: None,
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
        type_expr: None,
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
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
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

/// Structural regression guard for issue #62: every `crate::error::X` type
/// referenced by the OpenAPI `all.rs` output must be defined by the scaffold
/// `error.rs` output. The content-only snapshot test could not catch this —
/// the pre-fix snapshot literally asserted the broken references (E0425 in
/// generated apps).
#[tokio::test]
async fn openapi_error_schemas_are_defined_by_error_module() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-openapi-error-refs");

    let scaffold = generate::scaffold::gen::ScaffoldGenerator::new(
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
    let scaffold_files = scaffold
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let error_file = scaffold_files
        .iter()
        .find(|f| f.path.ends_with("error.rs"))
        .expect("Should generate error.rs");
    let error_content = &error_file.content;

    let openapi = generate::api::openapi::OpenApiGenerator::new(&output_dir);
    let openapi_files = openapi
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let all_file = openapi_files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("openapi/all.rs"))
        .expect("Should produce openapi/all.rs");
    let openapi_content = &all_file.content;

    // Manual scan (no regex dependency): read the identifier after each
    // `crate::error::` occurrence.
    let mut referenced: Vec<String> = Vec::new();
    for chunk in openapi_content.split("crate::error::").skip(1) {
        let ident: String = chunk
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !ident.is_empty() && !referenced.contains(&ident) {
            referenced.push(ident);
        }
    }

    assert!(
        !referenced.is_empty(),
        "openapi/all.rs must reference at least one crate::error:: type. Got:\n{openapi_content}"
    );

    for required in ["ErrorResponse", "ErrorBody", "FieldError"] {
        assert!(
            referenced.iter().any(|t| t == required),
            "openapi/all.rs must reference crate::error::{required}. Got: {referenced:?}\n{openapi_content}"
        );
    }

    for ty in &referenced {
        assert!(
            error_content.contains(&format!("pub struct {ty}"))
                || error_content.contains(&format!("pub enum {ty}")),
            "Issue #62 regression: openapi/all.rs references crate::error::{ty} but \
             error.rs does not define it.\n\
             Referenced types: {referenced:?}\n\
             --- error.rs (definitions) ---\n{error_content}\n\
             --- openapi/all.rs (usage) ---\n{openapi_content}"
        );
    }

    // Issue #63: ErrorBody embeds `Vec<FieldError>` and derives Clone + ToSchema,
    // so FieldError must carry both derives or the generated app fails E0277.
    assert!(
        error_content.contains(
            "#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]\npub struct FieldError"
        ),
        "Issue #63 regression: FieldError must derive Clone + utoipa::ToSchema \
         (ErrorBody embeds Vec<FieldError>).\n--- error.rs ---\n{error_content}"
    );
    // ErrorBody/ErrorResponse themselves must derive Clone + ToSchema.
    for ty in ["ErrorBody", "ErrorResponse"] {
        assert!(
            error_content.contains(&format!(
                "#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]\npub struct {ty}"
            )),
            "Issue #63 regression: {ty} must derive Clone + utoipa::ToSchema.\n--- error.rs ---\n{error_content}"
        );
    }
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
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "Handler generator should produce a file");
    let content = &files[0].content;

    // The get_by_id path should include the parent path segment with {parent_id}
    assert!(
        content.contains("/api/v1/compensation/compensation/{compensation_id}/reward/{reward_id}"),
        "Child handler utoipa path must include nested parent path. Got:\n{content}"
    );

    // The create path should include only {parent_id}
    assert!(
        content.contains("/api/v1/compensation/compensation/{compensation_id}/reward"),
        "Child handler create path must include parent prefix. Got:\n{content}"
    );

    // Path extractor should destructure (parent_id, id) for get_by_id
    assert!(
        content.contains("Path((parent_id, id)): Path<(Uuid, Uuid)>"),
        "Child handler should destructure (parent_id, id) from path. Got:\n{content}"
    );
}
