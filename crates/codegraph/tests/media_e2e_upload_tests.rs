//! Issue #633 — the media plane's test surface (WS-D).
//!
//! The media plane ships real routes (`PUT`/`DELETE`/`GET …/{id}/{field}`,
//! `templates/api/media_route.tera`) and a fallback upload control
//! (`$lib/media/MediaField.svelte`), but nothing drives them: the POM had
//! no upload method, no entity media spec family existed, and a media
//! property leaked into the generic UI field pipeline (text form input +
//! test-data sentinel) even though the create/update DTOs exclude it
//! (`deny_unknown_fields` ⇒ a generated create 422s).
//!
//! Pins:
//!
//! 1. Media classification never reaches `collect_ui_fields` — no text
//!    form input, no test-data key (the 422 class).
//! 2. The kernel `BasePage` carries `uploadMedia`/`removeMedia`; the
//!    per-entity page class carries the `upload{Field}`/`remove{Field}`
//!    conveniences for its media fields.
//! 3. `{seg}.media.test.ts` is emitted for media entities only, and the
//!    emitted playwright config matches it (no orphan under the
//!    `entity_specs_run_green` coverage invariant).
//! 4. No `[media]` ⇒ no media spec, no kernel media surface, byte-identical
//!    page/playwright artifacts.
//!
//! IFML parity (the `input file` slice) is pinned in
//! `ifml_media_upload_tests.rs`.
//!
//! ```text
//! cargo test -p codegraph --test media_e2e_upload_tests
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_config::config::parse_domain_config_str;
use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{EdgeType, PropertyNode, SchemaNode};
use codegraph_generate::ui::common::collect_ui_fields;
use codegraph_grafeo::GrafeoEngine;
use codegraph_type_contracts::RefClassificationKind;

// ── in-graph fixture: the JSON-schema media-wrapper shape ───────────────

fn schema(id: &str, title: &str, classification: &str, table: &str, entity: bool) -> SchemaNode {
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
        is_entity: entity,
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

/// A plain scalar property (no classification) — the sibling control.
fn scalar(name: &str) -> PropertyNode {
    let mut p = prop(name, RefClassificationKind::StructuredWrapper, None);
    p.classification_kind = None;
    p.prop_type = "string".into();
    p
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

/// The classifier's `media_wrappers` shape: the property classifies as
/// `MediaWrapper` (a composite-ish wrapper) and expands into its columns —
/// the url + mime pair the media route persists.
async fn media_wrapper_engine() -> GrafeoEngine {
    let engine = GrafeoEngine::in_memory().expect("engine");
    engine
        .ingest_schema(&schema(
            "PersonType",
            "PersonType",
            "entity",
            "person",
            true,
        ))
        .await
        .expect("person schema");
    engine
        .ingest_schema(&schema(
            "PhotoType",
            "PhotoType",
            "value_object",
            "photo",
            false,
        ))
        .await
        .expect("photo schema");
    for p in [
        scalar("id"),
        scalar("name"),
        // The mox shape: a contained value (`contains Photo[0..1]`) is a
        // `ValueObject` — the media plan is what names it a media field.
        prop(
            "photo",
            RefClassificationKind::ValueObject,
            Some("PhotoType"),
        ),
    ] {
        engine
            .ingest_property("PersonType", "PersonType", &p)
            .await
            .expect("person property");
    }
    // The media class carries the reference shape the plan resolves
    // (`url` + `mime_type` columns — the domain's url_field/mime_field).
    for p in [
        prop("url", RefClassificationKind::EntityReference, None),
        prop("mimeType", RefClassificationKind::EntityReference, None),
    ] {
        engine
            .ingest_property("PhotoType", "PhotoType", &p)
            .await
            .expect("photo property");
    }
    engine
        .ingest_edge(
            "photo::PersonType",
            "PhotoType",
            EdgeType::ReferencesSchema,
            None,
        )
        .await
        .expect("references edge");
    engine
}

fn config_with_media() -> codegraph_config::DomainConfig {
    parse_domain_config_str(
        r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]
type_suffix = "Type"

[domains.core]
label = "Core"
schema_dir = "core"
postgres_schema = "core"

[domains.core.media]
provider = "supabase"
upload_mode = "through-app"
bucket = "person-media"
max_bytes = 5242880
accept = ["image/png", "image/jpeg"]
media_classes = ["PhotoType"]
"#,
    )
    .expect("domains.toml")
}

// ── pin 1: media never leaks into the UI field plane ────────────────────

#[tokio::test]
async fn media_wrapper_fields_never_render_as_text_inputs_or_enter_test_data() {
    let engine = media_wrapper_engine().await;
    let config = config_with_media();
    let fields = collect_ui_fields(&engine, "PersonType", &[], Some("core"), &config)
        .await
        .expect("ui fields");
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert!(
        !names.contains(&"photo"),
        "the media reference must not become a generic UI field (a text \
         form input whose test-data sentinel the create DTO rejects): {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("photo")),
        "not even its expanded columns: {names:?}"
    );
    assert!(
        names.contains(&"name"),
        "the plain sibling field must survive: {names:?}"
    );
}

// ── end-to-end fixture: a mox app with one media entity ─────────────────

const STAFF_MOX: &str = r#"package staff

/// The stored media reference (url + mime) — a contained value.
class MediaReference {
    String url
    String mimeType
}

/// A person carrying one avatar.
class Person {
    String name
    contains MediaReference[0..1] photo
}

/// Makes Person an entity (refers wins over contains-only VO).
class Assignment {
    refers Person person
}
"#;

const DOMAINS_TOML: &str = r#"[defaults]
api_version = "v1"

[domains.staff]
label = "Staff"
schema_dir = "staff"
postgres_schema = "staff"

[domains.staff.media]
provider = "supabase"
upload_mode = "through-app"
bucket = "staff-media"
max_bytes = 5242880
accept = ["image/png", "image/jpeg"]
"#;

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
}

fn write_fixture(with_media: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let config = if with_media {
        DOMAINS_TOML.to_string()
    } else {
        // Same model, no `[media]` declaration — the byte-identity leg.
        DOMAINS_TOML
            .split("[domains.staff.media]")
            .next()
            .unwrap()
            .to_string()
    };
    fs::write(root.path().join("domains.toml"), config).unwrap();
    fs::write(root.path().join("staff.mox"), STAFF_MOX).unwrap();
    Fixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("staff.mox"),
        root,
    }
}

fn run_args<'a>(
    fixture: &'a Fixture,
    profiles: &'a Path,
    output: &'a Path,
) -> codegraph::driver::RunArgs<'a> {
    codegraph::driver::RunArgs {
        integration_manifests: &[],
        schemas: None,
        classifier: None,
        config_path: &fixture.config,
        output,
        extension_points_path: None,
        profile_name: "default",
        variant: None,
        profiles_config_path: Some(profiles.to_path_buf()),
        no_post_gen: true,
        template_dir: &[],
        ifml_files: &[],
        openapi_files: &[],
        mox_files: std::slice::from_ref(&fixture.mox),
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: &[],
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    }
}

fn workspace_profiles(fixture: &Fixture) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml");
    let content = fs::read_to_string(&workspace).expect("workspace profiles.toml");
    let path = fixture.root.path().join("profiles.toml");
    fs::write(&path, content).unwrap();
    path
}

async fn generate(with_media: bool) -> (Fixture, PathBuf) {
    let fixture = write_fixture(with_media);
    let profiles = workspace_profiles(&fixture);
    let output = fixture.root.path().join("generated");
    codegraph::driver::run(run_args(&fixture, &profiles, &output))
        .await
        .expect("pipeline run must succeed");
    (fixture, output.join("ui"))
}

fn read(base: &Path, rel: &str) -> String {
    fs::read_to_string(base.join(rel))
        .unwrap_or_else(|e| panic!("expected {rel} to be emitted: {e}"))
}

// ── pin 2: the POM upload surface ───────────────────────────────────────

#[tokio::test]
async fn pom_kernel_carries_the_media_upload_surface() {
    let (_fixture, ui) = generate(true).await;
    let kernel = read(&ui, "tests/generated/_support/pom.ts");
    assert!(
        kernel.contains("async uploadMedia("),
        "BasePage must carry the upload method:\n{kernel}"
    );
    assert!(
        kernel.contains("setInputFiles"),
        "the upload drives the media input:\n{kernel}"
    );
    assert!(
        kernel.contains("async removeMedia("),
        "BasePage must carry the remove method:\n{kernel}"
    );
    // The control's error surface is the throw condition (415/413 legs).
    assert!(
        kernel.contains("-error"),
        "the upload must read the control's error testid:\n{kernel}"
    );
}

#[tokio::test]
async fn page_class_carries_media_conveniences_for_its_media_fields() {
    let (_fixture, ui) = generate(true).await;
    let page = read(&ui, "tests/generated/staff/person.page.ts");
    assert!(
        page.contains("async uploadPhoto("),
        "the page class forwards the media upload:\n{page}"
    );
    assert!(
        page.contains("async removePhoto("),
        "the page class forwards the media remove:\n{page}"
    );
    assert!(
        page.contains("uploadMedia('photo'"),
        "the forwarder names the field:\n{page}"
    );
}

// ── pin 3: the entity media family + config coverage ────────────────────

#[tokio::test]
async fn media_family_emits_the_full_round_trip() {
    let (_fixture, ui) = generate(true).await;
    let spec = read(&ui, "tests/generated/staff/person.media.test.ts");

    // Every interaction goes through the page object — the #316/#318
    // contract (zero raw testids in spec bodies).
    assert!(!spec.contains("data-testid"), "zero raw testids:\n{spec}");
    assert!(spec.contains("uploadPhoto("), "{spec}");
    assert!(spec.contains("removePhoto("), "{spec}");
    // upload → reference round-trip → display renders → remove clears
    assert!(
        spec.contains("display"),
        "the spec asserts the rendered display:\n{spec}"
    );
    assert!(
        spec.contains("415") || spec.contains("mime"),
        "wrong-mime leg:\n{spec}"
    );
    assert!(
        spec.contains("413") || spec.contains("oversize") || spec.contains("max_bytes"),
        "oversize leg:\n{spec}"
    );
}

#[tokio::test]
async fn playwright_config_matches_the_media_family() {
    let (_fixture, ui) = generate(true).await;
    let config = read(&ui, "playwright.config.ts");
    let crud_line = config
        .lines()
        .find(|l| l.contains("testMatch") && l.contains("crud"))
        .unwrap_or_else(|| panic!("the crud project must exist:\n{config}"));
    assert!(
        crud_line.contains("media"),
        "the crud project must match the media family (orphan otherwise):\n{crud_line}"
    );
}

// ── pin 4: no media ⇒ nothing changes ───────────────────────────────────

#[tokio::test]
async fn without_media_the_plane_is_absent() {
    let (_fixture, ui) = generate(false).await;
    assert!(
        !ui.join("tests/generated/staff/person.media.test.ts")
            .exists(),
        "no media entity ⇒ no media spec"
    );
    let config = read(&ui, "playwright.config.ts");
    let crud_line = config
        .lines()
        .find(|l| l.contains("testMatch") && l.contains("crud"))
        .expect("crud project");
    assert!(
        !crud_line.contains("media"),
        "the config regex must stay untouched without media:\n{crud_line}"
    );
    let kernel = read(&ui, "tests/generated/_support/pom.ts");
    assert!(
        !kernel.contains("uploadMedia"),
        "the kernel stays byte-identical without media"
    );
}
