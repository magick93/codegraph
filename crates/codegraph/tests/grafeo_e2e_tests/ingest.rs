//! Ingestion + graph-level tests: what lands in the Grafeo graph (schemas,
//! properties, classification kinds, edges) before any generator runs.
//!
//! Stragglers housed here (nearest cohesive): `entity_names_extracted_from_fixture_config`
//! tests `setup::entity_names_from_config`, the ingestion input; and the
//! `child_dto_*_uses_classified_type` pair, which — despite the name — only
//! asserts classification on graph properties (no generation).

use std::path::Path;

use codegraph_core::traits::GraphQuerier;
use codegraph_type_contracts::RefClassificationKind;

use crate::setup::{entity_names_from_config, setup_grafeo};
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
