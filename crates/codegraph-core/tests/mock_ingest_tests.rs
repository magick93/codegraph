use codegraph_core::mock::MockEngine;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{ConditionKind, ConditionNode, PropertyNode, SchemaNode};

fn test_schema() -> SchemaNode {
    SchemaNode {
        namespace: None,
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
        has_definitions: true,
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn test_property() -> PropertyNode {
    PropertyNode {
        name: "givenName".into(),
        prop_type: "string".into(),
        description: None,
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
        type_expr: None,
    }
}

#[tokio::test]
async fn ingest_and_query_schema() {
    let engine = MockEngine::new();
    let schema = test_schema();

    let id = engine.ingest_schema(&schema).await.unwrap();
    assert!(!id.is_empty());

    let found = engine.get_schema("PersonType").await.unwrap();
    assert_eq!(found, Some(schema));
}

#[tokio::test]
async fn ingest_and_query_properties() {
    let engine = MockEngine::new();
    let schema = test_schema();
    let prop = test_property();

    engine.ingest_schema(&schema).await.unwrap();
    engine
        .ingest_property("PersonType", "test/PersonType", &prop)
        .await
        .unwrap();

    let props = engine.get_properties("PersonType").await.unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].name, "givenName");
}

#[tokio::test]
async fn query_nonexistent_schema_returns_none() {
    let engine = MockEngine::new();
    let found = engine.get_schema("NoSuchType").await.unwrap();
    assert_eq!(found, None);
}

#[tokio::test]
async fn list_schemas_filters_by_domain() {
    let engine = MockEngine::new();
    let mut schema = test_schema();
    engine.ingest_schema(&schema).await.unwrap();

    schema.schema_id = "recruiting/json/CandidateType.json".into();
    schema.title = "CandidateType".into();
    schema.domain = Some("recruiting".into());
    engine.ingest_schema(&schema).await.unwrap();

    let all = engine.list_schemas(None).await.unwrap();
    assert_eq!(all.len(), 2);

    let common = engine.list_schemas(Some("common")).await.unwrap();
    assert_eq!(common.len(), 1);
    assert_eq!(common[0].title, "PersonType");
}

#[tokio::test]
async fn finalize_returns_stats() {
    let engine = MockEngine::new();
    let schema = test_schema();
    let prop = test_property();

    engine.ingest_schema(&schema).await.unwrap();
    engine
        .ingest_property("PersonType", "test/PersonType", &prop)
        .await
        .unwrap();

    let stats = engine.finalize().await.unwrap();
    assert_eq!(stats.schema_count, 1);
    assert_eq!(stats.property_count, 1);
}

#[tokio::test]
async fn get_child_schemas_returns_inline_defs() {
    let engine = MockEngine::new();
    let parent = test_schema();
    engine.ingest_schema(&parent).await.unwrap();

    let mut child = test_schema();
    child.schema_id = "common/json/PersonType.json#/definitions/PersonName".into();
    child.title = "PersonName".into();
    child.parent_schema = Some("PersonType".into());
    child.is_entity = false;
    engine.ingest_schema(&child).await.unwrap();

    let children = engine.get_child_schemas("PersonType").await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].title, "PersonName");
}

#[tokio::test]
async fn get_child_schemas_derives_refers_children() {
    let engine = MockEngine::new();
    engine.ingest_schema(&test_schema()).await.unwrap(); // PersonType (entity)

    let mut item = test_schema();
    item.schema_id = "common/json/ItemType.json".into();
    item.title = "ItemType".into();
    engine.ingest_schema(&item).await.unwrap();

    // Non-entity target (codelist) must NOT resolve as a child.
    let mut codelist = test_schema();
    codelist.schema_id = "common/json/PriorityCode.json".into();
    codelist.title = "PriorityCode".into();
    codelist.is_entity = false;
    codelist.is_codelist = true;
    engine.ingest_schema(&codelist).await.unwrap();

    // mox refers lowering: array-of-entity-ref property on the parent whose
    // ref_target names the child.
    let mut refers = test_property();
    refers.name = "items".into();
    refers.is_array = true;
    refers.ref_target = Some("ItemType".into());
    engine
        .ingest_property("PersonType", "test/PersonType", &refers)
        .await
        .unwrap();

    // File-path ref targets reduce to the same candidate title.
    let mut path_refers = test_property();
    path_refers.name = "extras".into();
    path_refers.is_array = true;
    path_refers.ref_target = Some("common/json/ItemType.json#".into());
    engine
        .ingest_property("PersonType", "test/PersonType", &path_refers)
        .await
        .unwrap();

    // Codelist array: ItemsOf-lowered too, but never a child.
    let mut codelist_prop = test_property();
    codelist_prop.name = "priorities".into();
    codelist_prop.is_array = true;
    codelist_prop.ref_target = Some("PriorityCode".into());
    engine
        .ingest_property("PersonType", "test/PersonType", &codelist_prop)
        .await
        .unwrap();

    let children = engine.get_child_schemas("PersonType").await.unwrap();
    let titles: Vec<_> = children.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, vec!["ItemType"]);

    // Reverse direction: the referenced entity has no children of its own.
    let reverse = engine.get_child_schemas("ItemType").await.unwrap();
    assert!(reverse.is_empty());
}

#[tokio::test]
async fn get_child_schemas_dedupes_inline_and_derived() {
    let engine = MockEngine::new();
    engine.ingest_schema(&test_schema()).await.unwrap(); // PersonType

    let mut child = test_schema();
    child.schema_id = "common/json/ItemType.json".into();
    child.title = "ItemType".into();
    child.parent_schema = Some("PersonType".into()); // inline route
    engine.ingest_schema(&child).await.unwrap();

    let mut refers = test_property();
    refers.name = "items".into();
    refers.is_array = true;
    refers.ref_target = Some("ItemType".into()); // derived route, same child
    engine
        .ingest_property("PersonType", "test/PersonType", &refers)
        .await
        .unwrap();

    let children = engine.get_child_schemas("PersonType").await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].title, "ItemType");
}

#[tokio::test]
async fn ingest_condition_round_trips_through_the_mock() {
    let engine = MockEngine::new();
    engine.ingest_schema(&test_schema()).await.unwrap();

    let condition = ConditionNode {
        name: "PersonType_one_of".into(),
        owner_title: "PersonType".into(),
        kind: ConditionKind::OneOf,
        expr_json: None,
        options: vec!["A".into(), "B".into()],
        definition: None,
        domain: Some("common".into()),
    };
    engine.ingest_condition(&condition).await.unwrap();

    let for_schema = engine
        .get_conditions_for_schema("PersonType")
        .await
        .unwrap();
    assert_eq!(for_schema, vec![condition.clone()]);
    let all = engine.list_conditions().await.unwrap();
    assert_eq!(all, vec![condition]);
    let missing = engine.get_conditions_for_schema("OtherType").await.unwrap();
    assert!(missing.is_empty());
}
