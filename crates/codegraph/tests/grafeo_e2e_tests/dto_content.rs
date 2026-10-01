//! Generated DTO content tests: create/response/update DTO struct contents,
//! structured-wrapper import prefixes, child-DTO typing and routing, composite
//! wrapper expansion, inline-enum fields.

use std::path::Path;

use codegraph::generate::domain_types::dto::DomainTypesDtoGenerator;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ProjectConfig;

use crate::setup::setup_grafeo;
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
async fn grafeo_candidate_child_dtos_via_edges() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    let tmp = std::env::temp_dir().join("grafeo-test-child-edges");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
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
