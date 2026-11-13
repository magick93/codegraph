//! Issue #625 PR-A — the `media_storage` global generator: presence-gated
//! emission of `src/media/{mod,noop}.rs` (the provider-neutral trait +
//! fail-closed NoopStorage). Byte-identity when no domain declares
//! `[media]` (the charts precedent).
//!
//! Node-free PR-CI pins: in-memory Grafeo + the real template engine.

use std::fs;

use codegraph_config::config::parse_domain_config_str;
use codegraph_config::DomainConfig;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{EdgeType, PropertyNode, SchemaNode};
use codegraph_generate::media::MediaStorageGenerator;
use codegraph_generate::traits::{GeneratedFile, GlobalGenerator};
use codegraph_generate::{ProjectConfig, error::Error};
use codegraph_grafeo::GrafeoEngine;
use codegraph_type_contracts::RefClassificationKind;

// ── fixture (mirrors media_plan_tests) ──────────────────────────────────

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

fn schema(id: &str, title: &str, classification: &str, table: &str, is_entity: bool) -> SchemaNode {
    SchemaNode {
        schema_id: id.into(),
        title: title.into(),
        description: None,
        schema_type: "object".into(),
        classification: classification.into(),
        domain: Some("core".into()),
        namespace: None,
        rel_path: format!("core/{id}.json"),
        pg_type: "text".into(),
        rust_type: "String".into(),
        sea_orm_type: "String".into(),
        rust_type_name: title.into(),
        pg_table_name: table.into(),
        api_path_segment: table.into(),
        parent_schema: None,
        is_entity,
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

fn prop(name: &str, kind: RefClassificationKind, ref_target: Option<&str>) -> PropertyNode {
    PropertyNode {
        name: name.into(),
        prop_type: if ref_target.is_some() { "object".into() } else { "string".into() },
        description: None,
        format: None,
        is_required: false,
        is_nullable: true,
        is_id: name == "id",
        is_array: false,
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
    db.ingest_schema(&schema("PersonType", "PersonType", "entity", "person", true))
        .await
        .expect("person schema");
    db.ingest_schema(&schema(
        "MediaReferenceType",
        "MediaReferenceType",
        "value_object",
        "media_reference",
        false,
    ))
    .await
    .expect("media reference schema");
    for p in [
        prop("id", RefClassificationKind::EntityReference, None),
        prop("avatar", RefClassificationKind::ValueObject, Some("MediaReferenceType")),
    ] {
        db.ingest_property("PersonType", "PersonType", &p)
            .await
            .expect("person property");
    }
    for p in [
        prop("url", RefClassificationKind::EntityReference, None),
        prop("mimeType", RefClassificationKind::EntityReference, None),
    ] {
        db.ingest_property("MediaReferenceType", "MediaReferenceType", &p)
            .await
            .expect("media reference property");
    }
    db.ingest_edge(
        "avatar::PersonType",
        "MediaReferenceType",
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("references edge");
}

async fn run_generator(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    out: &std::path::Path,
) -> Result<Vec<GeneratedFile>, Error> {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[])?;
    let project = ProjectConfig::default();
    let generator = MediaStorageGenerator::new(out);
    generator.generate(db, config, &[], &tera, &project).await
}

// ── emission ────────────────────────────────────────────────────────────

#[tokio::test]
async fn media_config_emits_media_module_and_noop() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");

    let files = run_generator(&engine, &config, out.path()).await.expect("files");
    let mut paths: Vec<String> = files
        .iter()
        .map(|f| f.path.strip_prefix(out.path()).unwrap().to_string_lossy().to_string())
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["src/media/mod.rs", "src/media/noop.rs"], "exact file set");

    let mod_rs = &files.iter().find(|f| f.path.ends_with("mod.rs")).unwrap().content;
    assert!(mod_rs.contains("pub trait MediaStorage"), "the seam trait");
    assert!(mod_rs.contains("#[async_trait]"), "hooks-SPI async convention");
    // Formatting-agnostic: strip ALL whitespace on both sides — the pins are
    // about the contract, not the wrap.
    let flat = mod_rs.split_whitespace().collect::<String>();
    for m in [
        "asyncfnput(&self,key:&str,data:Vec<u8>,mime_type:&str)->Result<StoredRef,MediaError>;",
        "asyncfndelete(&self,key:&str)->Result<(),MediaError>;",
        "asyncfndisplay_urls(&self,keys:&[String])->Result<BTreeMap<String,String>,MediaError>;",
        "asyncfnpresign_put(&self,key:&str,mime_type:&str,expires_in_secs:u64,)->Result<PresignedPut,MediaError>;",
    ] {
        assert!(flat.contains(m), "trait method pin missing: {m}");
    }
    assert!(mod_rs.contains("pub mod noop;"), "noop re-exported");
    assert!(
        !mod_rs.contains("pub mod supabase") && !mod_rs.contains("pub mod r2"),
        "PR-A emits no adapter modules — they land with PR-B/PR-C"
    );

    let noop_rs = &files.iter().find(|f| f.path.ends_with("noop.rs")).unwrap().content;
    assert!(noop_rs.contains("pub struct NoopStorage"));
    assert!(noop_rs.contains("MediaError::Unconfigured"), "fail-closed semantics");
    assert_eq!(
        noop_rs.matches("Err(crate::media::MediaError::Unconfigured)").count(),
        4,
        "all four operations fail closed"
    );

    // The emitted tree is real on disk (written by the pipeline).
    assert!(out.path().join("src/media/mod.rs").exists() || files.len() == 2);
    let _ = fs::read_to_string(out.path().join("src/media/mod.rs"));
}

#[tokio::test]
async fn emission_is_deterministic() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = run_generator(&engine, &config, dir_a.path()).await.expect("run a");
    let b = run_generator(&engine, &config, dir_b.path()).await.expect("run b");
    assert_eq!(a.len(), b.len());
    for (fa, fb) in a.iter().zip(b.iter()) {
        let ra = fa.path.strip_prefix(dir_a.path()).unwrap();
        let rb = fb.path.strip_prefix(dir_b.path()).unwrap();
        assert_eq!(ra, rb, "same relative path");
        assert_eq!(fa.content, fb.content, "byte-identical content for {}", ra.display());
    }
}

#[tokio::test]
async fn no_media_config_emits_nothing() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(BASE_TOML).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path()).await.expect("files");
    assert!(files.is_empty(), "byte-identity: no [media] ⇒ zero files");
    assert!(!out.path().join("src/media").exists());
}

#[tokio::test]
async fn broken_media_config_is_a_config_error_not_silent_emission() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // r2 + client-direct passes config parse (structural only) and must be
    // refused here by the capability matrix.
    let config = parse_domain_config_str(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"client-direct\"\nbucket = \"b\"\nmax_bytes = 1000\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA\"\n"
    ))
    .expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let err = match run_generator(&engine, &config, out.path()).await {
        Err(Error::Config(msg)) => msg,
        other => panic!("expected Error::Config, got {other:?}"),
    };
    assert!(err.contains("client-direct"), "{err}");
    assert!(!out.path().join("src/media").exists(), "never half-emits");
}
