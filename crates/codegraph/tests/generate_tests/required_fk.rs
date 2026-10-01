//! Required-vs-optional FK nullability semantics across generator layers
//! (entity model, DTOs, DDL, repository emitter, `?include=` fetch code).
//! Split from repository.rs to keep every submodule within the size budget.

use crate::fixtures::{mock_schema, prop, test_domain_config, test_project_config};
use codegraph::generate;
use codegraph::generate::traits::EntityGenerator;
use codegraph::generate::ProjectConfig;
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{DetectionSource, ParentCandidate};
use codegraph_type_contracts::RefClassificationKind;
use std::path::Path;

/// A REQUIRED genuine EntityReference must be consistent with the JSON schema's
/// `required` across every layer — the schema is the source of truth:
///   DDL          → NOT NULL UUID column + FK
///   entity model → `Uuid` (non-Option)
///   repository   → `Set(v)` (not Set(Some(v)))
///   DTO          → `uuid::Uuid` required (non-Option)
///
/// This is the inverse of the VO→entity synthetic FK, which is always nullable
/// (never populated by DTO/repo).
#[tokio::test]
async fn required_genuine_entity_ref_is_not_null_across_all_layers() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/ApplicationType.json",
            "ApplicationType",
            "application",
            "recruiting",
            "entity_reference",
        ))
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "ApplicationType",
            vec![
                prop("title", "String", "TEXT", true, None, None, None, false),
                // Required genuine EntityReference → candidate_id FK.
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
    let project = test_project_config();
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();

    // 1. Entity model: required ref → Uuid (non-Option).
    let entity_gen = generate::db::entity::SeaOrmEntityGenerator::new(Path::new("/tmp/out"));
    let entity_files = entity_gen
        .generate(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            &tera,
            &project,
        )
        .await
        .unwrap();
    let entity = entity_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("application.rs"))
        .expect("should produce application.rs entity file")
        .content
        .clone();
    assert!(
        entity.contains("pub candidate_id: Uuid,"),
        "required entity ref must be non-Option Uuid in the model. Got:\n{entity}"
    );
    assert!(
        !entity.contains("pub candidate_id: Option<Uuid>"),
        "required entity ref must NOT be Option<Uuid>. Got:\n{entity}"
    );

    // 2. Repository emitter: required ref → Set(v).
    let emitter = generate::ddd::repository_emitter::RepositoryImplEmitter;
    let repo_code = emitter
        .emit(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    assert!(
        repo_code.contains("model.candidate_id = Set(v);"),
        "required entity ref must emit Set(v). Got:\n{repo_code}"
    );
    assert!(
        !repo_code.contains("model.candidate_id = Set(Some(v));"),
        "required entity ref must NOT emit Set(Some(v)). Got:\n{repo_code}"
    );
    // 3. DTO: required ref → non-Option uuid::Uuid in the CREATE and RESPONSE DTOs.
    // (The update DTO intentionally makes all fields Option for partial updates.)
    let tmp = std::env::temp_dir().join("required-genuine-ref-dto");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dto_files = dto_gen
        .generate(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            &tera,
            &project,
        )
        .await
        .unwrap();

    let create_dto = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file")
        .content
        .clone();
    assert!(
        create_dto.contains("pub candidate_id: uuid::Uuid"),
        "create DTO required entity ref must be non-Option uuid::Uuid. Got:\n{create_dto}"
    );
    assert!(
        !create_dto.contains("pub candidate_id: Option<uuid::Uuid>"),
        "create DTO required entity ref must NOT be Option<uuid::Uuid>. Got:\n{create_dto}"
    );

    let response_dto = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_response"))
        .expect("should produce dto_response file")
        .content
        .clone();
    assert!(
        response_dto.contains("pub candidate_id: uuid::Uuid"),
        "response DTO required entity ref must be non-Option uuid::Uuid. Got:\n{response_dto}"
    );
    assert!(
        !response_dto.contains("pub candidate_id: Option<uuid::Uuid>"),
        "response DTO required entity ref must NOT be Option<uuid::Uuid>. Got:\n{response_dto}"
    );
}

/// Regression test: a required genuine EntityReference that is ALSO detected as
/// a parent candidate (ScalarRef) must stay non-nullable across ALL layers.
///
/// The parent-candidates injection in `entity.rs` historically hardcoded
/// `Option<Uuid>` / `is_nullable: true` and won the dedup over the
/// property-driven EntityReference branch, producing:
///   - entity model: `candidate_id: Option<Uuid>`   (contradicts schema required)
///   - DDL:          `candidate_id UUID` (nullable) (contradicts schema required)
/// while the DTO and repository emitter correctly honored `required`
/// (`uuid::Uuid` / `Set(v)`), causing E0308 type mismatches at build time.
///
/// JSON Schema `required` is the source of truth: all layers must agree.
#[tokio::test]
async fn required_entity_ref_with_parent_candidate_is_not_null_across_all_layers() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "recruiting/json/ApplicationType.json",
            "ApplicationType",
            "application",
            "recruiting",
            "entity_reference",
        ))
        .with_schema(mock_schema(
            "recruiting/json/CandidateType.json",
            "CandidateType",
            "candidate",
            "recruiting",
            "entity_reference",
        ))
        .with_properties(
            "ApplicationType",
            vec![
                prop("title", "String", "TEXT", true, None, None, None, false),
                // Required genuine EntityReference → candidate_id FK. This is
                // ALSO detected as a ScalarRef parent candidate, which triggers
                // the parent-candidates injection path in entity.rs/ddl.rs.
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
        .with_ref_target(
            "candidate_id",
            "ApplicationType",
            mock_schema(
                "recruiting/json/CandidateType.json",
                "CandidateType",
                "candidate",
                "recruiting",
                "entity_reference",
            ),
        )
        .with_parent_candidate(ParentCandidate {
            child_title: "ApplicationType".to_string(),
            parent_title: "CandidateType".to_string(),
            field_name: "candidate_id".to_string(),
            source: DetectionSource::ScalarRef,
        })
        .build();

    let config = test_domain_config();
    let project = test_project_config();
    let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let tera = generate::template_engine::create_tera(&template_dir).unwrap();
    let parent_candidates = vec![ParentCandidate {
        child_title: "ApplicationType".to_string(),
        parent_title: "CandidateType".to_string(),
        field_name: "candidate".to_string(),
        source: DetectionSource::ScalarRef,
    }];

    // 1. Entity model: required ref + parent candidate → Uuid (non-Option).
    let entity_gen = generate::db::entity::SeaOrmEntityGenerator::new(Path::new("/tmp/out"));
    let entity_files = entity_gen
        .generate(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            &tera,
            &project,
        )
        .await
        .unwrap();
    let entity = entity_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("application.rs"))
        .expect("should produce application.rs entity file")
        .content
        .clone();
    assert!(
        entity.contains("pub candidate_id: Uuid,"),
        "required entity ref with parent candidate must be non-Option Uuid in the model. Got:\n{entity}"
    );
    assert!(
        !entity.contains("pub candidate_id: Option<Uuid>"),
        "required entity ref with parent candidate must NOT be Option<Uuid>. Got:\n{entity}"
    );

    // 2. Repository emitter: required ref → Set(v).
    let emitter = generate::ddd::repository_emitter::RepositoryImplEmitter;
    let repo_code = emitter
        .emit(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            None,
            &[],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    assert!(
        repo_code.contains("model.candidate_id = Set(v);"),
        "required entity ref must emit Set(v). Got:\n{repo_code}"
    );
    assert!(
        !repo_code.contains("model.candidate_id = Set(Some(v));"),
        "required entity ref must NOT emit Set(Some(v)). Got:\n{repo_code}"
    );

    // 3. DTO: required ref → non-Option uuid::Uuid in the CREATE DTO.
    let tmp = std::env::temp_dir().join("required-parent-candidate-dto");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dto_files = dto_gen
        .generate(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            &tera,
            &project,
        )
        .await
        .unwrap();

    let create_dto = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file")
        .content
        .clone();
    assert!(
        create_dto.contains("pub candidate_id: uuid::Uuid"),
        "create DTO required entity ref must be non-Option uuid::Uuid. Got:\n{create_dto}"
    );
    assert!(
        !create_dto.contains("pub candidate_id: Option<uuid::Uuid>"),
        "create DTO required entity ref must NOT be Option<uuid::Uuid>. Got:\n{create_dto}"
    );

    // 4. DDL: required ref + parent candidate → NOT NULL UUID column.
    let ddl_gen = generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"))
        .with_parent_candidates(parent_candidates);
    let ddl_files = ddl_gen
        .generate(
            &mock,
            "ApplicationType",
            "recruiting",
            &config,
            &tera,
            &project,
        )
        .await
        .unwrap();
    let ddl = ddl_files
        .iter()
        .map(|f| f.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        ddl.contains("candidate_id UUID NOT NULL"),
        "DDL required entity ref with parent candidate must be NOT NULL. Got:\n{ddl}"
    );
}

/// Issue #59: include-path fetch generation must not assume FK fields are
/// `Option<uuid::Uuid>`. When the schema marks a genuine EntityReference as
/// `required`, the entity model emits `Uuid` and the DTO emits `uuid::Uuid`,
/// so the repository fetch code must access the field directly (no
/// `.and_then()` / `if let Some` / `match ... Some(v)`). Optional FKs must
/// keep the Option patterns.
#[tokio::test]
async fn required_include_fk_emits_direct_lookup_not_option_patterns() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "surveys/json/SurveyType.json",
            "SurveyType",
            "survey",
            "surveys",
            "entity_reference",
        ))
        .with_schema(mock_schema(
            "surveys/json/SurveyResponseType.json",
            "SurveyResponseType",
            "survey_response",
            "surveys",
            "entity_reference",
        ))
        .with_properties(
            "SurveyResponseType",
            vec![
                prop("title", "String", "TEXT", true, None, None, None, false),
                // Required genuine EntityReference → survey_id FK is Uuid.
                prop(
                    "survey_id",
                    "Uuid",
                    "UUID",
                    true,
                    Some("entity_reference"),
                    Some(RefClassificationKind::EntityReference),
                    Some("surveys/json/SurveyType.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();

    // Required-FK include path: survey_id is a required genuine EntityReference.
    let required_path = codegraph::generate::api::include_path::ResolvedIncludePath {
        alias: "survey".to_string(),
        segments: vec![codegraph::generate::api::include_path::IncludeSegment {
            entity_name: "SurveyType".to_string(),
            schema_title: "SurveyType".to_string(),
            module_name: "survey".to_string(),
            domain: "surveys".to_string(),
            table: "\"surveys\".\"survey\"".to_string(),
            fk_column: "survey_id".to_string(),
            reverse_fk_column: "survey_id".to_string(),
            fk_is_required: true,
            reverse_fk_is_required: false,
            is_array: false,
            child_table_override: None,
        }],
        response_rust_type: "SurveyResponse".to_string(),
        fetch_method: "fetch_survey_for_survey_response".to_string(),
        batch_fetch_method: "fetch_survey_batch_for_survey_response".to_string(),
    };

    let emitter = generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "SurveyResponseType",
            "surveys",
            &config,
            None,
            &[required_path],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    // Batch one-to-one lookup: direct target lookup, no inner .and_then on the FK.
    assert!(
        code.contains("target_by_id.get(&s.survey_id).cloned()"),
        "required FK must look up the target directly. Got:\n{code}"
    );
    assert!(
        !code.contains("s.survey_id.and_then(|fk|"),
        "required FK must NOT use .and_then() on the FK field. Got:\n{code}"
    );
    // Batch FK collection: no `if let Some(fk) = source.survey_id`.
    assert!(
        !code.contains("if let Some(fk) = source.survey_id"),
        "required FK must NOT use `if let Some` to collect. Got:\n{code}"
    );
    assert!(
        code.contains("fk_values.push(source.survey_id);"),
        "required FK must be pushed directly into fk_values. Got:\n{code}"
    );
    // Single fetch: no `match source.survey_id { Some(v) => ... }`.
    assert!(
        !code.contains("match source.survey_id"),
        "required FK must NOT use match on the FK field. Got:\n{code}"
    );
    assert!(
        code.contains("let fk_value = source.survey_id;"),
        "required FK must be read directly in the single fetch. Got:\n{code}"
    );
}

/// Optional FK include paths must keep the Option patterns
/// (`.and_then()`, `if let Some`, `match ... Some(v)`) — the regression test
/// for the required case must not have removed the optional path.
#[tokio::test]
async fn optional_include_fk_keeps_option_patterns() {
    let mock = MockEngine::builder()
        .with_schema(mock_schema(
            "common/json/PersonType.json",
            "PersonType",
            "person",
            "common",
            "entity_reference",
        ))
        .with_schema(mock_schema(
            "common/json/WorkerType.json",
            "WorkerType",
            "worker",
            "common",
            "entity_reference",
        ))
        .with_properties(
            "WorkerType",
            vec![
                prop("title", "String", "TEXT", true, None, None, None, false),
                // Optional genuine EntityReference → person_id FK is Option<Uuid>.
                prop(
                    "person_id",
                    "Uuid",
                    "UUID",
                    false,
                    Some("entity_reference"),
                    Some(RefClassificationKind::EntityReference),
                    Some("common/json/PersonType.json"),
                    false,
                ),
            ],
        )
        .build();

    let config = test_domain_config();

    let optional_path = codegraph::generate::api::include_path::ResolvedIncludePath {
        alias: "person".to_string(),
        segments: vec![codegraph::generate::api::include_path::IncludeSegment {
            entity_name: "PersonType".to_string(),
            schema_title: "PersonType".to_string(),
            module_name: "person".to_string(),
            domain: "common".to_string(),
            table: "\"common\".\"person\"".to_string(),
            fk_column: "person_id".to_string(),
            reverse_fk_column: "worker_id".to_string(),
            fk_is_required: false,
            reverse_fk_is_required: false,
            is_array: false,
            child_table_override: None,
        }],
        response_rust_type: "PersonResponse".to_string(),
        fetch_method: "fetch_person_for_worker".to_string(),
        batch_fetch_method: "fetch_person_batch_for_worker".to_string(),
    };

    let emitter = generate::ddd::repository_emitter::RepositoryImplEmitter;
    let code = emitter
        .emit(
            &mock,
            "WorkerType",
            "common",
            &config,
            None,
            &[optional_path],
            &ProjectConfig::default(),
        )
        .await
        .unwrap();

    assert!(
        code.contains("s.person_id.and_then(|fk| target_by_id.get(&fk).cloned())"),
        "optional FK must keep the .and_then() lookup. Got:\n{code}"
    );
    assert!(
        code.contains("if let Some(fk) = source.person_id"),
        "optional FK must keep the if-let collection. Got:\n{code}"
    );
    assert!(
        code.contains("let fk_value = match source.person_id"),
        "optional FK must keep the match on the FK field. Got:\n{code}"
    );
}
