use crate::fixtures::{mock_properties, mock_schema, prop, prop_split, test_domain_config};
use codegraph::generate::ProjectConfig;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::SchemaNode;
use codegraph_type_contracts::RefClassificationKind;

// === Repository Emitter Tests ===

#[tokio::test]
async fn test_repository_emitter_produces_impl() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties("CandidateType", mock_properties())
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(code.contains("CandidateRepository"));
    assert!(code.contains("async fn create"));
    assert!(code.contains("async fn find_by_id"));
    assert!(code.contains("async fn update"));
    // CandidateType has operations = ["create", "read", "update", "list"] — no delete
    assert!(!code.contains("async fn delete"));
    assert!(code.contains("async fn list"));
}

#[tokio::test]
async fn test_repository_emitter_uses_num_items() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties("CandidateType", mock_properties())
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(
        code.contains("num_items()"),
        "Repository list should use num_items() for total count. Got:\n{}",
        code
    );
    assert!(
        !code.contains("num_pages()"),
        "Repository list must NOT use num_pages() (returns page count, not item count)"
    );
}

// === Repository Emitter Snapshot Tests ===

#[tokio::test]
async fn snapshot_repository_emitter_simple_entity() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties("CandidateType", mock_properties())
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    insta::assert_snapshot!("repo_simple_entity", code);
}

#[tokio::test]
async fn snapshot_repository_emitter_codelist_columns() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "CandidateType",
            vec![
                prop(
                    "given_name",
                    "String",
                    "TEXT",
                    true,
                    None,
                    None,
                    None,
                    false,
                ),
                // Non-nullable codelist
                prop(
                    "gender_code",
                    "String",
                    "TEXT",
                    true,
                    Some("codelist_reference"),
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/GenderCodeList.json"),
                    false,
                ),
                // Nullable codelist
                prop(
                    "currency_code",
                    "String",
                    "TEXT",
                    false,
                    Some("codelist_reference"),
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/CurrencyCodeList.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    insta::assert_snapshot!("repo_codelist_columns", code);
}

#[tokio::test]
async fn snapshot_repository_emitter_structured_wrapper() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "CandidateType",
            vec![
                prop(
                    "given_name",
                    "String",
                    "TEXT",
                    true,
                    None,
                    None,
                    None,
                    false,
                ),
                // Scalar JSONB (non-nullable)
                prop(
                    "address",
                    "serde_json::Value",
                    "JSONB",
                    true,
                    Some("structured_wrapper"),
                    Some(RefClassificationKind::StructuredWrapper),
                    None,
                    false,
                ),
                // Nullable scalar JSONB
                prop(
                    "metadata",
                    "serde_json::Value",
                    "JSONB",
                    false,
                    Some("structured_wrapper"),
                    Some(RefClassificationKind::StructuredWrapper),
                    None,
                    false,
                ),
                // Array JSONB (non-nullable)
                prop(
                    "tags",
                    "serde_json::Value",
                    "JSONB",
                    true,
                    Some("structured_wrapper"),
                    Some(RefClassificationKind::StructuredWrapper),
                    None,
                    true,
                ),
                // Nullable array JSONB
                prop(
                    "preferences",
                    "serde_json::Value",
                    "JSONB",
                    false,
                    Some("structured_wrapper"),
                    Some(RefClassificationKind::StructuredWrapper),
                    None,
                    true,
                ),
            ],
        )
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    insta::assert_snapshot!("repo_structured_wrapper", code);
}

#[tokio::test]
async fn snapshot_repository_emitter_child_tables() {
    // Create a ValueObject child schema
    let child_schema = SchemaNode {
        namespace: None,
        schema_id: "recruiting/json/PersonNameType.json".to_string(),
        title: "PersonNameType".to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "value_object".to_string(),
        domain: Some("recruiting".to_string()),
        rel_path: "recruiting/json/PersonNameType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PersonName".to_string(),
        pg_table_name: "candidate_person_name".to_string(),
        api_path_segment: "person-names".to_string(),
        parent_schema: Some("CandidateType".to_string()),
        is_entity: false,
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
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_schema(child_schema.clone())
        .with_properties(
            "CandidateType",
            vec![
                prop(
                    "given_name",
                    "String",
                    "TEXT",
                    true,
                    None,
                    None,
                    None,
                    false,
                ),
                // Array child (Vec)
                prop(
                    "person_name",
                    "String",
                    "TEXT",
                    false,
                    Some("value_object"),
                    Some(RefClassificationKind::ValueObject),
                    Some("recruiting/json/PersonNameType.json"),
                    true,
                ),
            ],
        )
        .with_properties(
            "PersonNameType",
            vec![
                prop(
                    "first_name",
                    "String",
                    "TEXT",
                    true,
                    None,
                    None,
                    None,
                    false,
                ),
                prop(
                    "last_name",
                    "String",
                    "TEXT",
                    false,
                    None,
                    None,
                    None,
                    false,
                ),
            ],
        )
        .with_ref_target("person_name", "CandidateType", child_schema)
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    insta::assert_snapshot!("repo_child_tables", code);
}

#[tokio::test]
async fn snapshot_repository_emitter_with_parent_ref() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/ApplicationType.json",
            "ApplicationType",
            "application",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "ApplicationType",
            vec![
                prop("title", "String", "TEXT", true, None, None, None, false),
                prop(
                    "candidate_id",
                    "Uuid",
                    "UUID",
                    true,
                    Some("entity_reference"),
                    Some(RefClassificationKind::EntityReference),
                    Some("recruiting/json/CandidateType.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            Some("candidate_id"),
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    insta::assert_snapshot!("repo_with_parent_ref", code);
}

/// Test that codelist properties with stripped `_code` in rust_field_name
/// (as done by real ingestion's strip_code_suffix_safe) produce correct
/// DTO field access — `cmd.{stripped}` never `cmd.{with_suffix}`.
#[tokio::test]
async fn repository_emitter_codelist_dto_access_uses_stripped_names() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/DeploymentType.json",
            "DeploymentType",
            "deployment",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "DeploymentType",
            vec![
                prop_split(
                    "assignment",
                    "assignment",
                    "assignment_id",
                    "Uuid",
                    "UUID",
                    true,
                    Some(RefClassificationKind::EntityReference),
                    Some("WorkAssignmentType"),
                    false,
                ),
                // Codelist reference — rust_field_name stripped (no _code)
                // pg_column_name retains _code suffix
                prop_split(
                    "assignment_reason",
                    "assignment_reason",
                    "assignment_reason_code",
                    "AssignmentReasonCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/AssignmentReasonCodeList.json"),
                    false,
                ),
                // Another codelist reference
                prop_split(
                    "status",
                    "status",
                    "status_code",
                    "DeploymentStatusCodeList",
                    "TEXT",
                    false,
                    Some(RefClassificationKind::CodelistReference),
                    Some("../common/json/codelist/DeploymentStatusCodeList.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "DeploymentType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // DTO access must use stripped rust_field_name:
    //   cmd.assignment_reason ✓
    //   cmd.status ✓
    assert!(
        code.contains("cmd.assignment_reason"),
        "DTO access must use stripped name, not pg_column_name with _code"
    );
    assert!(
        code.contains("cmd.status"),
        "DTO access must use stripped name, not pg_column_name with _code"
    );

    // DTO access must NEVER use pg_column_name with _code suffix:
    assert!(
        !code.contains("cmd.assignment_reason_code"),
        "DTO access must NOT use pg_column_name with _code suffix"
    );
    assert!(
        !code.contains("cmd.status_code"),
        "DTO access must NOT use pg_column_name with _code suffix"
    );

    // Entity model field access should use stripped names too
    // (model.{field} = Set(cmd.{field}))
    assert!(code.contains("model.assignment_reason"));
    assert!(code.contains("model.status"));
    assert!(!code.contains("model.assignment_reason_code"));
    assert!(!code.contains("model.status_code"));

    // Response construction (row.{field}) should use stripped names
    assert!(code.contains("row.assignment_reason"));
    assert!(code.contains("row.status"));
    // Not the _code column name for row access
    assert!(!code.contains("row.assignment_reason_code"));
    assert!(!code.contains("row.status_code"));
}

/// Test child table INSERT/UPDATE code uses stripped DTO field names
/// when accessing codelist array values from parent DTOs.
#[tokio::test]
async fn child_insert_uses_stripped_dto_field_names() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/DeploymentType.json",
            "DeploymentType",
            "deployment",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "DeploymentType",
            vec![prop_split(
                "position_schedule_types",
                "position_schedule_types",
                "position_schedule_type_codes",
                "Vec<String>",
                "TEXT[]",
                false,
                Some(RefClassificationKind::CodelistReference),
                Some("../common/json/codelist/PositionScheduleTypeCodeList.json"),
                true, // array codelist → child table
            )],
        )
        .build();

    let config = test_domain_config();
    let emitter = codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "DeploymentType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // Child table INSERT uses child.field_name for `cmd.{field}` access
    // `child.field_name` should be the stripped rust_field_name
    assert!(
        code.contains("cmd.position_schedule_types"),
        "child INSERT must use stripped DTO field name: position_schedule_types"
    );
    assert!(
        !code.contains("cmd.position_schedule_type_codes"),
        "child INSERT must NOT use pg_column_name with _code suffix"
    );
}
