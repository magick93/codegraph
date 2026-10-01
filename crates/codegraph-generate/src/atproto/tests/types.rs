use std::path::PathBuf;

use codegraph_core::mock::MockEngine;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{AtprotoNamespaceNode, LexiconNode, PropertyNode};
use codegraph_type_contracts::RefClassificationKind;
use tera::Tera;

use super::super::types_gen::AtprotoTypesEmitter;
use super::{
    make_domain_config as make_types_domain_config, make_primitive_prop as types_primitive_prop,
    make_project as make_types_project, make_schema as types_schema,
};
use crate::traits::EntityGenerator;

fn make_types_tera() -> Tera {
    let mut tera = Tera::default();
    tera.add_raw_template(
        "atproto/rust_type.tera",
        include_str!("../../../templates/atproto/rust_type.tera"),
    )
    .unwrap();
    tera.add_raw_template(
        "atproto/rust_record_impl.tera",
        include_str!("../../../templates/atproto/rust_record_impl.tera"),
    )
    .unwrap();
    tera.add_raw_template(
        "atproto/rust_enum.tera",
        include_str!("../../../templates/atproto/rust_enum.tera"),
    )
    .unwrap();
    tera
}

fn types_prop_with_kind(
    name: &str,
    prop_type: &str,
    is_required: bool,
    kind: RefClassificationKind,
) -> PropertyNode {
    let mut p = types_primitive_prop(name, prop_type, is_required);
    p.classification_kind = Some(kind);
    p
}

fn types_prop_with_format(
    name: &str,
    prop_type: &str,
    is_required: bool,
    format: &str,
) -> PropertyNode {
    let mut p = types_primitive_prop(name, prop_type, is_required);
    p.format = Some(format.to_string());
    p
}

// ── Test 1: Basic record struct generation ────────────────────────

#[tokio::test]
async fn generates_pub_struct_for_record() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Grant", "grants"))
        .with_properties(
            "Grant",
            vec![
                types_primitive_prop("name", "string", true),
                types_primitive_prop("amount", "integer", false),
            ],
        )
        .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A grant record".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert_eq!(result.len(), 1, "should produce one file");
    let content = &result[0].content;

    eprintln!("Generated types:\n{}", content);

    assert!(
        content.contains("pub struct GrantRecord"),
        "should contain pub struct GrantRecord"
    );
    assert!(
        content.contains("#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]"),
        "should have serde derives"
    );
    assert!(
        content.contains("#[serde(rename_all = \"camelCase\")]"),
        "should have camelCase rename"
    );
}

// ── Test 2: NSID constant and $type field ──────────────────────────

// `#[rustfmt::skip]` preserves the pre-split body verbatim (rustfmt joins
// the wrapped `.contains(...)` at the shallower indent).
#[rustfmt::skip]
#[tokio::test]
async fn record_has_nsid_and_type_field() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Grant", "grants"))
        .with_properties("Grant", vec![types_primitive_prop("name", "string", true)])
        .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A grant record".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    let content = &result[0].content;

    assert!(
        content.contains("pub const NSID: &str"),
        "should have NSID constant"
    );
    assert!(
        content.contains("nz.gravy.grants.grant"),
        "NSID should have the correct value"
    );
    assert!(
        content.contains("fn type_default()"),
        "should have type_default function"
    );
    assert!(
        content
            .contains("#[serde(rename = \"$type\", default = \"GrantRecord::type_default\")]"),
        "should have $type field with serde rename"
    );
    assert!(
        content.contains("pub r#type: String"),
        "should have r#type field"
    );
}

// ── Test 3: Option wrapping for non-required fields ────────────────

#[tokio::test]
async fn non_required_fields_are_options() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Grant", "grants"))
        .with_properties(
            "Grant",
            vec![
                types_primitive_prop("name", "string", true),
                types_primitive_prop("description", "string", false),
            ],
        )
        .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A grant record".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    let content = &result[0].content;

    eprintln!("Generated with optional field:\n{}", content);

    assert!(
        content.contains("pub description: Option<String>"),
        "non-required string field should be Option<String>"
    );
    assert!(
        content.contains("pub name: String"),
        "required string field should be String (not Option)"
    );
}

// ── Test 4: DateTime field maps to chrono::DateTime<chrono::Utc> ──

#[tokio::test]
async fn datetime_field_maps_to_chrono() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Grant", "grants"))
        .with_properties(
            "Grant",
            vec![
                types_primitive_prop("name", "string", true),
                types_prop_with_format("createdAt", "string", true, "date-time"),
            ],
        )
        .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A grant record".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    let content = &result[0].content;

    eprintln!("Generated with datetime:\n{}", content);
    assert!(
        content.contains("chrono::DateTime<chrono::Utc>"),
        "date-time format should map to chrono::DateTime<chrono::Utc>"
    );
}

// ── Test 5: Bytes field maps to Vec<u8> with serde_bytes ──────────

#[tokio::test]
async fn bytes_field_maps_to_vec_u8_with_serde_bytes() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Media", "media"))
        .with_properties(
            "Media",
            vec![types_prop_with_format("hash", "string", false, "byte")],
        )
        .with_lexicon_mapping("Media", "nz.gravy.media.hash")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "media".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.media.hash".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A media record".to_string()),
        domain: "media".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Media",
            "media",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    let content = &result[0].content;

    eprintln!("Generated with bytes:\n{}", content);
    assert!(content.contains("Vec<u8>"), "bytes should map to Vec<u8>");
    assert!(
        content.contains("#[serde(with = \"serde_bytes\")]"),
        "bytes field should have serde_bytes with attribute"
    );
}

// ── Test 6: Boolean and integer field mappings ─────────────────────

#[tokio::test]
async fn boolean_and_integer_field_mappings() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Flags", "test"))
        .with_properties(
            "Flags",
            vec![
                types_primitive_prop("active", "boolean", true),
                types_primitive_prop("count", "integer", false),
            ],
        )
        .with_lexicon_mapping("Flags", "nz.gravy.test.flag")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "test".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.test.flag".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("Flags record".to_string()),
        domain: "test".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Flags",
            "test",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    let content = &result[0].content;

    eprintln!("Generated with bool and int:\n{}", content);
    assert!(
        content.contains("pub active: bool"),
        "boolean should be bool"
    );
    assert!(
        content.contains("pub count: Option<i64>"),
        "integer should be i64, optional when not required"
    );
}

// ── Test 7: Media/Blob field generates BlobRef ─────────────────────

#[tokio::test]
async fn media_field_generates_blob_ref() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("File", "files"))
        .with_properties(
            "File",
            vec![types_prop_with_kind(
                "avatar",
                "string",
                true,
                RefClassificationKind::MediaWrapper,
            )],
        )
        .with_lexicon_mapping("File", "nz.gravy.files.avatar")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "files".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.files.avatar".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A file record".to_string()),
        domain: "files".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "File",
            "files",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    let content = &result[0].content;

    eprintln!("Generated with blob:\n{}", content);
    assert!(
        content.contains("pub struct BlobRef"),
        "should contain BlobRef struct when a blob field is present"
    );
    assert!(
        content.contains("pub avatar: BlobRef"),
        "media field should use BlobRef type"
    );
}

// ── Test 8: Entity reference maps to String ────────────────────────

#[tokio::test]
async fn entity_reference_maps_to_string() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Grant", "grants"))
        .with_properties(
            "Grant",
            vec![
                types_primitive_prop("name", "string", true),
                types_prop_with_kind(
                    "grantee",
                    "string",
                    false,
                    RefClassificationKind::EntityReference,
                ),
            ],
        )
        .with_lexicon_mapping("Grant", "nz.gravy.grants.grant")
        .build();

    let ns = AtprotoNamespaceNode {
        authority: "nz.gravy".to_string(),
        segment: "".to_string(),
        domain: "grants".to_string(),
    };
    engine.ingest_atproto_namespace(&ns).await.unwrap();

    let lex = LexiconNode {
        nsid: "nz.gravy.grants.grant".to_string(),
        lex_type: "record".to_string(),
        key_strategy: "tid".to_string(),
        revision: Some(1),
        description: Some("A grant record".to_string()),
        domain: "grants".to_string(),
    };
    engine.ingest_lexicon(&lex).await.unwrap();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    let content = &result[0].content;

    eprintln!("Generated with entity ref:\n{}", content);
    assert!(
        content.contains("pub grantee: Option<String>"),
        "entity reference field should map to Option<String>"
    );
}

// ── Test 9: Skips non-atproto schemas ──────────────────────────────

#[tokio::test]
async fn skips_non_atproto_schema() {
    let engine = MockEngine::builder()
        .with_schema(types_schema("Grant", "grants"))
        .with_properties("Grant", vec![types_primitive_prop("name", "string", true)])
        .build();

    let tera = make_types_tera();
    let project = make_types_project();
    let emitter = AtprotoTypesEmitter::new(&PathBuf::from("/tmp/test-out"));

    let result = emitter
        .generate(
            &engine,
            "Grant",
            "grants",
            &make_types_domain_config(),
            &tera,
            &project,
        )
        .await
        .expect("generation should succeed");

    assert!(
        result.is_empty(),
        "should return empty when no lexicon mapping exists"
    );
}
