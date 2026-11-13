//! Compile pin for the media plane (issue #625 PR-A — the #551 policy:
//! every generator that emits Rust ships a test that COMPILES its output).
//!
//! The emitted `src/media/{mod,noop}.rs` (the provider-neutral
//! `MediaStorage` trait + fail-closed `NoopStorage`) must compile in a
//! scratch crate carrying ONLY the plane's dependency set: `async-trait` +
//! `thiserror`. No db/runtime deps — the seam is provider-pure by design
//! (the events-#505 wasm lesson: the worker planes compile the module on
//! wasm32; that leg lands with the PR-C worker-binding adapter, whose
//! claim it belongs to — this pin's doc comment is the recorded scope).
//!
//! Teeth are validated per repo discipline (à la #550): break the trait
//! signature in `templates/media/mod.tera`, this pin must fail; revert.

use std::fs;
use std::path::Path;
use std::process::Command;

use codegraph_config::DomainConfig;
use codegraph_config::config::parse_domain_config_str;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{EdgeType, PropertyNode, SchemaNode};
use codegraph_generate::ProjectConfig;
use codegraph_generate::media::MediaStorageGenerator;
use codegraph_generate::traits::GlobalGenerator;
use codegraph_grafeo::GrafeoEngine;
use codegraph_type_contracts::RefClassificationKind;

const BASE_TOML: &str = r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]
type_suffix = "Type"

[domains.core]
label = "Core"
schema_dir = "core"
postgres_schema = "core"

[domains.core.media]
provider = "supabase"
upload_mode = "client-direct"
bucket = "person-media"
max_bytes = 5242880
accept = ["image/png"]
"#;

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
        prop_type: if ref_target.is_some() {
            "object".into()
        } else {
            "string".into()
        },
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
    db.ingest_schema(&schema(
        "PersonType",
        "PersonType",
        "entity",
        "person",
        true,
    ))
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
        prop(
            "avatar",
            RefClassificationKind::ValueObject,
            Some("MediaReferenceType"),
        ),
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

/// Emit the plane into `out` and return the two file contents.
async fn emit_media_plane(out: &Path) -> (String, String) {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config: DomainConfig = parse_domain_config_str(BASE_TOML).expect("domains.toml");
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[])
        .expect("template engine");
    let generator = MediaStorageGenerator::new(out);
    let files = generator
        .generate(&engine, &config, &[], &tera, &ProjectConfig::default())
        .await
        .expect("generation");
    assert_eq!(files.len(), 2, "PR-A emits exactly mod.rs + noop.rs");
    let mod_rs = files
        .iter()
        .find(|f| f.path.ends_with("mod.rs"))
        .expect("mod.rs")
        .content
        .clone();
    let noop_rs = files
        .iter()
        .find(|f| f.path.ends_with("noop.rs"))
        .expect("noop.rs")
        .content
        .clone();
    (mod_rs, noop_rs)
}

/// Scratch crate: ONLY async-trait + thiserror — the plane's dependency
/// set. No db, no runtime, no sea_orm: the provider-purity claim.
fn cargo_check_media_plane(mod_rs: &str, noop_rs: &str) {
    let scratch = tempfile::tempdir().expect("scratch crate dir");
    let root = scratch.path();
    fs::create_dir_all(root.join("src").join("media")).expect("src/media");

    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "media_plane_pin"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
thiserror = "2"

[workspace]
"#,
    )
    .expect("scratch Cargo.toml");
    fs::write(root.join("src").join("lib.rs"), "pub mod media;\n").expect("lib.rs");
    fs::write(root.join("src").join("media").join("mod.rs"), mod_rs).expect("mod.rs");
    fs::write(root.join("src").join("media").join("noop.rs"), noop_rs).expect("noop.rs");

    let out = Command::new("cargo")
        .current_dir(root)
        .args(["check", "--all-targets"])
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("cargo check must run");
    assert!(
        out.status.success(),
        "the emitted media plane must compile standalone (async-trait + thiserror only):\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// #551 pin: emit → assemble scratch crate → cargo check.
#[tokio::test]
async fn emitted_media_plane_compiles_standalone() {
    let out = tempfile::tempdir().expect("emission dir");
    let (mod_rs, noop_rs) = emit_media_plane(out.path()).await;
    cargo_check_media_plane(&mod_rs, &noop_rs);
}

/// Teeth (à la #550): the pin must reject a broken trait signature. This
/// test assembles a scratch crate with a MUTATED trait (a method renamed —
/// the drift the pin exists to catch) and asserts cargo check FAILS — the
/// inverse tripwire proving the pin has teeth without touching the real
/// template.
#[tokio::test]
async fn the_pin_rejects_a_drifted_trait_signature() {
    let out = tempfile::tempdir().expect("emission dir");
    let (mut mod_rs, noop_rs) = emit_media_plane(out.path()).await;
    assert!(
        mod_rs.contains("async fn presign_put"),
        "fixture sanity: the trait carries presign_put"
    );
    mod_rs = mod_rs.replace("async fn presign_put", "async fn presigned_put");
    // The noop impl still implements presign_put → the crate must NOT
    // compile (missing trait method on the impl side).
    let scratch = tempfile::tempdir().expect("scratch crate dir");
    let root = scratch.path();
    fs::create_dir_all(root.join("src").join("media")).expect("src/media");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "media_plane_teeth"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
thiserror = "2"

[workspace]
"#,
    )
    .expect("Cargo.toml");
    fs::write(root.join("src").join("lib.rs"), "pub mod media;\n").expect("lib.rs");
    fs::write(root.join("src").join("media").join("mod.rs"), &mod_rs).expect("mod.rs");
    fs::write(root.join("src").join("media").join("noop.rs"), noop_rs).expect("noop.rs");
    let out = Command::new("cargo")
        .current_dir(root)
        .args(["check", "--all-targets"])
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("cargo check must run");
    assert!(
        !out.status.success(),
        "a drifted trait signature must fail the compile — the pin has no teeth otherwise"
    );
}

/// Template-drift tripwire on the PIN itself: if the emitted mod.rs ever
/// drops one of the four operations (or renames the trait), this string
/// pin fails fast before the scratch-crate run even starts — the cheap
/// first line of defense behind the cargo check.
#[tokio::test]
async fn emitted_plane_carries_the_full_trait_contract() {
    let out = tempfile::tempdir().expect("emission dir");
    let (mod_rs, _) = emit_media_plane(out.path()).await;
    for needle in [
        "pub trait MediaStorage: Send + Sync",
        "async fn put",
        "async fn delete",
        "async fn display_urls",
        "async fn presign_put",
        "pub mod noop;",
    ] {
        assert!(mod_rs.contains(needle), "emitted mod.rs missing {needle}");
    }
}
