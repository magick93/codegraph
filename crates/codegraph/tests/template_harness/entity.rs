use crate::harness::{
    parent_child_mock, setup_mock, test_domain_config, test_project_config, test_tera,
};
use codegraph::generate;
use codegraph::generate::db::dialect::{dialect_for_target, DatabaseTarget};
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};

// === Entity Model Template Tests ===

#[tokio::test]
async fn candidate_entity_model() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-entity");

    let gen = generate::db::entity::SeaOrmEntityGenerator::new(&output_dir);
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
    assert!(files[0].content.contains("DeriveEntityModel"));
    assert!(files[0].content.contains("given_name"));
    assert!(files[0].content.contains("family_name"));
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
