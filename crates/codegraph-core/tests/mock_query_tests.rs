use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::*;

#[tokio::test]
async fn builder_creates_engine_with_preloaded_data() {
    let schema = SchemaNode {
        schema_id: "common/json/PersonType.json".into(),
        title: "PersonType".into(),
        description: Some("A person".into()),
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("common".into()),
        rel_path: "common/json/PersonType.json".into(),
        pg_type: "TABLE".into(),
        rust_type: "PersonType".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "PersonType".into(),
        pg_table_name: "person".into(),
        api_path_segment: "person".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };

    let prop = PropertyNode {
        name: "givenName".into(),
        prop_type: "string".into(),
        description: None,
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
        render_strategy: "flat".into(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    };

    let tree = CompositionTree {
        root: CompositionNode {
            field_name: "person".into(),
            schema_title: "PersonType".into(),
            table_schema: "common".into(),
            table_name: "person".into(),
            fk: None,
            is_collection: false,
            columns: vec![],
            jsonb_columns: vec![],
            children: vec![],
            composite_range: None,
            consumed_fields: vec![],
        },
    };

    let engine = MockEngine::builder()
        .with_schema(schema.clone())
        .with_properties("PersonType", vec![prop.clone()])
        .with_composition_tree("PersonType", tree.clone())
        .build();

    let found = engine.get_schema("PersonType").await.unwrap();
    assert_eq!(found, Some(schema));

    let props = engine.get_properties("PersonType").await.unwrap();
    assert_eq!(props.len(), 1);

    let ct = engine.get_composition_tree("PersonType").await.unwrap();
    assert_eq!(ct.root.schema_title, "PersonType");
}

#[tokio::test]
async fn composition_tree_not_found_returns_error() {
    let engine = MockEngine::new();
    let result = engine.get_composition_tree("NoSuchType").await;
    assert!(result.is_err());
}
