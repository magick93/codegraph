use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::*;
use codegraph_grafeo::GrafeoEngine;

fn test_schema_node() -> SchemaNode {
    SchemaNode {
        schema_id: "common/PersonType".to_string(),
        title: "PersonType".to_string(),
        description: Some("A person".to_string()),
        schema_type: "object".to_string(),
        classification: "entity".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/PersonType.json".to_string(),
        pg_type: "TABLE".to_string(),
        rust_type: "PersonType".to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: "PersonType".to_string(),
        pg_table_name: "person_type".to_string(),
        api_path_segment: "person-type".to_string(),
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

fn test_property_node() -> PropertyNode {
    PropertyNode {
        name: "givenName".to_string(),
        prop_type: "string".to_string(),
        description: Some("First name".to_string()),
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
        sea_orm_type: "String".to_string(),
        render_strategy: "scalar".to_string(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

#[tokio::test]
async fn test_ingest_schema() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let schema = test_schema_node();
    let id = engine.ingest_schema(&schema).await.unwrap();
    assert_eq!(id, "common/PersonType");

    let session = engine.db().session();
    let result = session
        .execute("MATCH (s:Schema {title: 'PersonType'}) RETURN s.schema_id")
        .unwrap();
    assert_eq!(result.rows.len(), 1, "Schema node not found after ingest");
    assert_eq!(result.rows[0][0].as_str().unwrap(), "common/PersonType");
}

#[tokio::test]
async fn test_ingest_property() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine.ingest_schema(&test_schema_node()).await.unwrap();
    let prop = test_property_node();
    engine.ingest_property("PersonType", &prop).await.unwrap();

    let session = engine.db().session();
    let result = session
        .execute("MATCH (:Schema {title: 'PersonType'})-[:HasProperty]->(p:Property) RETURN p.name")
        .unwrap();
    assert_eq!(
        result.rows.len(),
        1,
        "Property not found via HasProperty edge"
    );
    assert_eq!(result.rows[0][0].as_str().unwrap(), "givenName");
}

#[tokio::test]
async fn test_ingest_codelist() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let codelist = CodeList {
        name: "GenderCodeList".to_string(),
        description: Some("Gender codes".to_string()),
        pg_table_name: "gender_code_list".to_string(),
        render_as: "dropdown".to_string(),
        check_expression: None,
    };
    engine.ingest_codelist(&codelist).await.unwrap();

    let session = engine.db().session();
    let result = session
        .execute("MATCH (c:CodeList {name: 'GenderCodeList'}) RETURN c.name")
        .unwrap();
    assert_eq!(result.rows.len(), 1, "CodeList not found after ingest");
}

#[tokio::test]
async fn test_ingest_enum_value() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let codelist = CodeList {
        name: "GenderCodeList".to_string(),
        description: None,
        pg_table_name: "gender_code_list".to_string(),
        render_as: "dropdown".to_string(),
        check_expression: None,
    };
    engine.ingest_codelist(&codelist).await.unwrap();

    let val = EnumValue {
        value: "Male".to_string(),
        display_name: Some("Male".to_string()),
        sort_order: 1,
    };
    engine
        .ingest_enum_value("GenderCodeList", &val)
        .await
        .unwrap();

    let session = engine.db().session();
    let result = session
        .execute(
            "MATCH (:CodeList {name: 'GenderCodeList'})-[:HasEnumValue]->(v:EnumValue) RETURN v.value",
        )
        .unwrap();
    assert_eq!(
        result.rows.len(),
        1,
        "EnumValue not found via HasEnumValue edge"
    );
}

#[tokio::test]
async fn test_ingest_edge() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine.ingest_schema(&test_schema_node()).await.unwrap();

    let mut schema2 = test_schema_node();
    schema2.schema_id = "common/AddressType".to_string();
    schema2.title = "AddressType".to_string();
    schema2.is_entity = false;
    engine.ingest_schema(&schema2).await.unwrap();

    engine
        .ingest_edge(
            "PersonType",
            "AddressType",
            EdgeType::DependsOn,
            Some(&EdgeProperties {
                dependency_type: Some("ref".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let session = engine.db().session();
    let result = session
        .execute(
            "MATCH (:Schema {title: 'PersonType'})-[r:DependsOn]->(:Schema {title: 'AddressType'}) RETURN r.dependency_type",
        )
        .unwrap();
    assert_eq!(
        result.rows.len(),
        1,
        "DependsOn edge not found after ingest_edge"
    );
    assert_eq!(result.rows[0][0].as_str().unwrap(), "ref");
}

#[tokio::test]
async fn test_finalize_stats() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine.ingest_schema(&test_schema_node()).await.unwrap();
    engine
        .ingest_property("PersonType", &test_property_node())
        .await
        .unwrap();

    let codelist = CodeList {
        name: "GenderCodeList".to_string(),
        description: None,
        pg_table_name: "gender_code_list".to_string(),
        render_as: "dropdown".to_string(),
        check_expression: None,
    };
    engine.ingest_codelist(&codelist).await.unwrap();

    let val = EnumValue {
        value: "Male".to_string(),
        display_name: None,
        sort_order: 1,
    };
    engine
        .ingest_enum_value("GenderCodeList", &val)
        .await
        .unwrap();

    let stats = engine.finalize().await.unwrap();
    assert_eq!(stats.schema_count, 1);
    assert_eq!(stats.property_count, 1);
    assert_eq!(stats.codelist_count, 1);
    assert_eq!(stats.enum_value_count, 1);
    assert!(stats.duration.as_nanos() > 0);
}
