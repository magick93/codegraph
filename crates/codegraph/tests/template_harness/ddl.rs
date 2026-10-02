use crate::harness::{
    candidate_schema, parent_child_mock, setup_mock, test_domain_config, test_generation_order,
    test_project_config, test_tera,
};
use codegraph::generate;
use codegraph::generate::db::basejump_setup::BasejumpSetupGenerator;
use codegraph::generate::db::dialect::{DatabaseTarget, dialect_for_target};
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};
use std::path::Path;

// === DDL Template Tests ===

#[tokio::test]
async fn candidate_ddl_table() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-ddl");

    let generator = generate::db::ddl::DdlGenerator::new(&output_dir);
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
    // App role DML grant must be emitted on the table so the RLS-aware
    // app_user role can access it regardless of application order.
    assert!(
        table_file.content.contains(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE recruiting.candidate TO app_user;"
        ),
        "Entity table must grant DML to app_user. Got:\n{}",
        table_file.content
    );
}

#[tokio::test]
async fn table_tera_grants_child_table_dml() {
    // Child tables rendered by db/table.tera must also carry the app-role DML
    // grant (the RLS-aware role queries child rows too).
    let tera = test_tera();
    let ctx = serde_json::json!({
        "schema_name": "recruiting",
        "table_name": "candidate",
        "columns": [{"name": "id", "pg_type": "UUID", "is_primary_key": true, "nullable": false}],
        "check_constraints": [],
        "is_auditable": false,
        "foreign_keys": [],
        "indexes": [],
        "child_tables": [{
            "schema_name": "recruiting",
            "table_name": "candidate_person_name",
            "parent_fk_column": "candidate_id",
            "parent_schema": "recruiting",
            "parent_table": "candidate",
            "columns": [{"name": "id", "pg_type": "UUID"}],
            "check_constraints": [],
            "foreign_keys": [],
            "comments": []
        }],
        "comments": [],
        "extensions": []
    });
    let rendered = generate::render_template_with_project(
        &tera,
        "db/table.tera",
        &ctx,
        &test_project_config(),
    )
    .unwrap();
    assert!(
        rendered.contains(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE recruiting.candidate TO app_user;"
        ),
        "Entity table must grant DML to app_user. Got:\n{rendered}"
    );
    assert!(
        rendered.contains(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE recruiting.candidate_person_name TO app_user;"
        ),
        "Child table must grant DML to app_user. Got:\n{rendered}"
    );
}

#[tokio::test]
async fn scaffold_api_key_migration_grants_app_user_dml() {
    // 0002_api_key_management.sql must grant the app role DML up front (before
    // any domain table exists) via CREATE SCHEMA + ALTER DEFAULT PRIVILEGES,
    // covering every configured domain schema + the infra schemas. This is what
    // makes the grant independent of migration application order.
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-scaffold-grants");

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

    let api_key_file = files
        .iter()
        .find(|f| f.path.ends_with("0002_api_key_management.sql"))
        .expect("Should generate 0002_api_key_management.sql");
    let content = &api_key_file.content;

    assert!(
        content.contains("CREATE SCHEMA IF NOT EXISTS"),
        "0002 must create schemas upfront. Got:\n{content}"
    );
    assert!(
        content.contains("ALTER DEFAULT PRIVILEGES IN SCHEMA %I GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_user"),
        "0002 must set default privileges for future tables. Got:\n{content}"
    );
    assert!(
        content.contains(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA %I TO app_user"
        ),
        "0002 must grant current tables too. Got:\n{content}"
    );
    // The configured domain schema + the infra schema appear in the list.
    for schema in ["recruiting", "platform"] {
        assert!(
            content.contains(&format!("'{schema}'")),
            "0002 grant list must include {schema}. Got:\n{content}"
        );
    }
    // `common` carries the generated codelist tables (app_user needs DML on
    // them); the list is derived from the configured domains, not the old
    // hardcoded domain list: with only the fixture domains in scope,
    // `screening`/`compliance` must NOT appear.
    for schema in ["'screening'", "'compliance'", "'payroll'"] {
        assert!(
            !content.contains(schema),
            "0002 grant list must not include hardcoded {schema}. Got:\n{content}"
        );
    }
    // The old order-dependent guard must be gone.
    assert!(
        !content.contains("information_schema.schemata WHERE schema_name"),
        "0002 must not be gated on pre-existing schemas. Got:\n{content}"
    );
}

#[tokio::test]
async fn candidate_ddl_trigger() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-trigger");

    let generator = generate::db::ddl::DdlGenerator::new(&output_dir);
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

    let generator = generate::db::ddl::DdlGenerator::new(&output_dir);
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

    let generator = generate::db::ddl::DdlGenerator::new(&output_dir);
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
    // Auditable entities should have DB-enforced scope policies (#169):
    // RESTRICTIVE policies for both pool roles calling enforce_api_key_scope().
    assert!(
        rls_file.content.contains("TO app_user, api_key"),
        "Scope policies must apply to both pool roles (app_user + api_key)"
    );
    assert!(
        rls_file.content.contains("AS RESTRICTIVE"),
        "Scope policies must be RESTRICTIVE so they AND-combine with org isolation"
    );
    assert!(
        rls_file.content.contains("enforce_api_key_scope"),
        "Scope policies should use enforce_api_key_scope()"
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
    let generator = generate::db::ddl::DdlGenerator::new(&output_dir)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));

    let project = test_project_config();

    let files = generator
        .generate(
            &mock,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &project,
        )
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

/// ArrayItems detection should derive FK from parent type name, not array property name.
/// Regression test: `rewards` (array prop) → `rewards_id` instead of `compensation_id`.
#[tokio::test]
async fn array_items_fk_uses_parent_type_name() {
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

    let generator = generate::api::router::RouterGenerator::new(&output_dir)
        .with_parent_candidates(candidates.clone());
    let files = generator
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
            min_items: None,
            max_items: None,
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
            type_expr: None,
        },
        PropertyNode {
            name: "employer".to_string(),
            prop_type: "object".to_string(),
            description: Some("The employer organization".to_string()),
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
            type_expr: None,
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
            min_items: None,
            max_items: None,
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
            type_expr: None,
        },
        PropertyNode {
            name: "gender".to_string(),
            prop_type: "string".to_string(),
            description: Some("Gender code".to_string()),
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
            type_expr: None,
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
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
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
                min_items: None,
                max_items: None,
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
                type_expr: None,
            }],
        )
        .build();
    let config = test_domain_config();
    let tera = test_tera();

    let ddl_gen =
        generate::db::ddl::DdlGenerator::new(Path::new("/tmp/test-codelist-no-double-code"));
    let ddl_files = ddl_gen
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &test_project_config(),
        )
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

// === Domain Event Trigger Tests ===

#[tokio::test]
async fn candidate_ddl_event_trigger() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-event-trigger");

    let generator = generate::db::ddl::DdlGenerator::new(&output_dir);
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

// === DDL No tenant_id Tests ===

#[tokio::test]
async fn candidate_ddl_no_tenant_id() {
    let mock = setup_mock().await;
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-harness-no-tenant-id");

    let generator = generate::db::ddl::DdlGenerator::new(&output_dir);
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

/// DDL generator must inject FK column + foreign key constraint for ParentCandidate relationships.
#[tokio::test]
async fn ddl_generator_injects_fk_for_parent_candidate() {
    let (mock, candidates) = parent_child_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-ddl-fk-injection");

    let generator =
        generate::db::ddl::DdlGenerator::new(&output_dir).with_parent_candidates(candidates);
    let files = generator
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
    let generator = BasejumpSetupGenerator::new(&output_dir)
        .with_dialect(dialect_for_target(DatabaseTarget::Sqlite));
    let files = generator
        .generate(
            &mock,
            &config,
            &generation_order,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &mock,
            &config,
            &generation_order,
            &tera,
            &test_project_config(),
        )
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
        .generate(
            &mock,
            &config,
            &generation_order,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();
    assert!(
        files3.is_empty(),
        "PlatformSchemaGenerator should return empty for SQLite. Got {} files",
        files3.len()
    );
}
