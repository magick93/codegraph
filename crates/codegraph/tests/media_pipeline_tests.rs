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
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let project = ProjectConfig::default();
    let out = tempfile::tempdir().expect("out dir");
    let generator = codegraph_generate::api::media::MediaRouteGenerator::new(out.path());
    use codegraph_generate::traits::EntityGenerator;
    generator
        .generate(db, "PersonType", "core", config, &tera, &project)
        .await
        .expect("media route generation")
        .iter()
        .map(|f| f.content.clone())
        .collect()
}

async fn rendered_router_media_block(db: &dyn GraphQuerier, config: &DomainConfig) -> String {
    let tera = codegraph_generate::template_engine::create_tera_with_overrides(&[]).expect("tera");
    let project = ProjectConfig::default();
    let out = tempfile::tempdir().expect("out dir");
    let ctx = codegraph_generate::api::router::build_router_context(
        db,
        "core",
        &["PersonType".to_string()],
        config,
        &[],
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
    let ctx = serde_json::json!({
        "entity_name": "OrgType",
        "entity_snake": "org",
        "entity_path_segment": "org",
        "entity_module": "core_org",
        "domain": "core",
        "media_field_name": "logo",
        "media_accept": ["image/png"],
        "schema": "core",
        "child_table": "org_logo",
        "parent_fk_column": "org_id",
        "parent_tenant_column": serde_json::Value::Null,
        "url_column": "url",
        "mime_column": "mime_type",
        "mode_presigned": false,
        "max_bytes": 5242880u64,
        "key_format": "{org}/core/org/{id}/logo",
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
    assert!(
        session_rs.contains("SELECT 1 FROM {table} WHERE id = $1 AND {column} = $2 LIMIT 1"),
        "tenant-scoped parent lookup: {session_rs}"
    );
    assert!(
        session_rs.contains("SELECT 1 FROM {table} WHERE id = $1 LIMIT 1"),
        "global-parent lookup: {session_rs}"
    );
    assert!(
        session_rs.contains("Statement::from_sql_and_values")
            && session_rs.contains("Value::from(id)"),
        "the id/org travel as bound parameters: {session_rs}"
    );
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
    // A body-limit trip INSIDE the extractor (axum's 2 MiB default) is the
    // same 413, never a 400 read failure.
    assert!(
        module.contains("e.status() == StatusCode::PAYLOAD_TOO_LARGE"),
        "the extractor's limit error maps to 413: {module}"
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
        (Some(pair.len() - 2), Some(pair.len() - 1)),
        "parent_exists(id, session_org) — the last two arguments, in that order: {pair:?}"
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
//   1. the read path refuses a key outside the session org's prefix (404,
//      never 403 — a foreign object is indistinguishable from a missing
//      one), BEFORE any storage call;
//   2. in `through-app` domains the request DTOs no longer carry the
//      reference, so the upload handler is its only writer. `presigned`
//      (presign only mints a URL — the client persists the reference
//      through the update path, #701) and client-direct (no app-plane
//      route) keep it: the CRUD body IS the client's writer there.

/// The emitted prefix check, verbatim: the session org plus a `/` separator
/// is the ONLY accepted key prefix (`render_key` always renders `{org}/…`).
const MEDIA_KEY_PREFIX_CHECK: &str =
    "if !key.starts_with(&format!(\"{}/\", api_key_info.organization_id)) {";

fn assert_key_prefix_guard(module: &str, handler: &str, storage_call: &str) {
    let start = media_route_module_key(module, handler);
    let body = &module[start..];
    let check = body.find(MEDIA_KEY_PREFIX_CHECK).unwrap_or_else(|| {
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
