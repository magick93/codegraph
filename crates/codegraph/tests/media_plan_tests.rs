//! Issue #625 PR-A — the media plan builder: `[domains.X.media]` + graph →
//! `MediaPlan` resolution, the provider capability matrix, and the strict
//! validation contract (unresolved classes, refers-wins targets, array
//! media fields, and missing columns are config errors naming the
//! offender).
//!
//! Node-free PR-CI pins: in-memory Grafeo — no node, no DB, no protoc.

use codegraph_config::config::parse_domain_config_str;
use codegraph_config::DomainConfig;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{EdgeType, PropertyNode, SchemaNode};
use codegraph_generate::error::Error;
use codegraph_generate::media::build_media_plans;
use codegraph_grafeo::GrafeoEngine;
use codegraph_type_contracts::RefClassificationKind;

// ── fixture ─────────────────────────────────────────────────────────────

const BASE_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]
type_suffix = "Type"

[domains.core]
label = "Core"
schema_dir = "core"
postgres_schema = "core"
"#;

fn media_toml(provider: &str, mode: &str) -> String {
    format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"{provider}\"\nupload_mode = \"{mode}\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\", \"image/jpeg\"]\n"
    )
}

fn person_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "PersonType".into(),
        title: "PersonType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "entity".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/person.json".into(),
        pg_type: "uuid".into(),
        rust_type: "Uuid".into(),
        sea_orm_type: "Uuid".into(),
        rust_type_name: "Person".into(),
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
        custom_annotations: Default::default(),
        access: None,
        annotations: None,
    }
}

fn media_reference_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "MediaReferenceType".into(),
        title: "MediaReferenceType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "value_object".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/media_reference.json".into(),
        pg_type: "text".into(),
        rust_type: "String".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "MediaReference".into(),
        pg_table_name: "media_reference".into(),
        api_path_segment: "media-reference".into(),
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
    }
}

fn person_name_schema() -> SchemaNode {
    SchemaNode {
        schema_id: "PersonNameType".into(),
        title: "PersonNameType".into(),
        description: None,
        schema_type: "object".into(),
        classification: "value_object".into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: "core/person_name.json".into(),
        pg_type: "text".into(),
        rust_type: "String".into(),
        sea_orm_type: "String".into(),
        rust_type_name: "PersonName".into(),
        pg_table_name: "person_name".into(),
        api_path_segment: "person-name".into(),
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
    }
}

fn prop(name: &str, kind: RefClassificationKind, ref_target: Option<&str>, is_array: bool) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: if ref_target.is_some() { "object".into() } else { "string".into() },
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_id: name == "id",
        is_array,
        min_items: None,
        max_items: None,
        pattern: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pg_column_name: codegraph_naming::to_snake_case(name),
        pg_column_type: "TEXT".into(),
        rust_field_name: codegraph_naming::to_snake_case(name),
        rust_field_type: "String".into(),
        sea_orm_type: "String".into(),
        render_strategy: "flat".into(),
        ref_target: ref_target.map(|t| t.to_string()),
        classification: None,
        classification_kind: Some(kind),
        projection: None,
        ui_override_detail: None,
        ui_override_list_cell: None,
        ui_override_form: None,
        ui_override_inline: None,
        type_expr: None,
    }
}

async fn ingest_model(db: &(impl GraphIngestor + GraphQuerier)) {
    db.ingest_schema(&person_schema()).await.expect("person schema");
    db.ingest_schema(&media_reference_schema())
        .await
        .expect("media reference schema");
    db.ingest_schema(&person_name_schema())
        .await
        .expect("person name schema");
    // Person: id, displayName (plain), avatar (VO → MediaReferenceType),
    // names (VO → PersonNameType, NOT media — the negative control).
    for p in [
        prop("id", RefClassificationKind::EntityReference, None, false),
        prop("displayName", RefClassificationKind::EntityReference, None, false),
        prop("avatar", RefClassificationKind::ValueObject, Some("MediaReferenceType"), false),
        prop("names", RefClassificationKind::ValueObject, Some("PersonNameType"), true),
    ] {
        db.ingest_property("PersonType", "PersonType", &p)
            .await
            .expect("person property");
    }
    // MediaReference: url + mimeType (the HR Open reference shape).
    for p in [
        prop("url", RefClassificationKind::EntityReference, None, false),
        prop("mimeType", RefClassificationKind::EntityReference, None, false),
    ] {
        db.ingest_property("MediaReferenceType", "MediaReferenceType", &p)
            .await
            .expect("media reference property");
    }
    // PersonName: given/family (no url column — the missing-column case's
    // target when media_classes is overridden).
    for p in [
        prop("givenName", RefClassificationKind::EntityReference, None, false),
        prop("familyName", RefClassificationKind::EntityReference, None, false),
    ] {
        db.ingest_property("PersonNameType", "PersonNameType", &p)
            .await
            .expect("person name property");
    }
    // Ref edges: get_property_ref_target resolves through
    // `(:Property)-[:ReferencesSchema]->(:Schema)` — the property node's
    // ref_target field alone does not resolve.
    for (from, to) in [
        ("avatar::PersonType", "MediaReferenceType"),
        ("names::PersonType", "PersonNameType"),
    ] {
        db.ingest_edge(from, to, EdgeType::ReferencesSchema, None)
            .await
            .expect("references edge");
    }
}

/// The array-media fixture's item edge: `get_array_item_schema` resolves
/// through `[:ItemsOf]`.
async fn ingest_gallery_items_edge(db: &(impl GraphIngestor + GraphQuerier)) {
    db.ingest_edge(
        "gallery::PersonType",
        "MediaReferenceType",
        EdgeType::ItemsOf,
        None,
    )
    .await
    .expect("items edge");
}

fn parse(toml: &str) -> DomainConfig {
    parse_domain_config_str(toml).expect("domains.toml")
}

// ── resolution ──────────────────────────────────────────────────────────

#[tokio::test]
async fn plan_resolves_child_table_and_columns() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));

    let plans = build_media_plans(&engine, &config).await.expect("plans");
    let plan = plans.get("core").expect("core plan");
    assert_eq!(plan.domain, "core");
    assert_eq!(
        plan.provider,
        codegraph_core::types::MediaProvider::Supabase
    );
    assert_eq!(
        plan.upload_mode,
        codegraph_core::types::UploadMode::ClientDirect
    );
    assert_eq!(
        plan.visibility,
        codegraph_core::types::MediaVisibility::Signed
    );
    assert_eq!(plan.media_classes, vec!["MediaReferenceType".to_string()]);
    assert_eq!(plan.fields.len(), 1, "names is not media: {:?}", plan.fields);
    let f = &plan.fields[0];
    assert_eq!(f.entity_title, "PersonType");
    assert_eq!(f.entity_table, "person");
    assert_eq!(f.field_name, "avatar");
    assert_eq!(f.field_snake, "avatar");
    assert_eq!(f.child_table, "person_avatar");
    assert_eq!(f.parent_fk_column, "person_id");
    assert_eq!(f.url_column, "url");
    assert_eq!(f.mime_column, "mime_type");
}

#[tokio::test]
async fn plans_are_deterministic_across_runs() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let a = build_media_plans(&engine, &config).await.expect("plans a");
    let b = build_media_plans(&engine, &config).await.expect("plans b");
    assert_eq!(a, b);
}

#[tokio::test]
async fn no_media_config_builds_no_plans() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(BASE_TOML);
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    assert!(plans.is_empty());
}

#[tokio::test]
async fn bare_media_class_resolves_via_type_suffix() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let plans = build_media_plans(&engine, &config).await.expect("plans");
    assert_eq!(
        plans["core"].media_classes,
        vec!["MediaReferenceType".to_string()],
        "MediaReference resolves to MediaReferenceType via the Type suffix"
    );
}

// ── validation contract ─────────────────────────────────────────────────

#[tokio::test]
async fn unresolved_media_class_is_config_error_naming_it() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct").replace(
        "accept = [\"image/png\", \"image/jpeg\"]",
        "accept = [\"image/png\"]\nmedia_classes = [\"DoesNotExist\"]",
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("DoesNotExist"), "{err}");
    assert!(err.contains("core"), "{err}");
}

#[tokio::test]
async fn refers_wins_media_target_is_config_error() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // The avatar property lowered as an entity reference (refers wins).
    db_refers_avatar(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("PersonType"), "{err}");
    assert!(err.contains("avatar"), "{err}");
}

async fn db_refers_avatar(db: &(impl GraphIngestor + GraphQuerier)) {
    let p = prop("avatar", RefClassificationKind::EntityReference, Some("MediaReferenceType"), false);
    db.ingest_property("PersonType", "PersonType", &p)
        .await
        .expect("avatar re-ingest");
}

#[tokio::test]
async fn array_media_field_is_config_error() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let p = prop("gallery", RefClassificationKind::ValueObject, Some("MediaReferenceType"), true);
    db_ingest_prop(&engine, "PersonType", &p).await;
    ingest_gallery_items_edge(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct"));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("gallery"), "{err}");
}

async fn db_ingest_prop(db: &(impl GraphIngestor + GraphQuerier), title: &str, p: &PropertyNode) {
    db.ingest_property(title, title, p).await.expect("property");
}

#[tokio::test]
async fn missing_mime_column_is_config_error_naming_class() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // Point mime_field at a column the class lacks.
    let config = parse(
        &media_toml("supabase", "client-direct")
            .replace("accept = [\"image/png\", \"image/jpeg\"]", "accept = [\"image/png\"]\nmime_field = \"mime_kind\""),
    );
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("MediaReferenceType"), "{err}");
    assert!(err.contains("mime_kind"), "{err}");
}

// ── the capability matrix (generation-side) ─────────────────────────────

#[tokio::test]
async fn r2_client_direct_is_matrix_error_naming_capability() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"client-direct\"\nbucket = \"b\"\nmax_bytes = 1000\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA\"\n"
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("r2"), "{err}");
    assert!(err.contains("client-direct"), "{err}");
}

#[tokio::test]
async fn bad_key_template_is_config_error_naming_template() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse(&media_toml("supabase", "client-direct").replace(
        "accept = [\"image/png\", \"image/jpeg\"]",
        "accept = [\"image/png\"]\nkey_template = \"{org}/{bucket}\"",
    ));
    let err = match build_media_plans(&engine, &config).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("bucket"), "{err}");
}
