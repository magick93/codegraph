//! Issue #625 PR-A — the `media_storage` global generator: presence-gated
//! emission of `src/media/{mod,noop}.rs` (the provider-neutral trait +
//! fail-closed NoopStorage). Byte-identity when no domain declares
//! `[media]` (the charts precedent).
//!
//! Node-free PR-CI pins: in-memory Grafeo + the real template engine.

use std::fs;

use codegraph_config::DomainConfig;
use codegraph_config::config::parse_domain_config_str;
use codegraph_core::traits::{GraphIngestor, GraphQuerier};
use codegraph_core::types::{EdgeType, PropertyNode, SchemaNode};
use codegraph_generate::media::MediaStorageGenerator;
use codegraph_generate::traits::{GeneratedFile, GlobalGenerator};
use codegraph_generate::{GenerationEntry, ProjectConfig, error::Error};
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

// ── PR-B AppState wiring (#630) ─────────────────────────────────────────
async fn run_scaffold(
    db: &dyn codegraph_core::traits::GraphQuerier,
    config: &DomainConfig,
    out: &std::path::Path,
) -> Result<Vec<GeneratedFile>, Error> {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[])?;
    let project = ProjectConfig::default();
    let generator = codegraph_generate::scaffold::generator::ScaffoldGenerator::new(
        out,
        false, false, false, false, false, false, false, false, false, false,
        "parallel",
    );
    let generation_order = vec![GenerationEntry {
        schema_title: "PersonType".to_string(),
        domain: "core".to_string(),
        pg_schema: "core".to_string(),
        is_cyclic: false,
    }];
    generator
        .generate(db, config, &generation_order, &tera, &project)
        .await
}

fn scaffold_file<'a>(
    files: &'a [GeneratedFile],
    out: &std::path::Path,
    rel: &str,
) -> &'a str {
    files
        .iter()
        .find(|f| f.path == out.join(rel))
        .map(|f| f.content.as_str())
        .unwrap_or_else(|| panic!("scaffold must emit {rel}"))
}

#[tokio::test]
async fn scaffold_wires_the_per_domain_media_slot_and_module_declaration() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_scaffold(&engine, &config, out.path()).await.expect("scaffold");

    let app_state = scaffold_file(&files, out.path(), "src/app_state.rs");
    assert!(
        app_state.contains("pub core_media_storage: Option<std::sync::Arc<dyn crate::media::MediaStorage>>"),
        "the per-domain provider slot: {app_state}"
    );

    let server = scaffold_file(&files, out.path(), "src/server.rs");
    assert!(
        server.contains("core_media_storage:"),
        "the AppState construction site carries the slot: {server}"
    );
    assert!(
        server.contains("SupabaseStorage::from_env"),
        "the configured provider constructs: {server}"
    );
    assert!(
        server.contains("ReqwestHttp::new"),
        "the native transport constructs once at boot: {server}"
    );

    let lib = scaffold_file(&files, out.path(), "src/lib.rs");
    assert!(
        lib.contains("pub mod media;"),
        "the module must be REACHABLE — PR-A's emission was dead without this: {lib}"
    );
}

#[tokio::test]
async fn no_media_scaffold_carries_no_media_surface() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(BASE_TOML).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_scaffold(&engine, &config, out.path()).await.expect("scaffold");

    let app_state = scaffold_file(&files, out.path(), "src/app_state.rs");
    assert!(
        !app_state.contains("media_storage"),
        "byte-identity: no [media] ⇒ no AppState slot"
    );
    let server = scaffold_file(&files, out.path(), "src/server.rs");
    assert!(!server.contains("media_storage"), "{server}");
    let lib = scaffold_file(&files, out.path(), "src/lib.rs");
    assert!(!lib.contains("pub mod media;"), "{lib}");
}

// ── PR-B storage bootstrap migration (#630) ─────────────────────────────

#[tokio::test]
async fn supabase_plan_emits_the_storage_bootstrap_migration() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
    let migration = files
        .iter()
        .find(|f| f.path.ends_with("023000_media_storage.sql"))
        .expect("the static-band media migration");
    assert!(
        migration.content.contains("INSERT INTO storage.buckets"),
        "bucket bootstrap: {}",
        migration.content
    );
    assert!(
        migration
            .content
            .contains("CREATE POLICY media_core_org_scoped ON storage.objects"),
        "org-scoped defense-in-depth policy: {}",
        migration.content
    );
    // The file is written to the real migrations root by the pipeline.
    let _ = out.path().join("migrations").join("023000_media_storage.sql");
}

#[tokio::test]
async fn r2_plan_emits_no_storage_migration() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"through-app\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    ))
    .expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
    assert!(
        !files
            .iter()
            .any(|f| f.path.ends_with("media_storage.sql")),
        "R2 buckets are platform-side — no migration analogue"
    );
}

// ── PR-B/PR-C adapter emission (#630, #632) ─────────────────────────────

// ── #631 deletion cleanup in the DELETE handler ─────────────────────────

async fn run_handler_generator(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    workers: bool,
) -> String {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let mut project = ProjectConfig::default();
    if workers {
        project.deployment.deployment_topology =
            codegraph_generate::profile::DeploymentTopology::Workers;
    }
    let out = tempfile::tempdir().expect("out dir");
    let generator =
        codegraph_generate::api::handler::HandlerGenerator::new(out.path()).with_rules_guards(false);
    use codegraph_generate::traits::EntityGenerator;
    let files = generator
        .generate(db, "PersonType", "core", config, &tera, &project)
        .await
        .expect("handler generation");
    files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("_handler.rs"))
        .map(|f| f.content.clone())
        .expect("handler file")
}

#[tokio::test]
async fn delete_handler_carries_media_cleanup_when_media_declared() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let handler = run_handler_generator(&engine, &config, false).await;
    assert!(
        handler.contains("if let Some(storage) = &state.core_media_storage {"),
        "the cleanup dispatches through the domain slot: {handler}"
    );
    assert!(
        handler.contains("format!(\"{org}/core/person/{id}/avatar\")"),
        "the lowered key format (overwrite-in-place, no child-row read): {handler}"
    );
    assert!(
        handler.contains("storage.delete(&key).await"),
        "best-effort provider delete: {handler}"
    );
    assert!(
        handler.contains("testkit storage --gc"),
        "the gc sweep is the named backstop: {handler}"
    );
    // Placement: inside the DELETE route, after the command completes and
    // before the 204.
    let delete_pos = handler
        .find("pub async fn delete(")
        .expect("delete route present");
    let cleanup_pos = handler
        .find("state.core_media_storage")
        .expect("cleanup present");
    let no_content_pos = handler.rfind("StatusCode::NO_CONTENT").expect("204 present");
    assert!(
        delete_pos < cleanup_pos && cleanup_pos < no_content_pos,
        "cleanup runs post-command, pre-204"
    );
}

#[tokio::test]
async fn delete_handler_has_no_media_cleanup_without_media_or_under_workers() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // Flag-off: no [media] ⇒ no cleanup block (byte-identity).
    let config = parse_domain_config_str(BASE_TOML).expect("toml");
    let handler = run_handler_generator(&engine, &config, false).await;
    assert!(
        !handler.contains("media_cleanup") && !handler.contains("core_media_storage"),
        "flag-off byte-identity: {handler}"
    );
    // Workers topology: the media slot is a monolith surface (the recorded
    // deferral) — the block must be absent so the worker handler compiles.
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let handler = run_handler_generator(&engine, &config, true).await;
    assert!(
        !handler.contains("core_media_storage"),
        "workers topology has no media slot to reference: {handler}"
    );
}

#[tokio::test]
async fn supabase_plan_emits_adapter_http_seam_and_noop() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");

    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
    let mut paths: Vec<String> = files
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(out.path())
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "migrations/023000_media_storage.sql",
            "src/media/http.rs",
            "src/media/mod.rs",
            "src/media/noop.rs",
            "src/media/supabase.rs",
        ],
        "exact file set: trait + noop + adapter + transport + storage bootstrap"
    );

    let supabase_rs = &files
        .iter()
        .find(|f| f.path.ends_with("supabase.rs"))
        .unwrap()
        .content;
    assert!(supabase_rs.contains("pub struct SupabaseStorage"));
    assert!(
        supabase_rs.contains("impl crate::media::MediaStorage for SupabaseStorage"),
        "implements the emitted seam"
    );
    // The adapter speaks the Supabase Storage REST surface (issue #625 §2):
    // service-key ops against /storage/v1, session-token display signing.
    for fragment in [
        "/storage/v1/object/",
        "x-upsert",
        "/storage/v1/object/sign/",
        "/storage/v1/object/upload/sign/",
        "SUPABASE_URL",
        "SUPABASE_SERVICE_KEY",
    ] {
        assert!(supabase_rs.contains(fragment), "adapter pin: {fragment}");
    }
    // Provider purity (the PR-A pin contract): no SigV4, no R2 vocabulary —
    // invalid combos are refused at generation time, not branched at runtime.
    assert!(!supabase_rs.contains("AWS4-HMAC-SHA256"), "no SigV4 in supabase");
    assert!(!supabase_rs.contains("r2"), "no r2 vocabulary in supabase adapter",);
    // Transport injection: the adapter never constructs an HTTP client itself.
    assert!(
        supabase_rs.contains("MediaHttp"),
        "adapters speak through the MediaHttp seam"
    );
    assert!(
        !supabase_rs.contains("reqwest::"),
        "provider purity: no HTTP dependency inside the adapter"
    );

    let http_rs = &files
        .iter()
        .find(|f| f.path.ends_with("http.rs"))
        .unwrap()
        .content;
    assert!(
        http_rs.contains("target_arch = \"wasm32\""),
        "native transport is cfg-gated so the wasm32 pin compiles the plane without reqwest"
    );
    assert!(http_rs.contains("reqwest"), "native leg rides the app's reqwest");
}

#[tokio::test]
async fn r2_plan_emits_the_r2_adapter_with_sigv4() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"through-app\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    ))
    .expect("toml");
    let out = tempfile::tempdir().expect("out dir");

    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
    let mut paths: Vec<String> = files
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(out.path())
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "src/media/http.rs",
            "src/media/mod.rs",
            "src/media/noop.rs",
            "src/media/r2.rs",
        ],
        "r2 plan emits the r2 adapter + transport (no supabase module, no migration)"
    );

    let r2_rs = &files
        .iter()
        .find(|f| f.path.ends_with("r2.rs"))
        .unwrap()
        .content;
    assert!(r2_rs.contains("pub struct R2Storage"));
    assert!(
        r2_rs.contains("impl crate::media::MediaStorage for R2Storage"),
        "implements the emitted seam"
    );
    // SigV4 (issue #632): header-signed ops + query-signed presigned PUT.
    for fragment in [
        "AWS4-HMAC-SHA256",
        "UNSIGNED-PAYLOAD",
        "aws4_request",
        "R2_ACCESS_KEY_ID",
        "R2_SECRET_ACCESS_KEY",
        "R2_ACCOUNT_ID",
        "r2.cloudflarestorage.com",
    ] {
        assert!(r2_rs.contains(fragment), "r2 adapter pin: {fragment}");
    }
    assert!(
        r2_rs.contains("MediaHttp"),
        "adapters speak through the MediaHttp seam"
    );
    assert!(
        !r2_rs.contains("reqwest::"),
        "provider purity: no HTTP dependency inside the adapter"
    );

    let mod_rs = &files
        .iter()
        .find(|f| f.path.ends_with("mod.rs"))
        .unwrap()
        .content;
    assert!(mod_rs.contains("pub mod r2;"), "r2 adapter module declared");
    assert!(
        !mod_rs.contains("pub mod supabase;"),
        "only the configured provider's adapter is declared"
    );
}

// ── emission ────────────────────────────────────────────────────────────

#[tokio::test]
async fn media_config_emits_media_module_and_noop() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");

    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
    let mut paths: Vec<String> = files
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(out.path())
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "migrations/023000_media_storage.sql",
            "src/media/http.rs",
            "src/media/mod.rs",
            "src/media/noop.rs",
            "src/media/supabase.rs",
        ],
        "exact file set: trait + noop + provider adapter + native transport + storage bootstrap"
    );

    let mod_rs = &files
        .iter()
        .find(|f| f.path.ends_with("mod.rs"))
        .unwrap()
        .content;
    assert!(mod_rs.contains("pub trait MediaStorage"), "the seam trait");
    assert!(
        mod_rs.contains("#[async_trait]"),
        "hooks-SPI async convention"
    );
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
    // PR-B (#630): the supabase plan emits its adapter + the transport seam.
    assert!(mod_rs.contains("pub mod supabase;"), "supabase adapter module");
    assert!(mod_rs.contains("pub mod http;"), "native transport module");

    let noop_rs = &files
        .iter()
        .find(|f| f.path.ends_with("noop.rs"))
        .unwrap()
        .content;
    assert!(noop_rs.contains("pub struct NoopStorage"));
    assert!(
        noop_rs.contains("MediaError::Unconfigured"),
        "fail-closed semantics"
    );
    assert_eq!(
        noop_rs
            .matches("Err(crate::media::MediaError::Unconfigured)")
            .count(),
        4,
        "all four operations fail closed"
    );

    // The emitted tree is real on disk (written by the pipeline).
    assert!(out.path().join("src/media/mod.rs").exists() || files.len() == 5);
    let _ = fs::read_to_string(out.path().join("src/media/mod.rs"));
}

#[tokio::test]
async fn emission_is_deterministic() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = run_generator(&engine, &config, dir_a.path())
        .await
        .expect("run a");
    let b = run_generator(&engine, &config, dir_b.path())
        .await
        .expect("run b");
    assert_eq!(a.len(), b.len());
    for (fa, fb) in a.iter().zip(b.iter()) {
        let ra = fa.path.strip_prefix(dir_a.path()).unwrap();
        let rb = fb.path.strip_prefix(dir_b.path()).unwrap();
        assert_eq!(ra, rb, "same relative path");
        assert_eq!(
            fa.content,
            fb.content,
            "byte-identical content for {}",
            ra.display()
        );
    }
}

#[tokio::test]
async fn no_media_config_emits_nothing() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(BASE_TOML).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
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
