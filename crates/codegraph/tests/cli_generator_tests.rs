//! Tests for the CLI generator pipeline.
//!
//! Validates that the three CLI generators (entity command, domain command,
//! scaffold) produce correct Rust source files from mock schema data.

use codegraph_config::{UiDomainConfig, UiOverrideConfig};
use codegraph::generate;
use codegraph::generate::template_engine;
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph::generate::GenerationEntry;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};
use std::path::Path;

fn test_domain_config() -> codegraph_config::DomainConfig {
    codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml")).unwrap()
}

fn test_tera() -> tera::Tera {
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    template_engine::create_tera(&template_dir).unwrap()
}

fn test_project_config() -> codegraph::generate::ProjectConfig {
    codegraph::generate::ProjectConfig::default()
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
        PropertyNode {
            name: "recruiterId".to_string(),
            prop_type: "string".to_string(),
            description: Some("The recruiter assigned to this candidate".to_string()),
            format: Some("uuid".to_string()),
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "recruiter_id".to_string(),
            pg_column_type: "UUID".to_string(),
            rust_field_name: "recruiter_id".to_string(),
            rust_field_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
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

fn setup_mock() -> MockEngine {
    MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties("CandidateType", candidate_properties())
        .build()
}

// === CLI Entity Command Generator Tests ===

#[tokio::test]
async fn cli_command_generator_includes_filter_fields_in_output() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-filter");

    let gen = generate::cli::command::CliCommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let file = &files[0];

    // ListArgs should use page/page_size instead of limit/offset
    assert!(
        file.content.contains("pub page:"),
        "Should have --page arg, got:\n{}",
        file.content
    );
    assert!(
        file.content.contains("pub page_size:"),
        "Should have --page-size arg"
    );
    assert!(
        !file.content.contains("pub limit:"),
        "Should NOT have old --limit arg"
    );
    assert!(
        !file.content.contains("pub offset:"),
        "Should NOT have old --offset arg"
    );

    // Should have --filter repeatable arg
    assert!(
        file.content.contains("pub filters: Vec<(String, String)>"),
        "Should have repeatable --filter arg"
    );

    // Should NOT have hardcoded --status
    assert!(
        !file.content.contains("pub status: Option<String>"),
        "Should NOT have hardcoded --status arg (use --filter instead)"
    );

    // Execute function should build filter[field]=value query params
    assert!(
        file.content.contains("filter["),
        "Should build filter[field]=value query params"
    );

    // Help text should list auto-discovered filter fields
    assert!(
        file.content.contains("recruiter_id"),
        "Help text should list 'recruiter_id' as an allowed filter field"
    );
}

#[tokio::test]
async fn cli_command_generator_produces_entity_commands() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-cmd");

    let gen = generate::cli::command::CliCommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert_eq!(files.len(), 1, "Should produce one file per entity");

    let file = &files[0];
    assert!(
        file.path
            .to_string_lossy()
            .contains("cli/src/commands/recruiting/candidate.rs"),
        "File should be at cli/src/commands/recruiting/candidate.rs, got: {:?}",
        file.path
    );

    // Should contain the enum with CRUD subcommands
    assert!(
        file.content.contains("enum CandidateCommand"),
        "Should define CandidateCommand enum"
    );
    assert!(
        file.content.contains("List("),
        "Should have List variant (operations include list)"
    );
    assert!(
        file.content.contains("Get {"),
        "Should have Get variant (operations include read)"
    );
    assert!(
        file.content.contains("Create("),
        "Should have Create variant (operations include create)"
    );
    assert!(
        file.content.contains("Update {"),
        "Should have Update variant (operations include update)"
    );
}

#[tokio::test]
async fn cli_command_generator_includes_workflow_subcommands() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-wf");

    let gen = generate::cli::command::CliCommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let file = &files[0];

    // CandidateType in test fixture has workflow enabled
    assert!(
        file.content.contains("Transition {"),
        "Should have Transition variant for workflow entities"
    );
    assert!(
        file.content.contains("Workflow {"),
        "Should have Workflow variant for workflow entities"
    );
    assert!(
        file.content.contains("WorkflowHistory {"),
        "Should have WorkflowHistory variant for workflow entities"
    );
}

#[tokio::test]
async fn cli_command_generator_includes_field_args_for_create() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-fields");

    let gen = generate::cli::command::CliCommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let file = &files[0];

    // given_name is required, so it should appear as a CLI arg
    assert!(
        file.content.contains("given_name"),
        "Should include required field 'given_name' as a CLI arg"
    );

    // Should have --from-file and --json options
    assert!(
        file.content.contains("from_file"),
        "Should support --from-file for JSON input"
    );
    assert!(
        file.content.contains("pub json: Option<String>"),
        "Should support --json for inline JSON input"
    );

    // Should import from shared util module instead of defining inline
    assert!(
        file.content.contains("use crate::util::"),
        "Should import shared util functions"
    );
    assert!(
        !file.content.contains("fn resolve_body("),
        "Should NOT define resolve_body inline (extracted to util)"
    );
    assert!(
        !file.content.contains("fn parse_key_value("),
        "Should NOT define parse_key_value inline (extracted to util)"
    );
}

#[tokio::test]
async fn cli_command_generator_includes_api_calls() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-api");

    let gen = generate::cli::command::CliCommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    let file = &files[0];

    assert!(
        file.content.contains("/api/recruiting/candidates"),
        "Should use correct API base path"
    );
    assert!(
        file.content.contains("client.get("),
        "Should call client.get for read operations"
    );
    assert!(
        file.content.contains("client.post("),
        "Should call client.post for create operations"
    );
}

#[tokio::test]
async fn cli_command_generator_skips_empty_table_name() {
    let mut schema = candidate_schema();
    schema.pg_table_name = String::new();

    let mock = MockEngine::builder()
        .with_schema(schema)
        .with_properties("CandidateType", candidate_properties())
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-skip");

    let gen = generate::cli::command::CliCommandGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, "CandidateType", "recruiting", &config, &tera, &test_project_config())
        .await
        .unwrap();

    assert!(
        files.is_empty(),
        "Should produce no files for entities with empty table name"
    );
}

// === CLI Domain Generator Tests ===

#[tokio::test]
async fn cli_domain_generator_produces_domain_module() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-domain");

    let gen = generate::cli::domain::CliDomainGenerator::new(&output_dir);
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

    assert_eq!(files.len(), 1, "Should produce one file per domain");

    let file = &files[0];
    assert!(
        file.path
            .to_string_lossy()
            .contains("cli/src/commands/recruiting/mod.rs"),
        "File should be domain mod.rs, got: {:?}",
        file.path
    );

    assert!(
        file.content.contains("pub mod candidate;"),
        "Should declare entity submodule"
    );
    assert!(
        file.content.contains("Command"),
        "Should define domain-level command enum"
    );
    assert!(
        file.content.contains("Candidate {"),
        "Should have entity variant in domain command"
    );
}

#[tokio::test]
async fn cli_domain_generator_deduplicates_modules() {
    let mut schema2 = candidate_schema();
    schema2.schema_id = "recruiting/json/CandidateType2.json".to_string();
    schema2.title = "CandidateType2".to_string();
    // Same table name - should be deduplicated
    schema2.pg_table_name = "candidate".to_string();

    let mock = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_schema(schema2)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-dedup");

    let gen = generate::cli::domain::CliDomainGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "recruiting",
            &["CandidateType".to_string(), "CandidateType2".to_string()],
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let file = &files[0];
    let count = file.content.matches("pub mod candidate;").count();
    assert_eq!(
        count, 1,
        "Should deduplicate entity modules with same table name"
    );
}

// === CLI Scaffold Generator Tests ===

#[tokio::test]
async fn cli_scaffold_generator_produces_all_files() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-scaffold");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let file_names: Vec<String> = files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    assert!(
        file_names.iter().any(|f| f.contains("cli/src/main.rs")),
        "Should produce main.rs, got: {:?}",
        file_names
    );
    assert!(
        file_names.iter().any(|f| f.contains("cli/Cargo.toml")),
        "Should produce Cargo.toml"
    );
    assert!(
        file_names.iter().any(|f| f.contains("cli/src/config.rs")),
        "Should produce config.rs"
    );
    assert!(
        file_names.iter().any(|f| f.contains("cli/src/output.rs")),
        "Should produce output.rs"
    );
    assert!(
        file_names.iter().any(|f| f.contains("cli/src/client.rs")),
        "Should produce client.rs"
    );
    assert!(
        file_names.iter().any(|f| f.contains("cli/src/util.rs")),
        "Should produce util.rs"
    );
    assert!(
        file_names
            .iter()
            .any(|f| f.contains("cli/src/commands/mod.rs")),
        "Should produce commands/mod.rs"
    );
}

#[tokio::test]
async fn cli_scaffold_main_contains_domain_routing() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-main");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("main.rs"))
        .expect("Should produce main.rs");

    assert!(
        main_file.content.contains("Recruiting"),
        "main.rs should contain Recruiting domain command variant"
    );
    assert!(
        main_file.content.contains("commands::recruiting"),
        "main.rs should route to recruiting domain module"
    );
    assert!(
        main_file.content.contains("Config {"),
        "main.rs should have Config subcommand"
    );
    assert!(
        main_file.content.contains("SetUrl"),
        "main.rs should have SetUrl config command"
    );
    assert!(
        main_file.content.contains("SetToken"),
        "main.rs should have SetToken config command"
    );
}

#[tokio::test]
async fn cli_scaffold_cargo_toml_has_correct_deps() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-cargo");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("Cargo.toml"))
        .expect("Should produce Cargo.toml");

    assert!(
        cargo_file.content.contains("clap"),
        "Should depend on clap for CLI parsing"
    );
    assert!(
        cargo_file.content.contains("reqwest"),
        "Should depend on reqwest for HTTP client"
    );
    assert!(
        cargo_file.content.contains("tokio"),
        "Should depend on tokio for async runtime"
    );
    assert!(
        cargo_file.content.contains("serde_json"),
        "Should depend on serde_json for JSON handling"
    );
    assert!(
        cargo_file.content.contains("hr-cli"),
        "Should use hr-cli as package name"
    );
}

#[tokio::test]
async fn cli_scaffold_client_has_crud_methods() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-client");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let client_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("client.rs"))
        .expect("Should produce client.rs");

    assert!(
        client_file.content.contains("pub async fn get("),
        "Client should have get method"
    );
    assert!(
        client_file.content.contains("pub async fn post("),
        "Client should have post method"
    );
    assert!(
        client_file.content.contains("pub async fn put("),
        "Client should have put method"
    );
    assert!(
        client_file.content.contains("pub async fn delete("),
        "Client should have delete method"
    );
    assert!(
        client_file.content.contains("Bearer"),
        "Client should support Bearer token auth"
    );
}

#[tokio::test]
async fn cli_scaffold_output_has_format_options() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-output");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let output_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("output.rs"))
        .expect("Should produce output.rs");

    assert!(
        output_file.content.contains("Table"),
        "Should support Table output format"
    );
    assert!(
        output_file.content.contains("Json"),
        "Should support Json output format"
    );
    assert!(
        output_file.content.contains("JsonPretty"),
        "Should support JsonPretty output format"
    );
    assert!(
        output_file.content.contains("fn print_table"),
        "Should have table rendering function"
    );
}

// === Multi-domain tests ===

#[tokio::test]
async fn cli_scaffold_handles_multiple_domains() {
    let name_schema = SchemaNode {
        schema_id: "common/json/NameType.json".to_string(),
        title: "NameType".to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/NameType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Name".to_string(),
        pg_table_name: "name".to_string(),
        api_path_segment: "names".to_string(),
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
        .with_schema(candidate_schema())
        .with_schema(name_schema)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-multi");

    let generation_order = vec![
        GenerationEntry {
            schema_title: "NameType".to_string(),
            domain: "common".to_string(),
            pg_schema: "common".to_string(),
            is_cyclic: false,
        },
        GenerationEntry {
            schema_title: "CandidateType".to_string(),
            domain: "recruiting".to_string(),
            pg_schema: "recruiting".to_string(),
            is_cyclic: false,
        },
    ];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("main.rs"))
        .expect("Should produce main.rs");

    assert!(
        main_file.content.contains("Common"),
        "Should contain Common domain variant"
    );
    assert!(
        main_file.content.contains("Recruiting"),
        "Should contain Recruiting domain variant"
    );

    let cmd_mod = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("commands/mod.rs"))
        .expect("Should produce commands/mod.rs");

    assert!(
        cmd_mod.content.contains("pub mod common;"),
        "Should declare common module"
    );
    assert!(
        cmd_mod.content.contains("pub mod recruiting;"),
        "Should declare recruiting module"
    );
}

// === Full pipeline integration test ===
// This test runs `run_generators` end-to-end with a mock graph,
// verifying that CLI files appear alongside API/DDL/DDD files.

#[tokio::test]
async fn cli_files_produced_by_full_pipeline() {
    use codegraph_core::types::{ColumnInfo, CompositionNode, CompositionTree};
    use codegraph_type_contracts::RefClassificationKind;

    let mock = MockEngine::builder()
        .with_schema(candidate_schema())
        .with_properties("CandidateType", candidate_properties())
        .with_composition_tree(
            "CandidateType",
            CompositionTree {
                root: CompositionNode {
                    field_name: "candidate".to_string(),
                    schema_title: "CandidateType".to_string(),
                    table_schema: "recruiting".to_string(),
                    table_name: "candidate".to_string(),
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
            },
        )
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = tempfile::tempdir().expect("create temp dir");
    let output_path = output_dir.path();

    let ui_overrides = UiOverrideConfig::default();
    let ui_domains = UiDomainConfig::default();
    let schema_base_dir = Path::new("");
    // Use dedicated temp dirs for domain-types and hooks output so the test
    // does not overwrite real workspace files with mock data.
    let domain_types_tmp = tempfile::tempdir().expect("create domain-types temp dir");
    let hooks_tmp = tempfile::tempdir().expect("create hooks temp dir");
    let report = generate::run_generators_with_domain_types_base(
        &mock,
        &config,
        output_path,
        &tera,
        &ui_overrides,
        &ui_domains,
        schema_base_dir,
        domain_types_tmp.path(),
        hooks_tmp.path(),
    )
    .await
    .expect("run_generators should succeed");

    // Collect all generated file paths for inspection
    let all_paths: Vec<String> = report
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    // Verify pipeline didn't error on CLI generators
    for err in &report.errors {
        if err.generator.starts_with("cli") {
            panic!(
                "CLI generator '{}' errored for entity '{}': {}",
                err.generator, err.entity, err.source
            );
        }
    }

    // --- CLI entity command files ---
    let cli_entity_files: Vec<&String> = all_paths
        .iter()
        .filter(|p| p.contains("cli/src/commands/") && p.ends_with(".rs") && !p.ends_with("mod.rs"))
        .collect();
    assert!(
        !cli_entity_files.is_empty(),
        "Pipeline should produce CLI entity command files. All paths:\n{}",
        all_paths
            .iter()
            .filter(|p| p.contains("cli"))
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        cli_entity_files
            .iter()
            .any(|p| p.contains("recruiting/candidate.rs")),
        "Should produce recruiting/candidate.rs CLI command"
    );

    // --- CLI domain mod files ---
    assert!(
        all_paths
            .iter()
            .any(|p| p.contains("cli/src/commands/recruiting/mod.rs")),
        "Should produce recruiting domain mod.rs"
    );

    // --- CLI scaffold files ---
    assert!(
        all_paths.iter().any(|p| p.contains("cli/src/main.rs")),
        "Should produce CLI main.rs"
    );
    assert!(
        all_paths.iter().any(|p| p.contains("cli/Cargo.toml")),
        "Should produce CLI Cargo.toml"
    );
    assert!(
        all_paths
            .iter()
            .any(|p| p.contains("cli/src/commands/mod.rs")),
        "Should produce CLI commands/mod.rs"
    );

    // --- Verify scaffold main.rs has domain subcommands ---
    let main_file = report
        .files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("cli/src/main.rs"))
        .expect("CLI main.rs should exist in report");
    assert!(
        main_file.content.contains("Recruiting"),
        "CLI main.rs should contain Recruiting variant. Content:\n{}",
        &main_file.content[..main_file.content.len().min(500)]
    );

    // --- Verify commands/mod.rs has domain modules ---
    let cmd_mod = report
        .files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("cli/src/commands/mod.rs"))
        .expect("CLI commands/mod.rs should exist in report");
    assert!(
        cmd_mod.content.contains("pub mod recruiting;"),
        "CLI commands/mod.rs should declare recruiting module. Content:\n{}",
        cmd_mod.content
    );

    // --- Verify files were actually written to disk ---
    assert!(
        output_path.join("cli/src/main.rs").exists(),
        "CLI main.rs should exist on disk"
    );
    assert!(
        output_path.join("cli/Cargo.toml").exists(),
        "CLI Cargo.toml should exist on disk"
    );
    assert!(
        output_path
            .join("cli/src/commands/recruiting/candidate.rs")
            .exists(),
        "CLI candidate.rs should exist on disk"
    );

    // --- Also verify API scaffold still works (no regression) ---
    assert!(
        all_paths
            .iter()
            .any(|p| p.contains("src/main.rs") && !p.contains("cli")),
        "API main.rs should still be generated"
    );
}

// === CLI Version Information Tests ===

#[tokio::test]
async fn cli_scaffold_cargo_toml_has_shadow_rs() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-shadow");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("Should have Cargo.toml");

    assert!(
        cargo_file.content.contains("shadow-rs"),
        "CLI Cargo.toml should have shadow-rs dependency"
    );
    assert!(
        cargo_file.content.contains("[build-dependencies]"),
        "CLI Cargo.toml should have [build-dependencies] section"
    );
}

#[tokio::test]
async fn cli_scaffold_generates_build_rs() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-build-rs");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let build_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("build.rs"))
        .expect("CLI should generate build.rs");

    assert!(
        build_rs.content.contains("ShadowBuilder::builder()"),
        "CLI build.rs should invoke ShadowBuilder::builder()"
    );
}

#[tokio::test]
async fn cli_scaffold_main_has_version_subcommand() {
    let mock = setup_mock();
    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-cli-version-cmd");

    let generation_order = vec![GenerationEntry {
        schema_title: "CandidateType".to_string(),
        domain: "recruiting".to_string(),
        pg_schema: "recruiting".to_string(),
        is_cyclic: false,
    }];

    let gen = generate::cli::scaffold::CliScaffoldGenerator::new(&output_dir);
    let files = gen
        .generate(&mock, &config, &generation_order, &tera, &test_project_config())
        .await
        .unwrap();

    let main_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("main.rs"))
        .expect("Should have main.rs");

    // shadow-rs macro
    assert!(
        main_file.content.contains("shadow!(build)"),
        "CLI main.rs should invoke shadow!(build)"
    );

    // Version subcommand in Commands enum
    assert!(
        main_file.content.contains("Version"),
        "Commands enum should have Version variant"
    );

    // Client version output (always shown)
    assert!(
        main_file.content.contains("Client Version"),
        "version handler should print Client Version"
    );

    // Server version with three states
    assert!(
        main_file.content.contains("Server Version"),
        "version handler should print Server Version"
    );
    assert!(
        main_file.content.contains("not configured"),
        "version handler should handle not-configured state"
    );
    assert!(
        main_file.content.contains("unreachable"),
        "version handler should handle unreachable state"
    );

    // Uses shadow-rs constants for client version
    assert!(
        main_file.content.contains("build::SHORT_COMMIT"),
        "version handler should use SHORT_COMMIT for client version"
    );
    assert!(
        main_file.content.contains("build::BUILD_TIME_3339"),
        "version handler should use BUILD_TIME_3339 for client version"
    );

    // Fetches /version from server
    assert!(
        main_file.content.contains("/version"),
        "version handler should fetch /version from server"
    );
}
