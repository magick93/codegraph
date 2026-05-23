use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::*;
use codegraph_grafeo::GrafeoEngine;

fn make_schema(title: &str, domain: &str, is_entity: bool) -> SchemaNode {
    SchemaNode {
        schema_id: format!("{domain}/{title}"),
        title: title.to_string(),
        description: None,
        schema_type: "object".to_string(),
        classification: if is_entity { "entity" } else { "value_object" }.to_string(),
        domain: Some(domain.to_string()),
        rel_path: format!("{domain}/{title}.json"),
        pg_type: "TABLE".to_string(),
        rust_type: title.to_string(),
        sea_orm_type: "Entity".to_string(),
        rust_type_name: title.to_string(),
        pg_table_name: title.to_string(),
        api_path_segment: title.to_string(),
        parent_schema: None,
        is_entity,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    }
}

fn make_property(name: &str, is_required: bool) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: None,
        format: None,
        is_required,
        is_nullable: false,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: name.to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: name.to_string(),
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

async fn seeded_engine() -> GrafeoEngine {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("AddressType", "common", false))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("PayRunType", "payroll", true))
        .await
        .unwrap();
    engine
}

// --- Task 5: Schema queries ---

#[tokio::test]
async fn test_get_schema() {
    let engine = seeded_engine().await;
    let found = engine.get_schema("PersonType").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().title, "PersonType");

    let missing = engine.get_schema("NoSuchType").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_list_schemas_all() {
    let engine = seeded_engine().await;
    let all = engine.list_schemas(None).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn test_list_schemas_by_domain() {
    let engine = seeded_engine().await;
    let common = engine.list_schemas(Some("common")).await.unwrap();
    assert_eq!(common.len(), 2);
    let payroll = engine.list_schemas(Some("payroll")).await.unwrap();
    assert_eq!(payroll.len(), 1);
}

#[tokio::test]
async fn test_get_entity_names() {
    let engine = seeded_engine().await;
    let mut names = engine.get_entity_names().await.unwrap();
    names.sort();
    assert_eq!(names, vec!["PayRunType", "PersonType"]);
}

#[tokio::test]
async fn test_get_entity_schema_map() {
    let engine = seeded_engine().await;
    let map = engine.get_entity_schema_map().await.unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("PersonType").unwrap(), "common/PersonType.json");
}

#[tokio::test]
async fn test_get_value_object_schemas() {
    let engine = seeded_engine().await;
    let vos = engine.get_value_object_schemas().await.unwrap();
    assert_eq!(vos.len(), 1);
    assert_eq!(vos[0].title, "AddressType");
}

#[tokio::test]
async fn test_get_child_schemas() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();

    let mut child = make_schema("PersonNameType", "common", false);
    child.parent_schema = Some("PersonType".to_string());
    engine.ingest_schema(&child).await.unwrap();

    let children = engine.get_child_schemas("PersonType").await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].title, "PersonNameType");
}

// --- Task 6: Property, codelist, composite queries ---

#[tokio::test]
async fn test_get_properties() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_property("PersonType", &make_property("givenName", true))
        .await
        .unwrap();
    engine
        .ingest_property("PersonType", &make_property("familyName", true))
        .await
        .unwrap();

    let props = engine.get_properties("PersonType").await.unwrap();
    assert_eq!(props.len(), 2);
}

#[tokio::test]
async fn test_get_codelist_and_enum_values() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let codelist = CodeList {
        name: "GenderCodeList".to_string(),
        description: None,
        pg_table_name: "gender_code_list".to_string(),
        render_as: "dropdown".to_string(),
        check_expression: None,
    };
    engine.ingest_codelist(&codelist).await.unwrap();

    let v1 = EnumValue {
        value: "Male".to_string(),
        display_name: Some("Male".to_string()),
        sort_order: 1,
    };
    let v2 = EnumValue {
        value: "Female".to_string(),
        display_name: Some("Female".to_string()),
        sort_order: 2,
    };
    engine
        .ingest_enum_value("GenderCodeList", &v1)
        .await
        .unwrap();
    engine
        .ingest_enum_value("GenderCodeList", &v2)
        .await
        .unwrap();

    let codelists = engine.list_codelists().await.unwrap();
    assert_eq!(codelists.len(), 1);

    let found = engine.get_codelist("GenderCodeList").await.unwrap();
    assert!(found.is_some());

    let values = engine.get_enum_values("GenderCodeList").await.unwrap();
    assert_eq!(values.len(), 2);
}

#[tokio::test]
async fn test_get_property_ref_target() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("AddressType", "common", false))
        .await
        .unwrap();

    let mut prop = make_property("address", true);
    prop.ref_target = Some("AddressType".to_string());
    engine.ingest_property("PersonType", &prop).await.unwrap();

    engine
        .ingest_edge(
            "address::PersonType",
            "AddressType",
            EdgeType::ReferencesSchema,
            Some(&EdgeProperties {
                ref_path: Some("#/definitions/AddressType".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let target = engine
        .get_property_ref_target("address", "PersonType")
        .await
        .unwrap();
    assert!(target.is_some());
    assert_eq!(target.unwrap().title, "AddressType");
}

#[tokio::test]
async fn test_get_codelist_for_property() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_property("PersonType", &make_property("gender", true))
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

    engine
        .ingest_edge(
            "gender::PersonType",
            "GenderCodeList",
            EdgeType::UsesCodeList,
            Some(&EdgeProperties {
                render_as: Some("dropdown".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let result = engine
        .get_codelist_for_property("gender", "PersonType")
        .await
        .unwrap();
    assert!(result.is_some());
    let (cl, render_as) = result.unwrap();
    assert_eq!(cl.name, "GenderCodeList");
    assert_eq!(render_as, "dropdown");
}

#[tokio::test]
async fn test_get_array_item_schema() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("PersonNameType", "common", false))
        .await
        .unwrap();

    let mut prop = make_property("names", false);
    prop.is_array = true;
    engine.ingest_property("PersonType", &prop).await.unwrap();

    engine
        .ingest_edge(
            "names::PersonType",
            "PersonNameType",
            EdgeType::ItemsOf,
            None,
        )
        .await
        .unwrap();

    let item = engine
        .get_array_item_schema("names", "PersonType")
        .await
        .unwrap();
    assert!(item.is_some());
    assert_eq!(item.unwrap().title, "PersonNameType");
}

#[tokio::test]
async fn test_get_composite_columns() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_property("PersonType", &make_property("name", true))
        .await
        .unwrap();

    let col = CompositeColumn {
        suffix: "_code".to_string(),
        pg_type: "TEXT".to_string(),
        rust_type: "String".to_string(),
        sea_orm_type: "String".to_string(),
        fk_target: None,
        dto_rust_type: None,
        wrapper_schema: "AmountType".into(),
    };
    engine.ingest_composite_column(&col).await.unwrap();
    engine
        .ingest_edge(
            "name::PersonType",
            "_code::AmountType",
            EdgeType::ExpandsTo,
            Some(&EdgeProperties {
                sort_order: Some(1),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let cols = engine
        .get_composite_columns("name", "PersonType")
        .await
        .unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].suffix, "_code");
}

#[tokio::test]
async fn test_get_composite_range_and_consumed_fields() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("EffectivePeriod", "common", false))
        .await
        .unwrap();
    engine
        .ingest_property("EffectivePeriod", &make_property("startDate", true))
        .await
        .unwrap();
    engine
        .ingest_property("EffectivePeriod", &make_property("endDate", false))
        .await
        .unwrap();

    let range = CompositeRange {
        pg_column_name: "effective_period".to_string(),
        pg_type: "DATERANGE".to_string(),
        rust_type: "DateRange".to_string(),
        start_field: "startDate".to_string(),
        end_field: "endDate".to_string(),
        open_end: true,
    };
    engine.ingest_composite_range(&range).await.unwrap();
    engine
        .ingest_edge(
            "EffectivePeriod",
            "effective_period",
            EdgeType::CollapsesTo,
            None,
        )
        .await
        .unwrap();
    engine
        .ingest_edge(
            "effective_period",
            "startDate::EffectivePeriod",
            EdgeType::ConsumesField,
            Some(&EdgeProperties {
                role: Some("start".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let found_range = engine.get_composite_range("EffectivePeriod").await.unwrap();
    assert!(found_range.is_some());
    assert_eq!(found_range.unwrap().pg_column_name, "effective_period");

    let consumed = engine.get_consumed_fields("EffectivePeriod").await.unwrap();
    assert_eq!(consumed.len(), 1);
    assert_eq!(consumed[0].0.name, "startDate");
    assert_eq!(consumed[0].1, "start");
}

// --- Task 7: Graph traversal and discovery ---

#[tokio::test]
async fn test_get_allof_targets() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("PersonBaseType", "common", false))
        .await
        .unwrap();

    engine
        .ingest_edge(
            "PersonType",
            "PersonBaseType",
            EdgeType::ExtendsSchema,
            Some(&EdgeProperties {
                composition_type: Some("allOf".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let targets = engine.get_allof_targets("PersonType").await.unwrap();
    assert_eq!(targets, vec!["PersonBaseType"]);
}

#[tokio::test]
async fn test_get_referencing_and_referenced_schemas() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("AddressType", "common", false))
        .await
        .unwrap();
    engine
        .ingest_property("PersonType", &make_property("address", true))
        .await
        .unwrap();

    engine
        .ingest_edge(
            "address::PersonType",
            "AddressType",
            EdgeType::ReferencesSchema,
            Some(&EdgeProperties {
                ref_path: Some("AddressType".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let referenced = engine.get_referenced_schemas("PersonType").await.unwrap();
    assert_eq!(referenced, vec!["AddressType"]);

    let referencing = engine.get_referencing_schemas("AddressType").await.unwrap();
    assert_eq!(referencing, vec!["PersonType"]);
}

#[tokio::test]
async fn test_get_generation_order() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("PayRunType", "payroll", true))
        .await
        .unwrap();

    // PayRunType depends on PersonType
    engine
        .ingest_edge(
            "PayRunType",
            "PersonType",
            EdgeType::DependsOn,
            Some(&EdgeProperties {
                dependency_type: Some("ref".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let order = engine.get_generation_order().await.unwrap();
    let person_idx = order.iter().position(|t| t == "PersonType").unwrap();
    let payrun_idx = order.iter().position(|t| t == "PayRunType").unwrap();
    assert!(
        person_idx < payrun_idx,
        "PersonType should come before PayRunType in topo order"
    );
}

#[tokio::test]
async fn test_get_parent_candidates() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_schema(&make_schema("PersonNameType", "common", true))
        .await
        .unwrap();

    let prop = make_property("person", true);
    engine
        .ingest_property("PersonNameType", &prop)
        .await
        .unwrap();

    engine
        .ingest_edge(
            "person::PersonNameType",
            "PersonType",
            EdgeType::ReferencesSchema,
            Some(&EdgeProperties {
                ref_path: Some("PersonType".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let candidates = engine.get_parent_candidates().await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].child_title, "PersonNameType");
    assert_eq!(candidates[0].parent_title, "PersonType");
    assert_eq!(candidates[0].field_name, "person");
}

#[tokio::test]
async fn test_get_required_extensions() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();

    // Insert Extension node directly via GQL
    let session = engine.db().session();
    session
        .execute("INSERT (:Extension {name: 'audit'})")
        .unwrap();

    engine
        .ingest_edge("PersonType", "audit", EdgeType::RequiresExtension, None)
        .await
        .unwrap();

    let extensions = engine.get_required_extensions("PersonType").await.unwrap();
    assert_eq!(extensions.len(), 1);
    assert_eq!(extensions[0].name, "audit");
}

#[tokio::test]
async fn test_get_classification_data() {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&make_schema("PersonType", "common", true))
        .await
        .unwrap();
    engine
        .ingest_property("PersonType", &make_property("givenName", true))
        .await
        .unwrap();
    engine
        .ingest_property("PersonType", &make_property("familyName", false))
        .await
        .unwrap();

    let data = engine.get_classification_data().await.unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].title, "PersonType");
    assert_eq!(data[0].field_count, 2);
    assert_eq!(data[0].required_field_count, 1);
    assert_eq!(data[0].schema_type, "object");
    assert!(!data[0].is_enum);
    assert!(!data[0].is_string_type);
}

// --- Task 8: Composition tree ---

#[tokio::test]
async fn test_get_composition_tree_not_found() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let result = engine.get_composition_tree("NoSuchType").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_composition_tree_simple() {
    let engine = GrafeoEngine::in_memory().unwrap();
    let schema = make_schema("PersonType", "common", true);
    engine.ingest_schema(&schema).await.unwrap();
    engine
        .ingest_property("PersonType", &make_property("givenName", true))
        .await
        .unwrap();

    let tree = engine.get_composition_tree("PersonType").await.unwrap();
    assert_eq!(tree.root.schema_title, "PersonType");
    assert_eq!(tree.root.table_name, "PersonType");
    assert!(tree.root.fk.is_none(), "Root should have no FK");
    assert!(!tree.root.columns.is_empty());
    assert!(tree.root.children.is_empty());
}
