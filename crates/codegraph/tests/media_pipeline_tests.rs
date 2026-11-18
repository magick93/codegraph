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

// ── PR-B UI upload control (#630) ───────────────────────────────────────

#[tokio::test]
async fn media_fields_emit_the_fallback_upload_control() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let generator =
        codegraph_generate::ui::scaffold::UiScaffoldGenerator::new(out.path(), false, false);
    use codegraph_generate::traits::GlobalGenerator;
    let files = generator
        .generate(&engine, &config, &[], &tera, &ProjectConfig::default())
        .await
        .expect("ui scaffold");

    let media_ts = files
        .iter()
        .find(|f| f.path.ends_with("media/media.ts"))
        .expect("the shared validation module");
    assert!(
        media_ts.content.contains("validateMediaFile") && media_ts.content.contains("maxBytes"),
        "pre-flight validation mirroring the API checks: {}",
        media_ts.content
    );
    let component = files
        .iter()
        .find(|f| f.path.ends_with("media/MediaField.svelte"))
        .expect("the fallback control");
    assert!(
        component
            .content
            .contains("data-testid=\"media-{spec.field}-input\""),
        "testid contract: {}",
        component.content
    );
    assert!(
        component
            .content
            .contains("upload: (id: string, file: File) => Promise<MediaUploadResult>"),
        "transport-agnostic upload callback (the override contract): {}",
        component.content
    );
}

#[tokio::test]
async fn no_media_emits_no_ui_media_module() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(BASE_TOML).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let generator =
        codegraph_generate::ui::scaffold::UiScaffoldGenerator::new(out.path(), false, false);
    use codegraph_generate::traits::GlobalGenerator;
    let files = generator
        .generate(&engine, &config, &[], &tera, &ProjectConfig::default())
        .await
        .expect("ui scaffold");
    assert!(
        !files
            .iter()
            .any(|f| f.path.to_string_lossy().contains("media")),
        "byte-identity: no [media] ⇒ no lib/media files"
    );
}

// ── PR-C route revival-lite (#632): mode-gated media routes ────────────

async fn run_media_route_generator(db: &dyn GraphQuerier, config: &DomainConfig) -> Vec<String> {
    run_media_route_generator_for(db, config, "PersonType").await
}

async fn run_media_route_generator_for(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    title: &str,
) -> Vec<String> {
    emitted_media_files(db, config, title)
        .await
        .into_iter()
        .map(|(_, content)| content)
        .collect()
}

async fn rendered_router_media_block(db: &dyn GraphQuerier, config: &DomainConfig) -> String {
    rendered_router_media_block_for(db, config, &["PersonType".to_string()]).await
}

async fn rendered_router_media_block_for(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    titles: &[String],
) -> String {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let project = ProjectConfig::default();
    let out = tempfile::tempdir().expect("out dir");
    // The router's Source-2 nesting (the graph parent candidates) — the real
    // pipeline passes them, so the rendered authority must too.
    let parent_candidates = db.get_parent_candidates().await.expect("parent candidates");
    let ctx = codegraph_generate::api::router::build_router_context(
        db,
        "core",
        titles,
        config,
        &parent_candidates,
        &project,
    )
    .await
    .expect("router context");
    let _ = out;
    codegraph_generate::render_template_with_project(&tera, "api/router.tera", &ctx, &project)
        .expect("router render")
}

#[tokio::test]
async fn through_app_mode_emits_child_row_upload_routes() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");

    let modules = run_media_route_generator(&engine, &config).await;
    assert_eq!(modules.len(), 1, "one module per media field");
    let module = &modules[0];
    assert!(
        module.contains("pub async fn upload"),
        "through-app mounts the multipart upload: {module}"
    );
    assert!(!module.contains("pub async fn presign"), "{module}");
    // The CHILD row model (R4) — the dormant template wrote parent columns:
    assert!(
        module.contains("person_avatar"),
        "child table writes: {module}"
    );
    assert!(module.contains("person_id"), "parent fk writes: {module}");
    // The AppState media slot (PR-B) — never the hooks-crate registry slot:
    assert!(
        module.contains(".core_media_storage"),
        "dispatches through the AppState slot: {module}"
    );
    assert!(
        !module.contains("registry.media_storage"),
        "the hooks-crate slot is not the runtime surface: {module}"
    );
    // Overwrite-in-place key from the domain template (R7):
    assert!(
        module.contains("format!(\"{org}/core/person/{id}/avatar\")"),
        "{module}"
    );

    let router = rendered_router_media_block(&engine, &config).await;
    assert!(
        router.contains("media_person_avatar::upload"),
        "router mounts the upload: {router}"
    );
    assert!(
        router.contains("media_person_avatar::display"),
        "router mounts signed display: {router}"
    );
    assert!(
        !router.contains("/presign"),
        "through-app mounts no presign route: {router}"
    );
}

/// Parse the integer literal after `marker` in an emitted source string
/// (digits and `_` separators only) — the shared reader both sides of the
/// #710 pin go through, so neither expectation is an independent hardcode.
fn literal_after(source: &str, marker: &str) -> u64 {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` missing from: {source}"))
        + marker.len();
    let rest = &source[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '_'))
        .unwrap_or(rest.len());
    rest[..end]
        .replace('_', "")
        .parse()
        .unwrap_or_else(|e| panic!("`{}` is not an integer literal: {e}", &rest[..end]))
}

/// The expression (up to the first `)`) after `marker` — used to read the
/// layer's `max_bytes + 64 * 1024` arithmetic verbatim, not just its head.
fn expression_after<'a>(source: &'a str, marker: &str) -> &'a str {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` missing from: {source}"))
        + marker.len();
    let rest = &source[start..];
    let end = rest
        .find(')')
        .unwrap_or_else(|| panic!("`{marker}` is not closed: {rest}"));
    rest[..end].trim()
}

/// Split a rendered router into its `.route(...)` blocks (each block runs to
/// the next top-level `.route(` or the end), so a pin can assert WHERE a
/// layer sits instead of merely that the text exists somewhere.
fn route_blocks(router: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = router[cursor..].find(".route(") {
        let start = cursor + rel;
        let after = start + ".route(".len();
        let end = router[after..]
            .find("\n        .route(")
            .map(|i| after + i)
            .unwrap_or(router.len());
        blocks.push(&router[start..end]);
        if end == router.len() {
            break;
        }
        cursor = end;
    }
    blocks
}

/// Issue #710 — the configured `max_bytes` must be REACHABLE: axum's global
/// `DefaultBodyLimit` (2 MiB) was too low, so a larger configured limit was
/// unreachable. The router scopes a layer to every upload METHOD route, with
/// `max_bytes + 64 KiB` of multipart slack so the handler's `max_bytes` check
/// (the FILE cap, matching utoipa and the UI pre-check) stays the boundary.
#[tokio::test]
async fn upload_route_scopes_the_body_limit_to_the_configured_max_bytes() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    // A SECOND media field so the pin covers EVERY emitted route, not just
    // the first occurrence.
    db_ingest_second_media_field(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let configured = config
        .domains
        .get("core")
        .and_then(|d| d.media.as_ref())
        .expect("media config")
        .max_bytes;

    let modules = run_media_route_generator(&engine, &config).await;
    assert_eq!(modules.len(), 2, "one module per media field");

    // Handler side: every emitted handler compares the file size against the
    // configured `max_bytes`.
    let handler_limits: Vec<u64> = modules
        .iter()
        .map(|m| literal_after(m, "if data.len() as u64 > "))
        .collect();
    assert_eq!(
        handler_limits,
        vec![configured, configured],
        "every handler checks the configured max_bytes"
    );

    let router = rendered_router_media_block(&engine, &config).await;
    let upload_blocks: Vec<&str> = route_blocks(&router)
        .into_iter()
        .filter(|b| b.contains("::upload"))
        .collect();
    assert_eq!(
        upload_blocks.len(),
        2,
        "one upload route per media field: {router}"
    );
    for block in &upload_blocks {
        // Attached to the upload method router BEFORE the get/delete merges:
        // moving it after `.merge(` (or onto the entity router) would widen
        // or lose the scoping.
        let layer_at = block
            .find(".layer(axum::extract::DefaultBodyLimit::max(")
            .unwrap_or_else(|| panic!("the upload method carries the layer: {block}"));
        let first_merge = block
            .find(".merge(")
            .unwrap_or_else(|| panic!("the upload route merges display/remove: {block}"));
        assert!(
            layer_at < first_merge,
            "the layer is on the upload method BEFORE the merges: {block}"
        );
        // The layer is the file cap PLUS the multipart envelope slack, so a
        // file of exactly `max_bytes` is not rejected by the layer (B2).
        assert_eq!(
            expression_after(block, "axum::extract::DefaultBodyLimit::max("),
            format!("{configured} + 64 * 1024"),
            "the layer is max_bytes + the envelope slack: {block}"
        );
    }

    // Above axum's 2 MiB default (else the layer is inert).
    let layer_limit = configured + 64 * 1024;
    assert!(
        layer_limit > 2 * 1024 * 1024,
        "the layered ceiling must exceed axum's 2 MiB default: {layer_limit}"
    );

    // Scoping, structurally: every `DefaultBodyLimit` lives inside an upload
    // route block — a `Router::layer`, a `route_layer`, or a sibling route's
    // layer fails this.
    let total = router.matches("DefaultBodyLimit").count();
    let inside_upload_routes: usize = upload_blocks
        .iter()
        .map(|b| b.matches("DefaultBodyLimit").count())
        .sum();
    assert_eq!(
        total, inside_upload_routes,
        "every DefaultBodyLimit sits inside an upload route: {router}"
    );
    assert!(
        !router.contains("route_layer(axum::extract::DefaultBodyLimit"),
        "never a router-wide layer: {router}"
    );
}

async fn db_ingest_second_media_field(db: &(impl GraphIngestor + GraphQuerier)) {
    db.ingest_property(
        "PersonType",
        "PersonType",
        &prop(
            "photo",
            RefClassificationKind::ValueObject,
            Some("MediaReferenceType"),
        ),
    )
    .await
    .expect("second media property");
    db.ingest_edge(
        "photo::PersonType",
        "MediaReferenceType",
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("second references edge");
}

#[tokio::test]
async fn presigned_mode_emits_the_presign_issuance_route() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"presigned\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    ))
    .expect("toml");

    let modules = run_media_route_generator(&engine, &config).await;
    assert_eq!(modules.len(), 1);
    let module = &modules[0];
    assert!(
        module.contains("pub async fn presign"),
        "presigned mode issues the presign op: {module}"
    );
    assert!(
        module.contains("presign_put"),
        "the adapter trait call: {module}"
    );
    assert!(
        !module.contains("pub async fn upload"),
        "no multipart upload in presigned mode: {module}"
    );

    let router = rendered_router_media_block(&engine, &config).await;
    assert!(
        router.contains("/presign"),
        "router mounts presign issuance: {router}"
    );
    assert!(
        !router.contains("::upload"),
        "presigned mode mounts no multipart upload route: {router}"
    );
}

#[tokio::test]
async fn client_direct_emits_no_media_routes_at_all() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");

    let modules = run_media_route_generator(&engine, &config).await;
    assert!(
        modules.is_empty(),
        "R5: client-direct speaks to the provider with its session — no routes"
    );
    let router = rendered_router_media_block(&engine, &config).await;
    assert!(
        !router.contains("media_person_avatar"),
        "no media routes for client-direct: {router}"
    );
}

// ── Issue #690 — media routes ride the #169 session bundle ─────────────
//
// The pre-fix template talked to the database outside the bundle: no
// `set_config` context, no `SET LOCAL ROLE app_user`, unqualified table
// names, and display/remove filtered only `WHERE {parent_fk} = '<uuid>'`
// (an IDOR). These pins are the red-first evidence for the rewrite.

fn media_route_module_key(module: &str, needle: &str) -> usize {
    module
        .find(needle)
        .unwrap_or_else(|| panic!("the emitted media module must carry `{needle}`:\n{module}"))
}

#[tokio::test]
async fn media_routes_ride_the_session_bundle_and_a_tenant_scoped_parent_gate() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");

    let modules = run_media_route_generator(&engine, &config).await;
    assert_eq!(modules.len(), 1, "one module per media field");
    let module = &modules[0];

    // The #169 session entry point (issue #690): every statement runs on a
    // transaction that installed the request scope bundle.
    for needle in [
        "crate::media::session::begin_scoped(",
        "crate::media::session::parent_exists(",
    ] {
        media_route_module_key(module, needle);
    }

    // EVERY handler reads the session bundle — display/remove historically
    // did not even take the extractor, which is how the IDOR survived.
    assert_eq!(
        module
            .matches("Extension(api_key_info): Extension<ApiKeyInfo>")
            .count(),
        3,
        "upload/display/remove all extract the session bundle:\n{module}"
    );

    // Schema-qualified identifiers: generated tables live under the
    // domain's Postgres schema, `app_user`'s search_path is
    // `public, extensions` — an unqualified name is not the table the DDL
    // created.
    media_route_module_key(module, "const PARENT_TABLE: &str = \"core.person\";");
    media_route_module_key(module, "const CHILD_TABLE: &str = \"core.person_avatar\";");
    media_route_module_key(
        module,
        "const PARENT_TENANT_COLUMN: Option<&str> = Some(\"platform_organization_id\");",
    );
    // Issue #699: the default fixture domain is auditable (no `auditable`
    // key, so the DDL's default ON applies) and not append-only — so the
    // owning table carries `deleted_at` and the gate must filter it.
    media_route_module_key(module, "const PARENT_SOFT_DELETE: bool = true;");
    media_route_module_key(module, "        PARENT_SOFT_DELETE,\n");

    // Every child statement carries the tenant predicate (the child table
    // inherits the parent's tenant column): upload DELETE + INSERT column,
    // display SELECT, remove SELECT + DELETE.
    assert_eq!(
        module.matches("platform_organization_id = $2").count(),
        4,
        "the child statements must carry the tenant predicate:\n{module}"
    );
    assert!(
        module.contains("(person_id, platform_organization_id, url, mime_type)"),
        "the INSERT must set the inherited tenant column:\n{module}"
    );

    // No bare SQL interpolation of the parent id survives.
    assert!(
        !module.contains("WHERE person_id = '"),
        "the parent predicate must be a bound parameter:\n{module}"
    );
    assert!(
        !module.contains("= '{}'"),
        "no quoted-literal SQL interpolation remains:\n{module}"
    );
}

#[tokio::test]
async fn every_media_handler_gates_the_parent_before_the_storage_slot() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");

    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];
    for handler in [
        "pub async fn upload(",
        "pub async fn display(",
        "pub async fn remove(",
    ] {
        let start = media_route_module_key(module, handler);
        let body = &module[start..];
        let gate = body
            .find("scoped_tx(&state, &api_key_info, id)")
            .unwrap_or_else(|| {
                panic!("{handler} must resolve the parent under the session bundle:\n{module}")
            });
        let slot = body
            .find("_media_storage")
            .unwrap_or_else(|| panic!("{handler} must consult the storage slot:\n{module}"));
        assert!(
            gate < slot,
            "{handler} must gate the parent BEFORE the storage-slot 503 — a foreign id \
             must 404 even where no provider is configured:\n{module}"
        );
    }
}

#[tokio::test]
async fn presigned_media_route_gates_the_parent_before_minting() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"presigned\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    ))
    .expect("toml");

    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];
    let start = media_route_module_key(module, "pub async fn presign(");
    let body = &module[start..];
    let gate = body
        .find("scoped_tx(&state, &api_key_info, id)")
        .unwrap_or_else(|| {
            panic!("presign must gate the parent under the session bundle:\n{module}")
        });
    let slot = body
        .find("_media_storage")
        .unwrap_or_else(|| panic!("presign must consult the storage slot:\n{module}"));
    assert!(gate < slot, "presign gates before minting:\n{module}");
}

#[tokio::test]
async fn media_module_doc_claim_matches_the_emitted_code() {
    // The #690 defect class: the module doc claimed "org-scoped from the
    // session bundle" while the code did not implement it. Pin the claim
    // AND the code together so a doc-only pass cannot recur.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    let doc_end = media_route_module_key(module, "use axum::");
    let doc = &module[..doc_end];
    assert!(
        doc.contains("session bundle"),
        "the doc must state the session-bundle contract it implements:\n{doc}"
    );
    assert!(
        doc.contains("404"),
        "the doc must state the 404-on-foreign-parent rule:\n{doc}"
    );
    assert!(
        module.contains("crate::media::session::begin_scoped"),
        "the documented contract must be the implemented one:\n{module}"
    );
    // The round-2 claims: two short upload transactions, and — since #700
    // retired the zeros tolerance — the exact-org child predicate it now
    // implements, with the issue that fixed the writers named.
    for claim in [
        "TWO short transactions",
        "filters on the session organization exactly",
        "#700",
    ] {
        assert!(doc.contains(claim), "the doc must state `{claim}`:\n{doc}");
    }
    assert!(
        !doc.contains("tolerate the legacy zeros tenant"),
        "the retired tolerance is still claimed:\n{doc}"
    );
}

// ── Issue #690 round 2 → retired by #700 ──────────────────────────────

/// #690 tolerated the DDL's zero tenant in the child statements because no
/// writer of a child row bound the tenant column. **#700 fixed the writers**
/// (the child tables now carry the parent's `set_org_id_from_session`
/// trigger, and each child's RLS migration backfills the legacy rows and
/// stops tolerating zeros), so the workaround is gone: every child statement
/// filters on the session organization EXACTLY, and still never on the
/// parent FK alone.
#[tokio::test]
async fn media_child_statements_filter_on_the_exact_session_tenant() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    assert!(
        !module.contains("LEGACY_ZERO_TENANT"),
        "the #700 workaround is retired:\n{module}"
    );
    assert!(
        !module.contains("00000000-0000-0000-0000-000000000000"),
        "no media statement names the zero tenant:\n{module}"
    );
    // Upload DELETE, display SELECT, remove SELECT, remove DELETE — all four
    // carry the exact-org predicate.
    assert_eq!(
        module.matches("platform_organization_id = $2").count(),
        4,
        "all four child statements filter on the session tenant:\n{module}"
    );
    // No child statement filters on the parent FK alone.
    assert_eq!(
        module
            .matches("WHERE person_id = $1 AND platform_organization_id = $2")
            .count(),
        4,
        "every child WHERE keeps the tenant predicate:\n{module}"
    );
    // The parent gate is what makes an unscoped child statement safe, and it
    // is unchanged by #700.
    assert!(
        module.contains("crate::media::session::parent_exists("),
        "the parent gate stays:\n{module}"
    );
    // The tenancy rule is three copies that must move together (not "one
    // rule the gate and the table cannot disagree about"), and the schema
    // source divergence is #702.
    for copy in ["db/entity.rs", "db/ddl/query.rs", "persistence.rs"] {
        assert!(
            module.contains(copy),
            "the const doc names `{copy}` as a copy that must move together:\n{module}"
        );
    }
    assert!(
        !module.contains("cannot disagree"),
        "the old overclaim is gone:\n{module}"
    );
    assert!(
        module.contains("#702"),
        "the schema-source divergence names its issue:\n{module}"
    );
}

/// The upload handler must not hold a pooled connection across the multipart
/// body read and the provider PUT: a short gate transaction, the network
/// work, then a second short scoped transaction that RE-RUNS the gate and
/// writes the row.
#[tokio::test]
async fn media_upload_uses_two_short_scoped_transactions() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    // upload has 2, display 1, remove 1.
    assert_eq!(
        module
            .matches("scoped_tx(&state, &api_key_info, id)")
            .count(),
        4,
        "upload opens TWO scoped transactions (gate + write):\n{module}"
    );

    let start = media_route_module_key(module, "pub async fn upload(");
    let body = &module[start..];
    let first_gate = body
        .find("scoped_tx(&state, &api_key_info, id)")
        .expect("first gate");
    let next_field = body.find("next_field").expect("body read");
    let put = body.find(".put(").expect("provider put");
    let second_gate = body[first_gate + 1..]
        .find("scoped_tx(&state, &api_key_info, id)")
        .expect("second gate")
        + first_gate
        + 1;
    let delete = body.find("let delete = Statement::").expect("row write");
    assert!(
        first_gate < next_field && next_field < put && put < second_gate && second_gate < delete,
        "gate -> body/provider work -> re-gate -> row write:\n{body}"
    );
}

/// DB-tier denials (the RESTRICTIVE `scope_enforced_*` P0403 payloads and
/// org-isolation write violations) are authorization failures: 403, never a
/// 500 with the DB text echoed. Mirrors `errors.tera::from_repo_err`.
#[tokio::test]
async fn media_db_denials_map_to_forbidden_not_internal() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    assert!(module.contains("fn db_err("), "{module}");
    for marker in [
        "INSUFFICIENT_SCOPE",
        "ROLE_FORBIDDEN",
        "violates row-level security policy",
    ] {
        assert!(
            module.contains(marker),
            "the classifier names `{marker}`:\n{module}"
        );
    }
    assert!(
        module.contains("AppError::forbidden("),
        "denials map to 403:\n{module}"
    );
    // Every DB map_err goes through the classifier (storage/multipart errors
    // keep their own envelopes).
    assert!(
        !module.contains("AppError::internal(format!(\"failed to"),
        "no DB failure bypasses db_err:\n{module}"
    );
}

/// The INSERT's bound-value ORDER is part of the contract: swapping the org
/// bind for the user id would keep every other pin green.
#[tokio::test]
async fn media_upload_insert_binds_the_tenant_second() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    let start = media_route_module_key(module, "let insert = Statement::from_sql_and_values(");
    let rest = &module[start..];
    let end = rest.find("\n    );").expect("insert statement end") + start;
    let insert = &module[start..end];
    assert!(
        insert.contains("VALUES ($1, $2, $3, $4)"),
        "tenant-scoped INSERT arity:\n{insert}"
    );
    assert!(
        insert.contains("(person_id, platform_organization_id, url, mime_type)"),
        "the INSERT names the inherited tenant column:\n{insert}"
    );
    let mut cursor = 0;
    for (idx, needle) in [
        "Value::from(id)",
        "Value::from(api_key_info.organization_id)",
        "Value::from(stored.key.clone())",
        "Value::from(mime_type.clone())",
    ]
    .iter()
    .enumerate()
    {
        let pos = insert[cursor..]
            .find(needle)
            .unwrap_or_else(|| panic!("bind {idx} (`{needle}`) missing:\n{insert}"))
            + cursor;
        cursor = pos + needle.len();
    }
}

/// A GLOBAL owning table renders the unscoped variant — the branch had no
/// render pin at all, so a typo in the template's `{% else %}` shipped
/// silently. Rendered directly so no graph fixture is needed.
#[tokio::test]
async fn global_parent_media_route_renders_the_unscoped_variant() {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let project = ProjectConfig::default();
    let mut ctx = serde_json::json!({
        "entity_name": "OrgType",
        "entity_snake": "org",
        "route_prefix": "/api/v1/core/org",
        "route_parents": [],
        "nesting_depth": 0,
        "path_extractor": "Path(id): Path<Uuid>,",
        "scoped_tx_call": "scoped_tx(&state, &api_key_info, id)",
        "parent_link_column": serde_json::Value::Null,
        "entity_module": "core_org",
        "domain": "core",
        "media_field_name": "logo",
        "media_accept": ["image/png"],
        "schema": "core",
        "child_table": "org_logo",
        "parent_fk_column": "org_id",
        "parent_tenant_column": serde_json::Value::Null,
        "parent_soft_delete": true,
        "url_column": "url",
        "mime_column": "mime_type",
        "mode_presigned": false,
        "max_bytes": 5242880u64,
        "key_format": "{org}/core/org/{id}/logo",
        "key_org_first": true,
        "key_prefix_format": "{org}/",
        "display_ttl_secs": 300u64,
        "presign_ttl_secs": 900u64,
    });
    let rendered = codegraph_generate::render_template_with_project(
        &tera,
        "api/media_route.tera",
        &ctx,
        &project,
    )
    .expect("global-parent render");

    assert!(
        rendered.contains("const PARENT_TENANT_COLUMN: Option<&str> = None;"),
        "the global branch declares None:\n{rendered}"
    );
    assert!(
        rendered.contains("GLOBAL owning table") && rendered.contains("deliberately unscoped"),
        "the unscoped variant carries its safety comment:\n{rendered}"
    );
    // No tenant predicate and no zeros branch on a global parent.
    assert!(
        !rendered.contains("platform_organization_id"),
        "a global parent emits no tenant column at all:\n{rendered}"
    );
    assert!(
        !rendered.contains("LEGACY_ZERO_TENANT"),
        "the zeros tolerance is tenant-scoped only:\n{rendered}"
    );
    // The gate still runs, with the None tenancy.
    assert!(
        rendered.contains("PARENT_TENANT_COLUMN,"),
        "the parent gate passes None:\n{rendered}"
    );
    // Issue #699: an auditable parent filters soft-deleted rows.
    assert!(
        rendered.contains("const PARENT_SOFT_DELETE: bool = true;"),
        "an auditable parent declares the filter (#699):\n{rendered}"
    );
    assert!(
        rendered.contains("        PARENT_SOFT_DELETE,\n"),
        "the gate passes the soft-delete decision (#699):\n{rendered}"
    );

    // …and a NON-auditable parent (the DDL emits no `deleted_at`) renders the
    // flag OFF, so the gate can never name a column the table lacks.
    ctx["parent_soft_delete"] = serde_json::json!(false);
    let unfiltered = codegraph_generate::render_template_with_project(
        &tera,
        "api/media_route.tera",
        &ctx,
        &project,
    )
    .expect("non-auditable render");
    assert!(
        unfiltered.contains("const PARENT_SOFT_DELETE: bool = false;"),
        "a non-auditable parent declares no filter (#699):\n{unfiltered}"
    );
    assert!(
        unfiltered.contains("        PARENT_SOFT_DELETE,\n"),
        "the gate still receives the (false) decision (#699):\n{unfiltered}"
    );
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
        out, false, false, false, false, false, false, false, false, false, false, "parallel",
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

fn scaffold_file<'a>(files: &'a [GeneratedFile], out: &std::path::Path, rel: &str) -> &'a str {
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
    let files = run_scaffold(&engine, &config, out.path())
        .await
        .expect("scaffold");

    let app_state = scaffold_file(&files, out.path(), "src/app_state.rs");
    assert!(
        app_state.contains(
            "pub core_media_storage: Option<std::sync::Arc<dyn crate::media::MediaStorage>>"
        ),
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
    let files = run_scaffold(&engine, &config, out.path())
        .await
        .expect("scaffold");

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
    let _ = out
        .path()
        .join("migrations")
        .join("023000_media_storage.sql");
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
        !files.iter().any(|f| f.path.ends_with("media_storage.sql")),
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
    let generator = codegraph_generate::api::handler::HandlerGenerator::new(out.path())
        .with_rules_guards(false);
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
    let no_content_pos = handler
        .rfind("StatusCode::NO_CONTENT")
        .expect("204 present");
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
            "src/media/session.rs",
            "src/media/supabase.rs",
        ],
        "exact file set: trait + noop + adapter + transport + session + storage bootstrap"
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
    assert!(
        !supabase_rs.contains("AWS4-HMAC-SHA256"),
        "no SigV4 in supabase"
    );
    assert!(
        !supabase_rs.contains("r2"),
        "no r2 vocabulary in supabase adapter",
    );
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
    assert!(
        http_rs.contains("reqwest"),
        "native leg rides the app's reqwest"
    );
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
            "hurl/09_media.hurl",
            "src/media/http.rs",
            "src/media/mod.rs",
            "src/media/noop.rs",
            "src/media/r2.rs",
            "src/media/session.rs",
        ],
        "r2 plan emits the r2 adapter + transport + session + hurl contract (no supabase module, no migration)"
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
    // SigV4 region is CONFIGURABLE (issue #695 Stage 0): R2 answers to "auto"
    // but any other S3-compatible store verifies the credential scope's
    // region, so `R2_S3_REGION` must reach every scope construction — the
    // header-signed request, the presigned PUT, the string-to-sign and the
    // signing key. A hardcoded `const SIGV4_REGION` in any of those is the
    // failure mode this pins against.
    assert!(
        r2_rs.contains(r#"std::env::var("R2_S3_REGION")"#),
        "the region is read from the environment: {r2_rs}"
    );
    assert!(
        r2_rs.contains(r#".unwrap_or_else(|| SIGV4_REGION.to_string())"#),
        "with R2's \"auto\" as the documented default: {r2_rs}"
    );
    assert_eq!(
        r2_rs.matches("region: SIGV4_REGION.to_string()").count(),
        0,
        "no signing site hardcodes the region: {r2_rs}"
    );
    assert!(
        r2_rs.contains("date_stamp, self.region, SIGV4_SERVICE"),
        "the presign credential scope uses the instance region: {r2_rs}"
    );
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
            "src/media/session.rs",
            "src/media/supabase.rs",
        ],
        "exact file set: trait + noop + provider adapter + native transport + session + storage bootstrap"
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
    // Issue #690: the session module the routes ride is declared with the
    // plane (native-only — SeaORM cannot link wasm32).
    assert!(
        mod_rs.contains("#[cfg(not(target_arch = \"wasm32\"))]\npub mod session;"),
        "the session module is declared (native-only): {mod_rs}"
    );
    // PR-B (#630): the supabase plan emits its adapter + the transport seam.
    assert!(
        mod_rs.contains("pub mod supabase;"),
        "supabase adapter module"
    );
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
    assert!(out.path().join("src/media/mod.rs").exists() || files.len() == 6);
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
    assert!(
        !out.path().join("src/media/session.rs").exists(),
        "byte-identity: a media-less tree must not grow the session helper"
    );
}

#[tokio::test]
async fn media_plane_emits_the_session_bundle_and_a_bound_parent_lookup() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
    let session_rs = &files
        .iter()
        .find(|f| f.path.ends_with("src/media/session.rs"))
        .expect("the session module")
        .content;
    // The query plane's five keys + the role flip, one simple-query payload.
    for key in [
        "app.current_api_key",
        "app.current_api_key_id",
        "app.organization_id",
        "app.user_id",
        "app.role",
        "SET LOCAL ROLE app_user",
        "execute_unprepared",
        "db.begin()",
    ] {
        assert!(
            session_rs.contains(key),
            "session bundle pin `{key}`: {session_rs}"
        );
    }
    // Bound parameters only: the parent lookup and org are $1/$2 values.
    // Issue #699: the predicate carries `{live_only}` — the soft-delete
    // filter the route module selects with its generation-time constant.
    assert!(
        session_rs
            .contains("SELECT 1 FROM {table} WHERE id = $1 AND {column} = $2{live_only} LIMIT 1"),
        "tenant-scoped parent lookup: {session_rs}"
    );
    assert!(
        session_rs.contains("SELECT 1 FROM {table} WHERE id = $1{live_only} LIMIT 1"),
        "global-parent lookup: {session_rs}"
    );
    assert!(
        session_rs.contains("let live_only = if soft_delete {")
            && session_rs.contains("\" AND deleted_at IS NULL\""),
        "the gate must treat a soft-deleted parent as absent (#699): {session_rs}"
    );
    assert!(
        session_rs.contains("soft_delete: bool,"),
        "the DDL's auditable decision arrives as a generation-time parameter (#699): {session_rs}"
    );
    assert!(
        session_rs.contains("Statement::from_sql_and_values")
            && session_rs.contains("Value::from(id)"),
        "the id/org travel as bound parameters: {session_rs}"
    );
}

#[tokio::test]
async fn nested_session_pins_the_linked_gate_sql_and_bind_order() {
    // Issue #722 review: the (a)-(f) pins cover the CALL of
    // `parent_exists_linked`, but nothing pinned its BODY — weakening the SQL
    // to an existence-only predicate (`id = $1 AND tenant = $2`) left all 56
    // node-free pins green and only the nightly runtime gate caught it. Pin
    // the predicate and the bind order here so PR CI sees the ownership
    // contract too.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    let config =
        parse_domain_config_str(&media_toml_with_nested_child("through-app")).expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("files");
    let session_rs = &files
        .iter()
        .find(|f| f.path.ends_with("src/media/session.rs"))
        .expect("the session module")
        .content;

    // Both tenancy variants: the linked predicate is `id AND link AND org`.
    for sql in [
        "SELECT 1 FROM {table} WHERE id = $1 AND {link_column} = $2 AND {column} = $3{live_only} LIMIT 1",
        "SELECT 1 FROM {table} WHERE id = $1 AND {link_column} = $2{live_only} LIMIT 1",
    ] {
        assert!(
            session_rs.contains(sql),
            "the linked gate must carry the ownership predicate `{sql}`: {session_rs}"
        );
    }
    // …and never degrade to the existence-only shape. (The plain
    // `parent_exists` in the same file legitimately carries it — the check is
    // scoped to the linked function below.)
    let body = session_rs
        .split("pub async fn parent_exists_linked<")
        .nth(1)
        .expect("the linked gate function");
    assert!(
        !body.contains("SELECT 1 FROM {table} WHERE id = $1 AND {column} = $2 LIMIT 1"),
        "the linked gate must not collapse to `id + org` (an IDOR): {body}"
    );
    let normalized = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        normalized.contains(
            "Value::from(id), sea_orm::Value::from(parent_id), \
             sea_orm::Value::from(organization_id),"
        ),
        "the binds are (id, parent_id, organization_id) in that order — a swap would \
         still compile: {normalized}"
    );
    assert!(
        normalized.contains("Value::from(id), sea_orm::Value::from(parent_id), ],"),
        "the GLOBAL arm binds (id, parent_id) — no org predicate for a global parent: \
         {normalized}"
    );
    assert_eq!(
        body.matches("Value::from(").count(),
        5,
        "tenant arm (id, parent_id, org) + global arm (id, parent_id): {body}"
    );
}

/// Issue #722 review: a depth-3 media entity is refused at generation time —
/// the emitted extractor binds at most three ids (the CRUD plane's limit), so
/// a deeper route could never be served (always 400).
#[tokio::test]
async fn depth_three_media_nesting_is_a_config_error() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_grandchild(&engine).await;
    // A child of the grandchild: `AddressType` under `ContactType` under
    // `PersonType`.
    db_ingest_depth_three(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{}\n[domains.core.entity_config.RegionType]\nrole = \"child\"\nparent = \"AddressType\"\nparent_ref = \"address_id\"\npath_segment = \"regions\"\n",
        media_toml_with_nested_grandchild()
    ))
    .expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let err = run_generator(&engine, &config, out.path())
        .await
        .expect_err("depth 3 must fail closed");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("RegionType") && msg.contains("3 levels deep"),
        "the config error names the entity and the depth: {msg}"
    );
}

/// A depth-3 entity with a media field (`RegionType` under `AddressType`).
async fn db_ingest_depth_three(db: &(impl GraphIngestor + GraphQuerier)) {
    db.ingest_schema(&schema(
        "RegionType",
        "RegionType",
        "entity",
        "region",
        true,
    ))
    .await
    .expect("region schema");
    for p in [
        prop("id", RefClassificationKind::EntityReference, None),
        prop(
            "avatar",
            RefClassificationKind::ValueObject,
            Some("MediaReferenceType"),
        ),
    ] {
        db.ingest_property("RegionType", "RegionType", &p)
            .await
            .expect("region property");
    }
    db.ingest_edge(
        "avatar::RegionType",
        "MediaReferenceType",
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("region references edge");
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

// ── #691: the media contract addresses the route the router serves ───────

/// The fixture with a config-declared path segment that differs from BOTH
/// the table name (`person`) and the title fallback (`persontype`). A pin
/// whose fixture used `person` for all three would pass on the broken
/// builder — this is the discriminating shape (issue #691).
fn media_toml_with_path_segment(mode: &str) -> String {
    // `api_version = "v2"` proves the contract reads the CONFIG rather than a
    // hardcoded `v1` (the sibling contracts all honour `defaults.api_version`).
    let base = media_toml("supabase", mode).replace(
        "type_suffix = \"Type\"",
        "type_suffix = \"Type\"\napi_version = \"v2\"",
    );
    format!("{base}\n[domains.core.entity_config.PersonType]\npath_segment = \"people\"\n")
}

#[tokio::test]
async fn media_hurl_contract_url_matches_the_router_route() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config =
        parse_domain_config_str(&media_toml_with_path_segment("through-app")).expect("toml");

    // The authority: the segment `router.tera` nests the entity routes under,
    // and the media route shape inside it.
    let router = rendered_router_media_block(&engine, &config).await;
    assert!(
        router.contains(r#".nest("/people", person_routes())"#),
        "the router nests the entity under the CONFIG segment: {router}"
    );
    assert!(
        router.contains(r#""/{id}/avatar""#),
        "the media route shape is the id then the field: {router}"
    );

    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("media generation");
    let hurl = files
        .iter()
        .find(|f| f.path.ends_with("09_media.hurl"))
        .expect("the media hurl contract is emitted");

    // A valid hurl URL needs a scheme: every sibling contract is
    // `{{base_url}}/api/...` (hurl rejects a scheme-less path outright).
    let expected = format!(
        "{{{{base_url}}}}/api/v2/core/people/{}/avatar",
        codegraph_generate::media::MEDIA_CONTRACT_MISSING_ID
    );
    assert!(
        hurl.content.contains(&format!("PUT {expected}")),
        "the contract addresses the router's route ({expected}): {}",
        hurl.content
    );
    assert!(
        !hurl
            .content
            .lines()
            .any(|l| l.starts_with("PUT /") || l.starts_with("GET /") || l.starts_with("POST /")),
        "no scheme-less request line (hurl: Invalid URL — Missing scheme): {}",
        hurl.content
    );
    // The three defects #691 names, each pinned so none can come back:
    assert!(
        !hurl.content.contains("/core/person/"),
        "no `pg_table_name` segment (the title-derived segment drops the underscore): {}",
        hurl.content
    );
    assert!(
        !hurl.content.contains("/avatar/"),
        "the field segment must not appear where the id belongs: {}",
        hurl.content
    );
    // Every REQUEST line carries the concrete sentinel — a bare `{id}`
    // placeholder is not a parseable uuid at the route (`Path<Uuid>`).
    for line in hurl
        .content
        .lines()
        .filter(|l| l.starts_with("PUT ") || l.starts_with("POST ") || l.starts_with("GET "))
    {
        assert!(
            !line.contains("{id}"),
            "a request line must carry the sentinel, not a placeholder: {line}"
        );
        assert!(
            line.contains(&codegraph_generate::media::MEDIA_CONTRACT_MISSING_ID.to_string()),
            "every request addresses the sentinel id: {line}"
        );
    }
    // Both legs are provider-free (#690 gates the parent before the storage
    // slot): the authn 401 and the ownership 404.
    assert!(
        hurl.content.contains("HTTP 401") && hurl.content.contains("UNAUTHORIZED"),
        "{}",
        hurl.content
    );
    assert!(
        hurl.content.contains("Authorization: Bearer {{api_key}}")
            && hurl.content.contains("HTTP 404"),
        "the ownership leg carries credentials and expects 404, never 403: {}",
        hurl.content
    );

    // The utoipa contract must document the same path the router serves.
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];
    assert!(
        module.contains(r#"path = "/api/v1/core/people/{id}/avatar""#),
        "the documented path is the router's, on the project's api version: {module}"
    );
    assert!(
        !module.contains(r#"path = "/api/v1/core/person/"#),
        "never the table name: {module}"
    );
    assert!(
        !module.contains("/api/{version}/"),
        "no literal `{{version}}` placeholder in the annotation: {module}"
    );
}

// ── #713: a child entity's media field nests under its parent route ──────

/// The fixture #713 names: a ROOT (`PersonType`) and a CHILD
/// (`ContactType`) whose title sorts FIRST — so the contract's anchor,
/// `plan.fields.first()`, is the child's field — plus a media field on the
/// child. The configured segments differ from both the table names and the
/// title fallbacks, so a table-derived or role-blind builder cannot pass.
async fn ingest_parent_and_child(db: &(impl GraphIngestor + GraphQuerier)) {
    ingest_model(db).await;
    db.ingest_schema(&schema(
        "ContactType",
        "ContactType",
        "entity",
        "contact",
        true,
    ))
    .await
    .expect("contact schema");
    for p in [
        prop("id", RefClassificationKind::EntityReference, None),
        prop(
            "avatar",
            RefClassificationKind::ValueObject,
            Some("MediaReferenceType"),
        ),
    ] {
        db.ingest_property("ContactType", "ContactType", &p)
            .await
            .expect("contact property");
    }
    db.ingest_edge(
        "avatar::ContactType",
        "MediaReferenceType",
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("contact references edge");
}

fn media_toml_with_nested_child(mode: &str) -> String {
    let base = media_toml("supabase", mode).replace(
        "type_suffix = \"Type\"",
        "type_suffix = \"Type\"\napi_version = \"v2\"",
    );
    format!(
        "{base}\n\
         [domains.core.entity_config.PersonType]\n\
         path_segment = \"persons\"\n\n\
         [domains.core.entity_config.ContactType]\n\
         role = \"child\"\n\
         parent = \"PersonType\"\n\
         parent_ref = \"person_id\"\n\
         path_segment = \"contacts\"\n"
    )
}

#[tokio::test]
async fn child_media_field_contract_nests_under_the_parent_route() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    let config =
        parse_domain_config_str(&media_toml_with_nested_child("through-app")).expect("toml");

    // The authority: what `router.tera` actually mounts. The root nests at
    // the domain level; the child nests INSIDE the root's router under
    // `/{parent_param}/{child_segment}` (issue #713).
    let router = rendered_router_media_block_for(
        &engine,
        &config,
        &["PersonType".into(), "ContactType".into()],
    )
    .await;
    assert!(
        router.contains(r#".nest("/persons", person_routes())"#),
        "the root nests under its CONFIG segment: {router}"
    );
    assert!(
        router.contains(r#".nest("/{person_id}/contacts", contact_routes())"#),
        "the child nests under the parent's named id and its own segment: {router}"
    );
    assert!(
        router.contains("media_contact_avatar::upload"),
        "the child's media handler is mounted inside the child's router: {router}"
    );

    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("media generation");
    let hurl = files
        .iter()
        .find(|f| f.path.ends_with("09_media.hurl"))
        .expect("the media hurl contract is emitted");

    // `ContactType` sorts before `PersonType`, so the contract anchors on the
    // CHILD's field — and must address the NESTED route the router serves.
    let sentinel = codegraph_generate::media::MEDIA_CONTRACT_MISSING_ID;
    let expected =
        format!("{{{{base_url}}}}/api/v2/core/persons/{sentinel}/contacts/{sentinel}/avatar");
    assert!(
        hurl.content.contains(&format!("PUT {expected}")),
        "the contract addresses the router's nested route ({expected}): {}",
        hurl.content
    );
    // The pre-#713 flat shape (the child at the domain level) must be gone.
    assert!(
        !hurl
            .content
            .contains(&format!("/core/contacts/{sentinel}/avatar")),
        "no flat child path — the router nests it: {}",
        hurl.content
    );
    // Every request line is a CONCRETE URL: no `{parent_id}`/`{id}`
    // placeholder survives into the contract (hurl does not template paths).
    let request_lines: Vec<&str> = hurl
        .content
        .lines()
        .filter(|l| l.starts_with("PUT ") || l.starts_with("POST ") || l.starts_with("GET "))
        .collect();
    assert_eq!(
        request_lines.len(),
        2,
        "the through-app contract's two provider-free legs: {:?}",
        request_lines
    );
    for line in &request_lines {
        assert!(
            line.matches(sentinel).count() == 2,
            "both the parent anchor and the entity id are concrete: {line}"
        );
    }
    // #722: the emitted file must no longer claim the nested route is dead —
    // the handlers now extract the ancestor tuple and prove ownership.
    assert!(
        !hurl.content.contains("not live"),
        "the child contract no longer claims the route is dead: {}",
        hurl.content
    );

    // The utoipa contract documents the SAME nested path (the plan's one
    // derivation), and names the parent path parameter it introduces.
    let modules = run_media_route_generator_for(&engine, &config, "ContactType").await;
    let module = &modules[0];
    assert!(
        module.contains(r#"path = "/api/v1/core/persons/{person_id}/contacts/{id}/avatar""#),
        "the documented path is the router's nested route: {module}"
    );
    assert!(
        module.contains(r#"("person_id" = Uuid, Path"#),
        "the parent path parameter is documented: {module}"
    );
    assert!(
        !module.contains(r#"path = "/api/v1/core/contacts/{id}/avatar""#),
        "never the flat child path: {module}"
    );
}

// ── #722: a nested media handler extracts its ancestors and gates ownership ─
//
// The #713 contract fix left the EXTRACTOR flat: `Path(id): Path<Uuid>` on a
// route mounted as `.nest("/{person_id}/contacts", …)` — axum's
// `PathDeserializer` rejects a single value when the route carries more than
// one url param, so every child media route answered 400. And the extractor
// alone is an IDOR: the parent gate only proves the row exists.

#[tokio::test]
async fn nested_media_handler_extracts_the_ancestor_tuple_before_the_gate() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    let config =
        parse_domain_config_str(&media_toml_with_nested_child("through-app")).expect("toml");

    let modules = run_media_route_generator_for(&engine, &config, "ContactType").await;
    assert_eq!(modules.len(), 1, "one module per media field");
    let module = &modules[0];

    // (a) The extractor mirrors the CRUD plane's `ancestor_path_params`:
    // depth 1 → `(parent_id, id)`.
    assert!(
        module.contains("Path((parent_id, id)): Path<(Uuid, Uuid)>,"),
        "the child handler must extract the URL parent alongside id: {module}"
    );
    assert!(
        !module.contains("Path(id): Path<Uuid>,"),
        "the flat extractor is the #722 400 defect: {module}"
    );
    for handler in [
        "pub async fn upload(",
        "pub async fn display(",
        "pub async fn remove(",
    ] {
        let start = media_route_module_key(module, handler);
        let body = &module[start..];
        let extractor = body
            .find("Path((parent_id, id)): Path<(Uuid, Uuid)>,")
            .unwrap_or_else(|| panic!("{handler} must bind the ancestor tuple: {module}"));
        let gate = body
            .find("scoped_tx(&state, &api_key_info, id, parent_id)")
            .unwrap_or_else(|| {
                panic!("{handler} must pass the URL parent to the ownership gate: {module}")
            });
        // Extractors are function parameters: they lexically precede the body
        // (and therefore the gate call). Pinned positionally, not by wording.
        assert!(
            extractor < gate,
            "{handler}: the ancestor extractor precedes the parent gate: {module}"
        );
    }

    // (b) The gate carries the parent id AND the link column — the ownership
    // predicate (`id` must belong to `parent_id`), never a wider existence
    // check.
    assert!(
        module.contains(r#"const PARENT_LINK_COLUMN: &str = "person_id";"#),
        "the child's configured `parent_ref` is the gate's link column: {module}"
    );
    media_route_module_key(module, "crate::media::session::parent_exists_linked(");
    assert!(
        !module.contains("crate::media::session::parent_exists("),
        "a nested route must never fall back to the existence-only gate: {module}"
    );
    let gate_call = module
        .split("session::parent_exists_linked(")
        .nth(1)
        .expect("parent_exists_linked call");
    let args = gate_call.split(')').next().unwrap_or_default();
    let lines: Vec<&str> = args
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(
        lines,
        vec![
            "&tx,",
            "PARENT_TABLE,",
            "PARENT_TENANT_COLUMN,",
            "PARENT_LINK_COLUMN,",
            "id,",
            "parent_id,",
            "api_key_info.organization_id,",
            "PARENT_SOFT_DELETE,",
        ],
        "parent_exists_linked(conn, table, tenant, link, id, parent_id, session_org, soft_delete): {module}"
    );

    // (f) The utoipa `params` block documents the SAME order the extractor
    // binds: the ancestor first, the owning id last.
    let ancestor = media_route_module_key(module, r#"("person_id" = Uuid, Path"#);
    let owning = media_route_module_key(module, r#"("id" = Uuid, Path"#);
    assert!(
        ancestor < owning,
        "utoipa documents the ancestor before the owning id, as the extractor binds: {module}"
    );
    // …and the documented path is still the router's nested route.
    assert!(
        module.contains(r#"path = "/api/v1/core/persons/{person_id}/contacts/{id}/avatar""#),
        "the documented path is the nested route: {module}"
    );
}

/// A grandchild beneath `ContactType` — the depth-2 shape (issue #722, the
/// router's recursive `.nest`).
async fn ingest_grandchild(db: &(impl GraphIngestor + GraphQuerier)) {
    ingest_parent_and_child(db).await;
    db.ingest_schema(&schema(
        "AddressType",
        "AddressType",
        "entity",
        "address",
        true,
    ))
    .await
    .expect("address schema");
    for p in [
        prop("id", RefClassificationKind::EntityReference, None),
        prop(
            "avatar",
            RefClassificationKind::ValueObject,
            Some("MediaReferenceType"),
        ),
    ] {
        db.ingest_property("AddressType", "AddressType", &p)
            .await
            .expect("address property");
    }
    db.ingest_edge(
        "avatar::AddressType",
        "MediaReferenceType",
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("address references edge");
}

fn media_toml_with_nested_grandchild() -> String {
    format!(
        "{}\n\
         [domains.core.entity_config.AddressType]\n\
         role = \"child\"\n\
         parent = \"ContactType\"\n\
         parent_ref = \"contact_id\"\n\
         path_segment = \"addresses\"\n",
        media_toml_with_nested_child("through-app")
    )
}

#[tokio::test]
async fn depth_two_media_handler_extracts_the_grandparent_tuple() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_grandchild(&engine).await;
    let config = parse_domain_config_str(&media_toml_with_nested_grandchild()).expect("toml");
    let titles = [
        "PersonType".to_string(),
        "ContactType".to_string(),
        "AddressType".to_string(),
    ];

    // The router's recursive `.nest` mounts the depth-2 route — the pin the
    // brief asks for even though no consumer uses it today.
    let router = rendered_router_media_block_for(&engine, &config, &titles).await;
    assert!(
        router.contains(r#".nest("/{person_id}/contacts", contact_routes())"#),
        "the child router nests under the root: {router}"
    );
    assert!(
        router.contains(r#".nest("/{contact_id}/addresses", address_routes())"#),
        "the grandchild nests under the child (depth 2): {router}"
    );

    let modules = run_media_route_generator_for(&engine, &config, "AddressType").await;
    let module = &modules[0];
    assert!(
        module.contains("Path((_grandparent_id, parent_id, id)): Path<(Uuid, Uuid, Uuid)>,"),
        "depth 2 binds the grandparent (unused, `_`-prefixed) then the parent: {module}"
    );
    assert!(
        module.contains(r#"const PARENT_LINK_COLUMN: &str = "contact_id";"#),
        "the IMMEDIATE parent's FK is the ownership predicate: {module}"
    );
    // Depth 2 checks the immediate parent only — the grandparent id addresses
    // the route and is not re-verified (mirrors `find_by_id_scoped`).
    assert!(
        module.contains("scoped_tx(&state, &api_key_info, id, parent_id)"),
        "the gate receives the immediate parent id, never the grandparent: {module}"
    );
    let ancestor = media_route_module_key(module, r#"("contact_id" = Uuid, Path"#);
    let grandparent = media_route_module_key(module, r#"("person_id" = Uuid, Path"#);
    let owning = media_route_module_key(module, r#"("id" = Uuid, Path"#);
    assert!(
        grandparent < ancestor && ancestor < owning,
        "utoipa documents grandparent → parent → id, the extractor's order: {module}"
    );
}

#[tokio::test]
async fn nested_media_field_without_a_link_column_is_config_error() {
    // Fail closed: the ancestor extractor without the ownership predicate is
    // an IDOR, so a nested route whose link column cannot be resolved is a
    // hard config error naming the entity, the domain and the fix.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{}\n[domains.core.entity_config.ContactType]\nrole = \"value_object\"\npath_segment = \"contacts\"\n",
        media_toml("supabase", "through-app")
    ))
    .expect("toml");
    let route_parents = vec![codegraph_core::types::MediaRouteParent {
        entity_title: "PersonType".into(),
        path_segment: "persons".into(),
        param_name: "person_id".into(),
    }];
    let mut candidates = None;
    let err = codegraph_generate::media::plan::resolve_media_parent_link_column(
        &engine,
        &config,
        "core",
        "ContactType",
        &route_parents,
        &mut candidates,
    )
    .await
    .expect_err("no link column ⇒ hard error");
    let Error::Config(msg) = err else {
        panic!("expected Error::Config, got {err:?}");
    };
    assert!(msg.contains("ContactType"), "{msg}");
    assert!(msg.contains("core"), "{msg}");
    assert!(msg.contains("parent_ref"), "the fix is named: {msg}");
}

#[tokio::test]
async fn root_media_handler_keeps_the_flat_extractor_and_gate() {
    // Byte-identity shape: a root entity's module must keep the pre-#722
    // single-id extractor and the existence-only parent gate.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator_for(&engine, &config, "PersonType").await;
    let module = &modules[0];
    assert!(
        module.contains("Path(id): Path<Uuid>,"),
        "a root media handler keeps its flat extractor: {module}"
    );
    assert!(
        module.contains("crate::media::session::parent_exists("),
        "a root media handler keeps the existence-only gate: {module}"
    );
    for needle in [
        "parent_exists_linked",
        "PARENT_LINK_COLUMN",
        "parent_id: Uuid",
        "Path((parent_id, id))",
    ] {
        assert!(
            !module.contains(needle),
            "root output must not carry the nesting surface `{needle}`: {module}"
        );
    }
}

/// A modeled scalar ref from the child to a candidate parent — the router's
/// Source-2 nesting trigger.
async fn ingest_contact_ref_candidate(
    db: &(impl GraphIngestor + GraphQuerier),
    property: &str,
    parent_title: &str,
) {
    let p = prop(
        property,
        RefClassificationKind::EntityReference,
        Some(parent_title),
    );
    db.ingest_property("ContactType", "ContactType", &p)
        .await
        .expect("candidate property");
    db.ingest_edge(
        &format!("{property}::ContactType"),
        parent_title,
        EdgeType::ReferencesSchema,
        None,
    )
    .await
    .expect("candidate references edge");
}

fn media_toml_with_value_object_role() -> String {
    let base = media_toml("supabase", "through-app").replace(
        "type_suffix = \"Type\"",
        "type_suffix = \"Type\"\napi_version = \"v2\"",
    );
    format!(
        "{base}\n[domains.core.entity_config.ContactType]\n\
         role = \"value_object\"\npath_segment = \"contacts\"\n"
    )
}

#[tokio::test]
async fn value_object_role_still_nests_like_the_router() {
    // #713 blocker: `role` is `"root" | "child" | "value_object"` and the
    // router nests EVERY explicit non-root role (its Source-2 gate is
    // `role != "root"`), so the media contract must not go flat here.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    ingest_contact_ref_candidate(&engine, "person", "PersonType").await;
    let config = parse_domain_config_str(&media_toml_with_value_object_role()).expect("toml");

    let router = rendered_router_media_block_for(
        &engine,
        &config,
        &["PersonType".into(), "ContactType".into()],
    )
    .await;
    assert!(
        router.contains(r#".nest("/person", person_routes())"#),
        "the root nests at the domain level: {router}"
    );
    assert!(
        router.contains(r#".nest("/{person_id}/contacts", contact_routes())"#),
        "a non-root role nests under the graph candidate: {router}"
    );

    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("media generation");
    let hurl = files
        .iter()
        .find(|f| f.path.ends_with("09_media.hurl"))
        .expect("the media hurl contract is emitted");
    let sentinel = codegraph_generate::media::MEDIA_CONTRACT_MISSING_ID;
    let expected =
        format!("{{{{base_url}}}}/api/v2/core/person/{sentinel}/contacts/{sentinel}/avatar");
    assert!(
        hurl.content.contains(&format!("PUT {expected}")),
        "the contract addresses the router's nested route ({expected}): {}",
        hurl.content
    );
    assert!(
        !hurl
            .content
            .contains(&format!("/core/contacts/{sentinel}/avatar")),
        "never the flat path for a routed non-root role: {}",
        hurl.content
    );
}

#[tokio::test]
async fn media_rejections_answer_413_and_415_not_400() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");

    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];
    // The utoipa block has promised 413/415 since PR-C while the handlers
    // answered 400 (issue #691) — the two planes must agree.
    assert!(
        module.contains("(status = 413,") && module.contains("(status = 415,"),
        "documented: {module}"
    );
    assert!(
        module.contains("AppError::payload_too_large("),
        "oversize → 413: {module}"
    );
    assert!(
        module.contains("AppError::unsupported_media_type("),
        "mime miss → 415: {module}"
    );
    // A malformed multipart body stays 400 (it is not a media-type or a
    // size problem): the class is preserved deliberately.
    assert!(
        module.contains(r#"AppError::bad_request("multipart body carries no file field")"#),
        "malformed body stays 400: {module}"
    );
    // The body limit is LAZY, so it can trip on EITHER multipart read — the
    // envelope's first frame (`next_field()`) or the file body
    // (`field.bytes()`). Both must answer the 413 envelope, never a 400 read
    // failure (issue #710).
    let next_field_block = {
        let start = module
            .find(".next_field()")
            .unwrap_or_else(|| panic!("multipart read: {module}"));
        let end = module[start..]
            .find("let mime_type")
            .map(|i| start + i)
            .unwrap_or_else(|| panic!("the next_field block is bounded: {module}"));
        &module[start..end]
    };
    assert!(
        next_field_block.contains("AppError::payload_too_large("),
        "a limit trip on next_field() maps to 413: {module}"
    );
    let bytes_block = {
        let start = module
            .find("field.bytes().await")
            .unwrap_or_else(|| panic!("file read: {module}"));
        let end = module[start..]
            .find("if data.len() as u64 > ")
            .map(|i| start + i)
            .unwrap_or_else(|| panic!("the bytes block is bounded: {module}"));
        &module[start..end]
    };
    assert!(
        bytes_block.contains("AppError::payload_too_large("),
        "a limit trip on field.bytes() maps to 413: {module}"
    );
    assert_eq!(
        module
            .matches("e.status() == StatusCode::PAYLOAD_TOO_LARGE")
            .count(),
        2,
        "BOTH lazy limit read sites map the 413: {module}"
    );

    // The presign branch answers 415 too (it was the unpinned half).
    let presigned = parse_domain_config_str(&format!(
        "{}\n[domains.core.media.r2]\nbinding = \"MEDIA\"\n",
        media_toml("r2", "presigned")
    ))
    .expect("toml");
    let presign_modules = run_media_route_generator(&engine, &presigned).await;
    let presign_module = &presign_modules[0];
    assert!(
        presign_module.contains("AppError::unsupported_media_type("),
        "presign mime miss → 415: {presign_module}"
    );
    assert!(
        presign_module.contains("(status = 415,"),
        "presign documents 415: {presign_module}"
    );
}

#[tokio::test]
async fn media_child_sql_rides_the_scoped_transaction_and_the_gate_gets_the_session_org() {
    // The two runtime properties the #690 fix depends on, pinned so a
    // refactor cannot silently unwind them: every child statement runs on
    // the SAME transaction that installed the session bundle (never the
    // pool), and the gate is handed the SESSION org — swapping the two Uuid
    // arguments would still compile and still 404/200 the wrong way.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    assert!(
        module.contains("tx.execute_raw(") && module.contains(".query_one_raw("),
        "child statements ride the transaction: {module}"
    );
    assert_eq!(
        module.matches("tx.execute_raw(").count(),
        3,
        "exactly upload's two writes (DELETE + INSERT) and remove's DELETE ride it: {module}"
    );
    assert!(
        !module.contains("state.db.execute_raw(") && !module.contains("state.db.query_one_raw("),
        "no media statement escapes to the pool: {module}"
    );
    assert!(
        module.contains("crate::media::session::parent_exists(\n        &tx,"),
        "the gate runs ON the transaction: {module}"
    );
    // Argument order: the owning id first, the SESSION org second — asserted
    // as an exact adjacent pair, so neither a reindent nor a new `…id,`
    // parameter can satisfy it by accident.
    let gate_call = module
        .split("session::parent_exists(")
        .nth(1)
        .expect("parent_exists call");
    let args = gate_call.split(')').next().unwrap_or_default();
    let pair: Vec<&str> = args
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let id_pos = pair.iter().position(|l| *l == "id,");
    let org_pos = pair
        .iter()
        .position(|l| *l == "api_key_info.organization_id,");
    assert_eq!(
        (id_pos, org_pos),
        (Some(pair.len() - 3), Some(pair.len() - 2)),
        "parent_exists(id, session_org, soft_delete) — id and the org keep their order, \
         with the #699 soft-delete constant LAST: {pair:?}"
    );
    assert_eq!(
        pair.last().copied(),
        Some("PARENT_SOFT_DELETE,"),
        "the soft-delete decision must be the last argument (#699): {pair:?}"
    );
}

#[tokio::test]
async fn presigned_hurl_contract_url_matches_the_router_route() {
    // Same discrimination as the through-app pin: the config segment differs
    // from the table name, and the presign URL keeps its `/presign` suffix on
    // the router's path (issue #691).
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{}\n[domains.core.entity_config.PersonType]\npath_segment = \"people\"\n\n[domains.core.media.r2]\nbinding = \"MEDIA\"\n",
        media_toml("r2", "presigned")
    ))
    .expect("toml");
    let out = tempfile::tempdir().expect("out dir");
    let files = run_generator(&engine, &config, out.path())
        .await
        .expect("media generation");
    let hurl = files
        .iter()
        .find(|f| f.path.ends_with("09_media.hurl"))
        .expect("the media hurl contract is emitted");
    let expected = format!(
        "{{{{base_url}}}}/api/v1/core/people/{}/avatar/presign",
        codegraph_generate::media::MEDIA_CONTRACT_MISSING_ID
    );
    assert!(
        hurl.content.contains(&format!("POST {expected}")),
        "the presign contract addresses the router's route ({expected}): {}",
        hurl.content
    );
    assert!(
        !hurl.content.contains("/core/person/"),
        "never the table name: {}",
        hurl.content
    );
    assert!(
        hurl.content.contains("HTTP 404") && hurl.content.contains("NOT_FOUND"),
        "the ownership leg is provider-free: {}",
        hurl.content
    );
}

// ── Issue #707 — the stored object key must stay inside the session org ──
//
// `display`/`remove` handed the `url` column straight to the provider with
// the SERVICE credential, and that column was client-writable (the
// create/update DTO nested the media reference). Org B could point its own
// record at `<orgA>/…` and sign or DELETE org A's object. Two fixes:
//   1. the read path refuses a key that does not belong to the session org
//      (404, never 403 — a foreign object is indistinguishable from a
//      missing one), BEFORE any storage call. The guard is DERIVED from the
//      domain's own `key_template` (see the two shapes below);
//   2. in `through-app` domains the request DTOs no longer carry the
//      reference, so the upload handler is its only writer. `presigned`
//      (presign only mints a URL — the client persists the reference
//      through the update path, #701) and client-direct (no app-plane
//      route) keep it: the CRUD body IS the client's writer there.

/// The emitted org-first guard, VERBATIM: the domain's own first key segment
/// (`KEY_PREFIX_FORMAT`, derived from the key template) rendered for the
/// session org. A plain-string `.replace` — never `format!` on runtime text.
const MEDIA_KEY_PREFIX_CHECK: &str =
    "if !key.starts_with(&KEY_PREFIX_FORMAT.replace(\"{org}\", &org)) {";

/// The emitted NON-org-first guard, VERBATIM: the full invariant — the key is
/// a pure function of (org, domain, entity, id, field).
const MEDIA_KEY_EXACT_CHECK: &str = "if key != render_key(&org, id) {";

fn assert_key_prefix_guard(module: &str, handler: &str, storage_call: &str) {
    assert_key_guard(module, handler, storage_call, MEDIA_KEY_PREFIX_CHECK)
}

fn assert_key_guard(module: &str, handler: &str, storage_call: &str, expected: &str) {
    let start = media_route_module_key(module, handler);
    let body = &module[start..];
    let check = body.find(expected).unwrap_or_else(|| {
        panic!(
            "{handler} must refuse a stored key outside the session org's prefix \
             before touching the provider (issue #707):\n{module}"
        )
    });
    let call = body
        .find(storage_call)
        .unwrap_or_else(|| panic!("{handler} must call `{storage_call}`:\n{module}"));
    assert!(
        check < call,
        "{handler} must check the key prefix BEFORE `{storage_call}`:\n{module}"
    );
    // The guard compares against the SESSION org, so the handler must bind it.
    let org_bind = body
        .find("let org = api_key_info.organization_id.to_string();")
        .unwrap_or_else(|| {
            panic!("{handler} must bind the session org for the key guard:\n{module}")
        });
    assert!(
        org_bind < check,
        "{handler} must bind the session org BEFORE the key guard:\n{module}"
    );
    let guarded = &body[check..call];
    assert!(
        guarded.contains("AppError::not_found("),
        "{handler}'s prefix refusal is a 404 — never 403 (a foreign key must be \
         indistinguishable from a missing one):\n{module}"
    );
    assert!(
        !guarded.contains("AppError::forbidden("),
        "{handler}'s prefix refusal must not leak a 403:\n{module}"
    );
}

#[tokio::test]
async fn media_display_and_remove_reject_keys_outside_the_session_org() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    assert_eq!(
        module.matches(MEDIA_KEY_PREFIX_CHECK).count(),
        2,
        "display AND remove guard the key prefix:\n{module}"
    );
    assert_key_prefix_guard(module, "pub async fn display(", ".display_urls(");
    assert_key_prefix_guard(module, "pub async fn remove(", ".delete(&key)");
    // The prefix is DERIVED from the domain's key template (the default first
    // segment is `{org}`), and it is a plain string, not a `format!` literal.
    assert!(
        module.contains("const KEY_PREFIX_FORMAT: &str = \"{org}/\";"),
        "the default template's first segment lowers to the org slot:\n{module}"
    );
    assert!(
        !module.contains("if !key.starts_with(&format!(\"{}/\""),
        "no hardcoded `{{org}}/` prefix survives (issue #707):\n{module}"
    );
    assert!(
        !module.contains(MEDIA_KEY_EXACT_CHECK),
        "an org-first template needs no full-key fallback:\n{module}"
    );
}

#[tokio::test]
async fn presigned_media_display_and_remove_guard_the_key_prefix_too() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"presigned\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    ))
    .expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    assert_eq!(
        module.matches(MEDIA_KEY_PREFIX_CHECK).count(),
        2,
        "the display/remove pair is shared by both mode legs:\n{module}"
    );
    assert_key_prefix_guard(module, "pub async fn display(", ".display_urls(");
    assert_key_prefix_guard(module, "pub async fn remove(", ".delete(&key)");

    // `presign` MINTS the key from the session org (`render_key`) — there is
    // no stored-key trust in it, so it carries no prefix check.
    let start = media_route_module_key(module, "pub async fn presign(");
    let end = media_route_module_key(module, "pub async fn display(");
    let presign = &module[start..end];
    assert!(
        !presign.contains(MEDIA_KEY_PREFIX_CHECK),
        "presign renders its own key — no stored-key guard applies:\n{presign}"
    );
    assert!(
        presign.contains("let key = render_key(&org, id);"),
        "presign mints from the session org:\n{presign}"
    );
}

/// A domain whose key template puts `{org}` somewhere OTHER than the leading
/// segment — explicitly legal (`KeyTemplate::parse` requires only `{id}` and
/// `{field}`; `{org}` may sit anywhere, `media/keys.rs`).
fn media_toml_with_key_template(mode: &str, template: &str) -> String {
    format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"supabase\"\nupload_mode = \"{mode}\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\nkey_template = \"{template}\"\n"
    )
}

/// Issue #707 review blocker: `key_template` is user-facing config and `{org}`
/// is optional/positional, so the guard must be DERIVED from it. A hardcoded
/// `{org}/` prefix would 404 every legitimate key for
/// `media/{org}/{domain}/{entity}/{id}/{field}` — a full media outage in a
/// security fix. With no segment a prefix test can compare, the guard is the
/// full invariant: the key must EQUAL `render_key(org, id)`.
#[tokio::test]
async fn non_org_first_key_template_guards_with_the_full_key_equality() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml_with_key_template(
        "through-app",
        "media/{org}/{domain}/{entity}/{id}/{field}",
    ))
    .expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    // The configured template IS the rendered key (not the default).
    assert!(
        module.contains("format!(\"media/{org}/core/person/{id}/avatar\")"),
        "the key renders from the configured template:\n{module}"
    );
    // No prefix machinery at all, and no outage-prone hardcoded prefix.
    assert!(
        !module.contains("const KEY_PREFIX_FORMAT"),
        "a non-org-first template emits no org prefix const:\n{module}"
    );
    assert!(
        !module.contains("if !key.starts_with(&format!(\"{}/\""),
        "no hardcoded `{{org}}/` prefix survives (issue #707):\n{module}"
    );
    assert_eq!(
        module.matches(MEDIA_KEY_EXACT_CHECK).count(),
        2,
        "display AND remove guard the full key:\n{module}"
    );
    assert_key_guard(
        module,
        "pub async fn display(",
        ".display_urls(",
        MEDIA_KEY_EXACT_CHECK,
    );
    assert_key_guard(
        module,
        "pub async fn remove(",
        ".delete(&key)",
        MEDIA_KEY_EXACT_CHECK,
    );
}

/// A MIXED first segment (`media-{org}`) is still not org-first for either
/// consumer: a prefix test cannot express it as a segment, and the bucket-RLS
/// predicate (`split_part(name, '/', 1) = org`) cannot compare it. Both read
/// the ONE predicate (`keys::key_template_is_org_first`), so the route falls
/// back to the (stronger) full-key equality instead of inventing its own rule.
#[tokio::test]
async fn mixed_first_segment_org_template_uses_the_full_key_equality() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml_with_key_template(
        "through-app",
        "media-{org}/{domain}/{entity}/{id}/{field}",
    ))
    .expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    assert!(
        module.contains("format!(\"media-{org}/core/person/{id}/avatar\")"),
        "the configured template is the rendered key:\n{module}"
    );
    assert!(
        !module.contains("const KEY_PREFIX_FORMAT"),
        "`media-{{org}}` is not a checkable segment prefix:\n{module}"
    );
    assert_eq!(
        module.matches(MEDIA_KEY_EXACT_CHECK).count(),
        2,
        "display AND remove guard the full key:\n{module}"
    );
    assert_key_guard(
        module,
        "pub async fn display(",
        ".display_urls(",
        MEDIA_KEY_EXACT_CHECK,
    );
}

/// A first segment that JOINS the org to another placeholder
/// (`{org}-{domain}/…`) is the case that a lexeme-only predicate gets wrong:
/// `KeyTemplate::parse` yields `[Org, "-", Domain, "/", …]`, so the first
/// LEXEME is the org while the first SEGMENT is `{org}-{domain}`. Claiming
/// org-first there emitted the `"{org}/"` prefix guard, which can never match
/// the rendered key `<org>-<domain>/…` — every legitimate display/remove
/// would 404. The template must take the full-key equality branch (found in
/// review of #707).
#[tokio::test]
async fn joined_first_segment_org_template_uses_the_full_key_equality() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml_with_key_template(
        "through-app",
        "{org}-{domain}/{entity}/{id}/{field}",
    ))
    .expect("toml");
    let modules = run_media_route_generator(&engine, &config).await;
    let module = &modules[0];

    assert!(
        module.contains("format!(\"{org}-core/person/{id}/avatar\")"),
        "the configured template is the rendered key:\n{module}"
    );
    assert!(
        !module.contains("const KEY_PREFIX_FORMAT"),
        "`{{org}}-{{domain}}` is not a checkable segment prefix — a `\"{{org}}/\"` \
         guard would 404 every legitimate key:\n{module}"
    );
    assert_eq!(
        module.matches(MEDIA_KEY_EXACT_CHECK).count(),
        2,
        "display AND remove guard the full key:\n{module}"
    );
    assert_key_guard(
        module,
        "pub async fn display(",
        ".display_urls(",
        MEDIA_KEY_EXACT_CHECK,
    );
    assert_key_guard(
        module,
        "pub async fn remove(",
        ".delete(&key)",
        MEDIA_KEY_EXACT_CHECK,
    );
}

async fn run_domain_types_dto(db: &dyn GraphQuerier, config: &DomainConfig) -> Vec<GeneratedFile> {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let out = tempfile::tempdir().expect("out dir");
    let generator = codegraph_generate::domain_types::dto::DomainTypesDtoGenerator::new_with_base(
        out.path().to_path_buf(),
    );
    use codegraph_generate::traits::EntityGenerator;
    generator
        .generate(
            db,
            "PersonType",
            "core",
            config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .expect("domain-types dto generation")
}

fn dto_file<'a>(files: &'a [GeneratedFile], name: &str) -> &'a str {
    files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(name))
        .map(|f| f.content.as_str())
        .unwrap_or_else(|| panic!("the DTO plane must emit {name}"))
}

#[tokio::test]
async fn media_reference_is_not_client_writable_but_stays_in_the_response() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let files = run_domain_types_dto(&engine, &config).await;

    let create = dto_file(&files, "dto_create.rs");
    let update = dto_file(&files, "dto_update.rs");
    let response = dto_file(&files, "dto_response.rs");

    // The upload handler is the ONLY writer: a client must not be able to
    // name the object key.
    for (name, dto) in [("create", create), ("update", update)] {
        assert!(
            !dto.contains("avatar"),
            "the {name} DTO must not carry the media reference (issue #707):\n{dto}"
        );
        assert!(
            !dto.contains("MediaReference"),
            "the {name} DTO must not emit the media child struct either:\n{dto}"
        );
    }

    // The response keeps it — the UI renders the reference and the media
    // handlers address the child row.
    assert!(
        response.contains("avatar: Option<PersonTypeMediaReferenceResponse>"),
        "the response DTO keeps the media reference:\n{response}"
    );
    assert!(
        response.contains("pub struct PersonTypeMediaReferenceResponse"),
        "the response keeps the child struct:\n{response}"
    );
    assert!(
        response.contains("url"),
        "the response child carries the key/url:\n{response}"
    );
}

#[tokio::test]
async fn modes_without_an_app_side_row_writer_keep_the_media_reference() {
    // The DTO exclusion is scoped to `through-app` — the ONE mode whose
    // upload handler writes the child row. Client-direct has no app-plane
    // route (the browser speaks to the provider with its session) and
    // `presigned` only MINTS a PUT URL ("the reference is persisted
    // afterwards through the normal update path", `media_route.tera`), so in
    // both the JSON CRUD body IS the client's writer. Stripping the field
    // there would make the reference unwritable.
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;

    let client_direct =
        parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let create = dto_file(
        &run_domain_types_dto(&engine, &client_direct).await,
        "dto_create.rs",
    )
    .to_string();
    assert!(
        create.contains("avatar: Option<CreatePersonTypeMediaReferenceRequest>"),
        "client-direct keeps the reference writable:\n{create}"
    );

    let presigned = parse_domain_config_str(&format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"presigned\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    ))
    .expect("toml");
    let create = dto_file(
        &run_domain_types_dto(&engine, &presigned).await,
        "dto_create.rs",
    )
    .to_string();
    assert!(
        create.contains("avatar: Option<CreatePersonTypeMediaReferenceRequest>"),
        "presigned keeps the reference writable — presign only mints the URL:\n{create}"
    );
}

async fn run_repository_impl(db: &dyn GraphQuerier, config: &DomainConfig) -> String {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let out = tempfile::tempdir().expect("out dir");
    let generator = codegraph_generate::ddd::repository::RepositoryTraitGenerator::new(out.path());
    use codegraph_generate::traits::EntityGenerator;
    generator
        .generate(
            db,
            "PersonType",
            "core",
            config,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .expect("repository generation")
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("repository_impl.rs"))
        .map(|f| f.content.clone())
        .expect("the SeaORM repository impl")
}

/// THE TRAP (issue #707): the update path deletes ALL child rows and
/// re-inserts them from the DTO. Skipping the media child only on INSERT
/// would make an ordinary `PUT` DELETE the uploaded reference — the object
/// row must survive an update that carries no media field at all.
#[tokio::test]
async fn repository_plane_never_writes_the_media_child_row() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let impl_src = run_repository_impl(&engine, &config).await;

    // The READ path keeps the child (the response tree and the media
    // handlers address the row).
    assert!(
        impl_src.contains("person_avatar"),
        "the read path keeps the media child table:\n{impl_src}"
    );
    // The WRITE paths do not: create's INSERT, update's delete-then-reinsert.
    assert!(
        !impl_src.contains("INSERT INTO core.person_avatar"),
        "the create path must not insert the media child row:\n{impl_src}"
    );
    assert!(
        !impl_src.contains("DELETE FROM core.person_avatar"),
        "the update path must NOT delete the media child row (the #707 trap — \
         delete-then-reinsert would wipe the uploaded reference on every PUT):\n{impl_src}"
    );

    // The exclusion is scoped to `through-app`. Client-direct and presigned
    // have NO app-side row writer (the client is the writer), so the
    // repository MUST keep writing the child row there.
    for (label, toml) in [
        ("client-direct", media_toml("supabase", "client-direct")),
        (
            "presigned",
            format!(
                "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"presigned\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
            ),
        ),
    ] {
        let config = parse_domain_config_str(&toml).expect("toml");
        let naive_impl = run_repository_impl(&engine, &config).await;
        assert!(
            naive_impl.contains("INSERT INTO core.person_avatar"),
            "{label} keeps the CRUD writer — the plane would be unwritable otherwise:\n{naive_impl}"
        );
        assert!(
            naive_impl.contains("DELETE FROM core.person_avatar"),
            "{label} keeps the replace-on-update path:\n{naive_impl}"
        );
    }
}

// ── Issue #704: the served OpenAPI documents register the media routes ──
//
// `api/media_route.tera` has always carried `#[utoipa::path]` annotations for
// presign/upload/display/remove, but nothing registered those handler
// functions in an `#[derive(OpenApi)]` document — so the generated spec
// contained NO media path while the annotations implied a published contract.
// The registration is derived from the SAME plan helpers the router uses
// (`media_route_surface` + `media_upload_mode`), so the documented surface and
// the mounted surface cannot drift.

fn presigned_media_toml() -> String {
    // Supabase cannot serve `presigned` (the capability matrix), so the r2
    // binding rides along — the same shape the sibling presigned pins use.
    format!(
        "{BASE_TOML}\n[domains.core.media]\nprovider = \"r2\"\nupload_mode = \"presigned\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\n"
    )
}

/// The emitted media handler modules for one entity, as `(module_stem,
/// source)` pairs. The stem is the file name the mod-file emitter declares
/// (`media_{entity_snake}_{field_snake}`), i.e. the path segment the OpenAPI
/// registration must address.
async fn emitted_media_files(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    title: &str,
) -> Vec<(String, String)> {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let out = tempfile::tempdir().expect("out dir");
    let generator = codegraph_generate::api::media::MediaRouteGenerator::new(out.path());
    use codegraph_generate::traits::EntityGenerator;
    generator
        .generate(db, title, "core", config, &tera, &ProjectConfig::default())
        .await
        .expect("media route generation")
        .into_iter()
        .map(|f| {
            let stem = f
                .path
                .file_stem()
                .expect("the media module has a file stem")
                .to_string_lossy()
                .into_owned();
            (stem, f.content)
        })
        .collect()
}

async fn run_openapi_generator(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    titles: &[&str],
) -> Vec<GeneratedFile> {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let out = tempfile::tempdir().expect("out dir");
    let generation_order: Vec<GenerationEntry> = titles
        .iter()
        .map(|title| GenerationEntry {
            schema_title: (*title).to_string(),
            domain: "core".to_string(),
            pg_schema: "core".to_string(),
            is_cyclic: false,
        })
        .collect();
    let generator = codegraph_generate::api::openapi::OpenApiGenerator::new(out.path());
    generator
        .generate(
            db,
            config,
            &generation_order,
            &tera,
            &ProjectConfig::default(),
        )
        .await
        .expect("openapi generation")
}

fn openapi_doc<'a>(files: &'a [GeneratedFile], name: &str) -> &'a str {
    files
        .iter()
        .find(|f| f.path.ends_with(format!("api/openapi/{name}")))
        .map(|f| f.content.as_str())
        .unwrap_or_else(|| panic!("the openapi generator must emit {name}"))
}

/// The registrations inside a `paths(...)` / `components(schemas(...))`
/// block, read POSITIONALLY between the opener and the first closing-paren
/// line (`    ),` for paths, `    )),` for the nested schemas call) — so a
/// pin can assert what the derive actually publishes instead of grepping the
/// whole file (where a comment or a DTO name would match).
fn registrations_after(source: &str, opener: &str) -> Vec<String> {
    let start = source
        .find(opener)
        .unwrap_or_else(|| panic!("`{opener}` missing from: {source}"))
        + opener.len();
    let rest = &source[start..];
    let mut registrations = Vec::new();
    for line in rest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(')') {
            return registrations;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        registrations.push(trimmed.trim_end_matches(',').to_string());
    }
    panic!("the `{opener}` block never closes: {rest}");
}

fn openapi_paths(source: &str) -> Vec<String> {
    registrations_after(source, "paths(")
}

fn openapi_schemas(source: &str) -> Vec<String> {
    registrations_after(source, "components(schemas(")
}

fn media_paths(source: &str) -> Vec<String> {
    openapi_paths(source)
        .into_iter()
        .filter(|path| path.contains("::media_"))
        .collect()
}

/// The handler fns a module ACTUALLY defines — the expectation source for the
/// drift pin, so neither list is an independent hardcode.
fn emitted_handlers(content: &str) -> Vec<&'static str> {
    ["presign", "upload", "display", "remove"]
        .into_iter()
        .filter(|handler| content.contains(&format!("pub async fn {handler}")))
        .collect()
}

#[tokio::test]
async fn openapi_registers_the_through_app_media_paths() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "through-app")).expect("toml");
    let files = run_openapi_generator(&engine, &config, &["PersonType"]).await;

    // Sanity: the fixture really is the through-app shape (the expectation
    // below is derived from this, not assumed).
    let emitted = emitted_media_files(&engine, &config, "PersonType").await;
    assert_eq!(emitted.len(), 1, "one module per media field");
    assert_eq!(emitted[0].0, "media_person_avatar");
    assert_eq!(emitted[0].1.matches("pub async fn upload").count(), 1);

    for doc in ["core.rs", "all.rs"] {
        let registered = media_paths(openapi_doc(&files, doc));
        for expected in [
            "crate::api::core::media_person_avatar::upload",
            "crate::api::core::media_person_avatar::display",
            "crate::api::core::media_person_avatar::remove",
        ] {
            assert!(
                registered.iter().any(|p| p == expected),
                "{doc} must register {expected}: {registered:?}"
            );
        }
        assert!(
            !registered
                .iter()
                .any(|p| p.ends_with("::media_person_avatar::presign")),
            "through-app publishes no presign route: {registered:?}"
        );
        // The multipart upload body documents NO request_body: there is no
        // `ToSchema` struct for it (deliberate — never invent one).
        assert!(
            !openapi_schemas(openapi_doc(&files, doc))
                .iter()
                .any(|s| s.contains("PresignRequest")),
            "{doc}: through-app registers no PresignRequest schema"
        );
    }
}

#[tokio::test]
async fn openapi_registers_the_presigned_media_paths_and_request_schema() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&presigned_media_toml()).expect("toml");
    let files = run_openapi_generator(&engine, &config, &["PersonType"]).await;

    let emitted = emitted_media_files(&engine, &config, "PersonType").await;
    assert_eq!(emitted.len(), 1, "one module per media field");
    assert!(
        emitted[0].1.contains("pub struct PresignRequest"),
        "the presigned module owns the request schema"
    );

    for doc in ["core.rs", "all.rs"] {
        let registered = media_paths(openapi_doc(&files, doc));
        for expected in [
            "crate::api::core::media_person_avatar::presign",
            "crate::api::core::media_person_avatar::display",
            "crate::api::core::media_person_avatar::remove",
        ] {
            assert!(
                registered.iter().any(|p| p == expected),
                "{doc} must register {expected}: {registered:?}"
            );
        }
        assert!(
            !registered
                .iter()
                .any(|p| p.ends_with("::media_person_avatar::upload")),
            "presigned publishes no multipart upload: {registered:?}"
        );
        // `request_body = PresignRequest` only compiles when the schema is
        // registered in the same document.
        assert!(
            openapi_schemas(openapi_doc(&files, doc))
                .iter()
                .any(|s| s == "crate::api::core::media_person_avatar::PresignRequest"),
            "{doc} must register the presign request schema: {:?}",
            openapi_schemas(openapi_doc(&files, doc))
        );
    }
}

#[tokio::test]
async fn openapi_documents_no_media_path_for_client_direct() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_model(&engine).await;
    let config = parse_domain_config_str(&media_toml("supabase", "client-direct")).expect("toml");
    let files = run_openapi_generator(&engine, &config, &["PersonType"]).await;

    assert!(
        emitted_media_files(&engine, &config, "PersonType")
            .await
            .is_empty(),
        "R5: client-direct emits no app-plane media module"
    );

    for doc in ["core.rs", "all.rs"] {
        let paths = openapi_paths(openapi_doc(&files, doc));
        // The document is otherwise populated — this is not a vacuous pass.
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with("person_handler::get_by_id")),
            "{doc}: the CRUD surface is still registered: {paths:?}"
        );
        assert!(
            media_paths(openapi_doc(&files, doc)).is_empty(),
            "{doc} must document no media path for client-direct"
        );
        assert!(
            !openapi_schemas(openapi_doc(&files, doc))
                .iter()
                .any(|s| s.contains("PresignRequest")),
            "{doc}: no presign schema without app-plane media"
        );
    }
}

/// THE DRIFT PIN (issue #704): the expected registration is derived from the
/// EMITTED module files — every `src/api/{domain}/media_*.rs` stem must appear
/// in the published path list with exactly the handlers that module defines.
/// A module rename, a handler rename, or a lost registration desyncs the two
/// planes and fails here.
#[tokio::test]
async fn openapi_media_registration_tracks_the_emitted_media_modules() {
    for (label, toml) in [
        ("through-app", media_toml("supabase", "through-app")),
        ("presigned", presigned_media_toml()),
    ] {
        let engine = GrafeoEngine::in_memory().expect("engine");
        ingest_model(&engine).await;
        let config = parse_domain_config_str(&toml).expect("toml");

        let emitted = emitted_media_files(&engine, &config, "PersonType").await;
        assert_eq!(emitted.len(), 1, "{label}: one module per media field");
        let mut expected: Vec<String> = emitted
            .iter()
            .flat_map(|(stem, content)| {
                emitted_handlers(content)
                    .into_iter()
                    .map(move |handler| format!("crate::api::core::{stem}::{handler}"))
            })
            .collect();
        expected.sort();
        assert!(
            expected.len() >= 3,
            "{label}: the fixture exposes at least display/remove: {expected:?}"
        );

        let files = run_openapi_generator(&engine, &config, &["PersonType"]).await;
        for doc in ["core.rs", "all.rs"] {
            let mut registered = media_paths(openapi_doc(&files, doc));
            registered.sort();
            assert_eq!(
                registered, expected,
                "{label}: {doc} must register exactly what the emitted media modules expose"
            );
        }
    }
}

/// A `role = "child"` entity's media route is NESTED by its utoipa annotation
/// (issue #713) but it is still an app-plane route that must be DOCUMENTED —
/// registration must not be root-gated.
#[tokio::test]
async fn openapi_registers_a_nested_child_media_module() {
    let engine = GrafeoEngine::in_memory().expect("engine");
    ingest_parent_and_child(&engine).await;
    let config =
        parse_domain_config_str(&media_toml_with_nested_child("through-app")).expect("toml");
    let files = run_openapi_generator(&engine, &config, &["PersonType", "ContactType"]).await;

    let emitted = emitted_media_files(&engine, &config, "ContactType").await;
    assert_eq!(emitted.len(), 1, "one module per media field");
    assert_eq!(emitted[0].0, "media_contact_avatar");
    // The module documents the NESTED path it is mounted at.
    assert!(
        emitted[0]
            .1
            .contains(r#"path = "/api/v1/core/persons/{person_id}/contacts/{id}/avatar""#),
        "the child module's own contract is nested: {}",
        emitted[0].1
    );

    for doc in ["core.rs", "all.rs"] {
        let registered = media_paths(openapi_doc(&files, doc));
        for expected in [
            "crate::api::core::media_contact_avatar::display",
            "crate::api::core::media_contact_avatar::remove",
            "crate::api::core::media_contact_avatar::upload",
        ] {
            assert!(
                registered.iter().any(|p| p == expected),
                "{doc} must register the nested child's {expected}: {registered:?}"
            );
        }
    }
}
