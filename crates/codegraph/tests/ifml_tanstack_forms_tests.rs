//! Node-free pin for the IFML TanStack form layer (issue #580,
//! `ifml_tanstack_forms`).
//!
//! Flag ON, typed form components render through `@tanstack/svelte-form`
//! (`createFormCreator` + `AppField`/`Subscribe`) with a zod schema
//! lowered from the DSL `validations: [...]` expressions +
//! positionally-paired `messages` (`crate::ifml::zod_glue` → the shared
//! `codegraph_generate::forms` IR). The form-creator scaffold
//! (`$lib/forms/form.ts`, `form-context.ts`, per-kind field components)
//! emits once per tree if-absent from Rust consts. Submit keeps BOTH
//! transports: the view's remote command when `ifml_remote_functions` is
//! on, else the client fetch — and maps non-ok messages into the form
//! error slot plus 422 `details: [{field,message}]` into per-field
//! errors. Flag OFF keeps the hand-rolled checkValidity/FormData form,
//! byte-identical.
//!
//! Pins:
//!
//! - flag ON — `src/lib/forms/` scaffold, page `AppField` markup, zod
//!   schema text (`z.string().min(2, "Title too short")` for
//!   `validations: [len(title) > 2]; messages: ["Title too short"]`),
//!   retained `data-validate-message`/`{comp}-form`/`{comp}-submit`/
//!   `{comp}-error` testids, `Subscribe selector={({ canSubmit,
//!   isSubmitting })}`, per-field error mapping, skeleton deps;
//! - flag OFF — `form.checkValidity()` handler unchanged, no `src/lib/
//!   forms/` plane, no zod/@tanstack anywhere in the IFML tree;
//! - the validation spec family still emits under the flag and asserts
//!   per-field message visibility through the POM.
//!
//! ```text
//! cargo test -p codegraph --test ifml_tanstack_forms_tests
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use codegraph_generate::forms::{TANSTACK_SVELTE_FORM_VERSION, ZOD_VERSION};

const PORTAL_MOX: &str = r#"package portal

/// Request lifecycle.
enum RequestStatus {
    Draft as "Draft" = 0
    Submitted as "Submitted" = 1
    Approved as "Approved" = 2
}

/// A reviewable request.
class Request {
    /// Request title.
    String title
    /// Lifecycle status.
    RequestStatus status
}
"#;

const APP_IFML: &str = r#"domain "portal" { schema "portal"; }

view "Home" {
    label "Home";
    landmark: true;

    component "home_grid" {
        type: list;
        data: Request;
        fields: [title, status];

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
            validations: [len(title) > 2];
            messages: ["Title too short"];
        }

        field status -> input dropdown {
            values: ["Draft", "Submitted", "Approved"];
        }

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

/// The workspace profiles.toml (the default profile carries
/// `ifml_tanstack_forms = true`), or a copy with the flag stripped for the
/// flag-off pin.
fn profiles_path(fixture: &Fixture, with_flag: bool) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml");
    let content = fs::read_to_string(&workspace).expect("workspace profiles.toml");
    let path = fixture.root.path().join("profiles.toml");
    if with_flag {
        fs::write(&path, content).unwrap();
    } else {
        let stripped = content
            .lines()
            .filter(|line| !line.trim_start().starts_with("ifml_tanstack_forms"))
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

/// Full pipeline (default profile ⇒ integrated mode: the IFML tree lands
/// under `ui/`).
async fn generate_integrated(with_flag: bool) -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let profiles = profiles_path(&fixture, with_flag);
    let output = fixture.root.path().join("generated");
    let frameworks = vec!["svelte".to_string()];
    codegraph::driver::run(run_args(&fixture, &profiles, &frameworks, &output))
        .await
        .expect("pipeline run must succeed");
    (fixture, output.join("ui"))
}

/// IFML-only pipeline (standalone `svelte/` tree — the shape that emits the
/// ifml-skeleton package.json).
async fn generate_standalone(with_flag: bool) -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let profiles = profiles_path(&fixture, with_flag);
    let output = fixture.root.path().join("svelte-out");
    codegraph::driver::ifml_generate(codegraph::driver::IfmlGenerateArgs {
        config_path: &fixture.config,
        output: &output,
        ifml_files: std::slice::from_ref(&fixture.ifml),
        schemas: None,
        classifier: None,
        frameworks: &["svelte".to_string()],
        profiles_config_path: Some(profiles.to_path_buf()),
        template_dir: &[],
        ifml_components: None,
        ifml_design_system: None,
        ux_rules: None,
    })
    .await
    .expect("ifml_generate must succeed");
    (fixture, output)
}

fn read(base: &Path, rel: &str) -> String {
    fs::read_to_string(base.join(rel))
        .unwrap_or_else(|e| panic!("expected {rel} to be emitted: {e}"))
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

#[tokio::test]
async fn ifml_tanstack_forms_flag_emits_the_form_plane() {
    let (_fixture, ui) = generate_integrated(true).await;

    // The form-creator scaffold (if-absent, once per tree).
    let form_ts = read(&ui, "src/lib/forms/form.ts");
    assert!(form_ts.contains("createFormCreator({"), "{form_ts}");
    assert!(form_ts.contains("createAppForm"), "{form_ts}");
    let form_context = read(&ui, "src/lib/forms/form-context.ts");
    assert!(
        form_context.contains("createFormCreatorContexts"),
        "{form_context}"
    );
    assert!(form_context.contains("useFieldContext"), "{form_context}");
    for field in [
        "text", "email", "number", "checkbox", "select", "datetime", "textarea",
    ] {
        let source = read(&ui, &format!("src/lib/forms/fields/{field}.svelte"));
        assert!(
            source.contains("useFieldContext"),
            "{field}.svelte must consume the field context:\n{source}"
        );
        assert!(
            source.contains("data-validate-message"),
            "{field}.svelte must carry the message attribute:\n{source}"
        );
    }

    // The TanStack page variant.
    let page = read(&ui, "src/routes/(views)/requestform/+page.svelte");
    assert!(
        page.contains("import { createAppForm } from '$lib/forms/form';"),
        "{page}"
    );
    assert!(page.contains("import { z } from 'zod';"), "{page}");
    assert!(
        page.contains("<editorForm.AppField name=\"title\">"),
        "typed fields render AppField slots:\n{page}"
    );
    assert!(page.contains("<editorForm.AppField name=\"status\">"), "{page}");
    assert!(
        page.contains("z.string().min(2, \"Title too short\")"),
        "the DSL len validation lowers into the zod schema:\n{page}"
    );
    assert!(
        page.contains("z.enum([\"Draft\", \"Submitted\", \"Approved\"])"),
        "dropdown values lower into a closed zod enum:\n{page}"
    );
    assert!(
        page.contains("selector={({ canSubmit, isSubmitting })"),
        "{page}"
    );
    // Testid contract preserved.
    assert!(page.contains("data-testid=\"editor-form\""), "{page}");
    assert!(page.contains("data-testid=\"editor-submit\""), "{page}");
    assert!(page.contains("data-testid=\"editor-error\""), "{page}");
    // Per-field message attributes survive.
    assert!(page.contains("message=\"Title too short\""), "{page}");
    // Submit maps non-ok messages into the form error slot and keeps the
    // per-field error plumbing (fieldErrors + the model message attr).
    assert!(page.contains("fieldErrors"), "{page}");
    assert!(page.contains("formError"), "{page}");
    // The remote command (ifml_remote_functions = true in the default
    // profile) carries the submit: TanStack composes with it.
    assert!(page.contains("await submitEditor({"), "{page}");
    assert!(page.contains("data: payload"), "{page}");
    // The hand-rolled handler is gone from the typed form.
    assert!(
        !page.contains("Object.fromEntries(new FormData(form))"),
        "{page}"
    );
}

/// The other transport leg: `ifml_remote_functions` OFF while tanstack is
/// ON — onSubmit falls back to the client fetch and maps 422
/// `details: [{field,message}]` into per-field errors.
#[tokio::test]
async fn ifml_tanstack_forms_composes_with_the_client_fetch_transport() {
    let fixture = write_fixture();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml");
    let content = fs::read_to_string(&workspace).expect("workspace profiles.toml");
    let stripped = content
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            !(t.starts_with("ifml_remote_functions") && !t.starts_with("entity_remote_functions"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let profiles = fixture.root.path().join("profiles.toml");
    fs::write(&profiles, stripped).unwrap();
    let output = fixture.root.path().join("generated");
    let frameworks = vec!["svelte".to_string()];
    codegraph::driver::run(run_args(&fixture, &profiles, &frameworks, &output))
        .await
        .expect("pipeline run must succeed");

    let page = read(
        &output.join("ui"),
        "src/routes/(views)/requestform/+page.svelte",
    );
    assert!(page.contains("<editorForm.AppField name=\"title\">"), "{page}");
    assert!(
        page.contains("await fetch(`${API_BASE}` +"),
        "remote off ⇒ the client fetch transport:\n{page}"
    );
    // 422 field details map onto the matching fields.
    assert!(page.contains("body?.details"), "{page}");
    assert!(page.contains("fieldErrors = map"), "{page}");
    assert!(page.contains("serverMessage={fieldErrors.title}"), "{page}");
    // Edit-mode default values hydrate from the load's formData contract.
    assert!(
        page.contains("(editor_form_state.title ?? '') as string"),
        "{page}"
    );
}

#[tokio::test]
async fn ifml_tanstack_forms_off_keeps_the_hand_rolled_form() {
    let (_fixture, ui) = generate_integrated(false).await;

    let page = read(&ui, "src/routes/(views)/requestform/+page.svelte");
    assert!(
        page.contains("form.checkValidity()"),
        "the hand-rolled client validation handler must stay:\n{page}"
    );
    assert!(
        page.contains("Object.fromEntries(new FormData(form))"),
        "{page}"
    );
    assert!(!page.contains("AppField"), "{page}");
    assert!(!page.contains("zod"), "{page}");
    assert!(!page.contains("@tanstack"), "{page}");

    // No form plane anywhere in the IFML (views) tree.
    let mut files = Vec::new();
    walk(&ui.join("src/routes/(views)"), &mut files);
    let stray: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            p.to_string_lossy().contains("lib/forms")
                || fs::read_to_string(p)
                    .map(|c| c.contains("@tanstack/svelte-form"))
                    .unwrap_or(false)
        })
        .collect();
    assert!(
        stray.is_empty(),
        "flag off must emit no tanstack surface under (views): {stray:?}"
    );
}

#[tokio::test]
async fn ifml_tanstack_forms_skeleton_carries_the_dependencies() {
    let (_fixture, out) = generate_standalone(true).await;

    let package_json = read(&out, "svelte/package.json");
    assert!(
        package_json.contains(&format!(
            "\"@tanstack/svelte-form\": \"{TANSTACK_SVELTE_FORM_VERSION}\""
        )),
        "{package_json}"
    );
    assert!(
        package_json.contains(&format!("\"zod\": \"{ZOD_VERSION}\"")),
        "{package_json}"
    );

    // Standalone route shape renders the same plane.
    let page = read(&out, "svelte/src/routes/requestform/+page.svelte");
    assert!(page.contains("<editorForm.AppField name=\"title\">"), "{page}");
    assert!(
        page.contains("z.string().min(2, \"Title too short\")"),
        "{page}"
    );
    assert!(page.contains("data-testid=\"editor-submit\""), "{page}");

    // The validation spec family still emits, extended with the per-field
    // message-visibility assertion.
    let spec = read(&out, "svelte/tests/ifml/request-form.spec.ts");
    assert!(
        spec.contains("form validation blocks empty submit"),
        "{spec}"
    );
    assert!(
        spec.contains("data-validate-message"),
        "the flag extends the validation spec with the per-field message \
         visibility assertion:\n{spec}"
    );
    let pom = read(&out, "svelte/tests/pages/request-form-page.ts");
    assert!(pom.contains("editorInput(field: string)"), "{pom}");
    assert!(pom.contains("submitEditor()"), "{pom}");
}

#[tokio::test]
async fn ifml_tanstack_forms_off_keeps_the_plain_validation_spec() {
    let (_fixture, out) = generate_standalone(false).await;

    let spec = read(&out, "svelte/tests/ifml/request-form.spec.ts");
    assert!(
        spec.contains("form validation blocks empty submit"),
        "{spec}"
    );
    assert!(
        !spec.contains("data-validate-message"),
        "flag off keeps the pre-#580 validation spec:\n{spec}"
    );
    let mut files = Vec::new();
    walk(&out.join("svelte/src/lib"), &mut files);
    assert!(
        files
            .iter()
            .all(|p| !p.to_string_lossy().contains("forms/")),
        "flag off must emit no form plane: {files:?}"
    );
    let package_json = read(&out, "svelte/package.json");
    assert!(
        !package_json.contains("@tanstack/svelte-form") && !package_json.contains("\"zod\""),
        "{package_json}"
    );
}
