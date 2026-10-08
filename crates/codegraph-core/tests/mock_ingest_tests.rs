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
        is_id: false,
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

#[tokio::test]
async fn ingest_ddd_model_round_trips_through_the_mock() {
    use codegraph_core::types::{
        DddApplicationNode, DddDesignFlags, DddDesignNode, DddDocumentField, DddModelGraph,
        DddModuleNode, DddPagination, DddParam, DddRepositoryNode, DddRepositoryOperation,
        DddSearchField, DddSearchNode, DddServiceNode, DddServiceOperation,
    };

    let engine = MockEngine::new();
    assert!(engine.get_ddd_models().await.unwrap().is_empty());

    let fixture = DddModelGraph {
        source_path: "model/library.ddd".to_string(),
        application: DddApplicationNode {
            name: "Library".to_string(),
            base: Some("nz.example.library".to_string()),
            source_path: "model/library.ddd".to_string(),
        },
        modules: vec![DddModuleNode {
            application: "Library".to_string(),
            name: "catalogue".to_string(),
            ordinal: 0,
        }],
        designs: vec![DddDesignNode {
            application: "Library".to_string(),
            module: "catalogue".to_string(),
            class: "Book".to_string(),
            resolved_title: Some("Book".to_string()),
            stereotype: "entity".to_string(),
            is_abstract: false,
            flags: DddDesignFlags {
                scaffold: true,
                cache: true,
                ..Default::default()
            },
            ordinal: 0,
        }],
        repositories: vec![DddRepositoryNode {
            application: "Library".to_string(),
            name: "BookRepository".to_string(),
            design_class: "Book".to_string(),
            operations: vec![
                DddRepositoryOperation {
                    name: "findById".to_string(),
                    builtin: Some("findById".to_string()),
                    return_type: None,
                    return_multiplicity: None,
                    params: vec![],
                    ordinal: 0,
                },
                DddRepositoryOperation {
                    name: "findByTitle".to_string(),
                    builtin: None,
                    return_type: Some(serde_json::json!({
                        "type": "primitive",
                        "value": "String"
                    })),
                    return_multiplicity: None,
                    params: vec![DddParam {
                        name: "title".to_string(),
                        type_json: serde_json::json!({
                            "type": "primitive",
                            "value": "String"
                        }),
                        multiplicity: None,
                    }],
                    ordinal: 1,
                },
            ],
        }],
        services: vec![DddServiceNode {
            application: "Library".to_string(),
            module: "catalogue".to_string(),
            name: "LoanService".to_string(),
            description: Some("Manages loans".to_string()),
            dependencies: vec!["LoanRepository".to_string()],
            operations: vec![DddServiceOperation {
                name: "renew".to_string(),
                return_type: None,
                return_multiplicity: None,
                params: vec![],
                delegation_target: Some("LoanRepository".to_string()),
                delegation_operation: Some("save".to_string()),
                capabilities: vec!["RenewBooks".to_string()],
                ordinal: 0,
            }],
            ordinal: 0,
        }],
        searches: vec![DddSearchNode {
            application: "Library".to_string(),
            module: "catalogue".to_string(),
            name: "BookSearch".to_string(),
            description: None,
            entity_class: "Book".to_string(),
            entity_title: Some("Book".to_string()),
            text: vec![DddSearchField {
                property: "title".to_string(),
                boost: Some(2.5),
                analyzer: None,
            }],
            filters: vec!["category".to_string()],
            sorts: vec!["title".to_string()],
            document: vec![DddDocumentField {
                name: "label".to_string(),
                expr: "title".to_string(),
            }],
            ranking: Some("bm25".to_string()),
            analyzer: Some("english".to_string()),
            pagination: Some(DddPagination {
                limit: Some(20),
                max_limit: None,
                cursor: true,
            }),
            capabilities: vec![],
            ordinal: 0,
        }],
    };

    engine.ingest_ddd_model(&fixture).await.unwrap();
    let loaded = engine.get_ddd_models().await.unwrap();
    assert_eq!(loaded, vec![fixture]);
}

#[tokio::test]
async fn ingest_evt_model_round_trips_through_the_mock() {
    use codegraph_core::types::{
        EvtChannelNode, EvtEventField, EvtEventNode, EvtModelGraph, EvtSubscriptionNode,
    };

    let engine = MockEngine::new();
    assert!(engine.get_evt_models().await.unwrap().is_empty());

    let fixture = EvtModelGraph {
        source_path: "model/billing.evt".to_string(),
        events: vec![EvtEventNode {
            source_path: "model/billing.evt".to_string(),
            name: "PaymentRequested".to_string(),
            version: Some("2.0".to_string()),
            fields: vec![
                EvtEventField {
                    name: "payment_id".to_string(),
                    type_json: serde_json::json!({
                        "type": "primitive",
                        "value": "Uuid"
                    }),
                    resolved_title: None,
                },
                EvtEventField {
                    name: "order".to_string(),
                    type_json: serde_json::json!({
                        "type": "class",
                        "value": { "name": "Order" }
                    }),
                    resolved_title: Some("OrderType".to_string()),
                },
            ],
            ordinal: 0,
        }],
        channels: vec![EvtChannelNode {
            source_path: "model/billing.evt".to_string(),
            name: "payments".to_string(),
            publishes: vec![
                "PaymentRequested".to_string(),
                "PaymentCompleted".to_string(),
            ],
            ordinal: 0,
        }],
        subscriptions: vec![EvtSubscriptionNode {
            source_path: "model/billing.evt".to_string(),
            name: "ledger-sync".to_string(),
            events: vec!["PaymentCompleted".to_string()],
            consumer: "ledger-service".to_string(),
            ordinal: 0,
        }],
    };

    engine.ingest_evt_model(&fixture).await.unwrap();

    // A second contract ingested FIRST by name — reads sort by source_path.
    let other = EvtModelGraph {
        source_path: "model/audit.evt".to_string(),
        events: vec![EvtEventNode {
            source_path: "model/audit.evt".to_string(),
            name: "AuditTrailWritten".to_string(),
            version: None,
            fields: vec![],
            ordinal: 0,
        }],
        channels: vec![EvtChannelNode {
            source_path: "model/audit.evt".to_string(),
            name: "audit".to_string(),
            publishes: vec!["AuditTrailWritten".to_string()],
            ordinal: 0,
        }],
        subscriptions: vec![EvtSubscriptionNode {
            source_path: "model/audit.evt".to_string(),
            name: "compliance".to_string(),
            events: vec!["AuditTrailWritten".to_string()],
            consumer: "compliance-exporter".to_string(),
            ordinal: 0,
        }],
    };
    engine.ingest_evt_model(&other).await.unwrap();

    let loaded = engine.get_evt_models().await.unwrap();
    let paths: Vec<&str> = loaded.iter().map(|m| m.source_path.as_str()).collect();
    assert_eq!(paths, vec!["model/audit.evt", "model/billing.evt"]);
    assert_eq!(loaded[0], other);
    assert_eq!(loaded[1], fixture);
}
