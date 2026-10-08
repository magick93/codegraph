//! Entity/DTO generator tests, plus the domain-types codelist generator test
//! (straggler, nearest cohesive: domain-types generator output).

use crate::fixtures::{
    mock_properties, mock_schema, prop_split, test_domain_config, test_project_config,
};
use codegraph::generate;
use codegraph::generate::traits::EntityGenerator;
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    CodeList, DeletionPropagation, EnumValue, PolicyKind, PolicyNode, PropertyNode,
    SoftDeleteMarker, SoftDeletePolicy, SoftDeleteVisibility, TenantIsolationPolicy,
    TenantPropagation, TenantStrategy,
};
use codegraph_type_contracts::RefClassificationKind;
use std::path::Path;

// === SeaORM Entity Generator Tests ===

#[tokio::test]
async fn test_entity_generator_produces_model() {
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
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-entity");
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    let generator = generate::db::entity::SeaOrmEntityGenerator::new(&output_dir);
    let files = generator
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
    assert!(files[0].content.contains("DeriveEntityModel"));
    assert!(files[0].content.contains("given_name"));
}

// === DTO Generator Tests ===

#[tokio::test]
async fn test_dto_generator_produces_create_and_response() {
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
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-dto");
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    let generator = generate::ddd::dto::DtoGenerator::new(&output_dir);
    let files = generator
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

    assert!(
        files.len() >= 2,
        "Expected at least 2 DTO files, got {}",
        files.len()
    );

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .unwrap();
    assert!(create_file.content.contains("CreateCandidateRequest"));

    let response_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .unwrap();
    assert!(response_file.content.contains("CandidateResponse"));
}

// === Domain Types Codelist Generator Tests ===

#[tokio::test]
async fn test_domain_types_codelist_generates_enum_not_string_alias() {
    use codegraph::generate::domain_types::codelist::DomainTypesCodelistGenerator;

    let mock = MockEngine::new();

    // Ingest a codelist with a few enum values
    mock.ingest_codelist(&CodeList {
        name: "CountryCodeList".to_string(),
        description: Some("ISO country codes".to_string()),
        pg_table_name: "country_code_list".to_string(),
        render_as: "enum".to_string(),
        check_expression: None,
    })
    .await
    .unwrap();

    for (value, order) in [("NZ", 0), ("AU", 1), ("US", 2)] {
        mock.ingest_enum_value(
            "CountryCodeList",
            &EnumValue {
                value: value.to_string(),
                display_name: None,
                sort_order: order,
            },
        )
        .await
        .unwrap();
    }

    let tmp_dir = std::env::temp_dir().join("hr-graph-test-codelist-enum");
    let _ = std::fs::remove_dir_all(&tmp_dir);

    let generator = DomainTypesCodelistGenerator::new_with_base(tmp_dir.clone());

    let mut tera = tera::Tera::default();
    tera.add_raw_template(
        "codelist/enum.tera",
        include_str!("../../../codegraph-generate/templates/codelist/enum.tera"),
    )
    .unwrap();

    let files = generator
        .generate_all(&mock, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(
        files.len() >= 2,
        "Expected at least 2 files (enum + mod.rs), got {}",
        files.len()
    );

    let enum_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("country_code_list.rs"))
        .expect("Should have country_code_list.rs");

    let content = &enum_file.content;

    // Must be a proper enum, NOT a string alias
    assert!(
        content.contains("pub enum CountryCodeList"),
        "Should contain 'pub enum CountryCodeList', got:\n{}",
        content
    );
    assert!(
        !content.contains("pub type CountryCodeList = String"),
        "Must NOT contain string alias 'pub type CountryCodeList = String'"
    );

    // Check variant names
    assert!(content.contains("Nz"), "Should contain variant Nz");
    assert!(content.contains("Au"), "Should contain variant Au");
    assert!(content.contains("Us"), "Should contain variant Us");

    // Check serde rename attributes
    assert!(
        content.contains(r#"#[serde(rename = "NZ")]"#),
        "Should contain serde rename for NZ"
    );
    assert!(
        content.contains(r#"#[serde(rename = "AU")]"#),
        "Should contain serde rename for AU"
    );
    assert!(
        content.contains(r#"#[serde(rename = "US")]"#),
        "Should contain serde rename for US"
    );
}

/// Verify the DTO generator uses stripped rust_field_name for struct fields
/// in dto_create.rs, dto_update.rs, and dto_response.rs.
#[tokio::test]
async fn dto_generator_uses_stripped_names_for_codelist_fields() {
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
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();
    let generator = codegraph::generate::ddd::dto::DtoGenerator::new(&std::path::PathBuf::from(
        "/tmp/test-dto-code",
    ));
    let files = generator
        .generate(
            &mock,
            "DeploymentType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "should generate DTO files");
    for file in &files {
        let path = file.path.to_string_lossy();
        let content = &file.content;

        // dto_create.rs and dto_update.rs are re-exports from the
        // domain-types crate — they don't define struct fields directly.
        if path.contains("dto_create") {
            assert!(
                content.contains("pub use"),
                "dto_create.rs should be a re-export from domain-types"
            );
        }
        if path.contains("dto_response") {
            // dto_response.rs contains both re-exports AND LinkedResponse wrapper.
            // The LinkedResponse wrapper references {Entity}Response which should
            // use stripped field names (verified by domain_types test).
            assert!(
                content.contains("LinkedResponse"),
                "dto_response.rs should contain LinkedResponse"
            );
        }
    }
}

/// Verify the entity model generator uses stripped rust_field_name for struct
/// fields, not pg_column_name with _code suffix.
#[tokio::test]
async fn entity_model_uses_stripped_names_for_codelist_fields() {
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
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();
    let generator = codegraph::generate::db::entity::SeaOrmEntityGenerator::new(
        &std::path::PathBuf::from("/tmp/test-entity-code"),
    );
    let files = generator
        .generate(
            &mock,
            "DeploymentType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert_eq!(files.len(), 1, "should generate one entity file");
    let content = &files[0].content;

    // Struct field names must use stripped rust_field_name
    assert!(
        content.contains("pub assignment_reason"),
        "struct field must use stripped name"
    );
    assert!(
        !content.contains("pub assignment_reason_code"),
        "struct field must NOT use _code suffix"
    );
    assert!(
        content.contains("pub status"),
        "struct field must use stripped name"
    );
    assert!(
        !content.contains("pub status_code"),
        "struct field must NOT use _code suffix"
    );

    // Column name attribute should still use pg_column_name
    assert!(
        content.contains("assignment_reason_code"),
        "column_name attr should use pg_column_name"
    );
    assert!(
        content.contains("status_code"),
        "column_name attr should use pg_column_name"
    );
}

/// Verify the domain-types DTO generator uses stripped rust_field_name for
/// struct fields — same as the app-level DTO generator but different output target.
#[tokio::test]
async fn domain_types_dto_uses_stripped_names_for_codelist_fields() {
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
                "assignment_reason",
                "assignment_reason",
                "assignment_reason_code",
                "AssignmentReasonCodeList",
                "TEXT",
                false,
                Some(RefClassificationKind::CodelistReference),
                Some("../common/json/codelist/AssignmentReasonCodeList.json"),
                false,
            )],
        )
        .build();

    let config = test_domain_config();
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();
    let generator = codegraph::generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
        std::path::PathBuf::from("/tmp/test-domain-types-dto"),
    );
    let files = generator
        .generate(
            &mock,
            "DeploymentType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "should generate DTO files");
    for file in &files {
        let path = file.path.to_string_lossy();
        let content = &file.content;
        if path.contains("dto_create")
            || path.contains("dto_update")
            || path.contains("dto_response")
        {
            assert!(
                content.contains("assignment_reason"),
                "missing stripped field in {path}"
            );
            assert!(
                !content.contains("assignment_reason_code"),
                "{path} has _code suffix in field name"
            );
        }
    }
}

#[tokio::test]
async fn generate_policy_driven_entity_with_soft_delete() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "test/json/TestEntityType.json",
            "TestEntityType",
            "test_entity",
            "test",
            "entity",
        ))
        .with_properties(
            "TestEntityType",
            vec![PropertyNode {
                is_id: false,
                name: "name".to_string(),
                prop_type: "string".to_string(),
                description: Some("The entity name".to_string()),
                format: None,
                is_required: true,
                is_nullable: false,
                is_array: false,
                min_items: None,
                max_items: None,
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
                ref_target: None,
                classification: Some("primitive_wrapper".to_string()),
                projection: None,
                classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
                ui_override_detail: None,
                ui_override_list_cell: None,
                ui_override_form: None,
                ui_override_inline: None,
                type_expr: None,
            }],
        )
        .build();

    // Ingest soft-delete, tenant, and audit policies into the MockEngine
    mock.ingest_policy(&PolicyNode {
        name: "test_soft_delete".into(),
        kind: PolicyKind::SoftDelete(SoftDeletePolicy {
            marker: SoftDeleteMarker::Timestamp("deleted_at".into()),
            visibility: SoftDeleteVisibility::ExcludeByDefault,
            cascade: DeletionPropagation::Restrict,
        }),
        target_schema: "TestEntityType".into(),
        domain: Some("test".into()),
    })
    .await
    .unwrap();

    mock.ingest_policy(&PolicyNode {
        name: "test_tenant".into(),
        kind: PolicyKind::TenantIsolation(TenantIsolationPolicy {
            strategy: TenantStrategy::Column {
                property: "org_id".into(),
            },
            propagation: TenantPropagation::Explicit,
        }),
        target_schema: "TestEntityType".into(),
        domain: Some("test".into()),
    })
    .await
    .unwrap();

    let config = codegraph_config::config::parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.test]
label = "Test"
schema_dir = "test"
postgres_schema = "test"
entities = ["TestEntityType"]
auditable = true
"#,
    )
    .unwrap();

    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-policy-entity");
    let generator = generate::db::entity::SeaOrmEntityGenerator::new(&output_dir);
    let files = generator
        .generate(
            &mock,
            "TestEntityType",
            "test",
            &config,
            &tera,
            &codegraph::generate::ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "Entity generator should produce files");

    let entity_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("test_test_entity.rs"))
        .expect("Should have an entity file");

    let content = &entity_file.content;

    // Soft-delete policy assertions
    assert!(
        content.contains("fn active()"),
        "should generate active() query scope"
    );
    assert!(
        content.contains("fn including_deleted()"),
        "should generate including_deleted() scope"
    );
    assert!(
        content.contains("Column::DeletedAt.is_null()"),
        "should filter active rows on deleted_at IS NULL"
    );
    assert!(
        content.contains("deleted_at"),
        "should have deleted_at column from soft-delete policy"
    );

    // Tenant isolation policy assertions
    assert!(
        content.contains("org_id"),
        "should use configured tenant column name from policy"
    );
    assert!(
        !content.contains("platform_organization_id"),
        "should NOT hardcode platform_organization_id when tenant policy is set"
    );
}
