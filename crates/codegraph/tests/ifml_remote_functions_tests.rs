//! Node-free pin for the IFML remote-functions data plane (issue #557,
//! `ifml_remote_functions`).
//!
//! Master's first #557 slice resolved `PUBLIC_API_URL` and attached a
//! `PUBLIC_API_KEY` Bearer on CLIENT-side fetches — workable, but the key
//! is baked into the browser bundle. The remote-functions plane replaces
//! it: view loads/posts call server-executed `data.remote.ts` exports
//! (`$app/server` query/command) that post through `$lib/server/api`
//! against the neutral `API_ORIGIN` (issue #574 S1; the deprecated
//! `IFML_API_ORIGIN` spelling reads as the fallback) — the origin and
//! credentials never reach the client, and the same-origin RPC hop removes
//! the split-topology 404/401 class entirely.
//!
//! Pins:
//!
//! - `src/lib/server/api.ts` — the server-only fetch helper
//!   (`$env/dynamic/private` + `getRequestEvent` auth forwarding);
//! - per-view `data.remote.ts` — `query('unchecked', ...)` for loads,
//!   `command('unchecked', ...)` for submit/transition/action posts;
//! - view `+page.ts`/`+page.svelte` — `await get…()` / `await submit…({…})`
//!   call sites and ZERO client `PUBLIC_API_KEY` plumbing;
//! - flag off — no `data.remote.ts` anywhere; the (views) client
//!   `API_BASE`/`apiHeaders` plane stays (the entity plane's loads share
//!   the helper unconditionally since #574 S1).
//!
//! ```text
//! cargo test -p codegraph --test ifml_remote_functions_tests
//! ```

use std::fs;
use std::path::{Path, PathBuf};

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
        pagination: true;

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

        on save -> navigate("Home");
        on cancel -> navigate("Home");
        on escalate -> action("EscalateRequest");
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

[domains.portal.entity_config.Request.workflow]
status_field = "status"
states = ["Draft", "Submitted", "Approved"]
initial_state = "Draft"
terminal_states = ["Approved"]
generate_action_endpoints = true

[domains.portal.entity_config.Request.workflow.transitions]
Draft = ["Submitted"]
Submitted = ["Approved"]
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
/// `ifml_remote_functions = true`), or a copy with the flag stripped for
/// the flag-off pin.
fn profiles_path(fixture: &Fixture, with_flag: bool) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml");
    let content = fs::read_to_string(&workspace).expect("workspace profiles.toml");
    let path = fixture.root.path().join("profiles.toml");
    if with_flag {
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

async fn generate(with_flag: bool) -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let profiles = profiles_path(&fixture, with_flag);
    let output = fixture.root.path().join("generated");
    let frameworks = vec!["svelte".to_string()];
    codegraph::driver::run(run_args(&fixture, &profiles, &frameworks, &output))
        .await
        .expect("pipeline run must succeed");
    // Integrated mode (issue #468 W1b): the default profile builds the
    // entity ui app, so the IFML tree lands under ui/.
    (fixture, output.join("ui"))
}

fn read(base: &Path, rel: &str) -> String {
    fs::read_to_string(base.join(rel))
        .unwrap_or_else(|e| panic!("expected {rel} to be emitted: {e}"))
}

#[tokio::test]
async fn remote_functions_flag_emits_the_server_side_data_plane() {
    let (_fixture, ui) = generate(true).await;

    // The server-only fetch helper.
    let helper = read(&ui, "src/lib/server/api.ts");
    assert!(
        helper.contains("$env/dynamic/private"),
        "the origin comes from private env, never the public bundle:\n{helper}"
    );
    assert!(
        helper.contains("env.API_ORIGIN ?? env.IFML_API_ORIGIN"),
        "the neutral origin env wins; the deprecated IFML spelling is the fallback:\n{helper}"
    );
    assert!(
        helper.contains("env.API_KEY ?? env.IFML_API_KEY"),
        "the neutral key env wins; the deprecated IFML spelling is the fallback:\n{helper}"
    );
    assert!(
        helper.contains("getRequestEvent()"),
        "the incoming RPC request's Authorization header is forwarded (persona isolation):\n{helper}"
    );
    assert!(
        helper.contains("headers.has('Authorization')"),
        "an explicit caller header wins over the forwarded/env legs (issue #574 S1):\n{helper}"
    );
    assert!(
        helper.contains("apiKey"),
        "the session leg: hooks.server.ts parks the Supabase cookie session on locals.apiKey:\n{helper}"
    );

    // The list view's remote module + load.
    let home_remote = read(&ui, "src/routes/(views)/home/data.remote.ts");
    assert!(home_remote.contains("query('unchecked'"), "{home_remote}");
    assert!(
        home_remote.contains("import { apiFetch } from '$lib/server/api';"),
        "{home_remote}"
    );
    assert!(
        home_remote.contains("page_size=${args.pageSize}"),
        "pagination rides the query args:\n{home_remote}"
    );
    let home_load = read(&ui, "src/routes/(views)/home/+page.ts");
    assert!(home_load.contains("await getHomeGrid("), "{home_load}");
    assert!(
        !home_load.contains("PUBLIC_API_KEY"),
        "the client key must be gone:\n{home_load}"
    );
    assert!(
        !home_load.contains("await fetch("),
        "no client fetch may remain:\n{home_load}"
    );

    // The form view: submit + workflow transition + action op commands.
    let form_remote = read(&ui, "src/routes/(views)/requestform/data.remote.ts");
    assert!(
        form_remote.contains("export const submitEditor = command('unchecked'"),
        "{form_remote}"
    );
    assert!(
        form_remote.contains("export const transitionEditor = command('unchecked'"),
        "the workflow transition posts through a command:\n{form_remote}"
    );
    assert!(
        form_remote.contains("export const escalateRequest = command('unchecked'"),
        "the #559 action op posts through a command:\n{form_remote}"
    );
    assert!(
        form_remote.contains("target_state: args.to"),
        "{form_remote}"
    );
    let form_page = read(&ui, "src/routes/(views)/requestform/+page.svelte");
    assert!(form_page.contains("await submitEditor({"), "{form_page}");
    assert!(
        form_page.contains("id: viewParams.id || null,"),
        "the edit branch forwards the row id (server PUTs the item):\n{form_page}"
    );
    assert!(
        !form_page.contains("PUBLIC_API_KEY"),
        "the client key must be gone:\n{form_page}"
    );
    assert!(
        !form_page.contains("await fetch("),
        "no client fetch may remain:\n{form_page}"
    );
}

#[tokio::test]
async fn remote_functions_off_keeps_the_client_plane() {
    let (_fixture, ui) = generate(false).await;

    // No remote surface anywhere.
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
    let mut files = Vec::new();
    walk(&ui, &mut files);
    let stray: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "data.remote.ts"))
        .collect();
    assert!(
        stray.is_empty(),
        "flag off must emit no remote-function surface: {stray:?}"
    );
    // (views) keeps no server helper import of its own: `src/lib/server/
    // api.ts` exists only because the ENTITY plane's +page.server.ts loads
    // share it (issue #574 S1) — the (views) client plane must not touch it.
    let home_load = read(&ui, "src/routes/(views)/home/+page.ts");
    assert!(!home_load.contains("$lib/server/api"), "{home_load}");

    // The client API_BASE/apiHeaders plane stays.
    let home_load = read(&ui, "src/routes/(views)/home/+page.ts");
    assert!(home_load.contains("PUBLIC_API_URL"), "{home_load}");
    assert!(home_load.contains("const API_BASE"), "{home_load}");
    let form_page = read(&ui, "src/routes/(views)/requestform/+page.svelte");
    assert!(form_page.contains("const apiHeaders"), "{form_page}");
    assert!(
        form_page.contains("await fetch(`${API_BASE}` +"),
        "{form_page}"
    );
    assert!(!form_page.contains("data.remote"), "{form_page}");
}
