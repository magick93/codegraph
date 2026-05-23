use codegraph::generate;
use codegraph::generate::traits::EntityGenerator;
use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    CodeList, ColumnInfo, CompositionNode, CompositionTree, EnumValue, PropertyNode, SchemaNode,
};
use codegraph_type_contracts::RefClassificationKind;
use std::path::Path;

fn mock_schema(
    schema_id: &str,
    title: &str,
    table_name: &str,
    schema_name: &str,
    classification: &str,
) -> SchemaNode {
    let rust_type_name = title.replace("Type", "");
    SchemaNode {
        schema_id: schema_id.to_string(),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: classification.to_string(),
        domain: Some(schema_name.to_string()),
        rel_path: schema_id.to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: rust_type_name.clone(),
        pg_table_name: table_name.to_string(),
        api_path_segment: table_name.replace('_', "-"),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn mock_properties() -> Vec<PropertyNode> {
    vec![
        PropertyNode {
            name: "given_name".to_string(),
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
            name: "family_name".to_string(),
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

fn mock_composition_tree(schema_title: &str, table_name: &str, domain: &str) -> CompositionTree {
    CompositionTree {
        root: CompositionNode {
            field_name: table_name.to_string(),
            schema_title: schema_title.to_string(),
            table_schema: domain.to_string(),
            table_name: table_name.to_string(),
            fk: None,
            is_collection: false,
            columns: vec![
                ColumnInfo {
                    name: "given_name".to_string(),
                    description: Some("The person's given name".to_string()),
                    rust_type: "String".to_string(),
                    postgres_type: "TEXT".to_string(),
                    is_optional: false,
                    is_codelist_fk: false,
                    composite_columns: vec![],
                    is_array: false,
                    classification: Some(RefClassificationKind::PrimitiveWrapper),
                    fk_target: None,
                    check_values: vec![],
                },
                ColumnInfo {
                    name: "family_name".to_string(),
                    description: Some("The person's family name".to_string()),
                    rust_type: "String".to_string(),
                    postgres_type: "TEXT".to_string(),
                    is_optional: true,
                    is_codelist_fk: false,
                    composite_columns: vec![],
                    is_array: false,
                    classification: Some(RefClassificationKind::PrimitiveWrapper),
                    fk_target: None,
                    check_values: vec![],
                },
            ],
            jsonb_columns: vec![],
            children: vec![],
            composite_range: None,
            consumed_fields: vec![],
        },
    }
}

fn test_domain_config() -> codegraph_config::DomainConfig {
    codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml")).unwrap()
}

// === Generation Ordering Tests ===

#[tokio::test]
async fn test_generation_ordering_with_empty_graph() {
    let mock = MockEngine::new();
    let config = test_domain_config();
    let order = generate::compute_generation_order(&mock, &config)
        .await
        .unwrap();
    assert!(order.is_empty());
}

#[tokio::test]
async fn test_generation_ordering_respects_domain_order() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_schema(mock_schema(
            "common/json/NameType.json",
            "NameType",
            "name",
            "common",
            "entity_reference",
        ))
        .build();

    let config = test_domain_config();
    let order = generate::compute_generation_order(&mock, &config)
        .await
        .unwrap();

    assert_eq!(order.len(), 2);
    // Common should come before recruiting
    assert_eq!(order[0].domain, "common");
    assert_eq!(order[1].domain, "recruiting");
}

// === DDL Generator Tests ===

#[tokio::test]
async fn test_ddl_generator_produces_table_sql() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties("CandidateType", mock_properties())
        .with_composition_tree(
            "CandidateType",
            mock_composition_tree("CandidateType", "candidate", "recruiting"),
        )
        .build();

    let config = test_domain_config();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-ddl");
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera)
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
        "Should contain CREATE TABLE"
    );
    assert!(
        table_file.content.contains("given_name TEXT NOT NULL"),
        "Should contain given_name column"
    );
    assert!(
        table_file.content.contains("family_name TEXT"),
        "Should contain family_name column"
    );
}

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

    let gen = generate::db::entity::SeaOrmEntityGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera)
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

    let gen = generate::ddd::dto::DtoGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera)
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
        .emit(&mock, "CandidateType", "recruiting", &config, None)
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
        .emit(&mock, "CandidateType", "recruiting", &config, None)
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

/// Helper: build a PropertyNode with defaults, overriding key fields.
fn prop(
    name: &str,
    rust_type: &str,
    pg_type: &str,
    required: bool,
    classification: Option<&str>,
    classification_kind: Option<RefClassificationKind>,
    ref_target: Option<&str>,
    is_array: bool,
) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: required,
        is_nullable: !required,
        is_array,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: pg_type.to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: rust_type.to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: classification.unwrap_or("direct_column").to_string(),
        ref_target: ref_target.map(|s| s.to_string()),
        classification: classification.map(|s| s.to_string()),
        projection: None,
        classification_kind,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

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
        .emit(&mock, "CandidateType", "recruiting", &config, None)
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
        .emit(&mock, "CandidateType", "recruiting", &config, None)
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
        .emit(&mock, "CandidateType", "recruiting", &config, None)
        .await
        .unwrap();

    insta::assert_snapshot!("repo_structured_wrapper", code);
}

#[tokio::test]
async fn snapshot_repository_emitter_child_tables() {
    // Create a ValueObject child schema
    let child_schema = SchemaNode {
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
        .emit(&mock, "CandidateType", "recruiting", &config, None)
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
        )
        .await
        .unwrap();

    insta::assert_snapshot!("repo_with_parent_ref", code);
}

// === GenerationReport Tests ===

#[test]
fn generation_report_summary_with_errors() {
    use codegraph::error::Error;
    use codegraph::generate::report::{GenerationError, GenerationReport, GenerationWarning};

    let mut report = GenerationReport::new();
    report.errors.push(GenerationError {
        entity: "CandidateType".into(),
        generator: "ddl".into(),
        source: Error::SchemaNotFound("missing".into()),
    });
    report.warnings.push(GenerationWarning {
        entity: "OrderType".into(),
        generator: "codelist".into(),
        check: "empty_codelist",
        message: "no enum values".into(),
    });

    assert!(report.has_errors());
    let summary = report.summary();
    assert!(summary.contains("1 error"));
    assert!(summary.contains("1 warning"));
    assert!(summary.contains("CandidateType"));
}

#[test]
fn generation_report_summary_clean() {
    use codegraph::generate::report::GenerationReport;

    let report = GenerationReport::new();
    assert!(!report.has_errors());
    let summary = report.summary();
    assert!(summary.contains("0 errors"));
}

// === Template Engine Tests ===

#[test]
fn test_template_engine_loads_all_templates() {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    // Should have loaded templates from all subdirectories
    let names: Vec<&str> = tera.get_template_names().collect();
    assert!(
        names.iter().any(|n| n.starts_with("db/")),
        "Should have db/ templates"
    );
    assert!(
        names.iter().any(|n| n.starts_with("ddd/")),
        "Should have ddd/ templates"
    );
    assert!(
        names.iter().any(|n| n.starts_with("api/")),
        "Should have api/ templates"
    );
    assert!(
        names.iter().any(|n| n.starts_with("scaffold/")),
        "Should have scaffold/ templates"
    );
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

    let gen = DomainTypesCodelistGenerator::new_with_base(tmp_dir.clone());

    let mut tera = tera::Tera::default();
    tera.add_raw_template(
        "codelist/enum.tera",
        include_str!("../templates/codelist/enum.tera"),
    )
    .unwrap();

    let files = gen.generate_all(&mock, &tera).await.unwrap();

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
