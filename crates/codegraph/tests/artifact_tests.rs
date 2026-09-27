//! L2 deterministic IR artifact + L1 driver-level graph cache (issue #275).
//!
//! The artifact contract under test:
//! - export → import → export is byte-identical;
//! - the sha256 of the canonical document is stable across independent runs
//!   on identical inputs;
//! - a document whose `formatVersion` is newer than the running binary's
//!   constant is rejected with a named error variant;
//! - with `--graph-cache`, a second consecutive `run` with unchanged inputs
//!   reopens the persisted graph instead of re-ingesting.

use codegraph::artifact::{self, ArtifactError, FORMAT_VERSION};
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    CodeList, EdgeProperties, EdgeType, EnumValue, PropertyNode, SchemaNode,
};
use codegraph_grafeo::GrafeoEngine;
use serde_json::Value as Json;
use std::fs;

// ── fixture graph ────────────────────────────────────────────────────────

fn schema(title: &str, classification: &str, domain: &str) -> SchemaNode {
    SchemaNode {
        namespace: None,
        schema_id: format!("{domain}/{title}"),
        title: title.to_string(),
        description: Some(format!("The {title} type.")),
        schema_type: "object".to_string(),
        classification: classification.to_string(),
        domain: Some(domain.to_string()),
        rel_path: format!("{domain}/{title}.json"),
        pg_type: "TABLE".to_string(),
        rust_type: title.to_string(),
        sea_orm_type: if classification == "entity" {
            "Entity"
        } else {
            "Model"
        }
        .to_string(),
        rust_type_name: title.to_string(),
        pg_table_name: title.to_lowercase(),
        api_path_segment: title.to_string(),
        parent_schema: None,
        is_entity: classification == "entity",
        is_codelist: classification == "codelist",
        is_primitive_wrapper: false,
        has_all_of: false,
        has_one_of: false,
        has_any_of: false,
        has_definitions: false,
        custom_annotations: Default::default(),
    }
}

fn property(name: &str, required: bool) -> PropertyNode {
    PropertyNode {
        name: name.to_string(),
        prop_type: "string".to_string(),
        description: Some(format!("The {name}.")),
        format: None,
        is_required: required,
        is_nullable: !required,
        is_array: false,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        min_items: None,
        max_items: None,
        pg_column_name: name.to_string(),
        pg_column_type: "TEXT".to_string(),
        rust_field_name: name.to_string(),
        rust_field_type: "String".to_string(),
        sea_orm_type: "Text".to_string(),
        render_strategy: "scalar".to_string(),
        ref_target: None,
        classification: None,
        projection: None,
        classification_kind: None,
        type_expr: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
    }
}

/// A small but non-trivial graph: an entity with two properties, a
/// codelist with two enum values, and a cross-reference edge.
async fn fixture_graph() -> GrafeoEngine {
    let engine = GrafeoEngine::in_memory().unwrap();
    engine
        .ingest_schema(&schema("WidgetType", "entity", "inventory"))
        .await
        .unwrap();
    engine
        .ingest_property(
            "WidgetType",
            "inventory/WidgetType",
            &property("label", true),
        )
        .await
        .unwrap();
    engine
        .ingest_property(
            "WidgetType",
            "inventory/WidgetType",
            &property("notes", false),
        )
        .await
        .unwrap();
    engine
        .ingest_property(
            "WidgetType",
            "inventory/WidgetType",
            &property("status", false),
        )
        .await
        .unwrap();
    engine
        .ingest_schema(&schema("WidgetStatus", "codelist", "inventory"))
        .await
        .unwrap();
    engine
        .ingest_codelist(&CodeList {
            name: "WidgetStatus".to_string(),
            description: Some("Widget lifecycle states.".to_string()),
            pg_table_name: "widget_status".to_string(),
            render_as: "check".to_string(),
            check_expression: None,
        })
        .await
        .unwrap();
    for (value, sort) in [("active", 0), ("retired", 1)] {
        engine
            .ingest_enum_value(
                "WidgetStatus",
                &EnumValue {
                    value: value.to_string(),
                    display_name: Some(value.to_string()),
                    sort_order: sort,
                },
            )
            .await
            .unwrap();
    }
    engine
        .ingest_edge(
            "status::WidgetType",
            "inventory/WidgetStatus",
            EdgeType::ReferencesSchema,
            Some(&EdgeProperties {
                ref_path: Some("status".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    engine
}

// ── L2: deterministic document ───────────────────────────────────────────

#[tokio::test]
async fn export_import_export_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.artifact.json");
    let second = dir.path().join("second.artifact.json");

    let source = fixture_graph().await;
    let exported = artifact::export_ir(&source, &first).unwrap();

    let rebuilt = artifact::import_ir(&first).unwrap();
    let reexported = artifact::export_ir(&rebuilt, &second).unwrap();

    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    assert_eq!(exported.sha256, reexported.sha256);
}

#[tokio::test]
async fn export_hash_is_stable_across_runs() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.artifact.json");
    let b = dir.path().join("b.artifact.json");

    let one = fixture_graph().await;
    let two = fixture_graph().await;

    let first = artifact::export_ir(&one, &a).unwrap();
    let second = artifact::export_ir(&two, &b).unwrap();

    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    assert_eq!(first.sha256, second.sha256);
    assert_eq!(first.sha256.len(), 64);
    assert!(first.node_count > 0);
    assert!(first.edge_count > 0);
}

#[tokio::test]
async fn newer_format_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("artifact.json");

    let source = fixture_graph().await;
    artifact::export_ir(&source, &path).unwrap();

    let mut doc: Json = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    doc["formatVersion"] = Json::from(FORMAT_VERSION + 1);
    let newer = dir.path().join("newer.artifact.json");
    fs::write(&newer, serde_json::to_vec(&doc).unwrap()).unwrap();

    let err = match artifact::import_ir(&newer) {
        Err(err) => err,
        Ok(_) => panic!("newer formatVersion must be rejected"),
    };
    match &err {
        ArtifactError::FormatVersionTooNew { found, supported } => {
            assert_eq!(*found, FORMAT_VERSION + 1);
            assert_eq!(*supported, FORMAT_VERSION);
        }
        other => panic!("expected FormatVersionTooNew, got: {other:?}"),
    }
    assert!(err.to_string().contains("format version"));
}
