//! Cross-layer consistency tests: DDL / SeaORM entity / DTO / repository
//! agreement, VO→entity FK nullability, array-type property inheritance.

use std::path::Path;

use codegraph::generate::ProjectConfig;
use codegraph::generate::ddd::repository_emitter::RepositoryImplEmitter;
use codegraph::generate::domain_types::dto::DomainTypesDtoGenerator;
use codegraph::generate::template_engine::create_tera;
use codegraph::generate::traits::EntityGenerator;
use codegraph_core::traits::GraphQuerier;
use codegraph_type_contracts::RefClassificationKind;

use crate::setup::setup_grafeo;
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
    let entity_files = entity_gen
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

// === Regression: required VO→entity refs (issue #48) ===
//
// A scalar VO property whose allOf chain reaches a known entity must not
// produce a required (NOT NULL) FK column: the DTO and repository generators
// model it as a nested child table, so nothing ever populates the FK column
// and every create fails with a NOT NULL violation.

#[tokio::test]
async fn required_vo_entity_ref_emits_nullable_fk() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // Sanity: the fixture graph has the expected shape.
    let event_prop = engine
        .get_properties("RsvpType")
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == "event")
        .expect("RsvpType should have an 'event' property");
    assert!(
        event_prop.is_required,
        "event must be required in the fixture"
    );
    assert_eq!(
        event_prop.effective_kind(),
        Some(RefClassificationKind::ValueObject),
        "event should classify as a ValueObject"
    );

    // 1. Composition tree: VO→entity ref becomes an FK column that is nullable.
    // (ColumnInfo keeps the raw property name; the DDL generator appends `_id`.)
    let tree = engine.get_composition_tree("RsvpType").await.unwrap();
    let event_col = tree
        .root
        .columns
        .iter()
        .find(|c| c.name == "event")
        .expect("composition tree should materialize an event FK column");
    assert!(
        event_col.is_optional,
        "required VO→entity FK column must be nullable — the DTO/repository model it as a nested child table"
    );
    let fk = event_col
        .fk_target
        .as_ref()
        .expect("event_id should have an FK target");
    assert_eq!(
        fk.schema, "events",
        "event_id should reference the events schema"
    );
    assert_eq!(
        fk.table, "public_event",
        "event_id should reference events.public_event, not an unrelated entity"
    );

    // 2. DDL: the FK column is nullable (no NOT NULL).
    let ddl_gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let ddl_files = ddl_gen
        .generate(
            &engine,
            "RsvpType",
            "rsvp",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    let ddl = ddl_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("rsvp_rsvp.sql"))
        .expect("should produce rsvp_rsvp.sql")
        .content
        .clone();
    assert!(
        ddl.contains("event_id UUID,"),
        "event_id column must be nullable. Got:\n{ddl}"
    );
    assert!(
        !ddl.contains("\n    event_id UUID NOT NULL"),
        "event_id must NOT be NOT NULL. Got:\n{ddl}"
    );

    // 3. Create DTO keeps the nested child-table VO contract.
    let tmp = std::env::temp_dir().join("grafeo-test-required-vo-dto");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
    let dto_files = dto_gen
        .generate(
            &engine,
            "RsvpType",
            "rsvp",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    let create = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .expect("should produce dto_create file")
        .content
        .clone();
    assert!(
        create.contains("pub event: Option<"),
        "create DTO must keep the nested VO field. Got:\n{create}"
    );

    // 4. Entity model: the FK field is Option<Uuid> (matches the nullable column).
    let entity_gen =
        codegraph::generate::db::entity::SeaOrmEntityGenerator::new(Path::new("/tmp/out"));
    let entity_files = entity_gen
        .generate(
            &engine,
            "RsvpType",
            "rsvp",
            &config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .unwrap();
    let entity = entity_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("rsvp_rsvp.rs"))
        .expect("should produce the rsvp_rsvp.rs entity file")
        .content
        .clone();
    assert!(
        entity.contains("pub event_id: Option<Uuid>"),
        "entity model FK field must be Option<Uuid>. Got:\n{entity}"
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

#[tokio::test]
async fn grafeo_composite_wrapper_cross_layer_consistency() {
    let (engine, config) = setup_grafeo().await;
    let tera = create_tera(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")).unwrap();

    // DDL (EntityGenerator trait)
    let ddl_gen = codegraph::generate::db::ddl::DdlGenerator::new(Path::new("/tmp/out"));
    let ddl_files = ddl_gen
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
    let ddl_content = &ddl_files[0].content;

    // DTO (domain_types generator for struct content)
    let tmp = std::env::temp_dir().join("grafeo-test-composite-cross");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dto_gen = DomainTypesDtoGenerator::new_with_base(tmp.clone());
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
    let dto_create = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .unwrap();

    // Repository (RepositoryImplEmitter::emit() → String)
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
    let entity_files = entity_gen
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
    let create_dto = dto_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("dto_create"))
        .map(|f| &f.content)
        .expect("Create DTO should be generated");

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
