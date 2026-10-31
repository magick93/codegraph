//! Node-free pin for the entity remote-functions data plane (issue #574
//! S2, `entity_remote_functions`).
//!
//! The entity plane carried the #557 disease one plane over: browser-side
//! `#lib/stores/*` doing `apiGet/Post/Put/Delete` (including workflow
//! transitions) backed by `PUBLIC_API_KEY`, which `env.ts` even
//! institutionalized in `defineEnvVars`. S2 replaces it with SvelteKit
//! remote functions — the proven #573 IFML pattern: colocated
//! `data.remote.ts` modules whose `query('unchecked', …)` /
//! `command('unchecked', …)` exports execute on the SvelteKit server and
//! post through the shared `$lib/server/api` helper (issue #574 S1). The
//! `+page.server.ts` loads keep their `items`/`item` contracts (POM/e2e
//! hold) but await the remote queries; page handlers call the commands'
//! `{ ok, … }` contract; the stores and the `PUBLIC_API_KEY` client plane
//! retire.
//!
//! Pins (flag ON):
//!
//! - per-entity `data.remote.ts` — `getList`/`getItem` queries,
//!   `createItem`/`updateItem`/`deleteItem`/`transitionItem` commands,
//!   child-section queries, all through `$lib/server/api`;
//! - loads await the remote queries — no `apiFetch`/`PUBLIC_API_URL` in
//!   the loads, no per-page 401 throw (the `(app)` layout guard owns the
//!   session redirect);
//! - pages call the commands — ZERO `#lib/stores/*` CRUD imports and ZERO
//!   `#lib/api/client.ts` imports in the emitted (app) pages;
//! - the entity store is NOT emitted; `api/client.ts` + `env.ts` lose
//!   `PUBLIC_API_KEY`;
//! - the ui scaffold carries the remote-functions experimental options.
//!
//! Flag off — none of the above; the store/client plane stays
//! byte-identical.
//!
//! ```text
//! cargo test -p codegraph --test entity_remote_functions_tests
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

/// The workspace profiles.toml, with the `entity_remote_functions` flag
/// kept (ON) or stripped (OFF).
fn profiles_path(fixture: &Fixture, with_flag: bool) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles.toml");
    let content = fs::read_to_string(&workspace).expect("workspace profiles.toml");
    let path = fixture.root.path().join("profiles.toml");
    if with_flag {
        fs::write(&path, content).unwrap();
    } else {
        let stripped = content
            .lines()
            .filter(|line| !line.trim_start().starts_with("entity_remote_functions"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, stripped).unwrap();
    }
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

async fn generate(with_flag: bool) -> (Fixture, PathBuf) {
    let fixture = write_fixture();
    let profiles = profiles_path(&fixture, with_flag);
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
async fn entity_remote_functions_flag_emits_the_server_side_plane() {
    let (_fixture, ui) = generate(true).await;

    // The colocated remote module: queries + commands through the shared
    // server helper — one module per route, carrying only that route's
    // surface.
    let remote = read(&ui, "src/routes/(app)/portal/request/data.remote.ts");
    assert!(
        remote.contains("import { apiFetch } from '$lib/server/api';"),
        "{remote}"
    );
    assert!(remote.contains("query('unchecked'"), "{remote}");
    assert!(remote.contains("command('unchecked'"), "{remote}");
    assert!(remote.contains("export const getList"), "{remote}");
    assert!(
        remote.contains("export const deleteItem"),
        "the ux row-action delete rides a list-route command:\n{remote}"
    );
    assert!(!remote.contains("getItem"), "{remote}");
    assert!(!remote.contains("transitionItem"), "{remote}");

    // The create route's module carries the create command only.
    let create_remote = read(&ui, "src/routes/(app)/portal/request/new/data.remote.ts");
    assert!(
        create_remote.contains("export const createItem"),
        "{create_remote}"
    );
    assert!(
        create_remote.contains("id: (json.data ?? json).id as string"),
        "the create command returns the created id:\n{create_remote}"
    );

    // The edit route's module: getItem + updateItem.
    let edit_remote = read(
        &ui,
        "src/routes/(app)/portal/request/[request_id]/edit/data.remote.ts",
    );
    assert!(
        edit_remote.contains("export const getItem"),
        "{edit_remote}"
    );
    assert!(
        edit_remote.contains("export const updateItem"),
        "{edit_remote}"
    );

    // The workflow transition posts through the DETAIL route's command.
    let detail_remote = read(
        &ui,
        "src/routes/(app)/portal/request/[request_id]/data.remote.ts",
    );
    assert!(
        detail_remote.contains("export const transitionItem"),
        "the workflow transition posts through a command:\n{detail_remote}"
    );
    assert!(
        detail_remote.contains("target_state: args.to"),
        "{detail_remote}"
    );
    assert!(
        detail_remote.contains("/actions/transition"),
        "{detail_remote}"
    );
    assert!(
        detail_remote.contains("item.workflow_state = wfJson.data ?? wfJson;"),
        "the workflow merge rides the detail query:\n{detail_remote}"
    );

    // The list load awaits the remote query — no apiFetch, no per-page
    // auth throw (the (app) layout guard owns the session redirect).
    let list_load = read(&ui, "src/routes/(app)/portal/request/+page.server.ts");
    assert!(
        list_load.contains("await getList("),
        "the load awaits the remote query:\n{list_load}"
    );
    assert!(!list_load.contains("apiFetch"), "{list_load}");
    assert!(!list_load.contains("PUBLIC_API_URL"), "{list_load}");
    assert!(!list_load.contains("Not authenticated"), "{list_load}");
    assert!(
        !list_load.contains("Bearer ${apiKey}"),
        "no explicit session header: the helper's session leg covers it:\n{list_load}"
    );

    // The detail load awaits getItem; the edit load reuses it too.
    let detail_load = read(
        &ui,
        "src/routes/(app)/portal/request/[request_id]/+page.server.ts",
    );
    assert!(detail_load.contains("await getItem("), "{detail_load}");
    assert!(!detail_load.contains("apiFetch"), "{detail_load}");
    let edit_load = read(
        &ui,
        "src/routes/(app)/portal/request/[request_id]/edit/+page.server.ts",
    );
    assert!(edit_load.contains("await getItem("), "{edit_load}");

    // Pages call the commands; no store imports, no api-client imports.
    let form_page = read(&ui, "src/routes/(app)/portal/request/new/+page.svelte");
    assert!(form_page.contains("await createItem("), "{form_page}");
    assert!(
        form_page.contains("result.ok"),
        "the command's {{ok}} contract drives the error UX:\n{form_page}"
    );
    assert!(
        !form_page.contains("#lib/stores/"),
        "the store plane is retired:\n{form_page}"
    );
    assert!(!form_page.contains("#lib/api/client.ts"), "{form_page}");

    let edit_page = read(
        &ui,
        "src/routes/(app)/portal/request/[request_id]/edit/+page.svelte",
    );
    assert!(edit_page.contains("await updateItem("), "{edit_page}");
    assert!(!edit_page.contains("#lib/stores/"), "{edit_page}");

    let detail_page = read(
        &ui,
        "src/routes/(app)/portal/request/[request_id]/+page.svelte",
    );
    assert!(
        detail_page.contains("await transitionItem("),
        "{detail_page}"
    );
    assert!(detail_page.contains("await deleteItem("), "{detail_page}");
    assert!(
        !detail_page.contains("#lib/stores/portal_request"),
        "{detail_page}"
    );
    assert!(!detail_page.contains("#lib/api/client.ts"), "{detail_page}");

    // Every (app) page is free of the retired browser planes.
    let mut files = Vec::new();
    walk(&ui.join("src").join("routes").join("(app)"), &mut files);
    let stray: Vec<String> = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "svelte"))
        .filter_map(|p| {
            let content = fs::read_to_string(p).unwrap_or_default();
            let path = p.to_string_lossy().into_owned();
            if content.contains("#lib/api/client.ts")
                || content.contains("'#lib/stores/portal_request'")
            {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    assert!(
        stray.is_empty(),
        "(app) pages must not touch the retired client planes: {stray:?}"
    );

    // The entity store is NOT emitted.
    assert!(
        !ui.join("src/lib/stores/portal_request.ts").exists(),
        "the entity store retires under the flag"
    );

    // The webhook settings pages migrate to the load/command plane —
    // including the rotate-secret command.
    let wh_list = read(&ui, "src/routes/(app)/settings/webhooks/+page.server.ts");
    assert!(wh_list.contains("await getEndpoints("), "{wh_list}");
    let wh_list_remote = read(&ui, "src/routes/(app)/settings/webhooks/data.remote.ts");
    assert!(
        wh_list_remote.contains("export const deleteEndpoint"),
        "{wh_list_remote}"
    );
    let wh_detail_load = read(
        &ui,
        "src/routes/(app)/settings/webhooks/[id]/+page.server.ts",
    );
    assert!(
        wh_detail_load.contains("await getEndpointData("),
        "{wh_detail_load}"
    );
    let wh_detail_remote = read(
        &ui,
        "src/routes/(app)/settings/webhooks/[id]/data.remote.ts",
    );
    assert!(
        wh_detail_remote.contains("export const rotateSecret"),
        "{wh_detail_remote}"
    );
    assert!(
        wh_detail_remote.contains("rotate-secret"),
        "{wh_detail_remote}"
    );
    assert!(
        wh_detail_remote.contains("export const deleteSubscription"),
        "{wh_detail_remote}"
    );
    let wh_form = read(&ui, "src/routes/(app)/settings/webhooks/new/+page.svelte");
    assert!(wh_form.contains("await createEndpoint("), "{wh_form}");
    assert!(!wh_form.contains("#lib/api/client.ts"), "{wh_form}");
    let wh_detail_page = read(&ui, "src/routes/(app)/settings/webhooks/[id]/+page.svelte");
    assert!(
        wh_detail_page.contains("await rotateSecret("),
        "{wh_detail_page}"
    );
    assert!(
        !wh_detail_page.contains("#lib/api/client.ts"),
        "{wh_detail_page}"
    );

    // The client key retires from api_client.ts + env.ts.
    let api_client = read(&ui, "src/lib/api/client.ts");
    assert!(
        !api_client.contains("PUBLIC_API_KEY"),
        "the browser key must be gone from the client:\n{api_client}"
    );
    let env_ts = read(&ui, "src/env.ts");
    assert!(
        !env_ts.contains("PUBLIC_API_KEY"),
        "the browser key must be gone from defineEnvVars:\n{env_ts}"
    );

    // The scaffold opts into the remote-functions toolchain.
    let vite = read(&ui, "vite.config.ts");
    assert!(vite.contains("remoteFunctions: true"), "{vite}");
    assert!(vite.contains("async: true"), "{vite}");
}

#[tokio::test]
async fn entity_remote_functions_off_keeps_the_client_plane() {
    let (_fixture, ui) = generate(false).await;

    // No remote surface under (app).
    let mut files = Vec::new();
    walk(&ui.join("src").join("routes").join("(app)"), &mut files);
    let stray: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            p.file_name().is_some_and(|n| n == "data.remote.ts")
                || p.file_name().is_some_and(|n| n == "+page.server.ts") && {
                    let c = fs::read_to_string(p).unwrap_or_default();
                    c.contains("data.remote") || c.contains("await getList(")
                }
        })
        .collect();
    assert!(
        stray.is_empty(),
        "flag off must emit no remote surface: {stray:?}"
    );

    // The store stays with its CRUD surface.
    let store = read(&ui, "src/lib/stores/portal_request.ts");
    assert!(
        store.contains("export async function createRequest"),
        "{store}"
    );
    assert!(
        store.contains("export async function transitionRequest"),
        "{store}"
    );
    assert!(store.contains("#lib/api/client.ts"), "{store}");

    // The loads keep the S1 server-fetch shape.
    let list_load = read(&ui, "src/routes/(app)/portal/request/+page.server.ts");
    assert!(list_load.contains("apiFetch"), "{list_load}");
    assert!(
        list_load.contains("throw error(401, 'Not authenticated')"),
        "{list_load}"
    );

    // The pages keep the store imports.
    let form_page = read(&ui, "src/routes/(app)/portal/request/new/+page.svelte");
    assert!(
        form_page.contains("#lib/stores/portal_request"),
        "{form_page}"
    );

    // The client key stays (flag-off byte-identity).
    let api_client = read(&ui, "src/lib/api/client.ts");
    assert!(api_client.contains("PUBLIC_API_KEY"), "{api_client}");
    let env_ts = read(&ui, "src/env.ts");
    assert!(env_ts.contains("PUBLIC_API_KEY"), "{env_ts}");

    // The webhook settings plane keeps the browser apiGet surface and
    // gains no remote modules.
    let wh_page = read(&ui, "src/routes/(app)/settings/webhooks/+page.svelte");
    assert!(wh_page.contains("#lib/api/client.ts"), "{wh_page}");
    assert!(
        !ui.join("src/routes/(app)/settings/webhooks/data.remote.ts")
            .exists(),
        "flag off emits no webhook remote module"
    );
    assert!(
        !ui.join("src/routes/(app)/settings/webhooks/+page.server.ts")
            .exists(),
        "flag off emits no webhook settings load"
    );

    // No experimental options in the scaffold.
    let vite = read(&ui, "vite.config.ts");
    assert!(!vite.contains("remoteFunctions"), "{vite}");
}
