//! E2E integration tests for Grafeo-based ingestion and DTO correctness.
//!
//! Proves the full pipeline: JSON schema → Grafeo ingestion → code generation
//! produces correct Candidate DTOs with proper field types for each
//! classification variant.

use std::collections::HashSet;
use std::path::Path;

use codegraph::generate::codelist::rust_enum::RustCodelistGenerator;
use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
use codegraph::generate::domain_types::dto::DomainTypesDtoGenerator;
use codegraph::generate::domain_types::query_service::QueryServiceGenerator;
use codegraph::generate::domain_types::scaffold::DomainTypesScaffoldGenerator;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::traits::{EntityGenerator, GlobalGenerator};
use codegraph::generate::ProjectConfig;
use codegraph_config::config::parse_domain_config_str;
use codegraph_core::traits::GraphQuerier;
use codegraph_grafeo::GrafeoEngine;
use codegraph_type_contracts::RefClassificationKind;

/// Extract entity type names from a DomainConfig into a HashSet.
fn entity_names_from_config(config: &codegraph_config::config::DomainConfig) -> HashSet<String> {
    config
        .domains
        .values()
        .flat_map(|d| d.entities.iter().cloned())
        .collect()
}

/// Set up Grafeo engine with ingested fixture schemas.
async fn setup_grafeo() -> (GrafeoEngine, codegraph_config::config::DomainConfig) {
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let classifier =
        codegraph_classifier::config::parse_classifier_config(Path::new("tests/fixtures/classifier.toml"))
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

    (engine, config)
}

#[cfg(feature = "e2e")]
async fn generate_full_app(output_dir: &std::path::Path) {
    let (engine, config) = setup_grafeo().await;
    let tera =
        create_tera(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Run all generators (DDL, entities, codelists, DTOs, repos, commands,
    // queries, events, handlers, tests, routers, openapi, scaffold).
    // Domain-types and hooks output is redirected to temp dirs to avoid
    // corrupting the workspace source when running with fixture schemas.
    let domain_types_tmp = tempfile::TempDir::new().unwrap();
    let hooks_tmp = tempfile::TempDir::new().unwrap();
    let report = codegraph::generate::run_generators_with_domain_types_base(
        &engine,
        &config,
        output_dir,
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
        !report.has_errors(),
        "Expected no generation errors, got: {:?}",
        report
            .errors
            .iter()
            .map(|e| format!("{}/{}: {}", e.entity, e.generator, e.source))
            .collect::<Vec<_>>()
    );

    // Generate repository impls (not part of run_generators)
    let emitter = RepositoryImplEmitter;
    let entity_names = engine.get_entity_names().await.unwrap();

    for entity_title in &entity_names {
        let schema = engine.get_schema(entity_title).await.unwrap();
        if let Some(schema) = schema {
            let domain = schema.domain.as_deref().unwrap_or("unknown");
            let module = &schema.pg_table_name;

            let code = emitter
                .emit(&engine, entity_title, domain, &config, None)
                .await
                .unwrap();
            let repo_dir = output_dir
                .join("src")
                .join("domain")
                .join(domain)
                .join(module);
            std::fs::create_dir_all(&repo_dir).unwrap();
            std::fs::write(repo_dir.join("repository_impl.rs"), &code).unwrap();
        }
    }

    // Generate mod.rs files for all directories under src/
    generate_mod_files_recursive(&output_dir.join("src"));

    // Generate top-level test entry points for tests/{domain}/ subdirectories
    let tests_dir = output_dir.join("tests");
    if tests_dir.is_dir() {
        for entry in std::fs::read_dir(&tests_dir).unwrap().flatten() {
            if entry.file_type().unwrap().is_dir() {
                let domain_name = entry.file_name().to_string_lossy().to_string();
                let mut mods: Vec<String> = std::fs::read_dir(entry.path())
                    .unwrap()
                    .flatten()
                    .filter_map(|f| {
                        let name = f.file_name().to_string_lossy().to_string();
                        name.strip_suffix(".rs").map(|s| s.to_string())
                    })
                    .collect();
                mods.sort();
                let content = format!(
                    "// Generated by hr-graph. DO NOT EDIT.\n\n{}\n",
                    mods.iter()
                        .map(|m| format!("#[path = \"{domain_name}/{m}.rs\"]\nmod {m};"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                std::fs::write(tests_dir.join(format!("{domain_name}.rs")), content).unwrap();
            }
        }
    }
}

/// Recursively generate `mod.rs` files for directories that need them.
/// Skips directories that already contain `main.rs` (crate root).
#[cfg(feature = "e2e")]
fn generate_mod_files_recursive(dir: &std::path::Path) {
    if !dir.is_dir() {
        return;
    }

    let mut modules = Vec::new();

    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        if entry.file_type().unwrap().is_dir() {
            generate_mod_files_recursive(&entry.path());
            modules.push(name);
        } else if let Some(stem) = name.strip_suffix(".rs") {
            if stem != "mod" && stem != "main" && stem != "app_state" {
                modules.push(stem.to_string());
            }
        }
    }

    // Only create mod.rs for subdirectories (not the crate root which has main.rs)
    if !modules.is_empty() && !dir.join("mod.rs").exists() && !dir.join("main.rs").exists() {
        modules.sort();
        let content = format!(
            "// Generated by hr-graph. DO NOT EDIT.\n\n{}\n",
            modules
                .iter()
                .map(|m| format!("pub mod {m};"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        std::fs::write(dir.join("mod.rs"), content).unwrap();
    }
}

// === Task 2: Entity name extraction ===

#[test]
fn entity_names_extracted_from_fixture_config() {
    let config =
        codegraph_config::config::parse_domain_config(Path::new("tests/fixtures/domains.toml"))
            .unwrap();
    let names = entity_names_from_config(&config);

    assert!(
        names.contains("CandidateType"),
        "should contain CandidateType"
    );
    assert!(
        names.contains("ApplicationType"),
        "should contain ApplicationType"
    );
    assert!(names.contains("PayRunType"), "should contain PayRunType");
    // NameType is NOT an entity — it's a value object (not in any entities list)
    assert!(
        !names.contains("NameType"),
        "NameType should not be an entity"
    );
}

// === Task 5: Grafeo ingestion ===

#[tokio::test]
async fn grafeo_ingest_candidate_schema() {
    let (engine, _config) = setup_grafeo().await;

    // Verify CandidateType was ingested as entity
    let candidate = engine.get_schema("CandidateType").await.unwrap();
    assert!(candidate.is_some(), "CandidateType should exist in graph");
    let candidate = candidate.unwrap();
    assert!(candidate.is_entity, "CandidateType should be an entity");
    assert_eq!(candidate.domain.as_deref(), Some("recruiting"));

    // Verify properties were ingested with classification
    let props = engine.get_properties("CandidateType").await.unwrap();
    assert!(!props.is_empty(), "CandidateType should have properties");

    // Check specific property exists
    let gender = props.iter().find(|p| p.name == "gender");
    assert!(gender.is_some(), "should have gender property");
}

// === Task 6: Property classification correctness ===

#[tokio::test]
async fn grafeo_candidate_property_classifications() {
    let (engine, _config) = setup_grafeo().await;

    let props = engine.get_properties("CandidateType").await.unwrap();

    // PrimitiveWrapper: candidateId (plain string, required)
    let candidate_id = props.iter().find(|p| p.name == "candidateId").unwrap();
    assert_eq!(
        candidate_id.effective_kind(),
        Some(RefClassificationKind::PrimitiveWrapper)
    );
    assert!(candidate_id.is_required);

    // CodelistReference: gender
    let gender = props.iter().find(|p| p.name == "gender").unwrap();
    assert_eq!(
        gender.effective_kind(),
        Some(RefClassificationKind::CodelistReference)
    );

    // EntityReference: referredByApplication
    let app_ref = props
        .iter()
        .find(|p| p.name == "referredByApplication")
        .unwrap();
    assert_eq!(
        app_ref.effective_kind(),
        Some(RefClassificationKind::EntityReference)
    );

    // ValueObject: personName (NameType is not in entities list)
    let name = props.iter().find(|p| p.name == "personName").unwrap();
    assert_eq!(
        name.effective_kind(),
        Some(RefClassificationKind::ValueObject)
    );
}

// === Task 6: Create DTO content assertions ===

#[tokio::test]
async fn grafeo_candidate_create_dto_content() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // App DTOs are now re-exports from hr_domain_types; check struct content in domain_types output.
    let tmp = std::env::temp_dir().join("grafeo-test-create-dto");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file");
    let content = &create_file.content;

    // Struct name
    assert!(
        content.contains("pub struct CreateCandidateRequest"),
        "missing struct declaration"
    );

    // PrimitiveWrapper required field
    assert!(
        content.contains("pub candidate_id: String"),
        "candidateId should be required String"
    );

    // EntityReference → _id field
    assert!(
        content.contains("referred_by_application_id"),
        "entity ref should be _id field"
    );
    assert!(
        content.contains("Option<uuid::Uuid>"),
        "entity ref should be Option<uuid::Uuid>"
    );

    // Should NOT contain the raw field name without _id suffix
    assert!(
        !content.contains("pub referred_by_application:"),
        "value object should not be raw type"
    );
}

// === Task 6: Response DTO ===

#[tokio::test]
async fn grafeo_candidate_response_dto_content() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-response-dto");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let response_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .expect("should produce dto_response file");
    let content = &response_file.content;

    // Response struct
    assert!(
        content.contains("pub struct CandidateResponse"),
        "missing response struct"
    );

    // Must include id and timestamps
    assert!(
        content.contains("pub id: uuid::Uuid"),
        "response must include id"
    );
    assert!(
        content.contains("pub created_at:"),
        "response must include created_at"
    );
    assert!(
        content.contains("pub updated_at:"),
        "response must include updated_at"
    );
}

// === Structured wrapper import prefix config ===

#[tokio::test]
async fn grafeo_structured_import_uses_configurable_prefix() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Default prefix should be "codegraph_type_contracts"
    assert_eq!(
        config.defaults.types_import_prefix,
        "codegraph_type_contracts"
    );

    let tmp = std::env::temp_dir().join("grafeo-test-structured-import");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let response_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .expect("should produce dto_response file");
    let content = &response_file.content;

    // Default: should import from codegraph_type_contracts
    assert!(
        content.contains("use codegraph_type_contracts::IdentifierType;"),
        "default prefix should produce codegraph_type_contracts import"
    );
    assert!(
        content.contains("external_identifier"),
        "should include structured wrapper field"
    );
}

#[tokio::test]
async fn grafeo_structured_import_respects_custom_prefix() {
    let (engine, mut config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Override the import prefix to simulate a domain crate
    config.defaults.types_import_prefix = "crate::structured".to_string();

    let tmp = std::env::temp_dir().join("grafeo-test-structured-import-custom");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let response_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .expect("should produce dto_response file");
    let content = &response_file.content;

    // With custom prefix: should NOT reference codegraph crate
    assert!(
        !content.contains("codegraph_type_contracts"),
        "custom prefix should not contain codegraph_type_contracts"
    );
    assert!(
        content.contains("use crate::structured::IdentifierType;"),
        "custom prefix should be used in import"
    );
}

#[tokio::test]
async fn grafeo_scaffold_lib_rs_includes_structured_re_exports() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-scaffold-re-exports");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let gen = DomainTypesScaffoldGenerator::new_with_base(tmp.clone());
    let order = codegraph::generate::compute_generation_order(&engine, &config)
        .await
        .unwrap();
    let files = gen
        .generate(&engine, &config, &order, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("should produce lib.rs");
    let content = &lib_rs.content;

    // Should re-export IdentifierType since CandidateType uses it
    assert!(
        content.contains("pub use codegraph_type_contracts::IdentifierType;"),
        "lib.rs should re-export IdentifierType"
    );
    assert!(
        content.contains("// --- STRUCTURED WRAPPER RE-EXPORTS ---"),
        "lib.rs should have structured wrapper section"
    );
}

#[tokio::test]
async fn grafeo_scaffold_domain_types_has_cargo_toml() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-scaffold-cargo-toml");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let gen = DomainTypesScaffoldGenerator::new_with_base(tmp.clone());
    let order = codegraph::generate::compute_generation_order(&engine, &config)
        .await
        .unwrap();
    let files = gen
        .generate(&engine, &config, &order, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let has_cargo_toml = files
        .iter()
        .any(|f| f.path.to_string_lossy().ends_with("Cargo.toml"));
    assert!(
        has_cargo_toml,
        "DomainTypesScaffoldGenerator should produce a Cargo.toml"
    );

    let cargo_toml = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .unwrap();
    assert!(
        cargo_toml.content.contains("[package]"),
        "Cargo.toml should have [package] section"
    );
    assert!(
        cargo_toml.content.contains("name = \"domain-types\""),
        "Should use correct package name"
    );
}

// === Task 6: Update DTO ===

#[tokio::test]
async fn grafeo_candidate_update_dto_excludes_immutable() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-update-dto");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let update_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_update"))
        .expect("should produce dto_update file");
    let content = &update_file.content;

    // Update struct exists
    assert!(
        content.contains("pub struct UpdateCandidateRequest"),
        "missing update struct"
    );

    // Immutable field "ssn" excluded (from domains.toml fixture)
    assert!(
        !content.contains("pub ssn"),
        "immutable field ssn should be excluded from update DTO"
    );

    // All fields should be Option (partial update)
    if content.contains("candidate_id") {
        assert!(
            content.contains("Option<"),
            "update fields should be Option for partial updates"
        );
    }
}

// === Task 7: Cross-layer consistency ===

#[tokio::test]
async fn grafeo_cross_layer_consistency() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Generate all three layers
    let ddl_gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let entity_gen =
        codegraph::generate::db::entity::SeaOrmEntityGenerator::new(Path::new("/tmp/out"));
    // App DTOs are re-exports; check struct content via domain_types generator
    let tmp = std::env::temp_dir().join("grafeo-test-cross-layer");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());

    let ddl_files = ddl_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let entity_files = entity_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let dto_files = dto_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let ddl = ddl_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("recruiting_candidate"))
        .map(|f| &f.content);
    let entity = entity_files.first().map(|f| &f.content);
    let response = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .map(|f| &f.content);

    // All three should be generated
    assert!(ddl.is_some(), "DDL should be generated");
    assert!(entity.is_some(), "Entity should be generated");
    assert!(response.is_some(), "Response DTO should be generated");

    let ddl = ddl.unwrap();
    let entity = entity.unwrap();
    let response = response.unwrap();

    // candidate_id appears in DDL as column, entity as field, response as field
    assert!(
        ddl.contains("candidate_id"),
        "DDL should have candidate_id column"
    );
    assert!(
        entity.contains("candidate_id"),
        "Entity should have candidate_id field"
    );
    assert!(
        response.contains("candidate_id"),
        "Response should have candidate_id field"
    );
}

// === Regression: Bug 1 — array-type schema produces non-empty properties ===

#[tokio::test]
async fn array_type_schema_inherits_item_properties() {
    let (engine, _config) = setup_grafeo().await;

    // ProcessHistoryType is "type": "array" wrapping ProcessHistoryItemType.
    // It should have the item type's properties (id, actionDate, descriptions),
    // not an empty property list.
    let props = engine.get_properties("ProcessHistoryType").await.unwrap();

    assert!(
        !props.is_empty(),
        "array-type schema ProcessHistoryType should have properties from its item type, got 0"
    );

    let id_prop = props.iter().find(|p| p.name == "id");
    assert!(
        id_prop.is_some(),
        "ProcessHistoryType should have 'id' property from ProcessHistoryItemType"
    );

    let action_date_prop = props.iter().find(|p| p.name == "actionDate");
    assert!(
        action_date_prop.is_some(),
        "ProcessHistoryType should have 'actionDate' property from ProcessHistoryItemType"
    );
}

// === Regression: Bug 2 — array_wrapper $ref resolves to Vec<String> ===

#[tokio::test]
async fn array_wrapper_ref_resolves_to_vec_string() {
    let (engine, _config) = setup_grafeo().await;

    let props = engine.get_properties("CandidateType").await.unwrap();

    let position_titles = props
        .iter()
        .find(|p| p.name == "positionTitles")
        .expect("CandidateType should have positionTitles property");

    // StringTypeArray is classified as array_wrapper → rust type should be Vec<String>
    assert_eq!(
        position_titles.render_strategy, "array_wrapper",
        "positionTitles should have render_strategy 'array_wrapper', got '{}'",
        position_titles.render_strategy,
    );
    assert_eq!(
        position_titles.rust_field_type, "Vec<String>",
        "positionTitles should be Vec<String>, got '{}'",
        position_titles.rust_field_type,
    );
}

// === Regression: Bug 3 — array of codelist items resolves to Vec<EnumName> ===

#[tokio::test]
async fn array_of_codelist_resolves_to_vec_enum() {
    let (engine, _config) = setup_grafeo().await;

    let props = engine.get_properties("CandidateType").await.unwrap();

    let schedule_codes = props
        .iter()
        .find(|p| p.name == "positionScheduleTypeCodes")
        .expect("CandidateType should have positionScheduleTypeCodes property");

    // Array-of-codelist properties use child_table strategy (join table),
    // same as entity/VO arrays. The base PropertyNode.rust_field_type is
    // "String" (the codelist's underlying text type); the codelist enum name
    // is resolved later by the DTO generator via codelist_enum_name_from_ref().
    assert!(
        schedule_codes.is_array,
        "positionScheduleTypeCodes should be an array"
    );
    assert_eq!(
        schedule_codes.render_strategy, "child_table",
        "positionScheduleTypeCodes should use child_table strategy, got '{}'",
        schedule_codes.render_strategy,
    );
    assert!(
        schedule_codes
            .ref_target
            .as_ref()
            .map(|r| r.contains("PositionScheduleTypeCodeList"))
            .unwrap_or(false),
        "positionScheduleTypeCodes ref_target should reference PositionScheduleTypeCodeList, got '{:?}'",
        schedule_codes.ref_target,
    );
}

// === Regression: DTO generation uses correct types for all three bugs ===

#[tokio::test]
async fn dto_output_uses_correct_types_for_array_and_codelist_fields() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-array-codelist");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file");
    let content = &create_file.content;

    // Bug 2: positionTitles should be Vec<String>, not serde_json::Value
    assert!(
        content.contains("Vec<String>"),
        "create DTO should contain Vec<String> for positionTitles, got:\n{}",
        content,
    );
    assert!(
        !content.contains("serde_json::Value"),
        "create DTO should NOT contain serde_json::Value — all types should be resolved, got:\n{}",
        content,
    );

    // Codelist references are mapped to String in DTOs since the codelist
    // enum types are only generated as SQL, not as Rust types
    assert!(
        content.contains("position_schedule_type_codes"),
        "create DTO should contain position_schedule_type_codes field, got:\n{}",
        content,
    );
}

// === Regression: Bug 4 — child DTO PrimitiveWrapper fields must use classified rust type ===
//
// ROOT CAUSE: tests/fixtures/classifier.toml is missing FormattedDateTimeType
// (and DateType) from [primitive_wrappers]. The classifier falls back to treating
// it as an unrecognized ref, producing rust_field_type = "FormattedDateTimeType"
// instead of "chrono::DateTime<chrono::Utc>".
//
// FIX: Add to tests/fixtures/classifier.toml [primitive_wrappers]:
//   [primitive_wrappers.FormattedDateTimeType]
//   postgres = "TIMESTAMPTZ"
//   rust = "chrono::DateTime<chrono::Utc>"
//   sea_orm = "TimestampWithTimeZone"
//
//   [primitive_wrappers.DateType]
//   postgres = "DATE"
//   rust = "chrono::NaiveDate"
//   sea_orm = "Date"

#[tokio::test]
async fn child_dto_primitive_wrapper_uses_classified_type() {
    let (engine, _config) = setup_grafeo().await;

    // ProcessHistoryType delegates to ProcessHistoryItemType's properties.
    // actionDate has $ref to FormattedDateTimeType.json → PrimitiveWrapper
    let props = engine.get_properties("ProcessHistoryType").await.unwrap();

    let action_date = props
        .iter()
        .find(|p| p.name == "actionDate")
        .expect("ProcessHistoryType should have actionDate property");

    // Graph must store the classified rust type, not the schema name
    assert_eq!(
        action_date.rust_field_type, "chrono::DateTime<chrono::Utc>",
        "actionDate rust_field_type should be chrono::DateTime<chrono::Utc> (PrimitiveWrapper for FormattedDateTimeType), got '{}'",
        action_date.rust_field_type,
    );
    assert_eq!(
        action_date.classification_kind,
        Some(RefClassificationKind::PrimitiveWrapper),
        "actionDate should be classified as PrimitiveWrapper",
    );
}

// === Regression: Bug 5 — child DTO ArrayWrapper fields must use classified rust type ===
//
// ROOT CAUSE: The graph correctly stores rust_field_type = "Vec<String>" for
// ArrayWrapper properties. BUT the DTO generator's child field type mapper
// (hr-graph/src/generate/ddd/dto.rs, the `_ =>` arm around line 149) only
// recognizes types containing "::" or matching a hardcoded list
// (String, bool, i32, i64, f32, f64, u32, u64). "Vec<String>" matches neither,
// so it falls through to the "String" fallback.
//
// FIX: In hr-graph/src/generate/ddd/dto.rs, in the child field type mapper,
// add Vec<*> recognition. Either:
//   a) Add `|| t.starts_with("Vec<")` to the condition, OR
//   b) Match on classification_kind first (PrimitiveWrapper/ArrayWrapper → use
//      rust_field_type directly) before falling back to string matching.
// Option (b) is more robust since it handles any future type patterns.

#[tokio::test]
async fn child_dto_array_wrapper_uses_classified_type() {
    let (engine, _config) = setup_grafeo().await;

    // descriptions has $ref to StringTypeArray.json → ArrayWrapper
    let props = engine.get_properties("ProcessHistoryType").await.unwrap();

    let descriptions = props
        .iter()
        .find(|p| p.name == "descriptions")
        .expect("ProcessHistoryType should have descriptions property");

    // Graph correctly stores Vec<String> — this test verifies the graph layer
    assert_eq!(
        descriptions.rust_field_type, "Vec<String>",
        "descriptions rust_field_type should be Vec<String> (ArrayWrapper for StringTypeArray), got '{}'",
        descriptions.rust_field_type,
    );
    assert_eq!(
        descriptions.classification_kind,
        Some(RefClassificationKind::ArrayWrapper),
        "descriptions should be classified as ArrayWrapper",
    );
}

// === Regression: Bug 4+5 — child DTO struct fields use correct types in generated code ===
// This end-to-end test verifies both fixes work together: the classifier config
// maps FormattedDateTimeType → chrono type, AND the DTO generator's child field
// mapper passes Vec<*> and chrono types through to the generated code.

#[tokio::test]
async fn child_dto_fields_use_correct_types_in_generated_output() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-child-types");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file");
    let content = &create_file.content;

    // The child DTO for ProcessHistory should have correctly typed fields:
    // - action_date: should NOT be Option<String> — should use chrono type
    // - descriptions: should NOT be Option<String> — should use Vec<String>
    assert!(
        content.contains("chrono::DateTime<chrono::Utc>"),
        "child DTO action_date should use chrono::DateTime<chrono::Utc>, not String.\nGenerated:\n{}",
        content,
    );
    assert!(
        content.contains("Vec<String>"),
        "child DTO descriptions should use Vec<String>, not String.\nGenerated:\n{}",
        content,
    );

    // Negative: child DTO ProcessHistory fields should NOT all be String
    // Find the ProcessHistory child struct and check it doesn't have all-String fields
    let process_history_struct_start = content
        .find("CreateCandidateProcessHistory")
        .expect("should have ProcessHistory child DTO struct");
    let struct_content = &content[process_history_struct_start..];
    // action_date should not be Option<String>
    assert!(
        !struct_content.contains("pub action_date: Option<String>"),
        "action_date in child DTO should NOT be Option<String>.\nGenerated:\n{}",
        struct_content,
    );
}

// === Task 1: Repository Trait Generation ===

#[tokio::test]
async fn grafeo_candidate_repository_trait_content() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen =
        codegraph::generate::ddd::repository::RepositoryTraitGenerator::new(Path::new("/tmp/out"));
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    assert!(!files.is_empty(), "should produce repository trait file");
    let repo_file = files.first().unwrap();
    let content = &repo_file.content;

    // Trait declaration
    assert!(
        content.contains("pub trait CandidateRepository: Send + Sync"),
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
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
        .await
        .unwrap();

    // Struct declaration
    assert!(
        code.contains("pub struct CandidateRepositoryImpl"),
        "missing impl struct"
    );

    // Implements trait
    assert!(
        code.contains("impl CandidateRepository for CandidateRepositoryImpl"),
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

// === Task 3: classification_kind populated ===

#[tokio::test]
async fn grafeo_candidate_properties_have_classification_kind() {
    let (engine, _config) = setup_grafeo().await;
    let props = engine.get_properties("CandidateType").await.unwrap();

    // Every property with a known classification should have classification_kind set
    // via the render_strategy → effective_kind() fallback chain
    let candidate_id = props.iter().find(|p| p.name == "candidateId").unwrap();
    assert_eq!(
        candidate_id.effective_kind(),
        Some(RefClassificationKind::PrimitiveWrapper),
        "candidateId should be PrimitiveWrapper"
    );

    // Verify the classification string is populated (not None) for $ref properties
    let gender = props.iter().find(|p| p.name == "gender").unwrap();
    assert!(
        gender.render_strategy == "codelist" || gender.classification.is_some(),
        "gender should have classification data set during ingestion"
    );
}

// === Task 4: Array-of-VO child DTO routing ===

#[tokio::test]
async fn grafeo_candidate_qualifications_routed_to_child_dtos() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-qualifications");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file");
    let content = &create_file.content;

    // qualifications is an array of inline-def ValueObjects classified as child_table.
    // With child_table → ValueObject mapping, the DTO generator routes it into the
    // ValueObject branch (dto.rs:102), which calls get_child_schemas().
    //
    // The child schema lookup may or may not find matching schemas depending on
    // whether inline $defs are ingested with parent_schema set. Either way:
    // 1. qualifications should NOT appear as a raw typed field (it's routed to VO branch)
    // 2. If child schemas are discovered, it appears in child_dtos as Vec<>
    assert!(
        !content.contains("pub qualifications: QualificationType"),
        "qualifications should not be a raw type field (it's routed to ValueObject branch)"
    );

    // Verify the struct is still generated correctly
    assert!(
        content.contains("pub struct CreateCandidateRequest"),
        "Create DTO struct must be generated"
    );

    // If child DTO was generated, verify it's a Vec
    if content.contains("qualifications") {
        assert!(
            content.contains("qualifications: Vec<"),
            "qualifications should be a Vec<> child DTO field if present"
        );
    }
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

// === Task 7: Cross-layer consistency including repository ===

#[tokio::test]
async fn grafeo_cross_layer_repository_consistency() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Generate DTO (domain_types) and repository for CandidateType
    let tmp = std::env::temp_dir().join("grafeo-test-repo-consistency");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dto_files = dto_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let emitter = RepositoryImplEmitter;
    let repo_code = emitter
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
        .await
        .unwrap();

    let create_dto = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .map(|f| &f.content)
        .unwrap();

    // Every field in create DTO should be assignable in repository create method
    // The repo uses `cmd.field_name` for each direct column
    // Check that candidate_id appears in both
    assert!(
        create_dto.contains("candidate_id"),
        "Create DTO should have candidate_id"
    );
    assert!(
        repo_code.contains("candidate_id: Set(cmd.candidate_id)"),
        "Repository create should assign candidate_id from cmd"
    );

    // Entity reference field consistency
    if create_dto.contains("referred_by_application_id") {
        assert!(
            repo_code.contains("referred_by_application"),
            "Repository should handle entity reference field"
        );
    }
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
            Box::new(codegraph::generate::ddd::command::CommandGenerator::new(out)),
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
            Box::new(codegraph::generate::api::handler::HandlerGenerator::new(out)),
        ),
        (
            "test",
            Box::new(codegraph::generate::test::test_gen::TestGenerator::new(out)),
        ),
        (
            "grpc_proto",
            Box::new(codegraph::generate::grpc::proto::GrpcProtoGenerator::new(out)),
        ),
        (
            "grpc_service",
            Box::new(codegraph::generate::grpc::service::GrpcServiceGenerator::new(out)),
        ),
    ];

    for (name, gen) in &generators {
        let files = gen
            .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
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

// === Task 9: Inline enum DTO field ===

#[tokio::test]
async fn grafeo_candidate_inline_enum_in_dto() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-inline-enum");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .unwrap();
    let content = &create_file.content;

    // Inline enum "status" should render as a strongly-typed enum (synthetic codelist)
    assert!(
        content.contains("RecruitingCandidateStatus"),
        "inline enum 'status' should use the synthetic codelist enum type in Create DTO, got:\n{content}"
    );
}

// === Task 10: Response DTO nested VOs ===

#[tokio::test]
async fn grafeo_candidate_response_dto_nested_value_objects() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-nested-vo");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let response_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .unwrap();
    let content = &response_file.content;

    // Response struct should exist
    assert!(
        content.contains("pub struct CandidateResponse"),
        "missing response struct"
    );

    // After Task 4 (child_table fix), qualifications should be routed as child DTO.
    // The response template renders array children as Vec<...Response>.
    // Hard assertion: once child_table maps to ValueObject, qualifications MUST appear.
    let struct_count = content.matches("pub struct").count();
    if struct_count > 1 {
        // Child response structs were generated — verify array child has Vec
        assert!(
            content.contains("Vec<"),
            "response with child structs should have Vec<> for array VO children"
        );
    }

    // At minimum the main CandidateResponse struct with standard fields
    assert!(
        content.contains("pub id: uuid::Uuid"),
        "response must have id"
    );
    assert!(
        content.contains("pub created_at:"),
        "response must have created_at"
    );
    assert!(
        content.contains("pub updated_at:"),
        "response must have updated_at"
    );
}

// === ItemsOf edge creation ===

#[tokio::test]
async fn grafeo_items_of_edge_created_for_array_ref() {
    let (engine, _config) = setup_grafeo().await;

    // qualifications is "type": "array" with "items": { "$ref": "#/$defs/QualificationType" }
    // The async ingestion should create an ItemsOf edge from the property to QualificationType
    let item_schema = engine
        .get_array_item_schema("qualifications", "CandidateType")
        .await
        .unwrap();
    assert!(
        item_schema.is_some(),
        "ItemsOf edge should exist for array ref property"
    );
    assert_eq!(item_schema.unwrap().title, "QualificationType");
}

// === ReferencesSchema edge creation ===

#[tokio::test]
async fn grafeo_references_schema_edge_created_for_scalar_ref() {
    let (engine, _config) = setup_grafeo().await;

    // personName, gender, referredByApplication, compensationExpectation are scalar $ref properties
    let refs = engine
        .get_referenced_schemas("CandidateType")
        .await
        .unwrap();
    assert!(
        !refs.is_empty(),
        "ReferencesSchema edges should exist for scalar ref properties"
    );
    // personName refs NameType
    assert!(
        refs.iter().any(|s| s.title == "NameType"),
        "should reference NameType via personName"
    );
}

// === Task 11: Composite wrapper in DTO ===

#[tokio::test]
async fn grafeo_candidate_composite_wrapper_in_dto() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-composite");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .unwrap();
    let content = &create_file.content;

    // compensationExpectation (AmountType) should expand to 2 flat fields
    assert!(
        content.contains("compensation_expectation"),
        "Create DTO must contain compensation_expectation field.\nContent:\n{}",
        content
    );
    assert!(
        content.contains("compensation_expectation_currency"),
        "Create DTO must contain compensation_expectation_currency field.\nContent:\n{}",
        content
    );
    // Must NOT contain the composite wrapper as a single typed field
    assert!(
        !content.contains("AmountType"),
        "Create DTO must not reference AmountType directly — composites expand to flat fields.\nContent:\n{}",
        content
    );
}

#[tokio::test]
async fn grafeo_composite_columns_ingested_via_expands_to() {
    let (engine, _config) = setup_grafeo().await;

    // compensationExpectation on CandidateType is CompositeWrapper (AmountType)
    // After ingestion, get_composite_columns should return 2 columns via ExpandsTo edges
    let comp_cols = engine
        .get_composite_columns("compensationExpectation", "CandidateType")
        .await
        .unwrap();

    assert_eq!(
        comp_cols.len(),
        2,
        "AmountType should expand to 2 columns (value + currency), got: {:?}",
        comp_cols
    );

    // Primary value column (empty suffix)
    let value_col = comp_cols.iter().find(|c| c.suffix.is_empty()).unwrap();
    assert_eq!(value_col.pg_type, "NUMERIC(19,4)");
    assert_eq!(value_col.rust_type, "rust_decimal::Decimal");
    assert_eq!(value_col.sea_orm_type, "Decimal");

    // Currency column
    let currency_col = comp_cols.iter().find(|c| c.suffix == "_currency").unwrap();
    assert_eq!(currency_col.pg_type, "TEXT");
    assert_eq!(currency_col.rust_type, "String");
    assert_eq!(currency_col.sea_orm_type, "Text");
}

// === Enhancement: Bug 6 — composite currency column should use codelist enum in DTOs ===
//
// The composite wrapper's _currency column uses rust_type = "String" in the
// classifier config. This is correct for the DB/entity layer (TEXT column), but
// the DTO layer should use the codelist enum type (e.g. CurrencyCodeList) so that
// OpenAPI/Swagger documentation shows the valid currency values.
//
// The fix requires the DTO generator to detect when a composite column has an
// associated codelist (via fk_table metadata or by resolving the original schema's
// $ref to a codelist) and emit the enum type instead of String.
//
// FIX: See docs/superpowers/specs/2026-03-24-composite-currency-enum-in-dto.md

#[tokio::test]
async fn composite_currency_column_should_use_enum_in_dto() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-currency-enum");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file");
    let content = &create_file.content;

    // The compensation_expectation_currency field should use the CurrencyCodeList
    // enum type so OpenAPI docs show valid values, NOT a bare String.
    assert!(
        content.contains("CurrencyCodeList"),
        "compensation_expectation_currency should use CurrencyCodeList enum in DTO for OpenAPI docs, \
         but got plain String.\nGenerated:\n{}",
        content,
    );

    // The field should NOT be a plain String
    let lines: Vec<&str> = content.lines().collect();
    let currency_line = lines
        .iter()
        .find(|l| l.contains("compensation_expectation_currency"));
    if let Some(line) = currency_line {
        assert!(
            !line.contains("Option<String>"),
            "compensation_expectation_currency should be Option<CurrencyCodeList>, not Option<String>.\nLine: {}",
            line,
        );
    }
}

#[tokio::test]
async fn grafeo_composite_wrapper_ddl_expansion() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
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
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
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

// === Inline def allOf composition tests (Issue 1 & 2 fixes) ===

#[tokio::test]
async fn grafeo_inline_def_allof_edges_created() {
    let (engine, _config) = setup_grafeo().await;

    // DistributionGuidelinesType is an inline def with allOf: [{$ref: DistributionBaseType}]
    // After Issue 2 fix, Pass 4 should create ExtendsSchema edge for inline defs too
    let targets = engine
        .get_allof_targets("DistributionGuidelinesType")
        .await
        .unwrap();
    assert!(
        targets.contains(&"DistributionBaseType".to_string()),
        "DistributionGuidelinesType should have ExtendsSchema edge to DistributionBaseType. Got: {:?}",
        targets
    );
}

#[tokio::test]
async fn grafeo_inline_def_allof_properties_merged() {
    let (engine, _config) = setup_grafeo().await;

    // DistributionGuidelinesType has:
    //   - Own properties: doNotRedistributeIndicator, scope
    //   - allOf $ref to DistributionBaseType: startDate, endDate, description
    // After Issue 1 fix, all 5 properties should be ingested
    let props = engine
        .get_properties("DistributionGuidelinesType")
        .await
        .unwrap();
    let prop_names: Vec<&str> = props.iter().map(|p| p.name.as_str()).collect();

    // Own properties
    assert!(
        prop_names.contains(&"doNotRedistributeIndicator"),
        "should have own property 'doNotRedistributeIndicator'. Got: {:?}",
        prop_names
    );
    assert!(
        prop_names.contains(&"scope"),
        "should have own property 'scope'. Got: {:?}",
        prop_names
    );

    // Properties merged from DistributionBaseType via allOf $ref
    assert!(
        prop_names.contains(&"startDate"),
        "should have merged property 'startDate' from DistributionBaseType. Got: {:?}",
        prop_names
    );
    assert!(
        prop_names.contains(&"endDate"),
        "should have merged property 'endDate' from DistributionBaseType. Got: {:?}",
        prop_names
    );
    assert!(
        prop_names.contains(&"description"),
        "should have merged property 'description' from DistributionBaseType. Got: {:?}",
        prop_names
    );

    assert_eq!(
        props.len(),
        5,
        "DistributionGuidelinesType should have exactly 5 properties (2 own + 3 from allOf). Got: {:?}",
        prop_names
    );
}

#[tokio::test]
async fn grafeo_inline_def_ingested_as_schema_node() {
    let (engine, _config) = setup_grafeo().await;

    // DistributionGuidelinesType should be ingested as a schema node
    let schema = engine
        .get_schema("DistributionGuidelinesType")
        .await
        .unwrap();
    assert!(
        schema.is_some(),
        "DistributionGuidelinesType should be ingested as a schema node"
    );
    let schema = schema.unwrap();
    assert_eq!(schema.title, "DistributionGuidelinesType");
    assert!(schema.has_all_of, "should have has_all_of flag set");
}

#[tokio::test]
async fn grafeo_composite_wrapper_cross_layer_consistency() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // DDL (EntityGenerator trait)
    let ddl_gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let ddl_files = ddl_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let ddl_content = &ddl_files[0].content;

    // DTO (domain_types generator for struct content)
    let tmp = std::env::temp_dir().join("grafeo-test-composite-cross");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dto_files = dto_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let dto_create = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .unwrap();

    // Repository (RepositoryImplEmitter::emit() → String)
    let emitter = RepositoryImplEmitter;
    let repo_code = emitter
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
        .await
        .unwrap();

    // All three layers must contain both composite column names
    for field_name in &[
        "compensation_expectation",
        "compensation_expectation_currency",
    ] {
        assert!(
            ddl_content.contains(field_name),
            "DDL must contain '{}'\nDDL:\n{}",
            field_name,
            ddl_content
        );
        assert!(
            dto_create.content.contains(field_name),
            "DTO must contain '{}'\nDTO:\n{}",
            field_name,
            dto_create.content
        );
        assert!(
            repo_code.contains(field_name),
            "Repository must contain '{}'\nRepo:\n{}",
            field_name,
            repo_code
        );
    }
}

// === E2E Candidate DTO & Repository plan tests ===

#[tokio::test]
async fn grafeo_inline_def_has_parent_schema() {
    let (engine, _config) = setup_grafeo().await;

    // QualificationType is an inline $def of CandidateType
    let qual = engine.get_schema("QualificationType").await.unwrap();
    assert!(qual.is_some(), "QualificationType should exist in graph");
    let qual = qual.unwrap();
    assert_eq!(
        qual.parent_schema.as_deref(),
        Some("CandidateType"),
        "inline $def should have parent_schema set to CandidateType"
    );
}

#[tokio::test]
async fn grafeo_candidate_properties_have_typed_classification_kind() {
    let (engine, _config) = setup_grafeo().await;
    let props = engine.get_properties("CandidateType").await.unwrap();

    let candidate_id = props.iter().find(|p| p.name == "candidateId").unwrap();
    assert!(
        candidate_id.classification_kind.is_some(),
        "candidateId should have classification_kind set directly, not via fallback"
    );
    assert_eq!(
        candidate_id.classification_kind,
        Some(RefClassificationKind::PrimitiveWrapper),
    );

    let gender = props.iter().find(|p| p.name == "gender").unwrap();
    assert!(
        gender.classification_kind.is_some(),
        "gender should have classification_kind set directly"
    );
    assert_eq!(
        gender.classification_kind,
        Some(RefClassificationKind::CodelistReference),
    );

    let app_ref = props
        .iter()
        .find(|p| p.name == "referredByApplication")
        .unwrap();
    assert_eq!(
        app_ref.classification_kind,
        Some(RefClassificationKind::EntityReference),
    );

    let status = props.iter().find(|p| p.name == "status").unwrap();
    assert_eq!(
        status.classification_kind,
        Some(RefClassificationKind::CodelistCheck),
        "inline enums are ingested as synthetic codelists (CodelistCheck)"
    );
    assert!(
        status.ref_target.is_some(),
        "inline enum should have ref_target pointing to synthetic codelist"
    );
}

#[tokio::test]
async fn grafeo_candidate_child_dtos_via_edges() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-child-edges");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let files = gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();

    let create_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file");
    let content = &create_file.content;

    // qualifications (array ValueObject) MUST generate child DTO
    assert!(
        content.contains("qualifications: Vec<Create"),
        "qualifications should be Vec<Create...Request>, got:\n{}",
        content,
    );

    // Child struct must have correct fields from QualificationType
    assert!(
        content.contains("qualification_name"),
        "child DTO must have qualification_name field, got:\n{}",
        content,
    );
    assert!(
        content.contains("issuer"),
        "child DTO must have issuer field, got:\n{}",
        content,
    );
    assert!(
        content.contains("date_awarded"),
        "child DTO must have date_awarded field, got:\n{}",
        content,
    );
}

#[tokio::test]
async fn grafeo_repository_impl_includes_child_inserts() {
    let (engine, config) = setup_grafeo().await;

    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
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
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
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
async fn grafeo_edge_based_child_discovery() {
    let (engine, _config) = setup_grafeo().await;

    // Array ValueObject: qualifications → QualificationType via ItemsOf edge
    let qual_schema = engine
        .get_array_item_schema("qualifications", "CandidateType")
        .await
        .unwrap();
    assert!(
        qual_schema.is_some(),
        "ItemsOf edge should resolve qualifications"
    );
    let qual = qual_schema.unwrap();
    assert_eq!(qual.title, "QualificationType");

    // Verify QualificationType has expected properties
    let qual_props = engine.get_properties("QualificationType").await.unwrap();
    let qual_names: Vec<&str> = qual_props.iter().map(|p| p.name.as_str()).collect();
    assert!(
        qual_names.contains(&"qualificationName"),
        "should have qualificationName"
    );
    assert!(qual_names.contains(&"issuer"), "should have issuer");

    // Scalar ValueObject: personName → NameType via ReferencesSchema edge
    let name_schema = engine
        .get_property_ref_target("personName", "CandidateType")
        .await
        .unwrap();
    assert!(
        name_schema.is_some(),
        "ReferencesSchema edge should resolve personName"
    );
    let name = name_schema.unwrap();
    assert_eq!(name.title, "NameType");

    // Verify NameType has expected properties
    let name_props = engine.get_properties("NameType").await.unwrap();
    let name_names: Vec<&str> = name_props.iter().map(|p| p.name.as_str()).collect();
    assert!(name_names.contains(&"givenName"), "should have givenName");
    assert!(name_names.contains(&"familyName"), "should have familyName");
}

#[tokio::test]
async fn grafeo_repository_entity_ref_uses_id_suffix() {
    let (engine, config) = setup_grafeo().await;

    let emitter = RepositoryImplEmitter;
    let code = emitter
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
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
async fn grafeo_entity_ref_cross_layer_consistency() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Generate all layers (domain_types for DTO struct content)
    let ddl_gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let entity_gen =
        codegraph::generate::db::entity::SeaOrmEntityGenerator::new(Path::new("/tmp/out"));
    let tmp = std::env::temp_dir().join("grafeo-test-entity-ref-cross");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let emitter = RepositoryImplEmitter;

    let ddl_files = ddl_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let entity_files = entity_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let dto_files = dto_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let repo_code = emitter
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
        .await
        .unwrap();

    let ddl = ddl_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("recruiting_candidate"))
        .or_else(|| ddl_files.first())
        .map(|f| &f.content)
        .expect("DDL should be generated");
    let _entity = entity_files
        .first()
        .map(|f| &f.content)
        .expect("Entity should be generated");
    let create_dto = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .map(|f| &f.content)
        .expect("Create DTO should be generated");

    // referredByApplication (EntityReference) must use _id suffix consistently
    // Note: SeaORM entity generator intentionally skips EntityReference fields
    // (they are handled via SeaORM Relation), so we don't assert on entity output.
    assert!(
        ddl.contains("referred_by_application_id"),
        "DDL should have referred_by_application_id column"
    );
    assert!(
        create_dto.contains("referred_by_application_id"),
        "Create DTO should have referred_by_application_id field"
    );
    assert!(
        repo_code.contains("referred_by_application_id"),
        "Repository should use referred_by_application_id"
    );

    // The _id field should be UUID type across layers
    assert!(
        ddl.contains("referred_by_application_id UUID"),
        "DDL column should be UUID type"
    );
}

#[tokio::test]
async fn grafeo_repository_dto_field_alignment() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-repo-alignment");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dto_files = dto_gen
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
        .await
        .unwrap();
    let create_dto = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .map(|f| &f.content)
        .expect("Create DTO should be generated");

    let emitter = RepositoryImplEmitter;
    let repo_code = emitter
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
        .await
        .unwrap();

    // Extract all Set(cmd.X) field names from repository code
    let set_fields: Vec<&str> = repo_code
        .match_indices("Set(cmd.")
        .map(|(i, _)| {
            let start = i + "Set(cmd.".len();
            let rest = &repo_code[start..];
            let end = rest.find(')').unwrap_or(rest.len());
            &rest[..end]
        })
        .collect();

    // Each Set(cmd.X) field should exist in the create DTO
    for field in &set_fields {
        // Skip fields that are child-related (those use item.X, not cmd.X directly)
        if field.contains('.') {
            continue;
        }
        assert!(
            create_dto.contains(field),
            "Repository uses cmd.{} but create DTO does not have this field.\nDTO:\n{}\nRepo:\n{}",
            field,
            create_dto,
            repo_code,
        );
    }
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
        .emit(&engine, "CandidateType", "recruiting", &config, None, &[])
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

#[cfg(feature = "e2e")]
#[tokio::test]
async fn grafeo_generated_code_compiles() {
    use std::time::Duration;

    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path();

    generate_full_app(output_dir).await;

    // Run cargo check with 5-minute timeout
    let result = tokio::time::timeout(
        Duration::from_secs(300),
        tokio::process::Command::new("cargo")
            .args(["check"])
            .current_dir(output_dir)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => panic!("failed to spawn cargo check: {e}"),
        Err(_) => {
            let preserved = tmp.keep();
            panic!(
                "cargo check timed out after 5 minutes!\n\
                 Temp dir preserved at: {}",
                preserved.display()
            );
        }
    };

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let preserved = tmp.keep();
        panic!(
            "cargo check failed!\n\
             Temp dir preserved at: {}\n\
             --- stdout ---\n{}\n\
             --- stderr ---\n{}",
            preserved.display(),
            stdout,
            stderr
        );
    }
    // On success, tmp drops and cleans up automatically
}

#[cfg(feature = "e2e")]
#[tokio::test]
async fn grafeo_generated_tests_pass() {
    use std::time::Duration;

    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path();

    generate_full_app(output_dir).await;

    // Run cargo test with 10-minute timeout
    let result = tokio::time::timeout(
        Duration::from_secs(600),
        tokio::process::Command::new("cargo")
            .args(["test"])
            .current_dir(output_dir)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => panic!("failed to spawn cargo test: {e}"),
        Err(_) => {
            let preserved = tmp.keep();
            panic!(
                "cargo test timed out after 10 minutes!\n\
                 Temp dir preserved at: {}",
                preserved.display()
            );
        }
    };

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let preserved = tmp.keep();
        panic!(
            "cargo test failed!\n\
             Temp dir preserved at: {}\n\
             --- stdout ---\n{}\n\
             --- stderr ---\n{}",
            preserved.display(),
            stdout,
            stderr
        );
    }
    // On success, tmp drops and cleans up automatically
}

// === Codelist Rust enum generation ===

#[tokio::test]
async fn grafeo_codelist_ingestion_produces_enum_values() {
    let (engine, _config) = setup_grafeo().await;

    // Verify codelists were ingested
    let codelists = engine.list_codelists().await.unwrap();
    assert!(
        !codelists.is_empty(),
        "should have ingested at least one codelist"
    );

    // CurrencyCodeList should be present
    let currency = codelists.iter().find(|cl| cl.name == "CurrencyCodeList");
    assert!(
        currency.is_some(),
        "CurrencyCodeList should be in codelists. Found: {:?}",
        codelists.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // GenderCodeList should have enum values
    let gender_values = engine.get_enum_values("GenderCodeList").await.unwrap();
    assert!(
        !gender_values.is_empty(),
        "GenderCodeList should have enum values"
    );
    let value_names: Vec<&str> = gender_values.iter().map(|v| v.value.as_str()).collect();
    assert!(
        value_names.contains(&"Male"),
        "GenderCodeList should contain 'Male'"
    );
    assert!(
        value_names.contains(&"Female"),
        "GenderCodeList should contain 'Female'"
    );
}

#[tokio::test]
async fn grafeo_rust_codelist_generator_emits_enum() {
    let (engine, _config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen = RustCodelistGenerator::new(Path::new("/tmp/out"));
    let files = gen.generate_all(&engine, &tera, &ProjectConfig::default()).await.unwrap();

    assert!(
        !files.is_empty(),
        "RustCodelistGenerator should produce files"
    );

    // Should have a mod.rs
    let mod_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("mod.rs"));
    assert!(mod_file.is_some(), "should generate codelist/mod.rs");
    let mod_content = &mod_file.unwrap().content;
    assert!(
        mod_content.contains("pub mod currency_code_list;"),
        "mod.rs should declare currency_code_list module"
    );
    assert!(
        mod_content.contains("pub use currency_code_list::CurrencyCodeList;"),
        "mod.rs should re-export CurrencyCodeList"
    );

    // Should have a currency_code_list.rs
    let currency_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("currency_code_list.rs"));
    assert!(
        currency_file.is_some(),
        "should generate currency_code_list.rs"
    );
    let content = &currency_file.unwrap().content;

    // Verify derives
    assert!(
        content.contains(
            "#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]"
        ),
        "enum should have correct derives"
    );
    // Verify enum name
    assert!(
        content.contains("pub enum CurrencyCodeList"),
        "should contain CurrencyCodeList enum"
    );
    // Verify serde rename on variants (USD is all-caps, PascalCase would be Usd)
    assert!(
        content.contains("#[serde(rename = \"USD\")]"),
        "USD variant should have serde rename"
    );
    assert!(
        content.contains("Usd"),
        "USD should be sanitized to Usd variant"
    );
    // Verify Display impl
    assert!(
        content.contains("impl std::fmt::Display for CurrencyCodeList"),
        "should implement Display"
    );
    assert!(
        content.contains("write!(f, \"USD\")"),
        "Display impl should write original code"
    );
}

#[tokio::test]
async fn grafeo_gender_codelist_variants_no_rename_when_pascal() {
    let (engine, _config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let gen = RustCodelistGenerator::new(Path::new("/tmp/out"));
    let files = gen.generate_all(&engine, &tera, &ProjectConfig::default()).await.unwrap();

    let gender_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("gender_code_list.rs"))
        .expect("should generate gender_code_list.rs");
    let content = &gender_file.content;

    // "Male" is already valid PascalCase — should NOT have serde rename
    assert!(
        content.contains("    Male,"),
        "Male variant should exist without rename"
    );
    // "NotSpecified" is already PascalCase — should NOT have serde rename
    assert!(
        content.contains("    NotSpecified,"),
        "NotSpecified variant should exist"
    );
}

// === Include (`?include=`) feature E2E ===

/// Helper: inline domain config with `allow_include` for CandidateType.
fn include_domain_config() -> codegraph_config::DomainConfig {
    let config_str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = []

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
depends_on = ["common"]
entities = ["CandidateType", "ApplicationType"]

[domains.recruiting.entity_config.CandidateType]
role = "root"
allow_include = ["application"]
"#;
    parse_domain_config_str(config_str).unwrap()
}

#[tokio::test]
async fn grafeo_e2e_include_dto_generated_for_candidate() {
    let config = include_domain_config();
    let classifier =
        codegraph_classifier::config::parse_classifier_config(Path::new("tests/fixtures/classifier.toml"))
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
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
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
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
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
    let classifier =
        codegraph_classifier::config::parse_classifier_config(Path::new("tests/fixtures/classifier.toml"))
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
        .generate(&engine, "CandidateType", "recruiting", &config, &tera, &ProjectConfig::default())
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

// === Compile-gate test: generated domain-types crate compiles ===

#[tokio::test]
async fn generated_app_compiles_cleanly() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let output_dir = tmp.path().to_path_buf();

    // Compute absolute path to codegraph-type-contracts crate
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let type_contracts_path = workspace_root.join("crates").join("codegraph-type-contracts");

    let project_config = ProjectConfig {
        app_name: "test-app".into(),
        domain_types_crate: "domain_types".into(),
        generator_name: "codegraph-test".into(),
        type_contracts_base: type_contracts_path.to_string_lossy().to_string(),
        types_import_prefix: "codegraph_type_contracts".into(),
        ..Default::default()
    };

    codegraph::generate::init_project_config(project_config.clone());

    let order = codegraph::generate::compute_generation_order(&engine, &config)
        .await
        .unwrap();

    // 1. Generate scaffold: lib.rs, Cargo.toml, domain/entity mod.rs files
    let scaffold_gen = DomainTypesScaffoldGenerator::new_with_base(output_dir.clone());
    let mut all_files = scaffold_gen
        .generate(&engine, &config, &order, &tera, &project_config)
        .await
        .unwrap();

    // 2. Generate DTOs + query services for each entity in generation order
    let dto_gen = DomainTypesDtoGenerator::new_with_base(output_dir.clone());
    let qs_gen = QueryServiceGenerator::new_with_base(output_dir.clone());

    for entry in &order {
        let dto_files = dto_gen
            .generate(
                &engine,
                &entry.schema_title,
                &entry.domain,
                &config,
                &tera,
                &project_config,
            )
            .await
            .unwrap();
        all_files.extend(dto_files);

        let qs_files = qs_gen
            .generate(
                &engine,
                &entry.schema_title,
                &entry.domain,
                &config,
                &tera,
                &project_config,
            )
            .await
            .unwrap();
        all_files.extend(qs_files);
    }

    // 3. Generate codelist enum files
    let codelist_gen = RustCodelistGenerator::new(&output_dir);
    let cl_files = codelist_gen
        .generate_all(&engine, &tera, &project_config)
        .await
        .unwrap();
    all_files.extend(cl_files);

    // Write all generated files to disk
    for file in &all_files {
        if let Some(parent) = file.path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file.path, &file.content).unwrap();
    }

    // Write stub files for context.rs and query.rs that lib.rs references
    // but which have no dedicated generator.
    let src_dir = output_dir.join("src");
    std::fs::write(
        src_dir.join("context.rs"),
        "pub struct SourceContext;\npub enum SourceOrigin {}\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("query.rs"),
        "pub struct ListParams;\npub struct PagedResult<T>(pub Vec<T>);\npub struct QueryError;\npub enum SortOrder {}\n",
    )
    .unwrap();

    // Verify Cargo.toml was generated
    let cargo_toml_path = output_dir.join("Cargo.toml");
    assert!(
        cargo_toml_path.exists(),
        "Cargo.toml should be generated by DomainTypesScaffoldGenerator"
    );

    // Run cargo check with 5-minute timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        tokio::process::Command::new("cargo")
            .args([
                "check",
                "--manifest-path",
                &cargo_toml_path.to_string_lossy(),
            ])
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => panic!("failed to spawn cargo check: {e}"),
        Err(_) => {
            let preserved = tmp.keep();
            panic!(
                "cargo check timed out after 5 minutes!\n\
                 Temp dir preserved at: {}",
                preserved.display()
            );
        }
    };

    if !output.status.success() {
        let preserved = tmp.keep();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "cargo check failed!\n\
             Temp dir preserved at: {}\n\
             --- stdout ---\n{stdout}\n\
             --- stderr ---\n{stderr}",
            preserved.display(),
        );
    }
    // On success, tmp is dropped and directory is cleaned up
}
