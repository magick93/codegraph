use crate::harness::{test_domain_config, test_project_config, test_tera};
use codegraph::generate;
#[allow(unused_imports)]
use codegraph::generate::traits::{DomainGenerator, EntityGenerator, GlobalGenerator};
use codegraph_core::mock::MockEngine;
use codegraph_core::types::{PropertyNode, SchemaNode};

// === Composite Range Collapsing Tests ===

#[tokio::test]
async fn composite_range_collapses_start_end_into_daterange() {
    use codegraph_core::types::CompositeRange;

    let start_prop = PropertyNode {
        name: "start".to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: Some("date-time".to_string()),
        is_required: false,
        is_nullable: true,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "start".to_string(),
        pg_column_type: "TIMESTAMPTZ".to_string(),
        rust_field_name: "start".to_string(),
        rust_field_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };

    let end_prop = PropertyNode {
        name: "end".to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: Some("date-time".to_string()),
        is_required: false,
        is_nullable: true,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "end".to_string(),
        pg_column_type: "TIMESTAMPTZ".to_string(),
        rust_field_name: "end".to_string(),
        rust_field_type: "chrono::DateTime<chrono::Utc>".to_string(),
        sea_orm_type: "TimestampWithTimeZone".to_string(),
        render_strategy: "direct_column".to_string(),
        ref_target: None,
        classification: Some("primitive_wrapper".to_string()),
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    };

    let title_prop = PropertyNode {
        name: "title".to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_array: false,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: "title".to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: "title".to_string(),
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
        type_expr: None,
    };

    let composite_range = CompositeRange {
        pg_column_name: "history_period".to_string(),
        pg_type: "DATERANGE".to_string(),
        rust_type: "std::ops::Range<chrono::NaiveDate>".to_string(),
        start_field: "start".to_string(),
        end_field: "end".to_string(),
        open_end: false,
    };

    let consumed_fields = vec![
        (start_prop.clone(), "start".to_string()),
        (end_prop.clone(), "end".to_string()),
    ];

    let schema = SchemaNode {
        namespace: None,
        schema_id: "common/json/PositionHistoryType.json".to_string(),
        title: "PositionHistoryType".to_string(),
        description: Some("A record of position history".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/PositionHistoryType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "PositionHistory".to_string(),
        pg_table_name: "position_history".to_string(),
        api_path_segment: "position-histories".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    let mock = MockEngine::builder()
        .with_schema(schema)
        .with_properties(
            "PositionHistoryType",
            vec![start_prop.clone(), end_prop.clone(), title_prop.clone()],
        )
        .with_composite_range("PositionHistoryType", composite_range)
        .with_consumed_fields("PositionHistoryType", consumed_fields)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-composite-range");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(
            &mock,
            "PositionHistoryType",
            "common",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let table_file = files
        .iter()
        .find(|f| {
            f.path
                .to_string_lossy()
                .contains("common_position_history.sql")
        })
        .expect("Should have a table SQL file");

    let content = &table_file.content;

    assert!(
        content.contains("history_period DATERANGE"),
        "DDL should contain history_period DATERANGE column. Got:\n{}",
        content
    );
    assert!(
        !content.contains("    start TIMESTAMPTZ"),
        "DDL should NOT contain start TIMESTAMPTZ (consumed by composite range). Got:\n{}",
        content
    );
    assert!(
        !content.contains("    end TIMESTAMPTZ"),
        "DDL should NOT contain end TIMESTAMPTZ (consumed by composite range). Got:\n{}",
        content
    );
    assert!(
        content.contains("title TEXT"),
        "DDL should still contain title TEXT (non-consumed field). Got:\n{}",
        content
    );
}

// === Recursive Child Table Tests ===

#[tokio::test]
async fn recursive_child_tables_with_full_classification() {
    // 3-level hierarchy:
    // PersonType (entity)
    //   └─ communication (ValueObject → CommunicationType)
    //        └─ address (ValueObject → AddressType)
    //             ├─ city (PrimitiveWrapper TEXT)
    //             └─ countryCode (CodelistReference)

    let person_schema = SchemaNode {
        namespace: None,
        schema_id: "common/json/PersonType.json".to_string(),
        title: "PersonType".to_string(),
        description: Some("A person".to_string()),
        schema_type: "object".to_string(),
        classification: "entity_reference".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/PersonType.json".to_string(),
        pg_type: "UUID".to_string(),
        rust_type: "Uuid".to_string(),
        sea_orm_type: "Uuid".to_string(),
        rust_type_name: "Person".to_string(),
        pg_table_name: "person".to_string(),
        api_path_segment: "persons".to_string(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    let communication_schema = SchemaNode {
        namespace: None,
        schema_id: "common/json/CommunicationType.json".to_string(),
        title: "CommunicationType".to_string(),
        description: Some("Communication details".to_string()),
        schema_type: "object".to_string(),
        classification: "value_object".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/CommunicationType.json".to_string(),
        pg_type: "JSONB".to_string(),
        rust_type: "Communication".to_string(),
        sea_orm_type: "JsonBinary".to_string(),
        rust_type_name: "Communication".to_string(),
        pg_table_name: "communication".to_string(),
        api_path_segment: "".to_string(),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    let address_schema = SchemaNode {
        namespace: None,
        schema_id: "common/json/AddressType.json".to_string(),
        title: "AddressType".to_string(),
        description: Some("Address details".to_string()),
        schema_type: "object".to_string(),
        classification: "value_object".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/AddressType.json".to_string(),
        pg_type: "JSONB".to_string(),
        rust_type: "Address".to_string(),
        sea_orm_type: "JsonBinary".to_string(),
        rust_type_name: "Address".to_string(),
        pg_table_name: "address".to_string(),
        api_path_segment: "".to_string(),
        parent_schema: None,
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    let country_codelist_schema = SchemaNode {
        namespace: None,
        schema_id: "common/json/codelist/CountryCodeList.json".to_string(),
        title: "CountryCodeList".to_string(),
        description: Some("Country codes".to_string()),
        schema_type: "object".to_string(),
        classification: "codelist".to_string(),
        domain: Some("common".to_string()),
        rel_path: "common/json/codelist/CountryCodeList.json".to_string(),
        pg_type: "TEXT".to_string(),
        rust_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        rust_type_name: "CountryCode".to_string(),
        pg_table_name: "country_code".to_string(),
        api_path_segment: "".to_string(),
        parent_schema: None,
        is_entity: false,
        is_codelist: true,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    };

    // Person has a "name" (PrimitiveWrapper) and "communication" (ValueObject)
    let person_props = vec![
        PropertyNode {
            name: "name".to_string(),
            prop_type: "string".to_string(),
            description: Some("Person name".to_string()),
            format: None,
            is_required: true,
            is_nullable: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "name".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "name".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: Some("common/json/CommunicationType.json".to_string()),
            classification: Some("primitive_wrapper".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
        PropertyNode {
            name: "communication".to_string(),
            prop_type: "object".to_string(),
            description: Some("Communication details".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "communication".to_string(),
            pg_column_type: "JSONB".to_string(),
            rust_field_name: "communication".to_string(),
            rust_field_type: "Communication".to_string(),
            sea_orm_type: "JsonBinary".to_string(),
            render_strategy: "value_object".to_string(),
            ref_target: Some("common/json/CommunicationType.json".to_string()),
            classification: Some("value_object".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
    ];

    // CommunicationType has "email" (PrimitiveWrapper) and "address" (ValueObject)
    let communication_props = vec![
        PropertyNode {
            name: "email".to_string(),
            prop_type: "string".to_string(),
            description: Some("Email address".to_string()),
            format: Some("email".to_string()),
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "email".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "email".to_string(),
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
            type_expr: None,
        },
        PropertyNode {
            name: "address".to_string(),
            prop_type: "object".to_string(),
            description: Some("Physical address".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "address".to_string(),
            pg_column_type: "JSONB".to_string(),
            rust_field_name: "address".to_string(),
            rust_field_type: "Address".to_string(),
            sea_orm_type: "JsonBinary".to_string(),
            render_strategy: "value_object".to_string(),
            ref_target: Some("common/json/AddressType.json".to_string()),
            classification: Some("value_object".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
    ];

    // AddressType has "city" (PrimitiveWrapper) and "countryCode" (CodelistReference)
    let address_props = vec![
        PropertyNode {
            name: "city".to_string(),
            prop_type: "string".to_string(),
            description: Some("City name".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "city".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "city".to_string(),
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
            type_expr: None,
        },
        PropertyNode {
            name: "countryCode".to_string(),
            prop_type: "string".to_string(),
            description: Some("Country code".to_string()),
            format: None,
            is_required: false,
            is_nullable: true,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: "country_code".to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: "country_code".to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "fk_lookup".to_string(),
            ref_target: Some("common/json/codelist/CountryCodeList.json".to_string()),
            classification: Some("codelist_reference".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        },
    ];

    let engine = MockEngine::builder()
        .with_schema(person_schema)
        .with_schema(communication_schema.clone())
        .with_schema(address_schema.clone())
        .with_schema(country_codelist_schema)
        .with_properties("PersonType", person_props)
        .with_properties("CommunicationType", communication_props)
        .with_properties("AddressType", address_props)
        // Wire up $ref resolution for ValueObject properties
        .with_ref_target("communication", "PersonType", communication_schema)
        .with_ref_target("address", "CommunicationType", address_schema)
        .build();

    let config = test_domain_config();
    let tera = test_tera();
    let output_dir = std::path::PathBuf::from("/tmp/hr-graph-test-recursive-child");

    let gen = generate::db::ddl::DdlGenerator::new(&output_dir);
    let files = gen
        .generate(
            &engine,
            "PersonType",
            "common",
            &config,
            &tera,
            &test_project_config(),
        )
        .await
        .unwrap();

    let table_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("common_person.sql"))
        .expect("Should have a table SQL file");

    let content = &table_file.content;

    // First-level child table: person_communication
    assert!(
        content.contains("CREATE TABLE IF NOT EXISTS common.person_communication"),
        "Should contain first-level child table person_communication. Got:\n{}",
        content
    );

    // First-level child should have email column
    assert!(
        content.contains("email TEXT"),
        "person_communication should have email TEXT column. Got:\n{}",
        content
    );

    // First-level child should have parent FK column
    assert!(
        content.contains("person_id UUID NOT NULL"),
        "person_communication should have person_id UUID NOT NULL column. Got:\n{}",
        content
    );

    // First-level child parent FK is now ALTER TABLE ADD CONSTRAINT (not inline REFERENCES)
    assert!(
        content.contains("FOREIGN KEY (person_id) REFERENCES common.person(id) ON DELETE CASCADE"),
        "person_communication should have ALTER TABLE FK to parent person. Got:\n{}",
        content
    );

    // Nested child table: person_communication_address
    assert!(
        content.contains("CREATE TABLE IF NOT EXISTS common.person_communication_address"),
        "Should contain nested child table person_communication_address. Got:\n{}",
        content
    );

    // Nested child should have city TEXT (properly typed, not empty)
    assert!(
        content.contains("city TEXT"),
        "person_communication_address should have city TEXT column. Got:\n{}",
        content
    );

    // Nested child should have country_code TEXT column
    assert!(
        content.contains("country_code TEXT"),
        "person_communication_address should have country_code TEXT column. Got:\n{}",
        content
    );

    // Nested child should have parent FK column
    assert!(
        content.contains("person_communication_id UUID NOT NULL"),
        "person_communication_address should have person_communication_id column. Got:\n{}",
        content
    );

    // Nested child parent FK is ALTER TABLE ADD CONSTRAINT (not inline REFERENCES)
    assert!(
        content.contains("FOREIGN KEY (person_communication_id) REFERENCES common.person_communication(id) ON DELETE CASCADE"),
        "person_communication_address should have ALTER TABLE FK to person_communication. Got:\n{}",
        content
    );

    // Codelist FK constraint for country_code in nested child
    assert!(
        content.contains("fk_person_communication_address_country_code"),
        "Should have FK constraint name containing person_communication_address_country_code. Got:\n{}",
        content
    );
}
