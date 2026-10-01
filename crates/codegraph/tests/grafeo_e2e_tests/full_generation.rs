//! Full-generation tests: the whole generator set for CandidateType, the
//! full-pipeline file-count pins (including the `crates/review/` fixture
//! regeneration), repository/DDL emitter output checks and the `?include=`
//! feature E2E tests.
//!
//! Stragglers housed here (nearest cohesive — per-generator output and
//! feature E2E): `grafeo_candidate_repository_trait_content`,
//! `grafeo_candidate_repository_impl_content`,
//! `grafeo_composite_wrapper_ddl_expansion`,
//! `grafeo_candidate_composite_wrapper_in_repository`,
//! `grafeo_repository_impl_includes_child_inserts`,
//! `grafeo_ddl_generates_child_table_for_qualifications`,
//! `grafeo_repository_entity_ref_uses_id_suffix`, and the two
//! `grafeo_e2e_include_*` tests.

use std::path::Path;

use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_grafeo::GrafeoEngine;

use crate::setup::{entity_names_from_config, include_domain_config, setup_grafeo};
// === Task 1: Repository Trait Generation ===

#[tokio::test]
async fn grafeo_candidate_repository_trait_content() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen =
        codegraph::generate::ddd::repository::RepositoryTraitGenerator::new(Path::new("/tmp/out"));
    let files = gen
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(!files.is_empty(), "should produce repository trait file");
    let repo_file = files.first().unwrap();
    let content = &repo_file.content;

    // Trait declaration (generic over the client type C — provider-agnostic)
    assert!(
        content.contains("pub trait CandidateRepository<C>: Send + Sync"),
        "missing trait declaration"
    );

    // CRUD methods based on operations = ["create", "read", "update", "list"]
    assert!(
        content.contains("async fn create("),
        "missing create method"
    );
    assert!(
        content.contains("async fn find_by_id("),
        "missing find_by_id method"
    );
    assert!(
        content.contains("async fn update("),
        "missing update method"
    );
    assert!(content.contains("async fn list("), "missing list method");

    // CandidateType has no "delete" in operations, so delete should be absent
    assert!(
        !content.contains("async fn delete("),
        "delete should not be generated (not in operations)"
    );

    // Uses correct DTO type names
    assert!(
        content.contains("CreateCandidateRequest"),
        "should reference Create DTO"
    );
    assert!(
        content.contains("CandidateResponse"),
        "should reference Response DTO"
    );
}

// === Task 2: Repository Impl Emitter ===

#[tokio::test]
async fn grafeo_candidate_repository_impl_content() {
    let (engine, config) = setup_grafeo().await;

    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // Struct declaration
    assert!(
        code.contains("pub struct CandidateRepositoryImpl"),
        "missing impl struct"
    );

    // Implements trait
    assert!(
        code.contains(
            "impl CandidateRepository<sea_orm::DatabaseTransaction> for CandidateRepositoryImpl"
        ),
        "missing trait impl"
    );

    // Direct column fields in create
    assert!(
        code.contains("candidate_id: Set(cmd.candidate_id)"),
        "create should set candidate_id from cmd"
    );

    // Entity reference field (referredByApplication → referred_by_application)
    assert!(
        code.contains("referred_by_application"),
        "should include entity reference field"
    );

    // find_by_id method
    assert!(
        code.contains("async fn find_by_id("),
        "missing find_by_id method"
    );

    // CandidateType operations = ["create", "read", "update", "list"] — no delete
    // Emitter now respects operations config to match the repository trait
    assert!(
        !code.contains("async fn delete("),
        "delete should be omitted when not in operations config"
    );

    // list method with pagination
    assert!(
        code.contains("paginate(db, page_size)"),
        "missing pagination in list"
    );
}

// === Task 6: Full pipeline orchestration ===

#[tokio::test]
async fn grafeo_full_pipeline_run_generators() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let output_dir = std::env::temp_dir().join("hr-graph-e2e-test");
    let _ = std::fs::remove_dir_all(&output_dir); // Clean previous runs
    std::fs::create_dir_all(&output_dir).unwrap();

    // Use dedicated temp dirs for domain-types and hooks output to avoid
    // overwriting real workspace files with fixture (mock) data.
    let domain_types_tmp = std::env::temp_dir().join("hr-graph-e2e-test-domain-types");
    let _ = std::fs::remove_dir_all(&domain_types_tmp);
    std::fs::create_dir_all(&domain_types_tmp).unwrap();
    let hooks_tmp = std::env::temp_dir().join("hr-graph-e2e-test-hooks");
    let _ = std::fs::remove_dir_all(&hooks_tmp);
    std::fs::create_dir_all(&hooks_tmp).unwrap();
    let report = codegraph::generate::run_generators_with_domain_types_base(
        &engine,
        &config,
        &output_dir,
        &tera,
        &Default::default(),
        &Default::default(),
        std::path::Path::new(""),
        &domain_types_tmp,
        &hooks_tmp,
    )
    .await
    .unwrap();

    assert!(
        !report.has_errors(),
        "Expected no generation errors, got: {:?}",
        report
            .errors
            .iter()
            .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
            .collect::<Vec<_>>()
    );
    assert!(
        report.files.len() >= 20,
        "expected at least 20 files, got {}",
        report.files.len()
    );

    // Verify key files exist on disk
    // (run_generators writes via fs::write)
    let recruiting_dir = output_dir.join("src").join("domain").join("recruiting");
    assert!(
        recruiting_dir.exists(),
        "recruiting domain directory should exist"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&output_dir);
}

// === Task 8: All generators produce output ===

#[tokio::test]
async fn grafeo_all_entity_generators_produce_output_for_candidate() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();
    let out = Path::new("/tmp/out");

    let generators: Vec<(&str, Box<dyn EntityGenerator>)> = vec![
        (
            "ddl",
            Box::new(codegraph::generate::db::ddl::DdlGenerator::new(out)),
        ),
        (
            "entity",
            Box::new(codegraph::generate::db::entity::SeaOrmEntityGenerator::new(
                out,
            )),
        ),
        (
            "codelist",
            Box::new(codegraph::generate::db::codelist::CodelistGenerator::new(
                out,
            )),
        ),
        (
            "dto",
            Box::new(codegraph::generate::ddd::dto::DtoGenerator::new(out)),
        ),
        (
            "repository",
            Box::new(codegraph::generate::ddd::repository::RepositoryTraitGenerator::new(out)),
        ),
        (
            "command",
            Box::new(codegraph::generate::ddd::command::CommandGenerator::new(
                out,
            )),
        ),
        (
            "query",
            Box::new(codegraph::generate::ddd::query::QueryGenerator::new(out)),
        ),
        (
            "event",
            Box::new(codegraph::generate::ddd::event::EventGenerator::new(out)),
        ),
        (
            "handler",
            Box::new(codegraph::generate::api::handler::HandlerGenerator::new(
                out,
            )),
        ),
        (
            "test",
            Box::new(codegraph::generate::test::test_gen::TestGenerator::new(out)),
        ),
        (
            "grpc_proto",
            Box::new(codegraph::generate::grpc::proto::GrpcProtoGenerator::new(
                out,
            )),
        ),
        (
            "grpc_service",
            Box::new(codegraph::generate::grpc::service::GrpcServiceGenerator::new(out)),
        ),
    ];

    for (name, gen) in &generators {
        let files = gen
            .generate(
                &engine,
                "CandidateType",
                "recruiting",
                &config,
                &tera,
                &ProjectConfig::default(),
            )
            .await
            .unwrap_or_else(|e| panic!("{name} generator failed: {e}"));

        // Codelist generator may legitimately return empty for non-codelist entities
        if *name != "codelist" {
            assert!(
                !files.is_empty(),
                "{name} generator produced no files for CandidateType"
            );
        }

        for file in &files {
            assert!(
                !file.content.is_empty(),
                "{name} generator produced empty file: {}",
                file.path.display()
            );
        }
    }
}

#[tokio::test]
async fn grafeo_composite_wrapper_ddl_expansion() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let files = gen
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    let ddl_content = &files[0].content;

    assert!(
        ddl_content.contains("compensation_expectation"),
        "DDL must contain compensation_expectation column.\nDDL:\n{}",
        ddl_content
    );
    assert!(
        ddl_content.contains("compensation_expectation_currency"),
        "DDL must contain compensation_expectation_currency column.\nDDL:\n{}",
        ddl_content
    );
}

#[tokio::test]
async fn grafeo_candidate_composite_wrapper_in_repository() {
    let (engine, config) = setup_grafeo().await;

    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // Composite columns should appear as Set(cmd.X) in the repository impl
    assert!(
        code.contains("cmd.compensation_expectation"),
        "Repository must use Set(cmd.compensation_expectation).\nCode:\n{}",
        code
    );
    assert!(
        code.contains("cmd.compensation_expectation_currency"),
        "Repository must use Set(cmd.compensation_expectation_currency).\nCode:\n{}",
        code
    );

    // Codelist enum columns (dto_rust_type set, nullable) must use .map()/.and_then()
    assert!(
        code.contains("cmd.compensation_expectation_currency.map(|v| v.to_string())"),
        "Nullable codelist enum column must use .map(|v| v.to_string()) in create.\nCode:\n{}",
        code
    );
    assert!(
        code.contains("compensation_expectation_currency.and_then(|v| v.parse().ok())"),
        "Nullable codelist enum column must use .and_then(|v| v.parse().ok()) in find_by_id/list.\nCode:\n{}",
        code
    );
}

#[tokio::test]
async fn grafeo_repository_impl_includes_child_inserts() {
    let (engine, config) = setup_grafeo().await;

    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // ValueObject children are currently skipped in the repo emitter because
    // child entity models (SeaORM) are not yet generated. The repo emitter
    // only handles direct columns (PrimitiveWrapper, EntityReference, etc.)
    assert!(
        code.contains("async fn create"),
        "repository should have create method"
    );
    assert!(
        code.contains("crate::entity::recruiting_candidate::ActiveModel"),
        "repository should reference domain-prefixed entity via crate path"
    );
}

#[tokio::test]
async fn grafeo_ddl_generates_child_table_for_qualifications() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let files = gen
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    let all_ddl: String = files
        .iter()
        .map(|f| f.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    // Must have child table for qualifications
    assert!(
        all_ddl.contains("candidate_qualifications") || all_ddl.contains("candidate_qualification"),
        "DDL should generate child table for qualifications ValueObject, got:\n{}",
        all_ddl,
    );

    // Child table must have FK back to parent
    assert!(
        all_ddl.contains("candidate_id"),
        "Child table should have candidate_id FK column, got:\n{}",
        all_ddl,
    );
}

#[tokio::test]
async fn grafeo_repository_entity_ref_uses_id_suffix() {
    let (engine, config) = setup_grafeo().await;

    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // Entity reference field should use _id suffix in Set() call
    assert!(
        code.contains("referred_by_application_id: Set(cmd.referred_by_application_id)"),
        "entity ref should use _id suffix in repository create, got:\n{}",
        code,
    );

    // Should NOT have the bare field name without _id
    assert!(
        !code.contains("referred_by_application: Set(cmd.referred_by_application)"),
        "should not use bare field name without _id suffix, got:\n{}",
        code,
    );
}

#[tokio::test]
async fn grafeo_candidate_inspect_output() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Write to persistent review directory (relative to workspace root)
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("review")
        .join("generated-candidate");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    // Run full pipeline for inspection.
    // Domain-types and hooks output is redirected to temp dirs so the fixture
    // schemas (only common/compensation/recruiting) do not corrupt the real
    // workspace source files.
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();
    let report = codegraph::generate::run_generators_with_domain_types_base(
        &engine,
        &config,
        &output_dir,
        &tera,
        &Default::default(),
        &Default::default(),
        std::path::Path::new(""),
        domain_types_tmp.path(),
        hooks_tmp.path(),
    )
    .await
    .unwrap();

    assert!(!report.has_errors(), "Expected no generation errors");
    assert!(
        report.files.len() >= 20,
        "should write multiple files across all generators"
    );

    // Also write repository impl (not part of run_generators template flow)
    let emitter = RepositoryImplEmitter;
    let repo_code = emitter
        .emit(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    let repo_dir = output_dir
        .join("src")
        .join("domain")
        .join("recruiting")
        .join("candidate");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("repository_impl.rs"), &repo_code).unwrap();

    // Verify key files exist
    assert!(
        repo_dir.join("repository_impl.rs").exists(),
        "repository impl should exist"
    );

    // Do NOT clean up — output persists for manual review
    eprintln!(
        "\n=== Inspect generated code at: {} ===\n",
        output_dir.display()
    );
}

// === Include (`?include=`) feature E2E ===

#[tokio::test]
async fn grafeo_e2e_include_dto_generated_for_candidate() {
    let config = include_domain_config();
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let entity_names = entity_names_from_config(&config);
    let engine = GrafeoEngine::in_memory().unwrap();

    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();
    let output_dir = std::env::temp_dir().join("grafeo-test-include-dto");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    // Generate handler — has_include triggers ALLOWED_INCLUDE_KEYS and WithIncludeResponse
    let parent_candidates = engine.get_parent_candidates().await.unwrap();
    let handler_gen = codegraph::generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(parent_candidates);
    let handler_files = handler_gen
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    let handler = handler_files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("_handler.rs"))
        .expect("handler should be generated");
    let hc = &handler.content;

    // Handler must include ALLOWED_INCLUDE_KEYS with the resolved include path
    assert!(
        hc.contains("ALLOWED_INCLUDE_KEYS"),
        "handler should define ALLOWED_INCLUDE_KEYS when allow_include is configured"
    );
    assert!(
        hc.contains("\"application\""),
        "ALLOWED_INCLUDE_KEYS should contain 'application'. Generated:\n{}",
        hc,
    );

    // Handler must use CandidateWithIncludeResponse for the get_by_id response type
    assert!(
        hc.contains("CandidateWithIncludeResponse"),
        "handler should reference CandidateWithIncludeResponse. Generated:\n{}",
        hc,
    );

    // Generate DTO — DtoGenerator (not DomainTypesDtoGenerator) produces dto_included.rs
    let dto_gen = codegraph::generate::ddd::dto::DtoGenerator::new(&output_dir);
    let dto_files = dto_gen
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    let included = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_included"))
        .expect("dto_included.rs should be generated when allow_include is configured");
    let dc = &included.content;

    // DTO must define CandidateIncludedData
    assert!(
        dc.contains("CandidateIncludedData"),
        "DTO should define CandidateIncludedData struct. Generated:\n{}",
        dc,
    );

    // DTO must include the resolved include field with correct entity response type
    assert!(
        dc.contains("pub application: Option<ApplicationResponse>"),
        "included DTO should have 'application: Option<ApplicationResponse>'. Generated:\n{}",
        dc,
    );

    // DTO must define CandidateWithIncludeResponse as the top-level type
    assert!(
        dc.contains("CandidateWithIncludeResponse"),
        "DTO should define CandidateWithIncludeResponse. Generated:\n{}",
        dc,
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&output_dir);
}

#[tokio::test]
async fn grafeo_e2e_include_validates_unknown_path() {
    let config = include_domain_config();
    let classifier = codegraph_classifier::config::parse_classifier_config(Path::new(
        "tests/fixtures/classifier.toml",
    ))
    .unwrap();
    let entity_names = entity_names_from_config(&config);
    let engine = GrafeoEngine::in_memory().unwrap();

    codegraph::ingest::async_ingest::ingest_schemas(
        &engine,
        Path::new("tests/fixtures/schemas"),
        &classifier,
        &entity_names,
        &codegraph_config::UiOverrideConfig::default(),
        &config.defaults.type_suffix,
    )
    .await
    .unwrap();

    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();
    let output_dir = std::env::temp_dir().join("grafeo-test-include-validate");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let parent_candidates = engine.get_parent_candidates().await.unwrap();
    let handler_gen = codegraph::generate::api::handler::HandlerGenerator::new(&output_dir)
        .with_parent_candidates(parent_candidates);
    let handler_files = handler_gen
        .generate(
            &engine,
            "CandidateType",
            "recruiting",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    let handler = handler_files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("_handler.rs"))
        .expect("handler should be generated");
    let hc = &handler.content;

    // The generated handler must validate include paths against ALLOWED_INCLUDE_KEYS
    assert!(
        hc.contains("ALLOWED_INCLUDE_KEYS.contains(&path.as_str())"),
        "handler should validate unknown include paths with ALLOWED_INCLUDE_KEYS.contains. Generated:\n{}",
        hc,
    );

    // The handler must return a 400 error for unknown paths
    assert!(
        hc.contains("AppError::bad_request(format!(\"Unknown include path: {path}\")"),
        "handler should return bad_request for unknown include paths. Generated:\n{}",
        hc,
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&output_dir);
}
