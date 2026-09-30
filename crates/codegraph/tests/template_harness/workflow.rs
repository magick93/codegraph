use crate::harness::{
    setup_mock, test_domain_config, test_generation_order, test_project_config, test_tera,
};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::SchemaNode;

// === Workflow Action Tests ===

#[tokio::test]
async fn workflow_action_calls_service() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-wf-action");

    let gen = generate::api::workflow_action::WorkflowActionGenerator::new(&output_dir);
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
        .generate(
            &mock,
            "RewardType",
            "compensation",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &mock,
            &config,
            &test_generation_order(),
            &tera,
            &test_project_config(),
        )
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

// === Workflow Identity Tests ===

#[tokio::test]
async fn workflow_action_uses_real_identity() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-workflow-identity");

    let gen = generate::api::workflow_action::WorkflowActionGenerator::new(&output_dir);
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
