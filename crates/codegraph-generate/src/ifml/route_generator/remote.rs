//! The remote-functions data plane (issue #557): when the
//! `ifml_remote_functions` flag is on, every svelte view gains a colocated
//! `data.remote.ts` whose `query`/`command` exports execute on the
//! SvelteKit server and post through `$lib/server/api`. The browser only
//! ever calls kit's generated same-origin RPC endpoints — the API origin
//! (`API_ORIGIN`, issue #574's neutral contract with the deprecated
//! `IFML_API_ORIGIN` spelling as fallback) and its credentials never reach
//! the client. A relative same-origin fetch dies under a split gateway
//! topology and an unauthenticated request 401s at the gateway; this plane
//! removes both failure modes at once.
//!
//! Auth forwarding order (in [`crate::server_api`]): an explicit caller
//! header first, then the incoming RPC request's `Authorization` header
//! (Playwright persona contexts attach per-persona keys at the network
//! layer, so RLS isolation stays real), then the request's Supabase cookie
//! session (`locals.apiKey`), then the deployment's `API_KEY` env.

use super::context::{PageLoadComponentContext, PageLoadContext, RemoteCommand, RemoteCommandKind};

/// The module path every view's `data.remote.ts` imports the shared
/// server-side fetch helper from (`crate::server_api` — shared with the
/// entity plane's emitter).
pub(crate) const SERVER_API_IMPORT: &str = crate::server_api::SERVER_API_IMPORT;

/// One query export: the load's single fetch (list / item / form) lowered
/// to a `query('unchecked', ...)`.
pub struct RemoteQuery {
    /// Export name (`getGrid`).
    pub export: String,
    /// Collection path (`/api/v1/sales/customer`).
    pub base_path: String,
    /// Entity label for the failure message.
    pub entity: String,
    pub paginate: bool,
    pub workflow: bool,
    /// Machine read-model query (issue #817): a machine-ONLY entity's
    /// read-model route is `/machine` (no `/workflow` exists) — this
    /// query fetches it and normalizes the payload into the same
    /// `workflow_state` shape the dual plane's merge produces. The live
    /// stream polls the SAME route (extended, never duplicated).
    pub machine: bool,
    /// Item/form query: fetches by id instead of the collection.
    pub by_id: bool,
}

/// One command export: submit / transition / action op lowered to a
/// `command('unchecked', ...)`.
pub(super) struct RemoteCommandSpec {
    command: RemoteCommand,
}

/// The exports one view's `data.remote.ts` carries, collected from the
/// page + load contexts.
pub struct RemoteModuleContext {
    pub queries: Vec<RemoteQuery>,
    pub commands: Vec<RemoteCommandSpec>,
    /// The live-query plane (issue #574 S4c parity): workflow-bound by-id
    /// queries gain a `query.live` workflow stream (the 5s poll, the
    /// entity plane's `getWorkflowLive` pattern).
    pub live: bool,
    /// Media upload commands (issue #633) — one per wired file field.
    pub media: Vec<RemoteMediaCommand>,
}

/// One media upload command export (issue #633): the DSL `input file`
/// field lowered onto the entity's media route. The command runs
/// SERVER-side like every other export here, so the upload posts
/// multipart FormData through the shared helper and the bytes never ride
/// a browser credential.
pub(super) struct RemoteMediaCommand {
    /// `upload{Field}`.
    pub export: String,
    /// The entity collection path (`/api/v1/sales/customer`).
    pub base_path: String,
    /// The media field snake (the route's last segment).
    pub field: String,
    /// The view's id param name (`id`).
    pub id_param: String,
}

impl RemoteModuleContext {
    /// An empty module (the pin-support constructor).
    pub fn empty() -> Self {
        Self {
            queries: Vec::new(),
            commands: Vec::new(),
            live: false,
            media: Vec::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.queries.is_empty() && self.commands.is_empty() && self.media.is_empty()
    }

    /// Collect from a view's load context (the query leg).
    pub(super) fn from_load(load: &PageLoadContext) -> Self {
        let mut queries: Vec<RemoteQuery> = load
            .components
            .iter()
            .filter_map(|comp: &PageLoadComponentContext| {
                comp.remote_query.as_ref().map(|export| RemoteQuery {
                    export: export.clone(),
                    base_path: comp
                        .api
                        .as_ref()
                        .map(|api| api.base_path.clone())
                        .unwrap_or_default(),
                    entity: comp.entity.to_lowercase(),
                    paginate: comp.paginate,
                    workflow: comp.workflow,
                    machine: false,
                    by_id: comp.fetch_item || comp.fetch_form,
                })
            })
            .collect();
        // The machine read-model query (issue #817): machine-ONLY entities
        // (an authored machine, no workflow config) carry the read model
        // on `/machine` — one query per by-id claimant, feeding the load's
        // `workflow_state` merge and the live poll.
        for comp in &load.components {
            if let (Some(export), Some(api)) = (&comp.machine_query, &comp.api) {
                queries.push(RemoteQuery {
                    export: export.clone(),
                    base_path: api.base_path.clone(),
                    entity: comp.entity.to_lowercase(),
                    paginate: false,
                    workflow: false,
                    machine: true,
                    by_id: true,
                });
            }
        }
        Self {
            queries,
            commands: Vec::new(),
            live: false,
            media: Vec::new(),
        }
    }

    pub(super) fn push_command(&mut self, command: &RemoteCommand) {
        self.commands.push(RemoteCommandSpec {
            command: command.clone(),
        });
    }

    /// Render the `data.remote.ts` module source.
    pub(super) fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("// Generated by codegraph. DO NOT EDIT.\n");
        s.push_str("//\n");
        s.push_str("// (views) data plane (issue #557): remote functions run SERVER-side —\n");
        s.push_str("// the browser only calls the generated same-origin endpoints, so the API\n");
        s.push_str("// origin and its credentials never reach the client.\n\n");
        let needs_query = !self.queries.is_empty();
        let needs_command = !self.commands.is_empty() || !self.media.is_empty();
        let imports = match (needs_query, needs_command) {
            (true, true) => "import { command, query } from '$app/server';",
            (true, false) => "import { query } from '$app/server';",
            (false, true) => "import { command } from '$app/server';",
            (false, false) => "",
        };
        if !imports.is_empty() {
            s.push_str(imports);
            s.push('\n');
        }
        if needs_query {
            s.push_str("import { error } from '@sveltejs/kit';\n");
        }
        s.push_str(&format!(
            "import {{ apiFetch }} from '{SERVER_API_IMPORT}';\n"
        ));
        for q in &self.queries {
            s.push('\n');
            s.push_str(&render_query(q));
        }
        // The live read-model stream (issue #574 S4c parity, extended by
        // issue #817): a by-id claimant gains ONE `query.live` stream per
        // read-model route — dual entities stream the merged `/workflow`
        // (which now carries the machine object), machine-only entities
        // stream `/machine`. The page guards the instance with `browser` —
        // kit does not start streams during SSR.
        if self.live {
            for q in &self.queries {
                if !(q.workflow || q.machine) || !q.by_id {
                    continue;
                }
                let pascal = if q.machine {
                    // The machine query's export already carries the
                    // `Machine` suffix (`getClaimInfoMachine`): the live
                    // stream is `getClaimInfoMachineLive`.
                    q.export.strip_suffix("Machine").unwrap_or(&q.export)
                } else {
                    q.export.strip_prefix("get").unwrap_or(&q.export)
                };
                let suffix = if q.machine {
                    "MachineLive"
                } else {
                    "WorkflowLive"
                };
                let route = if q.machine { "machine" } else { "workflow" };
                s.push('\n');
                let normalize = if q.machine {
                    "{ ...data, machine: data }"
                } else {
                    "data"
                };
                s.push_str(&format!(
                    "export const get{pascal}{suffix} = query.live('unchecked', async function* (id: string) {{\n\
                     \twhile (true) {{\n\
                     \t\tconst res = await apiFetch(`{base}/${{id}}/{route}`);\n\
                     \t\tif (res.ok) {{\n\
                     \t\t\tconst json = await res.json();\n\
                     \t\t\tconst data = json.data ?? json;\n\
                     \t\t\tyield {normalize};\n\
                     \t\t}}\n\
                     \t\tawait new Promise((resolve) => setTimeout(resolve, 5000));\n\
                     \t}}\n\
                     }});\n",
                    base = q.base_path,
                    normalize = normalize,
                ));
            }
        }
        // The single-flight pairing (issue #574 S4a): a command's server
        // handler refreshes the queries whose client instances the page is
        // showing, so the fresh data rides the command response. Kit keys
        // instances by serialized args — a by-id query keyed with
        // `args.id` always matches, and a PLAIN (arg-less) collection
        // query keys as `()` which the command knows. Paginated
        // collections key on the load's page args, which the command
        // cannot know, so they are never refreshed blind.
        let item_refresh = self
            .queries
            .iter()
            .find(|q| q.by_id)
            .map(|q| q.export.clone());
        let list_refresh = self
            .queries
            .iter()
            .find(|q| !q.by_id && !q.paginate)
            .map(|q| q.export.clone());
        for m in &self.media {
            s.push('\n');
            s.push_str(&render_media_upload(m));
        }
        for c in &self.commands {
            s.push('\n');
            let (item, list) = match &c.command.kind {
                RemoteCommandKind::Create => (None, list_refresh.as_deref()),
                RemoteCommandKind::Update => (item_refresh.as_deref(), list_refresh.as_deref()),
                RemoteCommandKind::Transition | RemoteCommandKind::Op(_) => {
                    (item_refresh.as_deref(), None)
                }
            };
            s.push_str(&render_command(&c.command, item, list));
        }
        s
    }
}

fn render_query(q: &RemoteQuery) -> String {
    let mut s = String::new();
    if q.machine {
        // The machine read model (issue #817): normalized into the same
        // shape the merged /workflow payload nests — `{ ...machine,
        // machine }` — so the page's badge and button derivations read
        // ONE path (`workflow_state.machine.…`) on both planes. A failed
        // fetch yields null: the load merges nothing (badge-less) instead
        // of failing the page.
        s.push_str(&format!(
            "export const {export_} = query('unchecked', async (id: string) => {{\n",
            export_ = q.export
        ));
        s.push_str(&format!(
            "\tconst response = await apiFetch(`{base}/${{id}}/machine`);\n",
            base = q.base_path
        ));
        s.push_str("\tif (!response.ok) {\n\t\treturn null;\n\t}\n");
        s.push_str("\tconst json = await response.json();\n");
        s.push_str("\tconst machine = json.data ?? json;\n");
        s.push_str("\treturn { ...machine, machine };\n");
        s.push_str("});\n");
        return s;
    }
    if q.by_id {
        s.push_str(&format!(
            "export const {export_} = query('unchecked', async (id: string) => {{\n",
            export_ = q.export
        ));
        s.push_str(&format!(
            "\tconst response = await apiFetch(`{base}/${{id}}`);\n",
            base = q.base_path
        ));
        s.push_str("\tif (!response.ok) {\n");
        s.push_str(&format!(
            "\t\terror(response.status, 'Failed to load {entity}');\n",
            entity = q.entity
        ));
        s.push_str("\t}\n");
        s.push_str("\tconst json = await response.json();\n");
        s.push_str("\tconst item = json.data ?? json;\n");
        if q.workflow {
            s.push_str(&format!(
                "\tconst wf = await apiFetch(`{base}/${{id}}/workflow`);\n",
                base = q.base_path
            ));
            s.push_str("\tif (wf.ok) {\n");
            s.push_str("\t\tconst wfJson = await wf.json();\n");
            s.push_str("\t\titem.workflow_state = wfJson.data ?? wfJson;\n");
            s.push_str("\t}\n");
        }
        s.push_str("\treturn item;\n");
    } else {
        if q.paginate {
            s.push_str(&format!(
                "export const {export_} = query('unchecked', async (args: {{ page: string; pageSize: string }}) => {{\n",
                export_ = q.export
            ));
            s.push_str(&format!(
                "\tconst response = await apiFetch(`{base}?page=${{args.page}}&page_size=${{args.pageSize}}`);\n",
                base = q.base_path
            ));
        } else {
            s.push_str(&format!(
                "export const {export_} = query('unchecked', async () => {{\n",
                export_ = q.export
            ));
            s.push_str(&format!(
                "\tconst response = await apiFetch(`{base}`);\n",
                base = q.base_path
            ));
        }
        s.push_str("\tif (!response.ok) {\n");
        s.push_str(&format!(
            "\t\terror(response.status, 'Failed to load {entity}');\n",
            entity = q.entity
        ));
        s.push_str("\t}\n");
        s.push_str("\tconst data = await response.json();\n");
        s.push_str("\treturn Array.isArray(data) ? data : data.data ?? [];\n");
    }
    s.push_str("});\n");
    s
}

/// The multipart upload command (issue #633): the file arrives server-side
/// as a kit-serialized `File`, is appended to a FormData, and is posted to
/// the media route. Fetch sets the multipart boundary itself — the headers
/// are left alone (a `Content-Type` would truncate the body).
fn render_media_upload(m: &RemoteMediaCommand) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "export const {export} = command('unchecked', async (args: {{ id: string; file: File }}) => {{\n",
        export = m.export
    ));
    s.push_str("\tconst form = new FormData();\n");
    s.push_str("\tform.append('file', args.file, args.file.name);\n");
    s.push_str(&format!(
        "\tconst response = await apiFetch(`{base}/${{args.id}}/{field}`, {{\n\t\tmethod: 'PUT',\n\t\tbody: form,\n\t}});\n",
        base = m.base_path,
        field = m.field
    ));
    s.push_str("\tconst body = await response.json().catch(() => null);\n");
    s.push_str("\tif (!response.ok) {\n");
    s.push_str(
        "\t\treturn { ok: false as const, message: body?.error?.message ?? `Upload failed (${response.status})` };\n",
    );
    s.push_str("\t}\n");
    s.push_str(
        "\treturn { ok: true as const, key: body?.key as string | undefined, url: body?.url as string | undefined };\n",
    );
    s.push_str("});\n");
    let _ = &m.id_param;
    s
}

fn render_command(
    c: &RemoteCommand,
    item_refresh: Option<&str>,
    list_refresh: Option<&str>,
) -> String {
    let mut s = String::new();
    // The single-flight refresh block sits between the ok check and the
    // success return; kit awaits it and carries the payloads on the
    // command's response (issue #574 S4a). Create/Update share one merged
    // args type whose `id` is `string | null` (create mode), so their
    // refresh guards on it; Transition/Op args carry a plain `string`.
    let null_guarded = matches!(
        c.kind,
        RemoteCommandKind::Create | RemoteCommandKind::Update
    );
    let refresh_block = {
        let mut block = String::new();
        if let Some(export) = item_refresh {
            if null_guarded {
                block.push_str(&format!(
                    "\tif (args.id) {{\n\t\tvoid {export}(args.id).refresh();\n\t}}\n"
                ));
            } else {
                block.push_str(&format!("\tvoid {export}(args.id).refresh();\n"));
            }
        }
        if let Some(export) = list_refresh {
            block.push_str(&format!("\tvoid {export}().refresh();\n"));
        }
        block
    };
    match &c.kind {
        RemoteCommandKind::Create | RemoteCommandKind::Update => {
            s.push_str(&format!(
                "export const {export_} = command('unchecked', async (args: {{ id: string | null; data: Record<string, unknown> }}) => {{\n",
                export_ = c.export
            ));
            s.push_str(&format!(
                "\tconst response = await apiFetch(args.id ? `{base}/${{args.id}}` : `{base}`, {{\n",
                base = c.base_path
            ));
            s.push_str("\t\tmethod: args.id ? 'PUT' : 'POST',\n");
            s.push_str("\t\theaders: { 'Content-Type': 'application/json' },\n");
            s.push_str("\t\tbody: JSON.stringify(args.data)\n");
            s.push_str("\t});\n");
            s.push_str("\tconst created = await response.json().catch(() => null);\n");
            // The 422 `VALIDATION_ERROR` body's `error.details` parses into
            // `fieldErrors` (the #582 deferral, entity-plane parity) and the
            // message reads the error OBJECT level — `error` is
            // `{ code, message, details }`, never a bare string.
            s.push_str("\tif (!response.ok) {\n");
            s.push_str("\t\tconst fieldErrors: Record<string, string> = {};\n");
            s.push_str("\t\tconst details = created?.error?.details;\n");
            s.push_str("\t\tif (Array.isArray(details)) {\n");
            s.push_str("\t\t\tfor (const d of details) {\n");
            s.push_str("\t\t\t\tif (d && typeof d.field === 'string' && typeof d.message === 'string') {\n");
            s.push_str("\t\t\t\t\tfieldErrors[d.field] = d.message;\n");
            s.push_str("\t\t\t\t}\n");
            s.push_str("\t\t\t}\n");
            s.push_str("\t\t}\n");
            s.push_str("\t\treturn { ok: false, message: created?.error?.message ?? created?.message ?? `Request failed (${response.status})`, fieldErrors };\n");
            s.push_str("\t}\n");
            // The created/updated row id rides back for post-submit
            // navigation (issue #572: a create-mode submit navigates to
            // `${result.id ?? viewParams.<param>}` — the view param is
            // empty in create mode, so the row id is the only real target).
            s.push_str(&refresh_block);
            s.push_str("\treturn { ok: true, id: created?.data?.id ?? created?.id as string | undefined };\n");
            s.push_str("});\n");
        }
        kind => {
            let (args_type, path, body) = match kind {
                RemoteCommandKind::Transition => (
                    "{ id: string; to: string }",
                    "`{base}/${args.id}/actions/transition`".replace("{base}", &c.base_path),
                    "JSON.stringify({ target_state: args.to })",
                ),
                RemoteCommandKind::Op(op) => (
                    "{ id: string }",
                    format!(
                        "`{}/{}/actions/{op}`",
                        c.base_path.replace('`', ""),
                        "${args.id}"
                    ),
                    "JSON.stringify({})",
                ),
                RemoteCommandKind::Create | RemoteCommandKind::Update => unreachable!(),
            };
            s.push_str(&format!(
                "export const {export_} = command('unchecked', async (args: {args_type}) => {{\n",
                export_ = c.export
            ));
            s.push_str(&format!("\tconst response = await apiFetch({path}, {{\n"));
            s.push_str("\t\tmethod: 'POST',\n");
            s.push_str("\t\theaders: { 'Content-Type': 'application/json' },\n");
            s.push_str(&format!("\t\tbody: {body}\n"));
            s.push_str("\t});\n");
            s.push_str("\tif (!response.ok) {\n");
            s.push_str("\t\tconst body = await response.json().catch(() => null);\n");
            s.push_str("\t\treturn { ok: false, message: body?.error?.message ?? body?.message ?? `Request failed (${response.status})` };\n");
            s.push_str("\t}\n");
            s.push_str(&refresh_block);
            s.push_str("\treturn { ok: true };\n");
            s.push_str("});\n");
        }
    }
    s
}

/// The shared server-side fetch helper (`src/lib/server/api.ts`), emitted
/// once per output tree when any remote module is emitted. The SOURCE is
/// shared with the entity plane's emitter (`crate::server_api`) so both
/// planes carry the identical contract (issue #574 S1).
pub(crate) fn render_server_api_helper() -> String {
    crate::server_api::server_api_helper_ts().to_string()
}
