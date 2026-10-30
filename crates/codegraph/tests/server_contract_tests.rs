//! Node-free pin for the neutral server contract (issue #574 S1): the
//! shared `$lib/server/api` helper + the entity `+page.server.ts` loads on
//! it.
//!
//! #557/#573 gave the IFML plane a server-side fetch helper reading the
//! `IFML_API_ORIGIN`/`IFML_API_KEY` envs — semantically wrong names for a
//! general API contract (IFML is the interaction-flow DSL, not "the API"),
//! and the entity plane kept its own story: loads importing
//! `PUBLIC_API_URL`, raw `fetch` calls, `BASE_URL` consts. S1 gives every
//! generated SvelteKit tree ONE helper and ONE env story:
//!
//! - `src/lib/server/api.ts` — emitted if-absent for the entity plane too
//!   (identical source to the IFML plane's helper); origin resolves
//!   `API_ORIGIN ?? IFML_API_ORIGIN` (deprecated fallback), and the
//!   Authorization header resolves in order: explicit caller header →
//!   incoming request header (persona keys on remote RPCs) → the Supabase
//!   cookie session (`hooks.server.ts` parks it on `locals.apiKey`) →
//!   `API_KEY ?? IFML_API_KEY` env;
//! - entity loads (list/detail/edit/org-chart) fetch through `apiFetch`,
//!   passing the session's `locals.apiKey` as the explicit header — no
//!   `PUBLIC_API_URL` import, no `BASE_URL` const, no raw `fetch`.
//!
//! ```text
//! cargo test -p codegraph --test server_contract_tests
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

const DOMAINS_TOML: &str = r#"[defaults]
api_version = "v1"

[domains.portal]
label = "Portal"
schema_dir = "portal"
postgres_schema = "portal"
entities = ["Request"]

[domains.portal.entity_config.Request]
role = "root"
has_orgchart = true

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
}

fn write_fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("domains.toml"), DOMAINS_TOML).unwrap();
    fs::write(root.path().join("portal.mox"), PORTAL_MOX).unwrap();
    Fixture {
        config: root.path().join("domains.toml"),
        mox: root.path().join("portal.mox"),
        root,
    }
}

/// The workspace profiles.toml (the default profile carries the entity ui
/// generators).
fn profiles_path(fixture: &Fixture) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml");
    let content = fs::read_to_string(&workspace).expect("workspace profiles.toml");
    let path = fixture.root.path().join("profiles.toml");
    fs::write(&path, content).unwrap();
    path
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

async fn generate() -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let profiles = profiles_path(&fixture);
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
async fn entity_plane_emits_the_shared_server_api_helper() {
    let (_fixture, ui) = generate().await;

    let helper = read(&ui, "src/lib/server/api.ts");
    assert!(
        helper.contains("env.API_ORIGIN ?? env.IFML_API_ORIGIN"),
        "the neutral origin env wins; the deprecated IFML spelling is the fallback:\n{helper}"
    );
    assert!(
        helper.contains("env.API_KEY ?? env.IFML_API_KEY"),
        "the neutral key env wins; the deprecated IFML spelling is the fallback:\n{helper}"
    );
    assert!(
        !helper.contains("PUBLIC_API_"),
        "no public env plumbing may reach the server helper:\n{helper}"
    );
    assert!(
        helper.contains("getRequestEvent()"),
        "the incoming request's Authorization header is forwarded:\n{helper}"
    );
    assert!(
        helper.contains("headers.has('Authorization')"),
        "an explicit caller header wins over the forwarded/env legs:\n{helper}"
    );
    assert!(
        helper.contains("apiKey"),
        "the session leg: hooks.server.ts parks the Supabase cookie session on locals.apiKey:\n{helper}"
    );
    assert!(
        helper.contains("export async function apiFetch"),
        "{helper}"
    );
}

#[tokio::test]
async fn entity_loads_fetch_through_the_shared_helper() {
    let (_fixture, ui) = generate().await;

    // The list load: helper import, session header, no public env, no raw
    // fetch, no BASE_URL.
    let list = read(&ui, "src/routes/(app)/portal/request/+page.server.ts");
    assert!(
        list.contains("import { apiFetch } from '$lib/server/api';"),
        "{list}"
    );
    assert!(
        list.contains("'Authorization': `Bearer ${apiKey}`"),
        "the session's locals.apiKey rides the explicit header leg:\n{list}"
    );
    assert!(
        list.contains("throw error(401, 'Not authenticated')"),
        "the auth guard is preserved:\n{list}"
    );
    assert!(!list.contains("PUBLIC_API_URL"), "{list}");
    assert!(!list.contains("BASE_URL"), "{list}");
    assert!(!list.contains("skFetch"), "{list}");

    // (entity loads under param dirs, org-chart) shares the story.
    let mut files = Vec::new();
    walk(&ui.join("src").join("routes"), &mut files);
    let loads: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "+page.server.ts"))
        .collect();
    let detail = loads
        .iter()
        .find(|p| {
            let s = p.to_string_lossy();
            s.contains("/request/[") && !s.contains("/edit")
        })
        .expect("a detail +page.server.ts must be emitted");
    let edit = loads
        .iter()
        .find(|p| p.to_string_lossy().contains("edit"))
        .expect("an edit +page.server.ts must be emitted");

    // NOTE: the org-chart load (ui/orgchart_load.tera) is NOT asserted —
    // UiOrgchartGenerator has no registry construction site (the
    // ui_orgchart capability exists but nothing builds the generator), so
    // no current run path emits it. The template is migrated anyway so the
    // plane carries the contract when the wiring lands.

    for (label, load) in [
        ("detail", fs::read_to_string(detail).unwrap()),
        ("edit", fs::read_to_string(edit).unwrap()),
    ] {
        assert!(
            load.contains("import { apiFetch } from '$lib/server/api';"),
            "{label} load must import the shared helper:\n{load}"
        );
        assert!(
            !load.contains("PUBLIC_API_URL"),
            "{label} load must not import the public env:\n{load}"
        );
        assert!(
            !load.contains("BASE_URL"),
            "{label} load must not carry a BASE_URL const:\n{load}"
        );
    }

    // Behavior parity: the detail load keeps its 404 branch and workflow
    // merge; pagination rides the query string.
    let detail_src = fs::read_to_string(detail).unwrap();
    assert!(
        detail_src.contains("throw error(404, 'Request not found')"),
        "{detail_src}"
    );
    assert!(
        detail_src.contains("/workflow`"),
        "the workflow merge fetch rides the helper too:\n{detail_src}"
    );
    assert!(
        list.contains("const apiPath = `/api/v1/portal/request`;")
            && list.contains("apiFetch(`${apiPath}?${qs.toString()}`, { headers })"),
        "the list path is origin-relative; the helper owns the origin:\n{list}"
    );
}
