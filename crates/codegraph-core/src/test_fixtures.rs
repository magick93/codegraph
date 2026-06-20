//! Canonical test data for backend conformance testing.
//! Each backend crate imports these fixtures and runs them through its engine.

use crate::types::*;

/// A minimal SchemaNode for an entity (PersonType).
pub fn person_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "common/PersonType".into(),
        title: "PersonType".into(),
        description: Some("A person".into()),
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("common".into()),
        rel_path: "common/PersonType.json".into(),
        pg_type: "TABLE".into(),
        rust_type: "PersonType".into(),
        sea_orm_type: "Entity".into(),
        rust_type_name: "PersonType".into(),
        pg_table_name: "person_type".into(),
        api_path_segment: "person-type".into(),
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

/// A codelist schema (GenderCodeList).
pub fn gender_codelist_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "common/GenderCodeList".into(),
        title: "GenderCodeList".into(),
        description: Some("Gender codes".into()),
        schema_type: "string".into(),
        classification: "codelist".into(),
        domain: Some("common".into()),
        rel_path: "common/GenderCodeList.json".into(),
        pg_type: "TEXT".into(),
        rust_type: "String".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "GenderCodeList".into(),
        pg_table_name: "gender_code_list".into(),
        api_path_segment: "gender-code-list".into(),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

/// Properties for PersonType.
pub fn person_properties() -> Vec<PropertyNode> {
    vec![
        PropertyNode {
            name: "givenName".into(),
            prop_type: "string".into(),
            description: Some("First name".into()),
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "given_name".into(),
            pg_column_type: "TEXT".into(),
            rust_field_name: "given_name".into(),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "scalar".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "familyName".into(),
            prop_type: "string".into(),
            description: Some("Last name".into()),
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "family_name".into(),
            pg_column_type: "TEXT".into(),
            rust_field_name: "family_name".into(),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "scalar".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
        PropertyNode {
            name: "gender".into(),
            prop_type: "string".into(),
            description: Some("Gender code".into()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "gender".into(),
            pg_column_type: "TEXT".into(),
            rust_field_name: "gender".into(),
            rust_field_type: "Option<String>".into(),
            sea_orm_type: "String".into(),
            render_strategy: "codelist_fk".into(),
            ref_target: Some("GenderCodeList".into()),
            classification: Some("codelist".into()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        },
    ]
}

/// CodeList fixture.
pub fn gender_codelist() -> CodeList {
    CodeList {
        name: "GenderCodeList".into(),
        description: Some("Gender codes".into()),
        pg_table_name: "gender_code_list".into(),
        render_as: "enum".into(),
        check_expression: None,
    }
}

/// EnumValue fixtures for GenderCodeList.
pub fn gender_enum_values() -> Vec<EnumValue> {
    vec![
        EnumValue {
            value: "Male".into(),
            display_name: Some("Male".into()),
            sort_order: 0,
        },
        EnumValue {
            value: "Female".into(),
            display_name: Some("Female".into()),
            sort_order: 1,
        },
        EnumValue {
            value: "NotSpecified".into(),
            display_name: Some("Not Specified".into()),
            sort_order: 2,
        },
    ]
}

/// Edge fixtures: the edges to create after ingesting schemas + properties.
/// Returns (from_id, to_id, edge_type, props) tuples.
///
/// Note: HasProperty edges are omitted because `ingest_property` already
/// creates them internally. Only cross-node reference edges are listed here.
pub fn person_edges() -> Vec<(String, String, EdgeType, Option<EdgeProperties>)> {
    vec![
        // gender -[ReferencesSchema]-> GenderCodeList
        (
            "gender::PersonType".into(),
            "common/GenderCodeList".into(),
            EdgeType::ReferencesSchema,
            Some(EdgeProperties {
                ref_path: Some("common/GenderCodeList.json".into()),
                resolved_classification: Some("codelist".into()),
                ..Default::default()
            }),
        ),
        // gender -[UsesCodeList]-> GenderCodeList
        (
            "gender::PersonType".into(),
            "GenderCodeList".into(),
            EdgeType::UsesCodeList,
            Some(EdgeProperties {
                render_as: Some("enum".into()),
                ..Default::default()
            }),
        ),
    ]
}

/// Ingest all fixtures into the given engine.
pub async fn ingest_fixtures(
    engine: &dyn crate::traits::GraphIngestor,
) -> Result<IngestStats, crate::error::GraphError> {
    // Schemas
    engine.ingest_schema(&person_schema()).await?;
    engine.ingest_schema(&gender_codelist_schema()).await?;

    // Properties
    for prop in person_properties() {
        engine.ingest_property("PersonType", "PersonType.json#", &prop).await?;
    }

    // Codelist + enum values
    engine.ingest_codelist(&gender_codelist()).await?;
    for val in gender_enum_values() {
        engine.ingest_enum_value("GenderCodeList", &val).await?;
    }

    // Edges
    for (from, to, edge_type, props) in person_edges() {
        engine
            .ingest_edge(&from, &to, edge_type, props.as_ref())
            .await?;
    }

    engine.finalize().await
}

/// Run conformance queries against a populated engine.
/// Call `ingest_fixtures` first.
pub async fn assert_conformance_queries(querier: &dyn crate::traits::GraphQuerier) {
    // get_schema
    let person = querier.get_schema("PersonType").await.unwrap();
    assert!(person.is_some(), "PersonType should exist");
    let person = person.unwrap();
    assert!(person.is_entity);
    assert_eq!(person.domain.as_deref(), Some("common"));

    // list_schemas
    let all = querier.list_schemas(None).await.unwrap();
    assert_eq!(all.len(), 2, "should have PersonType + GenderCodeList");

    let common = querier.list_schemas(Some("common")).await.unwrap();
    assert_eq!(common.len(), 2);

    // get_properties
    let props = querier.get_properties("PersonType").await.unwrap();
    assert_eq!(props.len(), 3, "PersonType should have 3 properties");
    let names: Vec<&str> = props.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"givenName"));
    assert!(names.contains(&"familyName"));
    assert!(names.contains(&"gender"));

    // get_entity_names
    let entities = querier.get_entity_names().await.unwrap();
    assert_eq!(entities, vec!["PersonType"]);

    // get_codelist
    let cl = querier.get_codelist("GenderCodeList").await.unwrap();
    assert!(cl.is_some());
    assert_eq!(cl.unwrap().render_as, "enum");

    // list_codelists
    let cls = querier.list_codelists().await.unwrap();
    assert_eq!(cls.len(), 1);

    // get_enum_values
    let vals = querier.get_enum_values("GenderCodeList").await.unwrap();
    assert_eq!(vals.len(), 3);

    // get_codelist_for_property
    let cl_prop = querier
        .get_codelist_for_property("gender", "PersonType")
        .await
        .unwrap();
    assert!(cl_prop.is_some(), "gender should reference GenderCodeList");

    // get_referenced_schemas
    let refs = querier.get_referenced_schemas("PersonType").await.unwrap();
    assert!(refs.iter().any(|s| s.title == "GenderCodeList"));
}
