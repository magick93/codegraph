use codegraph_core::types::{
    CodeList, ColumnInfo, CompositeColumn, CompositeRange, CompositionNode, CompositionTree,
    EdgeProperties, EdgeType, EnumValue, Extension, FkDirection, IngestStats, ParentCandidate,
    PropertyNode, SchemaNode,
};
use std::time::Duration;

#[test]
fn schema_node_serde_round_trip() {
    let node = SchemaNode {
        schema_id: "recruiting/json/CandidateType.json".into(),
        title: "CandidateType".into(),
        description: Some("A candidate for a position".into()),
        schema_type: "object".into(),
        classification: "entity_reference".into(),
        domain: Some("recruiting".into()),
        rel_path: "recruiting/json/CandidateType.json".into(),
        pg_type: "TABLE".into(),
        rust_type: "CandidateType".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "CandidateType".into(),
        pg_table_name: "candidate".into(),
        api_path_segment: "candidate".into(),
        parent_schema: None,
        is_entity: true,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: true,
    };
    let json = serde_json::to_string(&node).unwrap();
    let deserialized: SchemaNode = serde_json::from_str(&json).unwrap();
    assert_eq!(node, deserialized);
}

#[test]
fn property_node_serde_round_trip() {
    let prop = PropertyNode {
        name: "givenName".into(),
        prop_type: "string".into(),
        description: Some("Given name".into()),
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
    let json = serde_json::to_string(&prop).unwrap();
    let deserialized: PropertyNode = serde_json::from_str(&json).unwrap();
    assert_eq!(prop, deserialized);
}

#[test]
fn schema_node_with_parent_schema() {
    let node = SchemaNode {
        schema_id: "common/json/PersonType.json#/definitions/PersonName".into(),
        title: "PersonName".into(),
        description: None,
        schema_type: "object".into(),
        classification: "value_object".into(),
        domain: Some("common".into()),
        rel_path: "common/json/PersonType.json".into(),
        pg_type: "TABLE".into(),
        rust_type: "PersonName".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "PersonName".into(),
        pg_table_name: "person_name".into(),
        api_path_segment: "person-name".into(),
        parent_schema: Some("PersonType".into()),
        is_entity: false,
        is_codelist: false,
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
    };
    assert_eq!(node.parent_schema, Some("PersonType".into()));
}

#[test]
fn codelist_serde_round_trip() {
    let cl = CodeList {
        name: "GenderCodeList".into(),
        description: Some("Gender codes".into()),
        pg_table_name: "gender_code".into(),
        render_as: "enum".into(),
        check_expression: None,
    };
    let json = serde_json::to_string(&cl).unwrap();
    let de: CodeList = serde_json::from_str(&json).unwrap();
    assert_eq!(cl, de);
}

#[test]
fn enum_value_serde_round_trip() {
    let ev = EnumValue {
        value: "Male".into(),
        display_name: Some("Male".into()),
        sort_order: 1,
    };
    let json = serde_json::to_string(&ev).unwrap();
    let de: EnumValue = serde_json::from_str(&json).unwrap();
    assert_eq!(ev, de);
}

#[test]
fn composition_tree_basic() {
    let tree = CompositionTree {
        root: CompositionNode {
            field_name: "candidate".into(),
            schema_title: "CandidateType".into(),
            table_schema: "recruiting".into(),
            table_name: "candidate".into(),
            fk: None,
            is_collection: false,
            columns: vec![ColumnInfo {
                name: "given_name".into(),
                description: None,
                rust_type: "String".into(),
                postgres_type: "TEXT".into(),
                is_optional: false,
                is_codelist_fk: false,
                composite_columns: vec![],
                is_array: false,
                classification: None,
                fk_target: None,
                check_values: vec![],
            }],
            jsonb_columns: vec![],
            children: vec![],
            composite_range: None,
            consumed_fields: vec![],
        },
    };
    assert_eq!(tree.node_count(), 1);
    assert_eq!(tree.all_schema_titles(), vec!["CandidateType"]);
    assert!(tree.root.is_root());
}

#[test]
fn edge_type_all_variants_exist() {
    let _variants = vec![
        EdgeType::HasProperty,
        EdgeType::ReferencesSchema,
        EdgeType::ItemsOf,
        EdgeType::ExtendsSchema,
        EdgeType::DependsOn,
        EdgeType::HasEnumValue,
        EdgeType::UsesCodeList,
        EdgeType::ExpandsTo,
        EdgeType::CollapsesTo,
        EdgeType::ConsumesField,
        EdgeType::RequiresExtension,
        EdgeType::InDomain,
        EdgeType::DomainDepends,
    ];
    assert_eq!(_variants.len(), 13);
}

#[test]
fn ingest_stats_defaults() {
    let stats = IngestStats::default();
    assert_eq!(stats.schema_count, 0);
    assert_eq!(stats.duration, Duration::default());
}

#[test]
fn composite_column_serde_round_trip() {
    let col = CompositeColumn {
        suffix: "amount".into(),
        pg_type: "NUMERIC".into(),
        rust_type: "f64".into(),
        sea_orm_type: "Double".into(),
        fk_target: None,
        dto_rust_type: None,
        wrapper_schema: "AmountType".into(),
    };
    let json = serde_json::to_string(&col).unwrap();
    let de: CompositeColumn = serde_json::from_str(&json).unwrap();
    assert_eq!(col, de);
}

#[test]
fn composite_range_serde_round_trip() {
    let range = CompositeRange {
        pg_column_name: "date_range".into(),
        pg_type: "DATERANGE".into(),
        rust_type: "DateRange".into(),
        start_field: "start_date".into(),
        end_field: "end_date".into(),
        open_end: false,
    };
    let json = serde_json::to_string(&range).unwrap();
    let de: CompositeRange = serde_json::from_str(&json).unwrap();
    assert_eq!(range, de);
}

#[test]
fn fk_direction_on_parent() {
    let node = CompositionNode {
        field_name: "address".into(),
        schema_title: "AddressType".into(),
        table_schema: "common".into(),
        table_name: "address".into(),
        fk: Some(FkDirection::OnParent {
            column: "person_id".into(),
        }),
        is_collection: false,
        columns: vec![],
        jsonb_columns: vec![],
        children: vec![],
        composite_range: None,
        consumed_fields: vec![],
    };
    assert_eq!(node.parent_fk_column(), Some("person_id"));
    assert_eq!(node.child_fk_column(), None);
    assert!(!node.is_root());
    assert_eq!(node.qualified_table_name(), "common.address");
}

#[test]
fn parent_candidate_serde_round_trip() {
    let pc = ParentCandidate {
        child_title: "AddressType".into(),
        parent_title: "PersonType".into(),
        field_name: "address".into(),
        source: codegraph_core::types::DetectionSource::ScalarRef,
    };
    let json = serde_json::to_string(&pc).unwrap();
    let de: ParentCandidate = serde_json::from_str(&json).unwrap();
    assert_eq!(pc, de);
}

#[test]
fn extension_serde_round_trip() {
    let ext = Extension {
        name: "my-extension".into(),
    };
    let json = serde_json::to_string(&ext).unwrap();
    let de: Extension = serde_json::from_str(&json).unwrap();
    assert_eq!(ext, de);
}

#[test]
fn edge_properties_default() {
    let ep = EdgeProperties::default();
    assert!(ep.sort_order.is_none());
    assert!(ep.ref_path.is_none());
}

#[test]
fn composition_node_dedup_fields() {
    let mut node = CompositionNode {
        field_name: "root".into(),
        schema_title: "RootType".into(),
        table_schema: "test".into(),
        table_name: "root".into(),
        fk: None,
        is_collection: false,
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                description: None,
                rust_type: "Uuid".into(),
                postgres_type: "UUID".into(),
                is_optional: false,
                is_codelist_fk: false,
                composite_columns: vec![],
                is_array: false,
                classification: None,
                fk_target: None,
                check_values: vec![],
            },
            ColumnInfo {
                name: "id".into(),
                description: None,
                rust_type: "Uuid".into(),
                postgres_type: "UUID".into(),
                is_optional: false,
                is_codelist_fk: false,
                composite_columns: vec![],
                is_array: false,
                classification: None,
                fk_target: None,
                check_values: vec![],
            },
        ],
        jsonb_columns: vec![],
        children: vec![],
        composite_range: None,
        consumed_fields: vec![],
    };
    node.dedup_fields();
    assert_eq!(node.columns.len(), 1);
}

#[test]
fn composition_tree_leaf_nodes() {
    let tree = CompositionTree {
        root: CompositionNode {
            field_name: "root".into(),
            schema_title: "RootType".into(),
            table_schema: "test".into(),
            table_name: "root".into(),
            fk: None,
            is_collection: false,
            columns: vec![],
            jsonb_columns: vec![],
            children: vec![
                CompositionNode {
                    field_name: "child1".into(),
                    schema_title: "ChildType1".into(),
                    table_schema: "test".into(),
                    table_name: "child1".into(),
                    fk: Some(FkDirection::OnChild {
                        column: "root_id".into(),
                    }),
                    is_collection: false,
                    columns: vec![],
                    jsonb_columns: vec![],
                    children: vec![],
                    composite_range: None,
                    consumed_fields: vec![],
                },
                CompositionNode {
                    field_name: "child2".into(),
                    schema_title: "ChildType2".into(),
                    table_schema: "test".into(),
                    table_name: "child2".into(),
                    fk: Some(FkDirection::OnChild {
                        column: "root_id".into(),
                    }),
                    is_collection: false,
                    columns: vec![],
                    jsonb_columns: vec![],
                    children: vec![],
                    composite_range: None,
                    consumed_fields: vec![],
                },
            ],
            composite_range: None,
            consumed_fields: vec![],
        },
    };
    assert_eq!(tree.node_count(), 3);
    let leaves = tree.leaf_nodes();
    assert_eq!(leaves.len(), 2);
    let all_titles = tree.all_schema_titles();
    assert_eq!(all_titles.len(), 3);
    assert!(all_titles.contains(&"RootType".to_string()));
}

use codegraph_core::traits::GraphIngestor;

// Verify trait is object-safe (can be used as dyn)
fn _assert_object_safe(_: &dyn GraphIngestor) {}

use codegraph_core::traits::GraphQuerier;

fn _assert_querier_object_safe(_: &dyn GraphQuerier) {}
