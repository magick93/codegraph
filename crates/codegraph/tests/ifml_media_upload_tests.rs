//! Node-free pin for the IFML `input file` → media upload wiring
//! (issue #633, WS-D).
//!
//! Before #633 a modeled `field x -> input file` rendered a
//! **value-bound** `<input type="file" value={form_state.x}>`: the browser
//! cannot set a file input's value from script, and the submit path
//! JSON-stringified whatever landed there — a control that could never
//! upload anything. The field now lowers onto the entity's media route:
//!
//! - the form renders the upload control (testids + `onchange` handler,
//!   NO value binding, and the submit payload never carries the File);
//! - the view's `data.remote.ts` gains `upload{Field}` — a server-side
//!   `command` posting multipart FormData through `$lib/server/api`
//!   (issue #574's neutral contract: no browser credential);
//! - the POM gains `fill{Form}{Field}(filePath)` via the kernel's
//!   `uploadMedia` (`setInputFiles` on the control's input testid) plus a
//!   `{form}{Field}File()` locator, and the render spec asserts the
//!   control is real (visible, `type=file`, no `value` attribute);
//! - a file field with no media route (or with the remote plane off) still
//!   renders a plain file input — never the value-bound dead shape.
//!
//! ```text
//! cargo test -p codegraph --test ifml_media_upload_tests
//! ```

use std::fs;
use std::path::{Path, PathBuf};

const PORTAL_MOX: &str = r#"package portal

/// The stored media reference (url + mime) — a contained value.
class MediaReference {
    String url
    String mimeType
}

/// A reviewable request carrying one attachment.
class Request {
    /// Request title.
    String title
    /// The uploaded receipt.
    contains MediaReference[0..1] receipt
}
"#;

const APP_IFML: &str = r#"domain "portal" { schema "portal"; }

view "Home" {
    label "Home";
    landmark: true;

    component "home_grid" {
        type: list;
        data: Request;
        fields: [title];

        on select(row) -> navigate("RequestForm", {
            id: row.id
        });
    }
}

view "RequestForm" {
    params { id: Uuid };
    label "Edit Request";

    component "editor" {
        type: form;
        data: Request;
        mode: edit;

        field title -> input text {
            required: true;
        }

        field receipt -> input file;

        on save -> navigate("Home");
        on cancel -> navigate("Home");
    }
}
"#;

const DOMAINS_TOML: &str = r#"[defaults]
api_version = "v1"

[domains.portal]
label = "Portal"
schema_dir = "portal"
postgres_schema = "portal"
entities = ["Request"]

[domains.portal.entity_config.Request]
role = "root"

[domains.portal.media]
provider = "supabase"
upload_mode = "through-app"
bucket = "request-media"
max_bytes = 5242880
accept = ["image/png", "application/pdf"]
"#;

struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    mox: PathBuf,
    ifml: PathBuf,
}

fn write_fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    fs::write(root.path().join("portal.mox"), PORTAL_MOX).unwrap();
    let ifml = root.path().join("app.ifml");
    fs::write(&ifml, APP_IFML).unwrap();
    Fixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("portal.mox"),
        ifml,
        root,
    }
}

/// The workspace profiles.toml, optionally with `ifml_remote_functions`
/// stripped (the flag-off leg).
fn profiles_path(fixture: &Fixture, remote: bool) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml");
    let content = fs::read_to_string(&workspace).expect("workspace profiles.toml");
    let path = fixture.root.path().join("profiles.toml");
    if remote {
        fs::write(&path, content).unwrap();
    } else {
        let stripped = content
            .lines()
            .filter(|line| !line.trim_start().starts_with("ifml_remote_functions"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, stripped).unwrap();
    }
    path
}

fn run_args<'a>(
    fixture: &'a Fixture,
    profiles: &'a Path,
    frameworks: &'a [String],
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
        ifml_files: std::slice::from_ref(&fixture.ifml),
        openapi_files: &[],
        mox_files: std::slice::from_ref(&fixture.mox),
        rosetta_files: &[],
        ddd_files: &[],
        evt_files: &[],
        deploy_files: &[],
        ifml_framework: frameworks,
        ifml_components: None,
        ifml_design_system: None,
        codegraph_rev: None,
        check: false,
        ux_rules: None,
    }
}

async fn generate(remote: bool) -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let profiles = profiles_path(&fixture, remote);
    let output = fixture.root.path().join("generated");
    let frameworks = vec!["svelte".to_string()];
    codegraph::driver::run(run_args(&fixture, &profiles, &frameworks, &output))
        .await
        .expect("pipeline run must succeed");
    (fixture, output.join("ui"))
}

fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            walk(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// Find exactly one emitted file whose path ends with `suffix`.
fn find_unique(base: &Path, suffix: &str) -> String {
    let mut files = Vec::new();
    walk(base, &mut files);
    let matches: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.to_string_lossy().ends_with(suffix))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one `{suffix}`, found {matches:?}"
    );
    fs::read_to_string(matches[0]).expect("read")
}

#[tokio::test]
async fn form_renders_a_real_non_value_bound_file_control() {
    let (_fixture, ui) = generate(true).await;
    let page = find_unique(&ui, "src/routes/(views)/requestform/+page.svelte");

    // The wired control: testids + the upload handler.
    assert!(
        page.contains("media-receipt-input"),
        "the file slot renders the upload control testids:\n{page}"
    );
    assert!(
        page.contains("onchange={handleFileReceipt}"),
        "the file input drives the upload handler:\n{page}"
    );
    assert!(
        page.contains("media-receipt-display"),
        "the control renders its display slot:\n{page}"
    );
    // The dead pre-#633 shape: a value binding on the file input.
    assert!(
        !page.contains("_form_state.receipt"),
        "the file input must NOT be value-bound (the #633 defect):\n{page}"
    );
    // The submit payload never carries the File object.
    assert!(
        !page.contains("payload.receipt"),
        "the submit payload must not carry the file field:\n{page}"
    );
}

#[tokio::test]
async fn remote_module_emits_the_server_side_multipart_upload() {
    let (_fixture, ui) = generate(true).await;
    let remote = find_unique(&ui, "src/routes/(views)/requestform/data.remote.ts");
    assert!(
        remote.contains("export const uploadReceipt = command('unchecked'"),
        "{remote}"
    );
    assert!(
        remote.contains("form.append('file', args.file, args.file.name);"),
        "the command posts multipart FormData:\n{remote}"
    );
    assert!(
        remote.contains("method: 'PUT'"),
        "the upload rides the media route's PUT:\n{remote}"
    );
    assert!(
        remote.contains("/receipt`"),
        "the route addresses the media field:\n{remote}"
    );
    // Never the browser credential plane.
    assert!(
        !remote.contains("PUBLIC_API_KEY"),
        "the upload must not reach a public key:\n{remote}"
    );
}

#[tokio::test]
async fn pom_drives_the_file_field_through_the_kernel_upload() {
    let (_fixture, ui) = generate(true).await;
    let page_class = find_unique(&ui, "tests/pages/request-form-page.ts");
    assert!(
        page_class.contains("async fillEditorReceipt("),
        "{page_class}"
    );
    assert!(
        page_class.contains("await this.uploadMedia(\"receipt\", value);"),
        "the fill forwards to the kernel upload:\n{page_class}"
    );
    assert!(
        page_class.contains("EditorReceiptFile(): Locator"),
        "the spec asserts through a locator method:\n{page_class}"
    );

    // The shared IFML kernel carries the media surface.
    let kernel = find_unique(&ui, "tests/pages/support/base-page.ts");
    assert!(kernel.contains("async uploadMedia("), "{kernel}");
    assert!(kernel.contains("async removeMedia("), "{kernel}");

    // The render spec asserts the control structurally (zero raw testids).
    let spec = find_unique(&ui, "tests/ifml/request-form.spec.ts");
    assert!(
        spec.contains("media file control renders for RequestForm"),
        "{spec}"
    );
    assert!(
        spec.contains("EditorReceiptFile()"),
        "the spec drives the POM:\n{spec}"
    );
    assert!(
        spec.contains("not.toHaveAttribute('value')"),
        "the spec pins the non-value-bound shape:\n{spec}"
    );
    assert!(!spec.contains("data-testid"), "zero raw testids:\n{spec}");
}

#[tokio::test]
async fn without_the_remote_plane_the_file_input_is_still_not_value_bound() {
    let (_fixture, ui) = generate(false).await;
    let page = find_unique(&ui, "src/routes/(views)/requestform/+page.svelte");
    assert!(
        page.contains("type=\"file\""),
        "the declared file field still renders:\n{page}"
    );
    assert!(
        !page.contains("_form_state.receipt"),
        "never the value-bound dead shape, flag or no flag:\n{page}"
    );
    assert!(
        !page.contains("uploadReceipt"),
        "no remote command without the remote plane:\n{page}"
    );
}
