//! Compile pin for the media plane (issue #625 PR-B/PR-C — the #551 policy:
//! every generator that emits Rust ships a test that COMPILES its output).
//!
//! The emitted `src/media/{mod,noop,http,supabase|r2}.rs` must compile in a
//! scratch crate carrying ONLY the plane's dependency set. Provider purity
//! is enforced here structurally:
//!
//! - **native leg**: the plane (trait + noop + native transport + adapter)
//!   compiles with `async-trait` + `thiserror` + `serde_json` (supabase
//!   response parsing) + `hmac`/`sha2`/`hex` (SigV4) — and `reqwest` is
//!   needed ONLY by the cfg-gated `http.rs` native module.
//! - **wasm32 leg** (issue #632): the SAME plane compiles for
//!   `wasm32-unknown-unknown` with NO reqwest at all — the `http` module
//!   cfg-folds to empty and adapters speak the `MediaHttp` seam.
//! - **SigV4 known-answer vector**: the R2 adapter's signing core is
//!   extracted VERBATIM into a scratch binary and EXECUTED against the
//!   AWS-published S3 GET example (the #518 event-signature precedent) —
//!   string pins cannot prove a signature algorithm.
//!
//! Teeth are validated per repo discipline (à la #550): break the trait
//! signature in `templates/media/mod.tera`, this pin must fail; revert.

use std::collections::BTreeMap;
use std::fs;
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

fn r2_toml() -> String {
    format!(
        "{BASE_TOML}\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    )
    .replace("provider = \"supabase\"\nupload_mode = \"client-direct\"", "provider = \"r2\"\nupload_mode = \"through-app\"")
}

/// The AWS-published S3 SigV4 GET example (AWS docs "Authenticating
/// Requests: Using the Authorization Header"). The canonical request below
/// was verified against the published signature before being pinned.
const SIGV4_KAT_EXPECTED: &str =
    "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41";

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

/// Emit the plane for `config` into a temp dir and return (name, content)
/// keyed by file stem — `mod`, `noop`, `http`, and the adapter.
async fn emit_media_plane(config: &DomainConfig) -> BTreeMap<String, String> {
    let out = tempfile::tempdir().expect("emission dir");
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[])
        .expect("template engine");
    let generator = MediaStorageGenerator::new(out.path());
    let files = generator
        .generate(&engine, config, &[], &tera, &ProjectConfig::default())
        .await
        .expect("generation");
    assert_eq!(
        files.len(),
        4,
        "the plane is trait + noop + native transport + one adapter"
    );
    let mut map = BTreeMap::new();
    for f in &files {
        let stem = f
            .path
            .file_stem()
            .expect("file stem")
            .to_string_lossy()
            .to_string();
        map.insert(stem, f.content.clone());
    }
    map
}

/// A stable shared target dir for the scratch crates: the pins run several
/// cargo checks and the dependency set compiles once instead of per-crate.
/// Cargo's own lock serializes concurrent invocations.
fn scratch_target_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("codegraph-media-pin-target")
}

/// Write a scratch crate and run `cargo` in it.
fn run_cargo(
    name: &str,
    deps: &str,
    media_files: &BTreeMap<String, String>,
    extra_src: Option<(&str, &str)>,
    args: &[&str],
) -> (bool, String, String) {
    let scratch = tempfile::tempdir().expect("scratch crate dir");
    let root = scratch.path();
    fs::create_dir_all(root.join("src").join("media")).expect("src/media");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{deps}\n\n[workspace]\n"
        ),
    )
    .expect("scratch Cargo.toml");
    fs::write(root.join("src").join("lib.rs"), "pub mod media;\n").expect("lib.rs");
    for (stem, content) in media_files {
        fs::write(
            root.join("src").join("media").join(format!("{stem}.rs")),
            content,
        )
        .expect("media module");
    }
    if let Some((path, content)) = extra_src {
        let full = root.join(path);
        fs::create_dir_all(full.parent().expect("parent")).expect("src dir");
        fs::write(full, content).expect("extra source");
    }
    let out = Command::new("cargo")
        .current_dir(root)
        .args(args)
        .env("CARGO_TARGET_DIR", scratch_target_dir())
        .output()
        .expect("cargo must run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

const CORE_DEPS: &str = "async-trait = \"0.1\"\nthiserror = \"2\"";
const SIG_DEPS: &str = "hmac = \"0.12\"\nsha2 = \"0.10\"\nhex = \"0.4\"";
const SUPABASE_DEPS: &str = "serde_json = \"1\"";

/// #551 pin (native): the supabase plane compiles standalone — adapter is
/// provider-pure (no reqwest); the cfg-gated native transport carries it.
#[tokio::test]
async fn supabase_plane_compiles_native() {
    let config: DomainConfig = parse_domain_config_str(BASE_TOML).expect("domains.toml");
    let files = emit_media_plane(&config).await;
    let deps = format!("{CORE_DEPS}\n{SUPABASE_DEPS}\nreqwest = {{ version = \"0.12\", default-features = false, features = [\"rustls-tls\"] }}\n");
    let (ok, stdout, stderr) = run_cargo(
        "media_supabase_pin",
        &deps,
        &files,
        None,
        &["check", "--all-targets"],
    );
    assert!(
        ok,
        "the emitted supabase plane must compile standalone:\n{stdout}\n{stderr}"
    );
}

/// #551 pin (native): the r2 plane compiles standalone — pure SigV4, no
/// serde_json needed.
#[tokio::test]
async fn r2_plane_compiles_native() {
    let config: DomainConfig = parse_domain_config_str(&r2_toml()).expect("domains.toml");
    let files = emit_media_plane(&config).await;
    let deps = format!("{CORE_DEPS}\n{SIG_DEPS}\nreqwest = {{ version = \"0.12\", default-features = false, features = [\"rustls-tls\"] }}\n");
    let (ok, stdout, stderr) = run_cargo(
        "media_r2_pin",
        &deps,
        &files,
        None,
        &["check", "--all-targets"],
    );
    assert!(ok, "the emitted r2 plane must compile standalone:\n{stdout}\n{stderr}");
}

/// Issue #632 wasm32 leg: the plane compiles for wasm32-unknown-unknown
/// with NO reqwest — `http.rs` cfg-folds to empty and the adapter speaks
/// the `MediaHttp` seam. (Run for BOTH adapters; the claim belongs to the
/// adapter that claims wasm per the #551 documentation rule.)
#[tokio::test]
async fn media_plane_compiles_on_wasm32_without_an_http_dependency() {
    for (label, config_str) in [
        ("supabase", BASE_TOML.to_string()),
        ("r2", r2_toml()),
    ] {
        let config: DomainConfig = parse_domain_config_str(&config_str).expect("domains.toml");
        let files = emit_media_plane(&config).await;
        let deps = match label {
            "supabase" => format!("{CORE_DEPS}\n{SUPABASE_DEPS}\n{SIG_DEPS}\n"),
            _ => format!("{CORE_DEPS}\n{SIG_DEPS}\n"),
        };
        let (ok, stdout, stderr) = run_cargo(
            &format!("media_{label}_wasm_pin"),
            &deps,
            &files,
            None,
            &["check", "--target", "wasm32-unknown-unknown"],
        );
        assert!(
            ok,
            "the {label} media plane must compile on wasm32 with no HTTP dependency:\n{stdout}\n{stderr}"
        );
    }
}

/// The SigV4 known-answer vector (issue #632): the R2 adapter's signing
/// core is extracted VERBATIM into a scratch binary and executed against
/// the AWS-published S3 GET example. A string pin cannot prove a
/// signature algorithm; running the emitted code can.
#[tokio::test]
async fn r2_sigv4_matches_the_published_aws_vector() {
    let config: DomainConfig = parse_domain_config_str(&r2_toml()).expect("domains.toml");
    let files = emit_media_plane(&config).await;
    let mod_rs = files.get("mod").expect("mod.rs");
    let r2_rs = files.get("r2").expect("r2 adapter emitted");
    // The KAT crate mirrors the compile-pin layout (lib + module tree) so
    // the adapter's `crate::media::…` paths resolve naturally. The `http`
    // module is dropped (the KAT exercises the signing core, not the
    // transport) — its declaration goes with it.
    let kat_mod_rs: String = mod_rs
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.starts_with("pub mod http;") || t.starts_with("//!"))
        })
        .collect::<Vec<_>>()
        .join("\n");

    let main_rs = r#"use media_sigv4_kat::media::r2::{sha256_hex, SigV4Request};

fn main() {
    let empty_hash = sha256_hex(b"");
    let headers = vec![
        ("host".to_string(), "examplebucket.s3.amazonaws.com".to_string()),
        ("range".to_string(), "bytes=0-9".to_string()),
        ("x-amz-content-sha256".to_string(), empty_hash.clone()),
        ("x-amz-date".to_string(), "20130524T000000Z".to_string()),
    ];
    let req = SigV4Request {
        method: "GET",
        canonical_uri: "/test.txt".to_string(),
        canonical_query: String::new(),
        headers,
        payload_hash: empty_hash,
        date_stamp: "20130524".to_string(),
        amz_date: "20130524T000000Z".to_string(),
        region: "us-east-1".to_string(),
        service: "s3".to_string(),
    };
    println!("{}", req.authorization("AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"));
}
"#;
    let scratch = tempfile::tempdir().expect("scratch crate dir");
    let root = scratch.path();
    fs::create_dir_all(root.join("src").join("media")).expect("src/media");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"media_sigv4_kat\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[[bin]]\nname = \"kat\"\npath = \"src/main.rs\"\n\n[dependencies]\n{CORE_DEPS}\n{SIG_DEPS}\n\n[workspace]\n"
        ),
    )
    .expect("Cargo.toml");
    fs::write(root.join("src").join("lib.rs"), "pub mod media;\n").expect("lib.rs");
    fs::write(root.join("src").join("media").join("mod.rs"), kat_mod_rs).expect("media/mod.rs");
    fs::write(
        root.join("src").join("media").join("noop.rs"),
        files.get("noop").expect("noop.rs"),
    )
    .expect("media/noop.rs");
    fs::write(root.join("src").join("media").join("r2.rs"), r2_rs).expect("media/r2.rs");
    fs::write(root.join("src").join("main.rs"), main_rs).expect("main.rs");
    let out = Command::new("cargo")
        .current_dir(root)
        .args(["run", "-q", "--bin", "kat"])
        .env("CARGO_TARGET_DIR", scratch_target_dir())
        .output()
        .expect("cargo run must work");
    assert!(
        out.status.success(),
        "KAT binary must run:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let authorization = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        authorization.contains("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,Signature="),
        "authorization header shape: {authorization}"
    );
    let signature = authorization
        .rsplit(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches("Signature=")
        .to_string();
    assert_eq!(
        signature, SIGV4_KAT_EXPECTED,
        "the emitted SigV4 core must reproduce the AWS-published signature"
    );
}

/// Teeth (à la #550): the pin must reject a broken trait signature — a
/// renamed method makes the noop impl fail to compile.
#[tokio::test]
async fn the_pin_rejects_a_drifted_trait_signature() {
    let config: DomainConfig = parse_domain_config_str(BASE_TOML).expect("domains.toml");
    let mut files = emit_media_plane(&config).await;
    let mod_rs = files.get_mut("mod").expect("mod.rs");
    assert!(
        mod_rs.contains("async fn presign_put"),
        "fixture sanity: the trait carries presign_put"
    );
    *mod_rs = mod_rs.replace("async fn presign_put", "async fn presigned_put");
    let deps = format!("{CORE_DEPS}\n{SUPABASE_DEPS}\n{SIG_DEPS}\n");
    let (ok, _stdout, stderr) = run_cargo(
        "media_teeth",
        &deps,
        &files,
        None,
        &["check", "--all-targets"],
    );
    assert!(
        !ok,
        "a drifted trait signature must fail the compile — the pin has no teeth otherwise:\n{stderr}"
    );
}

/// Template-drift tripwire on the PIN itself: if the emitted mod.rs ever
/// drops one of the four operations (or renames the trait), this string
/// pin fails fast before the scratch-crate run even starts.
#[tokio::test]
async fn emitted_plane_carries_the_full_trait_contract() {
    let config: DomainConfig = parse_domain_config_str(BASE_TOML).expect("domains.toml");
    let files = emit_media_plane(&config).await;
    let mod_rs = files.get("mod").expect("mod.rs");
    for needle in [
        "pub trait MediaStorage: Send + Sync",
        "pub trait MediaHttp: Send + Sync",
        "async fn put",
        "async fn delete",
        "async fn display_urls",
        "async fn presign_put",
        "pub mod noop;",
        "pub mod http;",
        "pub mod supabase;",
    ] {
        assert!(mod_rs.contains(needle), "emitted mod.rs missing {needle}");
    }
    let http_rs = files.get("http").expect("http.rs");
    assert!(
        http_rs.contains("target_arch = \"wasm32\""),
        "native transport stays cfg-gated — the wasm leg links the plane bare"
    );
}
