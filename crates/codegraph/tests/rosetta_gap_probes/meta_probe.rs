//! WP1.4 — meta/annotation probe (see docs/rosetta/findings/wp1.4-meta-annotations.md).
//!
//! Question: where do Rune DSL meta fields — `[metadata id/key/scheme]`,
//! `[ruleReference]`, `[docReference]`, rationale/labels — land in the
//! JSON-schema pipeline? The JSON-schema analogs are top-level vs
//! property-level `x-*` annotations.
//!
//! Hypothesis under test: top-level `x-*` keys persist into
//! `SchemaNode::custom_annotations` (prefix stripped at ingest);
//! property-level `x-*` keys are dropped entirely (`PropertyNode` has no
//! annotations field, property.rs:79-127).

use crate::support;

/// Distinctive payloads so absence proofs in generator output can't hit
/// empty-string or incidental-token false positives.
const META_ID_TOKEN: &str = "ROSETTA-META-ID-TOKEN-7734";
const META_SCHEME_TOKEN: &str = "ROSETTA-SCHEME-TOKEN-42";
const RULE_REF_TOKEN: &str = "ROSETTA-RULE-REF-TOKEN-9931";

/// One entity carrying BOTH surfaces: a top-level `x-rosetta-metadata`
/// annotation (the `[metadata id/key/scheme]` analog) and a property-level
/// `x-rosetta-rule-ref` annotation (the `[ruleReference]` analog).
const PERMIT_SCHEMA: &str = r#"{
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "PermitType",
    "type": "object",
    "x-rosetta-metadata": {
        "id": "ROSETTA-META-ID-TOKEN-7734",
        "key": "permit",
        "scheme": "ROSETTA-SCHEME-TOKEN-42"
    },
    "properties": {
        "permit_name": {
            "type": "string",
            "x-rosetta-rule-ref": "ROSETTA-RULE-REF-TOKEN-9931"
        },
        "authority": { "type": "string" }
    },
    "required": ["permit_name"]
}"#;

fn fixture_files() -> Vec<(&'static str, &'static str)> {
    vec![("permit_type.json", PERMIT_SCHEMA)]
}

/// Probe 1 — top-level `x-*` annotations persist into
/// `SchemaNode::custom_annotations`. Also characterizes the transformation:
/// the `x-` prefix is STRIPPED at ingest (async_ingest.rs:337-344), so
/// `x-rosetta-metadata` is stored under key `rosetta-metadata`, with the
/// nested JSON value preserved verbatim.
#[tokio::test]
async fn top_level_x_annotations_persist_to_custom_annotations() {
    let dir = tempfile::tempdir().unwrap();
    let be =
        support::ingest_into_graph(dir.path(), "rosetta", &["PermitType"], &fixture_files()).await;

    let schema = be
        .querier()
        .get_schema("PermitType")
        .await
        .unwrap()
        .expect("PermitType SchemaNode ingested");

    let keys: Vec<&String> = schema.custom_annotations.keys().collect();
    assert!(
        !schema.custom_annotations.contains_key("x-rosetta-metadata"),
        "expected the x- prefix to be stripped at ingest; actual keys: {keys:?}"
    );

    let meta = schema
        .custom_annotations
        .get("rosetta-metadata")
        .unwrap_or_else(|| {
            panic!("x-rosetta-metadata should persist as `rosetta-metadata`; keys: {keys:?}")
        });

    // The [metadata id/key/scheme] analog survives as intact JSON.
    assert_eq!(meta.get("id").and_then(|v| v.as_str()), Some(META_ID_TOKEN));
    assert_eq!(meta.get("key").and_then(|v| v.as_str()), Some("permit"));
    assert_eq!(
        meta.get("scheme").and_then(|v| v.as_str()),
        Some(META_SCHEME_TOKEN)
    );
}

/// Probe 2 — property-level `x-*` annotations are dropped entirely.
/// `PropertyNode` (codegraph-core/src/types/property.rs:79-127) has no
/// annotations/custom_annotations field, so the honest assertion is via
/// real accessors only: the property ingests fine (the drop is silent),
/// and no serialized view of the node carries the annotation.
#[tokio::test]
async fn property_level_x_annotations_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let be =
        support::ingest_into_graph(dir.path(), "rosetta", &["PermitType"], &fixture_files()).await;

    let props = be.querier().get_properties("PermitType").await.unwrap();
    let name_prop = props
        .iter()
        .find(|p| p.name == "permit_name")
        .expect("permit_name property ingested normally despite its x- annotation");

    // The property itself is intact — the annotation was dropped silently,
    // without failing or degrading ingestion.
    assert_eq!(name_prop.rust_field_type, "String");
    assert_eq!(name_prop.pg_column_name, "permit_name");

    // Serde view of the node (the exhaustive field accessor): no
    // annotation-shaped key and no trace of the annotation payload.
    let value = serde_json::to_value(name_prop).unwrap();
    let map = value
        .as_object()
        .expect("PropertyNode serializes to an object");
    let offender = map
        .keys()
        .find(|k| k.contains("annotation") || k.contains("rosetta") || k.starts_with("x-"));
    assert!(
        offender.is_none(),
        "PropertyNode unexpectedly exposes an annotation-shaped field {offender:?}; keys: {:?}",
        map.keys().collect::<Vec<_>>()
    );
    let serialized = serde_json::to_string(name_prop).unwrap();
    assert!(
        !serialized.contains(RULE_REF_TOKEN),
        "rule-ref payload leaked into PropertyNode serde view: {serialized}"
    );
}

/// Probe 3 — neither the property-level annotation NOR the top-level
/// metadata surfaces anywhere in the standard generated tree. Graph-side
/// persistence of top-level `x-*` (probe 1) does not reach DDL / COMMENT /
/// entity / DTO / API / CLI output: no generator or template outside the
/// atproto family reads `custom_annotations`.
#[tokio::test]
async fn property_annotations_absent_from_generator_output() {
    let dir = tempfile::tempdir().unwrap();
    let files =
        support::run_pipeline(dir.path(), "rosetta", &["PermitType"], &fixture_files()).await;
    assert!(
        files.len() > 10,
        "expected a real generated tree, got {} files",
        files.len()
    );

    // The metadata id/scheme and rule-ref payloads appear nowhere.
    let payload_hits: Vec<&String> = files
        .iter()
        .filter(|(_, content)| {
            content.contains(META_ID_TOKEN)
                || content.contains(META_SCHEME_TOKEN)
                || content.contains(RULE_REF_TOKEN)
        })
        .map(|(path, _)| path)
        .collect();
    assert!(
        payload_hits.is_empty(),
        "annotation payloads surfaced in generated output: {payload_hits:?}"
    );

    // Nor does even the annotation *concept* render (key names are absent).
    // Note: the bare word "rosetta" legitimately appears (postgres schema
    // name), so only annotation-specific spellings are asserted.
    for (path, content) in &files {
        assert!(
            !content.contains("rosetta-metadata"),
            "annotation key name surfaced in {path}"
        );
        assert!(
            !content.contains("x-rosetta"),
            "raw x- key surfaced in {path}"
        );
    }
}
