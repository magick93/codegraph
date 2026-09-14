use crate::ProjectConfig;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::{DomainConfig, IfmlComponentMapping, IfmlComponentMappings, SemanticRole};
use codegraph_core::traits::GraphQuerier;
use codegraph_ifml_dsl::{
    BinOp, ChartKind, ChartSpec, ColumnDef, ComponentSpec, Expression, FormSpec, InputFieldType,
    TableSpec, UnaryOp,
};
use serde::Serialize;

use crate::error::Result;
use crate::render_template;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;

use super::api_paths::{id_param_from, resolve_entity_api, ResolvedApi};
use super::context::{IfmlAction, IfmlComponent, IfmlEvent, IfmlViewContainer};
use super::querier::*;

pub struct IfmlRouteGenerator {
    output_dir: PathBuf,
    framework: String,
    output_paths: super::output_paths::OutputPaths,
    mappings: Option<IfmlComponentMappings>,
}

impl IfmlRouteGenerator {
    pub fn new(output_dir: &Path, framework: &str) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            framework: framework.to_string(),
            output_paths: super::output_paths::OutputPaths::for_framework(framework),
            mappings: None,
        }
    }

    pub fn with_mappings(mut self, mappings: Option<IfmlComponentMappings>) -> Self {
        self.mappings = mappings;
        self
    }

    /// Framework layout path for the landmark shell; `None` for frameworks
    /// without a layout convention in this slice.
    fn route_layout(&self) -> Option<PathBuf> {
        match self.framework.as_str() {
            "svelte" => Some(PathBuf::from("src/routes/+layout.svelte")),
            _ => None,
        }
    }

    /// Layout render context when a landmark view exists and a `shell`
    /// mapping resolves; `None` emits no layout (byte-identical no-pack
    /// output).
    fn layout_context(&self, model: &super::context::IfmlModel) -> Option<LayoutSvelteContext> {
        self.route_layout()?;
        shell_nav(&model.view_containers, self.mappings.as_ref())
            .map(|shell| LayoutSvelteContext { shell })
    }

    /// The `$lib/roles` helper when at least one view declares roles or
    /// capability requirements and the framework has the SvelteKit load
    /// convention; `None` otherwise. An existing file is never overwritten.
    /// Content varies: with an ingested policy the helper embeds the
    /// actor→effective-capabilities map; with requirements but no policy
    /// `can()` consults the runtime `__USER_CAPABILITIES__` override only.
    fn roles_helper(&self, model: &super::context::IfmlModel) -> Option<GeneratedFile> {
        if self.framework != "svelte" {
            return None;
        }
        let any_roles = model.view_containers.iter().any(|vc| !vc.roles.is_empty());
        let any_requires = model
            .view_containers
            .iter()
            .any(|vc| !vc.requires.is_empty());
        if !any_roles && !any_requires {
            return None;
        }
        let path = self.output_dir.join("src/lib/roles.ts");
        if path.exists() {
            return None;
        }
        let content = match &model.policy {
            Some(policy) => policy_roles_ts(policy),
            None if any_requires => REQUIRES_ONLY_ROLES_TS.to_string(),
            None => ROLES_TS.to_string(),
        };
        Some(GeneratedFile { path, content })
    }
}

#[async_trait]
impl GlobalGenerator for IfmlRouteGenerator {
    fn name(&self) -> &str {
        "ifml-route"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let querier = IfmlGraphQuerier::new(db);
        let model = querier
            .get_ifml_model()
            .await
            .map_err(crate::error::Error::Graph)?;

        if model.view_containers.is_empty() {
            return Ok(vec![]);
        }

        let mut files = vec![];

        let page_template = format!("ifml/{}/page.tera", self.framework);
        let load_template = format!("ifml/{}/page_load.tera", self.framework);

        let modal_targets: HashSet<String> = model
            .view_containers
            .iter()
            .filter(|vc| modal_wrapper_active(vc.is_modal, &vc.name, self.mappings.as_ref()))
            .map(|vc| vc.name.clone())
            .collect();
        let denial = denial_target(&model);

        for vc in ordered_view_containers(&model) {
            let ctx = build_page_context(
                db,
                config,
                &project.api_version,
                vc,
                self.mappings.as_ref(),
                &modal_targets,
            )
            .await;

            if let Ok(content) = render_template(tera, &page_template, &ctx) {
                files.push(GeneratedFile {
                    path: self
                        .output_dir
                        .join((self.output_paths.route_page)(&vc.name)),
                    content,
                });
            }

            if let Some(ref route_load_fn) = self.output_paths.route_load {
                let load_ctx =
                    build_load_context(&project.api_version, vc, &ctx.components, &denial);
                if let Ok(content) = render_template(tera, &load_template, &load_ctx) {
                    files.push(GeneratedFile {
                        path: self.output_dir.join(route_load_fn(&vc.name)),
                        content,
                    });
                }
            }
        }

        if let Some(roles_helper) = self.roles_helper(&model) {
            files.push(roles_helper);
        }

        if let Some(ctx) = self.layout_context(&model) {
            let layout_template = format!("ifml/{}/layout.tera", self.framework);
            if let Ok(content) = render_template(tera, &layout_template, &ctx) {
                if let Some(rel) = self.route_layout() {
                    files.push(GeneratedFile {
                        path: self.output_dir.join(rel),
                        content,
                    });
                }
            }
        }

        Ok(files)
    }
}

/// `$lib/roles` helper emitted once per project when any view carries roles.
/// Consumers with real auth populate `__USER_ROLES__` (e.g. from
/// `+layout.ts` server data); unset means no roles and every guarded page
/// redirects to the denial target.
const ROLES_TS: &str = r#"// Generated by codegraph. DO NOT EDIT.
export function currentRoles(): string[] {
	return (globalThis as any).__USER_ROLES__ ?? [];
}
"#;

/// `$lib/roles` helper variant for models with capability requirements but
/// no ingested policy: `can()` consults the runtime `__USER_CAPABILITIES__`
/// override only.
const REQUIRES_ONLY_ROLES_TS: &str = r#"// Generated by codegraph. DO NOT EDIT.
export function currentRoles(): string[] {
	return (globalThis as any).__USER_ROLES__ ?? [];
}

export function can(capability: string): boolean {
	const held: string[] = (globalThis as any).__USER_CAPABILITIES__ ?? [];
	return held.includes(capability);
}
"#;

/// `$lib/roles` helper for models with an ingested policy: the
/// generation-time actor→effective-capabilities map (resolved through
/// `extends` with forbid-wins) unioned with the runtime
/// `__USER_CAPABILITIES__` override.
fn policy_roles_ts(policy: &super::context::PolicyContext) -> String {
    let mut ts = String::from(
        "// Generated by codegraph. DO NOT EDIT.\n\
         export function currentRoles(): string[] {\n\
         \treturn (globalThis as any).__USER_ROLES__ ?? [];\n\
         }\n\n\
         const ROLE_CAPABILITIES: Record<string, string[]> = {\n",
    );
    for (actor, caps) in &policy.actors {
        let list = caps
            .iter()
            .map(|cap| js_quote(cap))
            .collect::<Vec<_>>()
            .join(", ");
        ts.push_str(&format!("\t{}: [{}],\n", js_quote(actor), list));
    }
    ts.push_str(
        "};\n\n\
         export function can(capability: string): boolean {\n\
         \tconst held = new Set<string>((globalThis as any).__USER_CAPABILITIES__ ?? []);\n\
         \tfor (const role of currentRoles()) {\n\
         \t\tfor (const cap of ROLE_CAPABILITIES[role] ?? []) {\n\
         \t\t\theld.add(cap);\n\
         \t\t}\n\
         \t}\n\
         \treturn held.has(capability);\n\
         }\n",
    );
    ts
}

/// The denial redirect target for guarded views: the first view (graph
/// order) carrying neither roles nor capability requirements — a page any
/// denied visitor can see — else `/`.
pub(crate) fn denial_target(model: &super::context::IfmlModel) -> String {
    model
        .view_containers
        .iter()
        .find(|vc| vc.roles.is_empty() && vc.requires.is_empty())
        .map(|vc| format!("/{}", vc.name.to_lowercase()))
        .unwrap_or_else(|| "/".to_string())
}

/// Order view containers by the model's computed generation order
/// (targets before sources). Containers absent from the order keep their
/// graph order at the end.
fn ordered_view_containers(
    model: &super::context::IfmlModel,
) -> Vec<&super::context::IfmlViewContainer> {
    let position: HashMap<&str, usize> = model
        .generation_order
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    let mut containers: Vec<&super::context::IfmlViewContainer> =
        model.view_containers.iter().collect();
    containers.sort_by_key(|vc| {
        position
            .get(vc.name.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    containers
}

#[derive(Debug, Serialize)]
pub struct PageSvelteContext {
    pub api_version: String,
    name: String,
    label: String,
    components: Vec<PageComponentContext>,
    params: Vec<super::context::ParameterDef>,
    view_events: Vec<RenderEvent>,
    imports: Vec<RenderImport>,
    needs_goto: bool,
    needs_on_mount: bool,
    has_submit: bool,
    /// Semantic slot role of the view: `modal-view` for modals, else
    /// `shell` for landmarks.
    view_role: Option<SemanticRole>,
    /// Semantic slot role of the container grouping: `presentation-container`
    /// for xor/wizard containers.
    container_role: Option<SemanticRole>,
    /// Roles allowed to view this page; empty when unrestricted. v1 keeps a
    /// single enforcement point in `+page.ts` — markup is role-agnostic.
    pub roles: Vec<String>,
    /// Capabilities required to view this page; empty when unrestricted.
    pub requires: Vec<String>,
    /// View-level markup gate for interactive controls; empty (no gate)
    /// for unguarded views — byte-identical output.
    control_gate: ControlGateContext,
    /// Modal wrapper for `modal: true` views with a resolved mapping or a
    /// non-empty mapping pack; `None` renders the plain page.
    modal: Option<RenderModal>,
    /// Presentation-container wrapper for xor view containers with a
    /// resolved mapping or a non-empty mapping pack; `None` renders the
    /// plain page.
    container: Option<RenderContainer>,
    /// View parameter names; non-empty emits the page-level `viewParams`
    /// derived const (query-param resolution — SvelteKit views have no
    /// dynamic segments here, so route params are always empty).
    view_params: Vec<String>,
}

/// Markup gate for a view's interactive controls (form submit/cancel buttons
/// and mapped action-control buttons): unauthorized users see nothing
/// instead of a dead button. Mirrors the `+page.ts` load guard exactly —
/// the requires-check AND the roles-check, each skipped when empty. The
/// load guard stays authoritative (it redirects); this is cosmetic hiding.
/// Whole-component mapped invocations are never wrapped — the mapped
/// component owns its internals.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ControlGateContext {
    /// `$lib/roles` import names the gate needs (`currentRoles`, `can`).
    pub imports: Vec<String>,
    /// Ready-to-render const declarations injected into the script
    /// (`const viewRequires = [...];`, `const viewRoles = [...];`,
    /// `const roles = currentRoles();`).
    pub consts: String,
    /// Ready-to-render opening marker `{#if <expr>}`; empty when inactive.
    pub open: String,
    /// Ready-to-render closing marker `{/if}`; empty when inactive.
    pub close: String,
}

impl ControlGateContext {
    /// The gate for a view: inactive unless the view is guarded AND a
    /// fallback form branch will render (mapped components own their
    /// internals and are never wrapped).
    fn for_view(vc: &IfmlViewContainer, components: &[PageComponentContext]) -> Self {
        let gateable = components.iter().any(|comp| {
            (comp.form.is_some() || comp.component_type == "form") && comp.mapping.is_none()
        });
        if !gateable {
            return Self::default();
        }
        Self::for_guards(&vc.requires, &vc.roles)
    }

    /// The gate for a guard pair: requires-check first, roles-check second,
    /// ANDed — mirroring the load guard's evaluation order and semantics.
    fn for_guards(requires: &[String], roles: &[String]) -> Self {
        if requires.is_empty() && roles.is_empty() {
            return Self::default();
        }
        let mut consts: Vec<String> = Vec::new();
        let mut checks: Vec<String> = Vec::new();
        if !requires.is_empty() {
            let list = requires
                .iter()
                .map(|c| js_quote(c))
                .collect::<Vec<_>>()
                .join(", ");
            consts.push(format!("const viewRequires = [{list}];"));
            checks.push("viewRequires.some((c) => can(c))".to_string());
        }
        if !roles.is_empty() {
            let list = roles
                .iter()
                .map(|r| js_quote(r))
                .collect::<Vec<_>>()
                .join(", ");
            consts.push(format!("const viewRoles = [{list}];"));
            consts.push("const roles = currentRoles();".to_string());
            checks.push("roles.some((r) => viewRoles.includes(r))".to_string());
        }
        // Import order mirrors the load template: currentRoles first, can second.
        let mut imports = Vec::new();
        if !roles.is_empty() {
            imports.push("currentRoles".to_string());
        }
        if !requires.is_empty() {
            imports.push("can".to_string());
        }
        Self {
            imports,
            consts: consts.join("\n\t"),
            open: format!("{{#if {}}}", checks.join(" && ")),
            close: "{/if}".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RenderImport {
    pub export_name: String,
    pub import_path: String,
}

#[derive(Debug, Serialize)]
pub struct PageComponentContext {
    name: String,
    /// Sanitized JS identifier (const/handler names).
    js_name: String,
    component_type: String,
    /// Semantic slot role of the whole component (`collection`, `display`,
    /// `selection-field`); forms keep `None` — their inputs carry roles.
    role: Option<SemanticRole>,
    entity: String,
    fields: Vec<String>,
    fields_with_types: Vec<(String, String)>,
    filter: String,
    table: Option<RenderTable>,
    form: Option<RenderForm>,
    chart: Option<RenderChart>,
    mapping: Option<RenderMapping>,
    events: Vec<RenderEvent>,
    /// Ready-to-render event callback props for mapped components,
    /// e.g. `onselect={comp_grid_select}` (Svelte 5 event-property form —
    /// `on:select` directives are not forwarded to components).
    event_props: Vec<String>,
    /// Ready-to-render data prop for mapped components, e.g. `data={data.items}`.
    data_prop: String,
    /// Ready-to-render fields prop for mapped components.
    fields_prop: String,
    /// Ready-to-render submit callback prop for mapped form components.
    submit_prop: Option<String>,
    /// Joined validation expressions for mapped form components
    /// (`data-validate` attribute).
    data_validate: Option<String>,
    /// First validation message for mapped form components
    /// (`data-validate-message` attribute).
    data_validate_message: Option<String>,
    api: Option<ResolvedApi>,
    /// View parameter carrying the entity id (edit mode), when any.
    id_param: Option<String>,
    /// Fields passed to a mapped component: declared fields when present,
    /// else derived from the typed spec (table columns / form fields).
    mapped_fields: Vec<String>,
    /// Handler for row-click navigation on list/table fallback markup.
    row_handler: Option<String>,
    submit: Option<RenderSubmit>,
    submit_handler: Option<String>,
    /// Mapped submit button replacing the hardcoded fallback `<button>`.
    submit_button: Option<RenderButton>,
    /// Mapped cancel/back/click buttons rendered after the form.
    buttons: Vec<RenderButton>,
    /// Workflow state display for the bound entity, resolved from the
    /// owning domain's config; `None` renders no badge (byte-identical).
    workflow: Option<RenderWorkflow>,
    /// Whether the fallback form branch renders: emits the typed
    /// `{js_name}_form_state` const used by value bindings and the
    /// workflow badge.
    form_state: bool,
    /// Ready-to-render typed payload construction lines for the submit
    /// handler (`const payload: ...` + per-field coercions); empty keeps
    /// the untyped `formData` body (byte-identical).
    form_payload: String,
}

/// Workflow config for a component's bound entity, pre-rendered into the
/// state badge markup for the component's markup context.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderWorkflow {
    pub status_field: String,
    pub states: Vec<String>,
    pub terminal_states: Vec<String>,
    pub initial_state: String,
    /// Ready-to-render badge: `<span class="workflow-state"
    /// data-testid="{component}-state" data-workflow-state={...}>...</span>`
    /// plus the terminal marker attribute when terminal states are known.
    pub badge_html: String,
}

/// Resolve the workflow config for a component's bound entity: the domain
/// entry listing the entity (plain or `{entity}Type` form) carries it.
/// Domain names are scanned in sorted order for deterministic resolution.
/// Config-driven only, so schema-less `ifml-generate` runs resolve too.
pub(crate) fn workflow_for_entity(config: &DomainConfig, entity: &str) -> Option<RenderWorkflow> {
    if entity.is_empty() {
        return None;
    }
    let mut domain_names: Vec<&String> = config.domains.keys().collect();
    domain_names.sort();
    for name in domain_names {
        let entry = &config.domains[name];
        let listed = [entity, &format!("{entity}Type")]
            .iter()
            .any(|candidate| entry.entities.iter().any(|e| e == candidate));
        if listed {
            return entry
                .get_entity_config(entity)
                .and_then(|ec| ec.workflow.as_ref())
                .map(|wf| RenderWorkflow {
                    status_field: wf.status_field.clone(),
                    states: wf.states.clone(),
                    terminal_states: wf.terminal_states.clone(),
                    initial_state: wf.initial_state.clone(),
                    badge_html: String::new(),
                });
        }
    }
    None
}

/// Workflow context for a component: resolution plus the badge markup
/// rendered for the component's markup context. Only collection, details,
/// and form components carry a badge.
fn component_workflow(config: &DomainConfig, c: &IfmlComponent) -> Option<RenderWorkflow> {
    if !is_form_component(c) && c.component_type != "details" && !is_collection(c) {
        return None;
    }
    let entity = c.entity.as_deref()?;
    let mut wf = workflow_for_entity(config, entity)?;
    let value_path = workflow_value_path(c, &wf.status_field);
    wf.badge_html = workflow_badge_html(&c.name, &value_path, &wf.terminal_states);
    Some(wf)
}

/// The JS expression reading the entity's current state in each markup
/// context: list/table rows iterate `item` and read the entity's status
/// column; forms and details read the workflow state merged into the load
/// payload by the `/workflow` fetch (`{...}.workflow_state?.current_state`)
/// — the entity payload's status column is not seeded at create time.
fn workflow_value_path(c: &IfmlComponent, status_field: &str) -> String {
    if is_form_component(c) {
        format!(
            "{}_form_state.workflow_state?.current_state",
            sanitize_ident(&c.name)
        )
    } else if c.component_type == "details" {
        "data.item?.workflow_state?.current_state".to_string()
    } else {
        format!("item.{status_field}")
    }
}

fn workflow_badge_html(component: &str, value_path: &str, terminal_states: &[String]) -> String {
    let mut html = format!(
        "<span class=\"workflow-state\" data-testid=\"{component}-state\" data-workflow-state={{{value_path}}}"
    );
    if !terminal_states.is_empty() {
        let list = terminal_states
            .iter()
            .map(|s| js_quote(s))
            .collect::<Vec<_>>()
            .join(", ");
        html.push_str(&format!(
            " data-workflow-terminal={{[{list}].includes({value_path} as string) ? \"true\" : \"false\"}}"
        ));
    }
    html.push_str(&format!(">{{{value_path}}}</span>"));
    html
}

fn js_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            _ => out.push(ch),
        }
    }
    out.push('\'');
    out
}

/// Submit wiring for a form component: fetch + success navigation.
#[derive(Debug, Serialize)]
pub struct RenderSubmit {
    handler_name: String,
    /// URL expression (JS literal) passed to fetch.
    url_expr: String,
    method: String,
    /// Navigation target URL expression from the view's save event.
    navigate_url: Option<String>,
    /// Emit the conservative client-side check that surfaces the first
    /// failing validation message before the network request.
    client_validate: bool,
}

#[derive(Debug, Serialize)]
pub struct RenderMapping {
    import_name: String,
    import_path: String,
    testid: Option<String>,
    row_testid: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RenderEvent {
    pub handler_name: String,
    pub event_type: String,
    /// "navigate" | "refresh" | "action" | "stay"
    pub action_kind: String,
    pub target: String,
    /// JS string expression evaluating to the navigation target URL.
    pub url_expr: String,
    /// Semantic slot role: `action-control` for save/submit/cancel/back/click
    /// button-style events.
    pub role: Option<SemanticRole>,
}

/// Modal wrapper for a `modal: true` view: a mapped `modal-view` component
/// (`<Dialog bind:open={dialog_open}>`) or the built-in div fallback.
#[derive(Debug, Serialize)]
pub struct RenderModal {
    /// Full opening markup line, e.g.
    /// `<Dialog bind:open={dialog_open} testid="x-modal">` or
    /// `<div class="modal" role="dialog" data-testid="x-modal">`.
    pub open_line: String,
    /// Full closing markup, e.g. `</Dialog>` or `</div>`.
    pub close_line: String,
    /// Testid of the generated close button.
    pub close_testid: String,
    /// Component import when the wrapper is a mapped dialog; `None` for the
    /// built-in div fallback.
    pub import: Option<RenderImport>,
}

/// Presentation-container wrapper for an xor view container: a mapped
/// `presentation-container` component (`<Card testid="...">`) or the
/// built-in section fallback. Children render inline inside the wrapper.
#[derive(Debug, Serialize)]
pub struct RenderContainer {
    /// Full opening markup line, e.g. `<Card testid="card">` or
    /// `<section data-testid="checkout-container">`.
    pub open_line: String,
    /// Full closing markup, e.g. `</Card>` or `</section>`.
    pub close_line: String,
    /// Testid carried by the wrapper.
    pub testid: String,
    /// Component import when the wrapper is a mapped container; `None` for
    /// the built-in section fallback.
    pub import: Option<RenderImport>,
}

/// One navigation entry in the landmark shell layout: `href_attr` is the
/// ready-to-render `href={...}` attribute using the same URL expression the
/// page goto handlers emit.
#[derive(Debug, Serialize)]
pub struct RenderNavItem {
    pub label: String,
    pub href_attr: String,
}

/// Landmark shell nav context: a mapped `shell` component wrapping nav
/// items built from the landmark views' navigate events.
#[derive(Debug, Serialize)]
pub struct RenderShellNav {
    pub import: RenderImport,
    pub testid: Option<String>,
    pub items: Vec<RenderNavItem>,
}

/// Render context for the framework layout shell
/// (`ifml/{fw}/layout.tera` → `src/routes/+layout.svelte`).
#[derive(Debug, Serialize)]
pub struct LayoutSvelteContext {
    pub shell: RenderShellNav,
}

/// A mapped action-control button replacing the hardcoded fallback `<button>`
/// (form submit) or rendering a cancel/back/click action.
#[derive(Debug, Serialize)]
pub struct RenderButton {
    pub import_name: String,
    pub import_path: String,
    /// Humanized event action: Save / Cancel / Submit / Back.
    pub label: String,
    /// Ready-to-render `onclick={handler}` prop; `None` when no handler fn
    /// is generated.
    pub onclick_prop: Option<String>,
    /// Ready-to-render `disabled={submitting}` prop for submit buttons.
    pub disabled_prop: Option<String>,
    /// Ready-to-render `testid="..."` prop.
    pub testid_prop: String,
}

/// Typed-table render context derived from a `ComponentSpec::Table`
#[derive(Debug, Serialize)]
pub struct RenderTable {
    pagination: bool,
    /// `pagination` when the list spec enables pagination.
    role: Option<SemanticRole>,
    columns: Vec<RenderColumn>,
}

/// One typed table column; `binding` is the ready-to-emit data path
/// (`property` for field/lookup columns, the rendered expression for
/// expression columns)
#[derive(Debug, Serialize)]
pub struct RenderColumn {
    label: String,
    kind: String,
    binding: String,
    lookup: String,
    expr: String,
}

/// Typed-form render context derived from a `ComponentSpec::Form`
#[derive(Debug, Serialize)]
pub struct RenderForm {
    fields: Vec<RenderInputField>,
}

#[derive(Debug, Serialize)]
pub struct RenderInputField {
    name: String,
    input_type: String,
    /// Per-input slot role: `selection-field` for dropdown/radio inputs.
    input_role: Option<SemanticRole>,
    is_textarea: bool,
    is_select: bool,
    is_radio: bool,
    required: bool,
    values: Vec<String>,
    /// Validation expressions joined for a `data-validate` attribute.
    data_validate: String,
    /// Message positionally paired with the first validation; rendered as
    /// `data-validate-message` next to `data-validate`.
    message: Option<String>,
}

/// Typed-chart render context derived from a `ComponentSpec::Chart`
#[derive(Debug, Serialize)]
pub struct RenderChart {
    kind: String,
    label_field: Option<String>,
    value_fields: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PageLoadContext {
    pub api_version: String,
    name: String,
    components: Vec<PageLoadComponentContext>,
    has_fetch: bool,
    /// All view parameters, resolved from `url.searchParams` (views have no
    /// dynamic route segments); entries with a DSL default carry the JS
    /// literal as the final `??` fallback. Resolved params are returned to
    /// the page as `result.params`.
    view_params: Vec<RenderViewParam>,
    /// Roles allowed to view this page; non-empty emits the load-level
    /// role guard plus the `$lib/roles` helper import.
    view_roles: Vec<String>,
    /// Capabilities required to view this page; non-empty emits the
    /// load-level `can()` guard.
    view_requires: Vec<String>,
    /// Ready-to-render guard const declarations (`const viewRoles = ...;`),
    /// emitted between the imports and the load function.
    guard_consts: String,
    /// Redirect target when a guard fails: the first unguarded view's
    /// route, else `/`.
    denial_target: String,
}

/// A view parameter resolved in the load function from the query string.
#[derive(Debug, Serialize)]
pub struct RenderViewParam {
    name: String,
    /// JS literal emitted as the final `??` fallback.
    default: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PageLoadComponentContext {
    /// Sanitized JS identifier for local variable names.
    name: String,
    component_type: String,
    entity: String,
    /// Legacy lowercase entity route (used by frameworks without a resolved
    /// API model).
    route_name: String,
    api: Option<ResolvedApi>,
    id_param: Option<String>,
    /// Whether the bound entity carries a workflow: the load merges the
    /// `/workflow` state into the payload so badges can read the
    /// authoritative current state.
    workflow: bool,
    paginate: bool,
    fetch_list: bool,
    fetch_item: bool,
    fetch_form: bool,
}

async fn build_page_context(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
    modal_targets: &HashSet<String>,
) -> PageSvelteContext {
    let id_param = id_param_from(&vc.params);
    let mut api_cache: HashMap<String, Option<ResolvedApi>> = HashMap::new();

    let mut components = Vec::new();
    for c in &vc.components {
        let ctx = page_component_context(
            db,
            config,
            api_version,
            vc,
            c,
            id_param.as_deref(),
            mappings,
            &mut api_cache,
            modal_targets,
        )
        .await;
        components.push(ctx);
    }

    let view_events: Vec<RenderEvent> = vc
        .events
        .iter()
        .map(|e| render_event(e, modal_targets))
        .collect();

    let mut imports: Vec<RenderImport> = Vec::new();
    let mut seen_imports: HashMap<String, String> = HashMap::new();
    let modal = modal_context(vc, mappings);
    if let Some(imp) = modal.as_ref().and_then(|m| m.import.as_ref()) {
        seen_imports.insert(imp.import_path.clone(), imp.export_name.clone());
        imports.push(RenderImport {
            export_name: imp.export_name.clone(),
            import_path: imp.import_path.clone(),
        });
    }
    let container = container_context(vc, mappings);
    if let Some(imp) = container.as_ref().and_then(|c| c.import.as_ref()) {
        let export = imp.export_name.clone();
        if seen_imports
            .insert(imp.import_path.clone(), export.clone())
            .is_none()
        {
            imports.push(RenderImport {
                export_name: export,
                import_path: imp.import_path.clone(),
            });
        }
    }
    let mut needs_goto = view_events.iter().any(|e| e.action_kind == "navigate");
    let mut has_submit = false;
    for comp in &components {
        for evt in &comp.events {
            if evt.action_kind == "navigate" {
                needs_goto = true;
            }
        }
        if let Some(ref submit) = comp.submit {
            has_submit = true;
            if submit.navigate_url.is_some() {
                needs_goto = true;
            }
        }
        if let Some(ref mapping) = comp.mapping {
            let export = mapping.import_name.clone();
            if seen_imports
                .insert(mapping.import_path.clone(), export.clone())
                .is_none()
            {
                imports.push(RenderImport {
                    export_name: export,
                    import_path: mapping.import_path.clone(),
                });
            }
        }
        for btn in comp.submit_button.iter().chain(comp.buttons.iter()) {
            if seen_imports
                .insert(btn.import_path.clone(), btn.import_name.clone())
                .is_none()
            {
                imports.push(RenderImport {
                    export_name: btn.import_name.clone(),
                    import_path: btn.import_path.clone(),
                });
            }
        }
    }
    let needs_on_mount = view_events
        .iter()
        .any(|e| e.action_kind == "navigate" && e.event_type == "load");
    let control_gate = ControlGateContext::for_view(vc, &components);

    PageSvelteContext {
        api_version: api_version.to_string(),
        name: vc.name.clone(),
        label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
        components,
        params: vc.params.clone(),
        view_params: {
            let mut seen: HashSet<String> = HashSet::new();
            vc.params
                .iter()
                .filter(|p| seen.insert(p.name.clone()))
                .map(|p| p.name.clone())
                .collect()
        },
        view_events,
        imports,
        needs_goto,
        needs_on_mount,
        has_submit,
        view_role: semantic_view_role(vc),
        container_role: semantic_container_role(vc),
        roles: vc.roles.clone(),
        requires: vc.requires.clone(),
        control_gate,
        modal,
        container,
    }
}

#[allow(clippy::too_many_arguments)]
async fn page_component_context(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    vc: &IfmlViewContainer,
    c: &IfmlComponent,
    id_param: Option<&str>,
    mappings: Option<&IfmlComponentMappings>,
    api_cache: &mut HashMap<String, Option<ResolvedApi>>,
    modal_targets: &HashSet<String>,
) -> PageComponentContext {
    let (table, form, chart) = match c.spec {
        Some(ComponentSpec::Table(ref spec)) => (Some(render_table(spec)), None, None),
        Some(ComponentSpec::Form(ref spec)) => (None, Some(render_form(spec)), None),
        Some(ComponentSpec::Chart(ref spec)) => (None, None, Some(render_chart(spec))),
        None => (None, None, None),
    };

    let kind = kind_of(c);
    let slot_role = component_role(c);
    let mapping = mappings
        .and_then(|m| m.resolve_slot(&vc.name, &c.name, &c.component_type, &kind, slot_role))
        .map(mapping_context);
    let events: Vec<RenderEvent> = c
        .events
        .iter()
        .map(|e| render_event(e, modal_targets))
        .collect();
    let event_props: Vec<String> = events
        .iter()
        .filter(|e| {
            e.action_kind == "navigate" && e.event_type != "submit" && e.event_type != "save"
        })
        .map(|e| format!("on{}={{{}}}", e.event_type, e.handler_name))
        .collect();

    let entity = c.entity.clone().unwrap_or_default();
    let api = match api_cache.get(&entity) {
        Some(resolved) => resolved.clone(),
        None => {
            let resolved = if entity.is_empty() {
                None
            } else {
                resolve_entity_api(db, config, &entity, api_version).await
            };
            api_cache.insert(entity.clone(), resolved.clone());
            resolved
        }
    };

    let mapped_fields = mapped_fields(c, table.as_ref(), form.as_ref(), chart.as_ref());

    let row_handler = if is_collection(c) {
        events
            .iter()
            .find(|e| e.action_kind == "navigate")
            .map(|e| e.handler_name.clone())
    } else {
        None
    };

    let submit = build_submit(c, api.as_ref(), id_param, &events);
    let submit_handler = submit.as_ref().map(|s| s.handler_name.clone());
    let submit_prop = submit_handler
        .as_ref()
        .map(|handler| format!("onsubmit={{{handler}}}"));
    let button_mapping = if mapping.is_none() {
        mappings.and_then(|m| {
            m.resolve_slot(
                &vc.name,
                &c.name,
                &c.component_type,
                &kind,
                Some(SemanticRole::ActionControl),
            )
        })
    } else {
        None
    };
    let (submit_button, buttons) = button_context(c, button_mapping, submit.as_ref(), &events);
    let data_validate = form.as_ref().map(|form| {
        form.fields
            .iter()
            .filter(|f| !f.data_validate.is_empty())
            .map(|f| f.data_validate.clone())
            .collect::<Vec<_>>()
            .join(" && ")
    });
    let data_validate_message = form.as_ref().and_then(|form| {
        form.fields
            .iter()
            .find(|f| !f.data_validate.is_empty())
            .and_then(|f| f.message.clone())
    });
    let data_prop = if is_collection(c) || chart.is_some() {
        "data={data.items}".to_string()
    } else if is_form_component(c) {
        "item={data.formData}".to_string()
    } else {
        "item={data.item}".to_string()
    };
    let fields_prop = format!(
        "fields={{[{}]}}",
        mapped_fields
            .iter()
            .map(|f| format!("'{f}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let workflow = component_workflow(config, c);
    let form_state = is_form_component(c) && mapping.is_none();
    let form_payload = if form_state {
        form_payload_block(c, form.as_ref())
    } else {
        String::new()
    };

    PageComponentContext {
        name: c.name.clone(),
        js_name: sanitize_ident(&c.name),
        component_type: c.component_type.clone(),
        role: slot_role,
        entity,
        fields: c.fields.clone(),
        fields_with_types: c.fields_with_types.clone(),
        filter: c.filter.clone().unwrap_or_default(),
        mapped_fields,
        mapping,
        events,
        event_props,
        data_prop,
        fields_prop,
        submit_prop,
        data_validate,
        data_validate_message,
        api,
        id_param: id_param.map(str::to_string),
        row_handler,
        submit,
        submit_handler,
        submit_button,
        buttons,
        workflow,
        form_state,
        form_payload,
        table,
        form,
        chart,
    }
}

/// Typed submit-payload construction for a fallback form: coerces FormData
/// string values to the bound schema's types (numbers, booleans, datetimes)
/// so the generated API accepts the body. Empty when no type information is
/// available (byte-identical untyped `formData` body).
fn form_payload_block(c: &IfmlComponent, form: Option<&RenderForm>) -> String {
    let field_names: Vec<String> = match form {
        Some(form) if !form.fields.is_empty() => {
            form.fields.iter().map(|f| f.name.clone()).collect()
        }
        _ => c.fields.clone(),
    };
    if field_names.is_empty() {
        return String::new();
    }
    let types: HashMap<&str, &str> = c
        .fields_with_types
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    let input_types: HashMap<&str, &str> = form
        .map(|form| {
            form.fields
                .iter()
                .map(|f| (f.name.as_str(), f.input_type.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let mut lines = Vec::new();
    for name in &field_names {
        let rust_type = types
            .get(name.as_str())
            .copied()
            .unwrap_or("String")
            .to_ascii_lowercase();
        let input_type = input_types.get(name.as_str()).copied().unwrap_or("");
        let value = format!("formData.{name}");
        let expr = if input_type == "checkbox" {
            format!("{value} === 'on'")
        } else if rust_type.contains("bool") {
            format!("{value} === '' ? null : {value} === 'true'")
        } else if is_numeric_rust_type(&rust_type) {
            format!("{value} === '' ? null : Number({value})")
        } else if rust_type.contains("datetime")
            || rust_type.contains("timestamp")
            || matches!(input_type, "datetime-local" | "date" | "time")
        {
            format!("{value} === '' ? null : new Date(String({value})).toISOString()")
        } else {
            value
        };
        lines.push(format!("\t\tpayload.{name} = {expr};"));
    }
    let mut block = String::from("\t\tconst payload: Record<string, unknown> = {};\n");
    block.push_str(&lines.join("\n"));
    block
}

pub(crate) fn is_numeric_rust_type(rust_type: &str) -> bool {
    [
        "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
    ]
    .iter()
    .any(|n| rust_type.contains(n))
        || rust_type.contains("decimal")
        || rust_type.contains("integer")
        || rust_type.contains("bigint")
}

/// The layout kind used for mapping resolution: the typed spec kind when a
/// spec is present, else the component type.
fn kind_of(c: &IfmlComponent) -> String {
    match c.spec {
        Some(ComponentSpec::Table(_)) => "table".to_string(),
        Some(ComponentSpec::Form(_)) => "form".to_string(),
        Some(ComponentSpec::Chart(_)) => "chart".to_string(),
        None => c.component_type.clone(),
    }
}

/// Whole-component slot role: tables/lists are collections, details/charts
/// are displays, dropdown/radio/checkbox component types are selection
/// fields. Forms keep `None` — the editing context is carried by their
/// per-input roles.
fn component_role(c: &IfmlComponent) -> Option<SemanticRole> {
    if is_form_component(c) {
        return None;
    }
    match c.spec {
        Some(ComponentSpec::Table(_)) => Some(SemanticRole::Collection),
        Some(ComponentSpec::Form(_)) => None,
        Some(ComponentSpec::Chart(_)) => Some(SemanticRole::Display),
        None => match c.component_type.as_str() {
            "table" | "list" => Some(SemanticRole::Collection),
            "details" | "chart" => Some(SemanticRole::Display),
            "dropdown" | "radio" | "checkbox" => Some(SemanticRole::SelectionField),
            _ => None,
        },
    }
}

/// Per-input slot role inside a form: dropdowns and radio groups are
/// selection fields.
fn input_field_role(input_type: &str) -> Option<SemanticRole> {
    match input_type {
        "dropdown" | "radio" => Some(SemanticRole::SelectionField),
        _ => None,
    }
}

/// Event/button slot role: submit-style and click events drive actions.
fn event_role(event_type: &str) -> Option<SemanticRole> {
    match event_type {
        "save" | "submit" | "cancel" | "back" | "click" => Some(SemanticRole::ActionControl),
        _ => None,
    }
}

/// View slot role: modals render as `modal-view`, landmarks are `shell`
/// candidates.
fn semantic_view_role(vc: &IfmlViewContainer) -> Option<SemanticRole> {
    if vc.is_modal {
        Some(SemanticRole::ModalView)
    } else if vc.is_landmark {
        Some(SemanticRole::Shell)
    } else {
        None
    }
}

/// Container slot role: xor/wizard groupings are presentation containers.
fn semantic_container_role(vc: &IfmlViewContainer) -> Option<SemanticRole> {
    if vc.is_xor {
        Some(SemanticRole::PresentationContainer)
    } else {
        None
    }
}

fn is_collection(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Table(_))) || c.component_type == "list"
}

fn is_form_component(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Form(_))) || c.component_type == "form"
}

fn mapped_fields(
    c: &IfmlComponent,
    table: Option<&RenderTable>,
    form: Option<&RenderForm>,
    chart: Option<&RenderChart>,
) -> Vec<String> {
    if !c.fields.is_empty() {
        return c.fields.clone();
    }
    if let Some(table) = table {
        return table
            .columns
            .iter()
            .filter(|col| col.kind != "expr")
            .map(|col| col.binding.clone())
            .collect();
    }
    if let Some(form) = form {
        return form.fields.iter().map(|f| f.name.clone()).collect();
    }
    if let Some(chart) = chart {
        return chart.value_fields.clone();
    }
    Vec::new()
}

fn mapping_context(m: &IfmlComponentMapping) -> RenderMapping {
    RenderMapping {
        import_name: m.export_name().to_string(),
        import_path: m.path.clone(),
        testid: m.testid("root").map(str::to_string),
        row_testid: m.testid("row").map(str::to_string),
    }
}

/// Build the submit wiring for a form component: POST for create views, PUT
/// for edit views (view carries an id param); success navigates per the
/// view's save event.
fn build_submit(
    c: &IfmlComponent,
    api: Option<&ResolvedApi>,
    id_param: Option<&str>,
    events: &[RenderEvent],
) -> Option<RenderSubmit> {
    if !is_form_component(c) {
        return None;
    }
    let api = api?;
    let handler_name = format!("submit_{}", sanitize_ident(&c.name));
    let (url_expr, method) = match id_param {
        Some(param) if api.has_update => (
            format!("`{}/${{viewParams.{param}}}`", api.base_path),
            "PUT".to_string(),
        ),
        _ => (format!("\"{}\"", api.base_path), "POST".to_string()),
    };
    let navigate_url = events
        .iter()
        .find(|e| {
            e.action_kind == "navigate" && (e.event_type == "save" || e.event_type == "submit")
        })
        .map(|e| e.url_expr.clone());
    Some(RenderSubmit {
        handler_name,
        url_expr,
        method,
        navigate_url,
        client_validate: form_has_messages(c),
    })
}

/// True when any typed form field pairs a validation with a message —
/// the signal for the submit handler's client-side message check.
fn form_has_messages(c: &IfmlComponent) -> bool {
    matches!(&c.spec, Some(ComponentSpec::Form(spec)) if spec.fields.iter().any(|f| {
        !f.validations.is_empty() && !f.messages.is_empty()
    }))
}

fn render_event(evt: &IfmlEvent, modal_targets: &HashSet<String>) -> RenderEvent {
    let (action_kind, target, binding) = match &evt.action {
        IfmlAction::Navigate { target, binding } => ("navigate", target.clone(), binding),
        IfmlAction::Refresh { target, binding } => ("refresh", target.clone(), binding),
        IfmlAction::Action(name) => ("action", name.clone(), &HashMap::new()),
        IfmlAction::Stay => ("stay", String::new(), &HashMap::new()),
    };
    let url_expr = if action_kind == "navigate" {
        let expr = nav_url_expr(&target, binding);
        if modal_targets.contains(&target) {
            with_dialog_param(&expr)
        } else {
            expr
        }
    } else {
        String::new()
    };
    RenderEvent {
        handler_name: sanitize_ident(&evt.name),
        event_type: evt.event_type.clone(),
        action_kind: action_kind.to_string(),
        target,
        url_expr,
        role: event_role(&evt.event_type),
    }
}

/// Append the `dialog=open` query param marking navigation into a modal
/// view: template-literal URLs get `&dialog=open`, plain literals get
/// `?dialog=open`.
fn with_dialog_param(url_expr: &str) -> String {
    if let Some(inner) = url_expr.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
        format!("`{inner}&dialog=open`")
    } else if let Some(inner) = url_expr.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        format!("\"{inner}?dialog=open\"")
    } else {
        url_expr.to_string()
    }
}

/// Whether a `modal: true` view renders a modal wrapper: a mapped
/// `modal-view` component when one resolves, else the built-in div fallback
/// whenever a non-empty mapping pack is present. Without mappings the view
/// renders as a plain page (byte-identical output).
pub(crate) fn modal_wrapper_active(
    is_modal: bool,
    _view: &str,
    mappings: Option<&IfmlComponentMappings>,
) -> bool {
    is_modal && mappings.is_some_and(|m| !m.components.is_empty())
}

/// The modal wrapper's `data-testid`: the resolved modal-view mapping's
/// `testids.root`, else `{view}-modal`.
pub(crate) fn modal_wrapper_testid(view: &str, mappings: Option<&IfmlComponentMappings>) -> String {
    mappings
        .and_then(|m| m.resolve_by_role(view, SemanticRole::ModalView))
        .and_then(|m| m.testid("root").map(str::to_string))
        .unwrap_or_else(|| format!("{}-modal", view.to_lowercase()))
}

/// Modal wrapper context for a view container; `None` renders the plain page.
fn modal_context(
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
) -> Option<RenderModal> {
    if !modal_wrapper_active(vc.is_modal, &vc.name, mappings) {
        return None;
    }
    let view_lower = vc.name.to_lowercase();
    let close_testid = format!("{view_lower}-modal-close");
    let mapping = mappings.and_then(|m| m.resolve_by_role(&vc.name, SemanticRole::ModalView));
    let (open_line, close_line, import) = match mapping {
        Some(m) => {
            let export = m.export_name();
            let testid = modal_wrapper_testid(&vc.name, mappings);
            (
                format!("<{export} bind:open={{dialog_open}} testid=\"{testid}\">"),
                format!("</{export}>"),
                Some(RenderImport {
                    export_name: export.to_string(),
                    import_path: m.path.clone(),
                }),
            )
        }
        None => (
            format!("<div class=\"modal\" role=\"dialog\" data-testid=\"{view_lower}-modal\">"),
            "</div>".to_string(),
            None,
        ),
    };
    Some(RenderModal {
        open_line,
        close_line,
        close_testid,
        import,
    })
}

/// Whether an xor view container renders a presentation-container wrapper:
/// a mapped `presentation-container` component when one resolves, else the
/// built-in section fallback whenever a non-empty mapping pack is present.
/// Without mappings the view renders as a plain page (byte-identical output).
pub(crate) fn container_wrapper_active(
    is_xor: bool,
    mappings: Option<&IfmlComponentMappings>,
) -> bool {
    is_xor && mappings.is_some_and(|m| !m.components.is_empty())
}

/// Resolve the presentation-container mapping for an xor view container:
/// name tier first (the container name), then the role tier.
fn resolve_container_mapping<'m>(
    vc: &IfmlViewContainer,
    mappings: Option<&'m IfmlComponentMappings>,
) -> Option<&'m IfmlComponentMapping> {
    mappings.and_then(|m| {
        m.resolve_slot(
            &vc.name,
            &vc.name,
            "",
            "",
            Some(SemanticRole::PresentationContainer),
        )
    })
}

/// The mapped container wrapper's `data-testid`: the resolved mapping's
/// `testids.root`, else `{view}-container`. `None` when the view is not xor
/// or no `presentation-container` mapping resolves (the e2e assertion gate).
pub(crate) fn mapped_container_testid(
    vc_name: &str,
    is_xor: bool,
    mappings: Option<&IfmlComponentMappings>,
) -> Option<String> {
    if !is_xor {
        return None;
    }
    let fallback = format!("{}-container", vc_name.to_lowercase());
    mappings
        .and_then(|m| {
            m.resolve_slot(
                vc_name,
                vc_name,
                "",
                "",
                Some(SemanticRole::PresentationContainer),
            )
        })
        .map(|m| m.testid("root").map(str::to_string).unwrap_or(fallback))
}

/// Presentation-container wrapper context for an xor view container; `None`
/// renders the plain page.
fn container_context(
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
) -> Option<RenderContainer> {
    if !container_wrapper_active(vc.is_xor, mappings) {
        return None;
    }
    let view_lower = vc.name.to_lowercase();
    let section_testid = format!("{view_lower}-container");
    match resolve_container_mapping(vc, mappings) {
        Some(m) => {
            let export = m.export_name();
            let testid = m
                .testid("root")
                .map(str::to_string)
                .unwrap_or_else(|| section_testid.clone());
            Some(RenderContainer {
                open_line: format!("<{export} testid=\"{testid}\">"),
                close_line: format!("</{export}>"),
                testid,
                import: Some(RenderImport {
                    export_name: export.to_string(),
                    import_path: m.path.clone(),
                }),
            })
        }
        None => Some(RenderContainer {
            open_line: format!("<section data-testid=\"{section_testid}\">"),
            close_line: "</section>".to_string(),
            testid: section_testid,
            import: None,
        }),
    }
}

/// Shell nav context for the landmark layout: resolves the `shell` mapping
/// against the landmark views and builds nav items from their navigate
/// events (same URL resolution as the page goto handlers). `None` when no
/// landmark views exist or no `shell` mapping resolves — no layout is
/// emitted then.
pub(crate) fn shell_nav(
    vcs: &[IfmlViewContainer],
    mappings: Option<&IfmlComponentMappings>,
) -> Option<RenderShellNav> {
    let labels: HashMap<&str, &str> = vcs
        .iter()
        .map(|vc| (vc.name.as_str(), vc.label.as_deref().unwrap_or(&vc.name)))
        .collect();
    let landmarks = vcs.iter().filter(|vc| vc.is_landmark && !vc.is_modal);
    let mapping = mappings.and_then(|m| {
        landmarks
            .clone()
            .find_map(|vc| m.resolve_by_role(&vc.name, SemanticRole::Shell))
    })?;
    let mut items: Vec<RenderNavItem> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for vc in landmarks {
        for evt in vc
            .events
            .iter()
            .chain(vc.components.iter().flat_map(|c| c.events.iter()))
        {
            if let IfmlAction::Navigate { target, binding } = &evt.action {
                let url_expr = nav_url_expr(target, binding);
                let label = labels.get(target.as_str()).copied().unwrap_or(target);
                if seen.insert((label.to_string(), url_expr.clone())) {
                    items.push(RenderNavItem {
                        label: label.to_string(),
                        href_attr: format!("href={{{url_expr}}}"),
                    });
                }
            }
        }
    }
    Some(RenderShellNav {
        import: RenderImport {
            export_name: mapping.export_name().to_string(),
            import_path: mapping.path.clone(),
        },
        testid: mapping.testid("root").map(str::to_string),
        items,
    })
}

/// Mapped action-control buttons for a component's fallback markup: the
/// form's save/submit button plus cancel/back/click buttons. A `None`
/// mapping keeps the hardcoded fallback markup (byte-identical output).
fn button_context(
    c: &IfmlComponent,
    mapping: Option<&IfmlComponentMapping>,
    submit: Option<&RenderSubmit>,
    events: &[RenderEvent],
) -> (Option<RenderButton>, Vec<RenderButton>) {
    let Some(m) = mapping else {
        return (None, Vec::new());
    };
    let export = m.export_name().to_string();
    let import_path = m.path.clone();
    let testid_prop = |fallback: String| {
        let testid = m.testid("root").map(str::to_string).unwrap_or(fallback);
        format!("testid=\"{testid}\"")
    };
    let onclick_prop = |handler: &str| format!("onclick={{{handler}}}");
    let submit_button = RenderButton {
        import_name: export.clone(),
        import_path: import_path.clone(),
        label: primary_button_label(events),
        onclick_prop: submit.map(|s| onclick_prop(&s.handler_name)),
        disabled_prop: submit
            .is_some()
            .then(|| "disabled={submitting}".to_string()),
        testid_prop: testid_prop(format!("{}-submit", c.name)),
    };
    let buttons = events
        .iter()
        .filter(|e| {
            e.action_kind == "navigate"
                && matches!(e.event_type.as_str(), "cancel" | "back" | "click")
        })
        .map(|e| {
            // Secondary buttons must not share the submit button's root
            // testid (duplicate selectors); prefer a per-event mapping key,
            // else the component-scoped slot name the fallback markup uses.
            let testid = m
                .testid(&e.event_type)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}-{}", c.name, e.event_type));
            RenderButton {
                import_name: export.clone(),
                import_path: import_path.clone(),
                label: humanize_event_label(&e.event_type),
                onclick_prop: Some(onclick_prop(&e.handler_name)),
                disabled_prop: None,
                testid_prop: format!("testid=\"{testid}\""),
            }
        })
        .collect();
    (Some(submit_button), buttons)
}

/// Label for the primary form button: the save/submit event's humanized
/// action, else "Submit".
fn primary_button_label(events: &[RenderEvent]) -> String {
    events
        .iter()
        .find(|e| e.event_type == "save" || e.event_type == "submit")
        .map(|e| humanize_event_label(&e.event_type))
        .unwrap_or_else(|| "Submit".to_string())
}

fn humanize_event_label(event_type: &str) -> String {
    let mut chars = event_type.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Build a JS template-literal URL for a navigation target: the target view's
/// generated route (`/{name-lowercase}`) plus query params from the binding
/// map (`?key=${expr}`). Binding expressions are emitted verbatim from the
/// model; keys are sorted for deterministic output.
fn nav_url_expr(target: &str, binding: &HashMap<String, String>) -> String {
    let path = format!("/{}", target.to_lowercase());
    if binding.is_empty() {
        return format!("\"{path}\"");
    }
    let mut pairs: Vec<(&String, &String)> = binding.iter().collect();
    pairs.sort();
    let query: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}=${{{v}}}")).collect();
    format!("`{path}?{}`", query.join("&"))
}

fn sanitize_ident(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "component".to_string()
    } else if cleaned.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("_{cleaned}")
    } else {
        cleaned
    }
}

fn render_table(spec: &TableSpec) -> RenderTable {
    RenderTable {
        pagination: spec.pagination,
        role: if spec.pagination {
            Some(SemanticRole::Pagination)
        } else {
            None
        },
        columns: spec.columns.iter().map(render_column).collect(),
    }
}

fn render_column(col: &ColumnDef) -> RenderColumn {
    match col {
        ColumnDef::Field { label, field } => RenderColumn {
            label: label.clone(),
            kind: "field".to_string(),
            binding: field.property.clone(),
            lookup: String::new(),
            expr: String::new(),
        },
        ColumnDef::Lookup {
            label,
            field,
            lookup,
        } => RenderColumn {
            label: label.clone(),
            kind: "lookup".to_string(),
            binding: field.property.clone(),
            lookup: lookup.clone(),
            expr: String::new(),
        },
        ColumnDef::Expression { label, expr } => RenderColumn {
            label: label.clone(),
            kind: "expr".to_string(),
            binding: render_expression(expr),
            lookup: String::new(),
            expr: render_expression(expr),
        },
    }
}

fn render_form(spec: &FormSpec) -> RenderForm {
    RenderForm {
        fields: spec
            .fields
            .iter()
            .map(|field| {
                let (input_type, is_textarea, is_select, is_radio) = match field.input {
                    InputFieldType::TextArea => ("textarea".to_string(), true, false, false),
                    InputFieldType::Dropdown => ("dropdown".to_string(), false, true, false),
                    InputFieldType::RadioGroup => ("radio".to_string(), false, false, true),
                    InputFieldType::Custom(ref custom) => (custom.clone(), false, false, false),
                    InputFieldType::Text => ("text".to_string(), false, false, false),
                    InputFieldType::Password => ("password".to_string(), false, false, false),
                    InputFieldType::Email => ("email".to_string(), false, false, false),
                    InputFieldType::Number => ("number".to_string(), false, false, false),
                    InputFieldType::Date => ("date".to_string(), false, false, false),
                    InputFieldType::Time => ("time".to_string(), false, false, false),
                    InputFieldType::DateTime => ("datetime-local".to_string(), false, false, false),
                    InputFieldType::Checkbox | InputFieldType::Toggle => {
                        ("checkbox".to_string(), false, false, false)
                    }
                    InputFieldType::File => ("file".to_string(), false, false, false),
                    InputFieldType::Hidden => ("hidden".to_string(), false, false, false),
                };
                let validations: Vec<String> =
                    field.validations.iter().map(render_expression).collect();
                let message = if validations.is_empty() {
                    None
                } else {
                    field.messages.first().cloned()
                };
                RenderInputField {
                    name: field.name.clone(),
                    input_role: input_field_role(&input_type),
                    input_type,
                    is_textarea,
                    is_select,
                    is_radio,
                    required: field.required,
                    values: field.values.clone(),
                    data_validate: validations.join(" && "),
                    message,
                }
            })
            .collect(),
    }
}

fn render_chart(spec: &ChartSpec) -> RenderChart {
    RenderChart {
        kind: match spec.kind {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Pie => "pie",
            ChartKind::Radar => "radar",
            ChartKind::Metric => "metric",
        }
        .to_string(),
        label_field: spec.label_field.clone(),
        value_fields: spec.value_fields.clone(),
    }
}

/// Render an IFML expression as a plain-text placeholder binding
/// (not evaluated — the generated markup keeps it verbatim).
fn render_expression(expr: &Expression) -> String {
    match expr {
        Expression::Ident(name) => name.clone(),
        Expression::StringLit(value) => format!("\"{}\"", value.replace('"', "\\\"")),
        Expression::NumLit(value) => format!("{value}"),
        Expression::BoolLit(value) => value.to_string(),
        Expression::FieldExpr { object, field } => {
            format!("{}.{}", render_expression(object), field)
        }
        Expression::BinOp { left, op, right } => format!(
            "{} {} {}",
            render_expression(left),
            bin_op_symbol(op),
            render_expression(right)
        ),
        Expression::UnaryOp { op, operand } => match op {
            UnaryOp::Not => format!("!{}", render_expression(operand)),
            UnaryOp::Neg => format!("-{}", render_expression(operand)),
        },
        Expression::Group(inner) => format!("({})", render_expression(inner)),
        Expression::Call { name, args } => {
            let rendered: Vec<String> = args.iter().map(render_expression).collect();
            format!("{name}({})", rendered.join(", "))
        }
    }
}

fn bin_op_symbol(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::RegexMatch => "=~",
        BinOp::NegRegex => "!~",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Build the `+page.ts` load context. Only the first list/details/form
/// component contributes a fetch per page, mirroring the single-return load
/// contract; each fetch resolves the entity API path from the graph. View
/// parameters resolve from the query string only (views have no dynamic
/// route segments) and are returned to the page as `result.params`.
fn build_load_context(
    api_version: &str,
    vc: &IfmlViewContainer,
    components: &[PageComponentContext],
    denial_target: &str,
) -> PageLoadContext {
    let id_param = id_param_from(&vc.params);
    // The graph may carry duplicate HasParameter edges (see
    // ingest_parameter_definition); params dedupe by name for resolution.
    let mut seen_params: HashSet<String> = HashSet::new();
    let view_params: Vec<RenderViewParam> = vc
        .params
        .iter()
        .filter(|p| seen_params.insert(p.name.clone()))
        .map(|p| RenderViewParam {
            name: p.name.clone(),
            default: p.default.clone(),
        })
        .collect();
    let mut load_components = Vec::new();
    let mut has_list = false;
    let mut has_details = false;
    let mut has_form_fetch = false;
    let mut has_fetch = false;

    for comp in components {
        let is_list = comp.table.is_some() || comp.component_type == "list";
        let is_details = comp.component_type == "details";
        let is_form = comp.form.is_some() || comp.component_type == "form";

        let mut fetch_list = false;
        let mut fetch_item = false;
        let mut fetch_form = false;
        if comp.api.is_some() && !comp.entity.is_empty() {
            if is_list && !has_list {
                fetch_list = true;
            } else if is_details && !has_details && id_param.is_some() {
                fetch_item = true;
            } else if is_form && !has_form_fetch && id_param.is_some() {
                fetch_form = true;
            }
        }
        has_list |= fetch_list;
        has_details |= fetch_item;
        has_form_fetch |= fetch_form;
        has_fetch |= fetch_list || fetch_item || fetch_form;

        load_components.push(PageLoadComponentContext {
            name: sanitize_ident(&comp.name),
            component_type: comp.component_type.clone(),
            entity: comp.entity.clone(),
            route_name: comp.entity.to_lowercase(),
            api: comp.api.clone(),
            id_param: id_param.clone(),
            workflow: comp.workflow.is_some(),
            paginate: fetch_list && (comp.table.as_ref().map(|t| t.pagination).unwrap_or(true)),
            fetch_list,
            fetch_item,
            fetch_form,
        });
    }

    PageLoadContext {
        api_version: api_version.to_string(),
        name: vc.name.clone(),
        components: load_components,
        has_fetch,
        view_params,
        view_roles: vc.roles.clone(),
        view_requires: vc.requires.clone(),
        guard_consts: guard_consts(vc),
        denial_target: denial_target.to_string(),
    }
}

/// The guard const declarations for a view, joined by newlines in
/// guard-evaluation order: capabilities first (`const viewRequires = ...;`),
/// roles second (`const viewRoles = ...;`).
fn guard_consts(vc: &IfmlViewContainer) -> String {
    let mut consts = Vec::new();
    if !vc.requires.is_empty() {
        let list = vc
            .requires
            .iter()
            .map(|c| js_quote(c))
            .collect::<Vec<_>>()
            .join(", ");
        consts.push(format!("const viewRequires = [{list}];"));
    }
    if !vc.roles.is_empty() {
        let list = vc
            .roles
            .iter()
            .map(|r| js_quote(r))
            .collect::<Vec<_>>()
            .join(", ");
        consts.push(format!("const viewRoles = [{list}];"));
    }
    consts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template_engine::create_tera;
    use codegraph_core::mock::MockEngine;
    use codegraph_ifml_dsl::PropertyRef;

    fn table_spec() -> ComponentSpec {
        ComponentSpec::Table(TableSpec {
            columns: vec![
                ColumnDef::Field {
                    label: "Name".to_string(),
                    field: PropertyRef {
                        entity: "Customer".to_string(),
                        property: "name".to_string(),
                    },
                },
                ColumnDef::Lookup {
                    label: "Status".to_string(),
                    field: PropertyRef {
                        entity: "Customer".to_string(),
                        property: "status".to_string(),
                    },
                    lookup: "status_labels".to_string(),
                },
                ColumnDef::Expression {
                    label: "Tenure".to_string(),
                    expr: Expression::Call {
                        name: "tenure_years".to_string(),
                        args: vec![Expression::FieldExpr {
                            object: Box::new(Expression::Ident("Customer".to_string())),
                            field: "hire_date".to_string(),
                        }],
                    },
                },
            ],
            pagination: true,
        })
    }

    fn component_with_spec(spec: Option<ComponentSpec>) -> IfmlComponent {
        IfmlComponent {
            name: "grid".to_string(),
            component_type: "table".to_string(),
            mode: None,
            entity: Some("Customer".to_string()),
            fields: Vec::new(),
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec,
        }
    }

    #[test]
    fn specless_component_yields_no_render_contexts() {
        let ctx = page_component_context_sync(&component_with_spec(None));
        assert!(ctx.table.is_none());
        assert!(ctx.form.is_none());
        assert!(ctx.chart.is_none());
        assert_eq!(ctx.component_type, "table");
    }

    fn page_component_context_sync(c: &IfmlComponent) -> PageComponentContext {
        page_component_context_for_tests(c, &IfmlComponentMappings::default())
    }

    fn test_config() -> DomainConfig {
        toml::from_str(
            r#"
[defaults]
api_version = "v1"

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
entities = ["CustomerType"]
"#,
        )
        .unwrap()
    }

    fn page_component_context_for_tests(
        c: &IfmlComponent,
        mappings: &IfmlComponentMappings,
    ) -> PageComponentContext {
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let mut cache = HashMap::new();
        futures::executor::block_on(page_component_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            c,
            None,
            Some(mappings),
            &mut cache,
            &HashSet::new(),
        ))
    }

    fn workflow_config() -> DomainConfig {
        toml::from_str(
            r#"
[defaults]
api_version = "v1"

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
entities = ["CustomerType"]

[domains.sales.entity_config.CustomerType.workflow]
status_field = "status"
initial_state = "received"
states = ["received", "review", "done"]
terminal_states = ["done"]
"#,
        )
        .unwrap()
    }

    fn page_component_context_with_config(
        c: &IfmlComponent,
        config: &DomainConfig,
    ) -> PageComponentContext {
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let mut cache = HashMap::new();
        futures::executor::block_on(page_component_context(
            &MockEngine::new(),
            config,
            "v1",
            &vc,
            c,
            None,
            Some(&IfmlComponentMappings::default()),
            &mut cache,
            &HashSet::new(),
        ))
    }

    #[test]
    fn workflow_context_populates_from_domain_config() {
        let mut c = component_with_spec(None);
        c.component_type = "list".to_string();
        let ctx = page_component_context_with_config(&c, &workflow_config());
        let wf = ctx.workflow.expect("workflow context");
        assert_eq!(wf.status_field, "status");
        assert_eq!(wf.initial_state, "received");
        assert_eq!(wf.states, vec!["received", "review", "done"]);
        assert_eq!(wf.terminal_states, vec!["done"]);
        assert!(
            wf.badge_html.contains(
                "<span class=\"workflow-state\" data-testid=\"grid-state\" data-workflow-state={item.status}"
            ),
            "{}",
            wf.badge_html
        );
        assert!(
            wf.badge_html.contains(
                " data-workflow-terminal={['done'].includes(item.status as string) ? \"true\" : \"false\"}"
            ),
            "{}",
            wf.badge_html
        );
        assert!(wf.badge_html.ends_with(">{item.status}</span>"));
    }

    #[test]
    fn workflow_value_path_follows_component_kind() {
        let form = form_component();
        let ctx = page_component_context_with_config(&form, &workflow_config());
        let wf = ctx.workflow.expect("form workflow");
        assert!(
            wf.badge_html
                .contains("data-workflow-state={editor_form_state.workflow_state?.current_state}"),
            "form badges read the merged workflow state: {}",
            wf.badge_html
        );

        let mut details = component_with_spec(None);
        details.component_type = "details".to_string();
        let ctx = page_component_context_with_config(&details, &workflow_config());
        let wf = ctx.workflow.expect("details workflow");
        assert!(
            wf.badge_html
                .contains("data-workflow-state={data.item?.workflow_state?.current_state}"),
            "{}",
            wf.badge_html
        );
    }

    #[test]
    fn no_workflow_leaves_context_unchanged() {
        let ctx = page_component_context_sync(&component_with_spec(Some(table_spec())));
        assert!(ctx.workflow.is_none());

        let mut chart = component_with_spec(None);
        chart.component_type = "chart".to_string();
        let ctx = page_component_context_with_config(&chart, &workflow_config());
        assert!(ctx.workflow.is_none(), "charts carry no state badge");
    }

    #[test]
    fn table_spec_maps_column_kinds_and_bindings() {
        let ctx = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let table = ctx.table.expect("table render context");
        assert!(table.pagination);
        assert_eq!(table.columns.len(), 3);

        assert_eq!(table.columns[0].kind, "field");
        assert_eq!(table.columns[0].binding, "name");
        assert_eq!(table.columns[1].kind, "lookup");
        assert_eq!(table.columns[1].lookup, "status_labels");
        assert_eq!(table.columns[1].binding, "status");
        assert_eq!(table.columns[2].kind, "expr");
        assert_eq!(table.columns[2].binding, "tenure_years(Customer.hire_date)");
    }

    #[test]
    fn form_payload_coerces_by_input_and_rust_types() {
        let mut c = component_with_spec(Some(ComponentSpec::Form(FormSpec {
            fields: vec![
                field_def("title", InputFieldType::Text),
                field_def("amount", InputFieldType::Number),
                field_def("urgent", InputFieldType::Checkbox),
                field_def("submittedAt", InputFieldType::DateTime),
            ],
        })));
        c.fields_with_types = vec![
            ("title".to_string(), "String".to_string()),
            ("amount".to_string(), "Option< i64 >".to_string()),
            ("urgent".to_string(), "bool".to_string()),
            (
                "submittedAt".to_string(),
                "Option< DateTime < Utc > >".to_string(),
            ),
        ];
        let form = match &c.spec {
            Some(ComponentSpec::Form(form)) => Some(render_form(form)),
            _ => None,
        };
        let block = form_payload_block(&c, form.as_ref());
        assert!(
            block.contains("\t\tpayload.title = formData.title;"),
            "{block}"
        );
        assert!(
            block.contains(
                "\t\tpayload.amount = formData.amount === '' ? null : Number(formData.amount);"
            ),
            "{block}"
        );
        assert!(
            block.contains("\t\tpayload.urgent = formData.urgent === 'on';"),
            "{block}"
        );
        assert!(
            block.contains(
                "\t\tpayload.submittedAt = formData.submittedAt === '' ? null : new Date(String(formData.submittedAt)).toISOString();"
            ),
            "{block}"
        );

        let no_types = component_with_spec(None);
        assert_eq!(
            form_payload_block(&no_types, None),
            String::new(),
            "components without form fields keep the untyped body"
        );
    }

    #[test]
    fn form_spec_maps_input_types_and_validations() {
        let spec = ComponentSpec::Form(FormSpec {
            fields: vec![
                codegraph_ifml_dsl::FieldDef {
                    name: "name".to_string(),
                    input: InputFieldType::Text,
                    required: true,
                    validations: vec![Expression::BinOp {
                        left: Box::new(Expression::Call {
                            name: "len".to_string(),
                            args: vec![Expression::Ident("name".to_string())],
                        }),
                        op: BinOp::Gt,
                        right: Box::new(Expression::NumLit(2.0)),
                    }],
                    values: Vec::new(),
                    messages: vec!["Name too short".to_string()],
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "start".to_string(),
                    input: InputFieldType::DateTime,
                    required: false,
                    validations: Vec::new(),
                    values: Vec::new(),
                    messages: Vec::new(),
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "tier".to_string(),
                    input: InputFieldType::Dropdown,
                    required: false,
                    validations: Vec::new(),
                    values: vec!["gold".to_string(), "silver".to_string()],
                    messages: Vec::new(),
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "stars".to_string(),
                    input: InputFieldType::Custom("stars".to_string()),
                    required: false,
                    validations: Vec::new(),
                    values: Vec::new(),
                    messages: Vec::new(),
                },
            ],
        });
        let ctx = page_component_context_sync(&component_with_spec(Some(spec)));
        let form = ctx.form.expect("form render context");
        assert_eq!(form.fields[0].input_type, "text");
        assert!(form.fields[0].required);
        assert_eq!(form.fields[0].data_validate, "len(name) > 2");
        assert_eq!(form.fields[0].message.as_deref(), Some("Name too short"));
        assert_eq!(form.fields[1].input_type, "datetime-local");
        assert!(form.fields[2].is_select);
        assert_eq!(form.fields[2].values, vec!["gold", "silver"]);
        assert_eq!(form.fields[3].input_type, "stars");
    }

    #[test]
    fn form_message_renders_validate_message_and_client_check() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let spec = ComponentSpec::Form(FormSpec {
            fields: vec![codegraph_ifml_dsl::FieldDef {
                name: "title".to_string(),
                input: InputFieldType::Text,
                required: true,
                validations: vec![Expression::BinOp {
                    left: Box::new(Expression::Call {
                        name: "len".to_string(),
                        args: vec![Expression::Ident("title".to_string())],
                    }),
                    op: BinOp::Gt,
                    right: Box::new(Expression::NumLit(2.0)),
                }],
                values: Vec::new(),
                messages: vec!["Title too short".to_string()],
            }],
        });
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            Some(spec),
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "data-validate=\"len(title) > 2\" data-validate-message=\"Title too short\""
            ),
            "{rendered}"
        );
        assert!(rendered.contains("data-testid=\"grid-form\""), "{rendered}");
        assert!(
            rendered.contains("data-testid=\"grid-submit\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"grid-error\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("form.querySelector(':invalid')"),
            "{rendered}"
        );
        assert!(
            rendered.contains("invalid?.getAttribute('data-validate-message')"),
            "{rendered}"
        );

        let plain = ComponentSpec::Form(FormSpec {
            fields: vec![codegraph_ifml_dsl::FieldDef {
                name: "title".to_string(),
                input: InputFieldType::Text,
                required: true,
                validations: Vec::new(),
                values: Vec::new(),
                messages: Vec::new(),
            }],
        });
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            Some(plain),
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("checkValidity"), "{rendered}");
        assert!(rendered.contains("data-testid=\"grid-form\""), "{rendered}");
        assert!(
            rendered.contains(
                "const formData = Object.fromEntries(new FormData(event.currentTarget as HTMLFormElement));"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn load_context_renders_param_default_fallbacks() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let list = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let mut details = component_with_spec(None);
        details.component_type = "details".to_string();
        let details = page_component_context_sync(&details);
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: vec![
                super::super::context::ParameterDef {
                    name: "slug".to_string(),
                    type_ref: "String".to_string(),
                    default: Some("'home'".to_string()),
                },
                super::super::context::ParameterDef {
                    name: "customerId".to_string(),
                    type_ref: "Uuid".to_string(),
                    default: Some("'00000000-0000-0000-0000-000000000000'".to_string()),
                },
            ],
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = build_load_context("v1", &vc, &[list, details], "/");
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(
            rendered.contains("viewParams['slug'] = url.searchParams.get('slug') ?? 'home';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("result.params = viewParams;"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "viewParams['customerId'] = url.searchParams.get('customerId') ?? '00000000-0000-0000-0000-000000000000';"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("const customerId = viewParams['customerId'];"),
            "fetch ids must resolve from viewParams only (no route params): {rendered}"
        );
        assert!(
            !rendered.contains("params."),
            "route params are dead for query-param views: {rendered}"
        );

        let bare_vc = IfmlViewContainer {
            params: vec![super::super::context::ParameterDef {
                name: "customerId".to_string(),
                type_ref: "Uuid".to_string(),
                default: None,
            }],
            ..vc
        };
        let list = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let ctx = build_load_context("v1", &bare_vc, &[list], "/");
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(
            rendered
                .contains("viewParams['customerId'] = url.searchParams.get('customerId') ?? '';"),
            "params without defaults fall back to the empty string: {rendered}"
        );
    }

    #[test]
    fn paramless_view_load_stays_free_of_param_resolution() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let list = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let vc = plain_vc("CustomerList");
        let ctx = build_load_context("v1", &vc, &[list], "/");
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(!rendered.contains("viewParams"), "{rendered}");
        assert!(!rendered.contains("result.params"), "{rendered}");
    }

    #[test]
    fn role_guard_renders_at_top_of_load_function() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = IfmlViewContainer {
            name: "AdminConsole".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            roles: vec!["admin".to_string(), "manager".to_string()],
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = build_load_context("v1", &vc, &[], "/");
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import { redirect } from '@sveltejs/kit';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("import { currentRoles } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("const viewRoles = ['admin', 'manager'];"),
            "{rendered}"
        );
        assert!(
            rendered.contains("const roles = currentRoles();"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "if (browser && viewRoles.length && !roles.some((r) => viewRoles.includes(r))) {"
            ),
            "{rendered}"
        );
        assert!(rendered.contains("throw redirect(303, '/');"), "{rendered}");
        assert!(!rendered.contains("can("), "{rendered}");
        let load_start = rendered.find("export const load").expect("load fn");
        let guard_start = rendered
            .find("const roles = currentRoles();")
            .expect("guard");
        assert!(
            guard_start > load_start,
            "guard must sit inside load: {rendered}"
        );
    }

    #[test]
    fn no_roles_load_is_byte_identical_to_pre_guard_output() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = build_load_context("v1", &vc, &[], "/");
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert_eq!(
            rendered,
            "import type { PageLoad } from './$types';\n\
             \n\
             export const load: PageLoad = async ({ params, url, fetch }) => {\n\
             \tconst result: Record<string, unknown> = {};\n\
             \n\
             \n\
             \treturn result;\n\
             };\n"
        );
    }

    fn roles_helper_model(roles: Vec<String>) -> super::super::context::IfmlModel {
        model_with_guards(roles, Vec::new(), None)
    }

    fn model_with_guards(
        roles: Vec<String>,
        requires: Vec<String>,
        policy: Option<super::super::context::PolicyContext>,
    ) -> super::super::context::IfmlModel {
        let vc = IfmlViewContainer {
            name: "AdminConsole".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            roles,
            requires,
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        super::super::context::IfmlModel {
            view_containers: vec![vc],
            actions: Vec::new(),
            navigation_edges: Vec::new(),
            data_flows: Vec::new(),
            generation_order: Vec::new(),
            policy,
        }
    }

    #[test]
    fn roles_helper_emitted_once_and_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let gen = IfmlRouteGenerator::new(tmp.path(), "svelte");

        let file = gen
            .roles_helper(&roles_helper_model(vec!["admin".to_string()]))
            .expect("roles.ts for role-guarded model");
        assert!(file.path.ends_with("src/lib/roles.ts"), "{:?}", file.path);
        assert!(
            file.content
                .contains("export function currentRoles(): string[] {"),
            "{}",
            file.content
        );
        assert!(
            file.content
                .contains("(globalThis as any).__USER_ROLES__ ?? []"),
            "{}",
            file.content
        );
        assert!(
            !file.content.contains("can("),
            "roles-only helpers without policy must stay in the legacy shape: {}",
            file.content
        );

        std::fs::create_dir_all(file.path.parent().expect("parent dir")).unwrap();
        std::fs::write(&file.path, "custom roles helper").unwrap();
        assert!(
            gen.roles_helper(&roles_helper_model(vec!["admin".to_string()]))
                .is_none(),
            "existing roles.ts must never be overwritten"
        );

        assert!(
            gen.roles_helper(&roles_helper_model(Vec::new())).is_none(),
            "no role-guarded views must mean no roles.ts"
        );

        let react = IfmlRouteGenerator::new(tmp.path(), "react");
        assert!(
            react
                .roles_helper(&roles_helper_model(vec!["admin".to_string()]))
                .is_none(),
            "guard helper is svelte-only in this slice"
        );
    }

    fn plain_vc(name: &str) -> IfmlViewContainer {
        IfmlViewContainer {
            name: name.to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        }
    }

    fn guarded_vc(name: &str, roles: Vec<String>, requires: Vec<String>) -> IfmlViewContainer {
        IfmlViewContainer {
            name: name.to_string(),
            roles,
            requires,
            ..plain_vc(name)
        }
    }

    fn model_of(vcs: Vec<IfmlViewContainer>) -> super::super::context::IfmlModel {
        super::super::context::IfmlModel {
            view_containers: vcs,
            actions: Vec::new(),
            navigation_edges: Vec::new(),
            data_flows: Vec::new(),
            generation_order: Vec::new(),
            policy: None,
        }
    }

    #[test]
    fn denial_target_prefers_first_unguarded_view() {
        assert_eq!(
            denial_target(&model_of(vec![
                guarded_vc("Admin", vec!["admin".to_string()], Vec::new()),
                plain_vc("Public"),
            ])),
            "/public"
        );
        assert_eq!(
            denial_target(&model_of(vec![
                guarded_vc("Vault", Vec::new(), vec!["open_vault".to_string()]),
                plain_vc("Public"),
            ])),
            "/public",
            "requires-only views count as guarded"
        );
        assert_eq!(
            denial_target(&model_of(vec![plain_vc("First"), plain_vc("Second")])),
            "/first",
            "graph order decides between unguarded views"
        );
        assert_eq!(
            denial_target(&model_of(vec![guarded_vc(
                "Admin",
                vec!["admin".to_string()],
                Vec::new()
            )])),
            "/",
            "no unguarded view falls back to /"
        );
        assert_eq!(denial_target(&model_of(vec![])), "/");
    }

    #[test]
    fn capability_guard_renders_can_checks_and_denial_target() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let model = model_of(vec![
            guarded_vc(
                "RefundConsole",
                Vec::new(),
                vec!["manage_refunds".to_string()],
            ),
            plain_vc("CustomerList"),
        ]);
        let target = denial_target(&model);
        assert_eq!(target, "/customerlist");
        let ctx = build_load_context("v1", &model.view_containers[0], &[], &target);
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import { redirect } from '@sveltejs/kit';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("import { can } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            !rendered.contains("currentRoles"),
            "capability-only views need no roles import: {rendered}"
        );
        assert!(
            rendered.contains("const viewRequires = ['manage_refunds'];"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "if (browser && viewRequires.length && !viewRequires.some((c) => can(c))) {"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("throw redirect(303, '/customerlist');"),
            "{rendered}"
        );
    }

    #[test]
    fn combined_guard_renders_capability_check_before_roles_check() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_vc(
            "AdminConsole",
            vec!["admin".to_string()],
            vec!["manage_refunds".to_string()],
        );
        let ctx = build_load_context("v1", &vc, &[], "/");
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import { currentRoles } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("import { can } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("const viewRoles = ['admin'];"),
            "{rendered}"
        );
        assert!(
            rendered.contains("const viewRequires = ['manage_refunds'];"),
            "{rendered}"
        );
        let cap_check = rendered
            .find("viewRequires.some((c) => can(c))")
            .expect("capability check");
        let role_check = rendered
            .find("roles.some((r) => viewRoles.includes(r))")
            .expect("roles check");
        assert!(
            cap_check < role_check,
            "capability check is primary and runs first: {rendered}"
        );
        assert_eq!(
            rendered.matches("throw redirect(303, '/');").count(),
            2,
            "both guards redirect to the denial target: {rendered}"
        );
    }

    fn form_component_named(name: &str, events: Vec<IfmlEvent>) -> IfmlComponent {
        let mut c = component_with_spec(Some(ComponentSpec::Form(FormSpec {
            fields: vec![field_def("name", InputFieldType::Text)],
        })));
        c.name = name.to_string();
        c.component_type = "form".to_string();
        c.events = events;
        c
    }

    fn guarded_view(
        name: &str,
        roles: Vec<String>,
        requires: Vec<String>,
        components: Vec<IfmlComponent>,
    ) -> IfmlViewContainer {
        IfmlViewContainer {
            roles,
            requires,
            components,
            ..plain_vc(name)
        }
    }

    fn page_context_for(
        vc: &IfmlViewContainer,
        mappings: Option<&IfmlComponentMappings>,
    ) -> PageSvelteContext {
        futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            vc,
            mappings,
            &HashSet::new(),
        ))
    }

    #[test]
    fn requires_view_gates_submit_behind_can_check() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_view(
            "RefundEdit",
            Vec::new(),
            vec!["manage_refunds".to_string()],
            vec![form_component_named("editor", Vec::new())],
        );
        let ctx = page_context_for(&vc, None);
        assert!(!ctx.control_gate.open.is_empty());
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import { can } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            !rendered.contains("currentRoles"),
            "capability-only views need no roles import: {rendered}"
        );
        assert!(
            rendered.contains("const viewRequires = ['manage_refunds'];"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "{#if viewRequires.some((c) => can(c))}<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>{/if}"
            ),
            "{rendered}"
        );
        let script_end = rendered.find("</script>").expect("script end");
        let consts = rendered.find("const viewRequires").expect("consts");
        assert!(
            consts < script_end,
            "gate consts must sit inside the script: {rendered}"
        );
    }

    #[test]
    fn roles_only_view_gates_submit_behind_role_check() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_view(
            "AdminConsole",
            vec!["admin".to_string(), "manager".to_string()],
            Vec::new(),
            vec![form_component_named("editor", Vec::new())],
        );
        let ctx = page_context_for(&vc, None);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import { currentRoles } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            !rendered.contains("can("),
            "role-only views need no capability import: {rendered}"
        );
        assert!(
            rendered.contains("const viewRoles = ['admin', 'manager'];"),
            "{rendered}"
        );
        assert!(
            rendered.contains("const roles = currentRoles();"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "{#if roles.some((r) => viewRoles.includes(r))}<button type=\"submit\" data-testid=\"editor-submit\""
            ),
            "{rendered}"
        );
        assert!(rendered.contains("</button>{/if}"), "{rendered}");
    }

    #[test]
    fn combined_view_gates_submit_with_and_of_checks_matching_load_guard() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_view(
            "AdminConsole",
            vec!["admin".to_string()],
            vec!["manage_refunds".to_string()],
            vec![form_component_named("editor", Vec::new())],
        );
        let ctx = page_context_for(&vc, None);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import { currentRoles } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("import { can } from '$lib/roles';"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "{#if viewRequires.some((c) => can(c)) && roles.some((r) => viewRoles.includes(r))}<button type=\"submit\""
            ),
            "markup must match the load guard's requires-AND-roles semantics: {rendered}"
        );
        let roles_import = rendered
            .find("import { currentRoles }")
            .expect("roles import");
        let can_import = rendered.find("import { can }").expect("can import");
        assert!(
            roles_import < can_import,
            "import order mirrors the load template: {rendered}"
        );
    }

    #[test]
    fn unguarded_view_emits_no_gate_and_no_roles_import() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_view(
            "CustomerEdit",
            Vec::new(),
            Vec::new(),
            vec![form_component_named("editor", Vec::new())],
        );
        let ctx = page_context_for(&vc, None);
        assert!(ctx.control_gate.open.is_empty());
        assert!(ctx.control_gate.imports.is_empty());
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("$lib/roles"), "{rendered}");
        assert!(!rendered.contains("viewRequires"), "{rendered}");
        assert!(!rendered.contains("viewRoles"), "{rendered}");
        assert!(!rendered.contains("can("), "{rendered}");
        assert!(
            rendered.contains(
                "<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>"
            ),
            "unguarded submit stays byte-identical: {rendered}"
        );
    }

    #[test]
    fn list_only_guarded_view_emits_no_unused_gate_imports() {
        let vc = guarded_view(
            "RefundConsole",
            Vec::new(),
            vec!["manage_refunds".to_string()],
            vec![component_with_spec(Some(table_spec()))],
        );
        let ctx = page_context_for(&vc, None);
        assert!(
            ctx.control_gate.open.is_empty(),
            "no fallback form branch must mean no gate imports/consts"
        );
    }

    #[test]
    fn mapped_submit_and_cancel_buttons_are_gated_too() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
testids = { root = "ui-button" }
"#,
        )
        .unwrap();
        let editor = form_component_named(
            "editor",
            vec![IfmlEvent {
                name: "comp_editor_cancel".to_string(),
                event_type: "cancel".to_string(),
                params: vec![],
                action: IfmlAction::Navigate {
                    target: "CustomerList".to_string(),
                    binding: HashMap::new(),
                },
            }],
        );
        let vc = guarded_view(
            "RefundEdit",
            Vec::new(),
            vec!["manage_refunds".to_string()],
            vec![editor],
        );
        let ctx = page_context_for(&vc, Some(&mappings));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "{#if viewRequires.some((c) => can(c))}<Button onclick={submit_editor} disabled={submitting} testid=\"ui-button\">Submit</Button>{/if}"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "{#if viewRequires.some((c) => can(c))}<Button onclick={comp_editor_cancel} testid=\"editor-cancel\">Cancel</Button>{/if}"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn gate_consts_pair_requires_and_roles_in_guard_order() {
        let gate =
            ControlGateContext::for_guards(&["manage_refunds".to_string()], &["admin".to_string()]);
        assert_eq!(
            gate.consts,
            "const viewRequires = ['manage_refunds'];\n\tconst viewRoles = ['admin'];\n\tconst roles = currentRoles();"
        );
        assert_eq!(
            gate.open,
            "{#if viewRequires.some((c) => can(c)) && roles.some((r) => viewRoles.includes(r))}"
        );
        assert_eq!(gate.close, "{/if}");
        assert_eq!(
            gate.imports,
            vec!["currentRoles".to_string(), "can".to_string()]
        );
    }

    #[test]
    fn policy_roles_helper_embeds_role_capabilities() {
        let tmp = tempfile::tempdir().unwrap();
        let gen = IfmlRouteGenerator::new(tmp.path(), "svelte");
        let policy = super::super::context::PolicyContext {
            actors: vec![
                (
                    "Admin".to_string(),
                    vec!["manage_refunds".to_string(), "approve_expense".to_string()],
                ),
                ("Intern".to_string(), Vec::new()),
            ],
            capabilities: vec!["approve_expense".to_string(), "manage_refunds".to_string()],
        };
        let file = gen
            .roles_helper(&model_with_guards(
                vec!["admin".to_string()],
                Vec::new(),
                Some(policy),
            ))
            .expect("roles.ts for policy model");
        assert!(
            file.content
                .contains("const ROLE_CAPABILITIES: Record<string, string[]> = {"),
            "{}",
            file.content
        );
        assert!(
            file.content
                .contains("'Admin': ['manage_refunds', 'approve_expense'],"),
            "{}",
            file.content
        );
        assert!(file.content.contains("'Intern': [],"), "{}", file.content);
        assert!(
            file.content
                .contains("export function can(capability: string): boolean {"),
            "{}",
            file.content
        );
        assert!(
            file.content
                .contains("(globalThis as any).__USER_CAPABILITIES__ ?? []"),
            "{}",
            file.content
        );
        assert!(
            file.content.contains("ROLE_CAPABILITIES[role] ?? []"),
            "{}",
            file.content
        );
    }

    #[test]
    fn requires_only_roles_helper_checks_user_capabilities_without_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let gen = IfmlRouteGenerator::new(tmp.path(), "svelte");
        let file = gen
            .roles_helper(&model_with_guards(
                Vec::new(),
                vec!["manage_refunds".to_string()],
                None,
            ))
            .expect("roles.ts for requires-only model");
        assert!(
            file.content
                .contains("export function can(capability: string): boolean {"),
            "{}",
            file.content
        );
        assert!(
            file.content
                .contains("(globalThis as any).__USER_CAPABILITIES__ ?? []"),
            "{}",
            file.content
        );
        assert!(
            !file.content.contains("ROLE_CAPABILITIES"),
            "no policy must mean no embedded capability map: {}",
            file.content
        );
    }

    #[test]
    fn chart_spec_maps_kind_and_axes() {
        let spec = ComponentSpec::Chart(ChartSpec {
            kind: ChartKind::Bar,
            label_field: Some("region".to_string()),
            value_fields: vec!["revenue".to_string(), "cost".to_string()],
        });
        let ctx = page_component_context_sync(&component_with_spec(Some(spec)));
        let chart = ctx.chart.expect("chart render context");
        assert_eq!(chart.kind, "bar");
        assert_eq!(chart.label_field.as_deref(), Some("region"));
        assert_eq!(chart.value_fields, vec!["revenue", "cost"]);
    }

    #[test]
    fn render_expression_covers_operators_and_calls() {
        let expr = Expression::BinOp {
            left: Box::new(Expression::Call {
                name: "len".to_string(),
                args: vec![Expression::Ident("name".to_string())],
            }),
            op: BinOp::Gt,
            right: Box::new(Expression::NumLit(2.0)),
        };
        assert_eq!(render_expression(&expr), "len(name) > 2");

        let and = Expression::BinOp {
            left: Box::new(Expression::BoolLit(true)),
            op: BinOp::And,
            right: Box::new(Expression::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(Expression::BoolLit(false)),
            }),
        };
        assert_eq!(render_expression(&and), "true && !false");

        let string = Expression::StringLit("a \"quoted\" b".to_string());
        assert_eq!(render_expression(&string), "\"a \\\"quoted\\\" b\"");
    }

    #[test]
    fn nav_url_builds_query_from_sorted_bindings() {
        let mut binding = HashMap::new();
        binding.insert("customerId".to_string(), "row.id".to_string());
        binding.insert("region".to_string(), "\"eu\"".to_string());
        assert_eq!(
            nav_url_expr("CustomerDetail", &binding),
            "`/customerdetail?customerId=${row.id}&region=${\"eu\"}`"
        );
        assert_eq!(
            nav_url_expr("CustomerList", &HashMap::new()),
            "\"/customerlist\""
        );
    }

    #[test]
    fn navigate_event_resolves_handler_and_url() {
        let evt = IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: vec!["row".to_string()],
            action: IfmlAction::Navigate {
                target: "CustomerDetail".to_string(),
                binding: HashMap::new(),
            },
        };
        let rendered = render_event(&evt, &HashSet::new());
        assert_eq!(rendered.handler_name, "comp_grid_select");
        assert_eq!(rendered.action_kind, "navigate");
        assert_eq!(rendered.url_expr, "\"/customerdetail\"");
    }

    fn svelte_context(components: Vec<PageComponentContext>) -> PageSvelteContext {
        PageSvelteContext {
            api_version: "v1".to_string(),
            name: "View".to_string(),
            label: "View".to_string(),
            components,
            params: Vec::new(),
            view_params: Vec::new(),
            view_events: Vec::new(),
            imports: Vec::new(),
            needs_goto: false,
            needs_on_mount: false,
            has_submit: false,
            view_role: None,
            container_role: None,
            roles: Vec::new(),
            requires: Vec::new(),
            control_gate: ControlGateContext::default(),
            modal: None,
            container: None,
        }
    }

    #[test]
    fn template_renders_typed_table_and_form_markup() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let table_ctx = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let ctx = svelte_context(vec![table_ctx]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("<table data-testid=\"grid-table\" data-pagination=\"true\">"),
            "{rendered}"
        );
        assert!(
            rendered.contains("<tr data-testid=\"grid-row\">"),
            "{rendered}"
        );
        assert!(rendered.contains("<th>Name</th>"), "{rendered}");
        assert!(
            rendered.contains("<td>{item.tenure_years(Customer.hire_date)}</td>"),
            "{rendered}"
        );

        let chart = ComponentSpec::Chart(ChartSpec {
            kind: ChartKind::Pie,
            label_field: None,
            value_fields: vec!["revenue".to_string()],
        });
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            Some(chart),
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(rendered.contains("data-chart-kind=\"pie\""), "{rendered}");
        assert!(
            rendered.contains("data-value-fields=\"revenue\""),
            "{rendered}"
        );
    }

    #[test]
    fn template_specless_table_component_renders_nothing() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            None,
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("<table"));
        assert!(!rendered.contains("<form"));
        assert!(!rendered.contains("data-chart-kind"));
        assert!(!rendered.contains("<h1>"));
        assert!(rendered.trim_end().ends_with("</svelte:head>"));
    }

    #[test]
    fn mapped_component_renders_invocation_with_import_and_events() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut c = component_with_spec(Some(table_spec()));
        c.events.push(IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: vec![],
            action: IfmlAction::Navigate {
                target: "CustomerDetail".to_string(),
                binding: HashMap::new(),
            },
        });
        let mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
testids = { root = "data-table", row = "data-row" }
"#,
        )
        .unwrap();
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![c],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&mappings),
            &HashSet::new(),
        ));

        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import DataTable from '$lib/components/DataTable.svelte';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("import { goto } from '$app/navigation';"),
            "{rendered}"
        );
        assert!(rendered.contains("<DataTable"), "{rendered}");
        assert!(rendered.contains("data={data.items}"), "{rendered}");
        assert!(
            rendered.contains("fields={['name', 'status']}"),
            "{rendered}"
        );
        assert!(
            rendered.contains("onselect={comp_grid_select}"),
            "Svelte 5 event-property form must reach the component: {rendered}"
        );
        assert!(
            rendered.contains("<h1>CustomerList</h1>"),
            "mapped component branches must keep the view heading: {rendered}"
        );
        let heading = rendered.find("<h1>").expect("heading");
        let invocation = rendered.find("<DataTable").expect("invocation");
        assert!(
            heading < invocation,
            "heading renders above the mapped component: {rendered}"
        );
        assert!(rendered.contains("testid=\"data-table\""), "{rendered}");
        assert!(rendered.contains("rowTestid=\"data-row\""), "{rendered}");
        assert!(!rendered.contains("<table"), "{rendered}");
        assert!(!rendered.contains("on:select"), "{rendered}");
    }

    #[test]
    fn load_context_wires_first_list_with_pagination() {
        let list = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let second_list = page_component_context_sync(&component_with_spec(None));
        let details = {
            let mut c = component_with_spec(None);
            c.component_type = "details".to_string();
            page_component_context_sync(&c)
        };
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: vec![super::super::context::ParameterDef {
                name: "customerId".to_string(),
                type_ref: "Uuid".to_string(),
                default: None,
            }],
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = build_load_context("v1", &vc, &[list, second_list, details], "/");
        assert!(ctx.has_fetch);
        assert_eq!(ctx.components.len(), 3);
        assert!(ctx.components[0].fetch_list);
        assert!(ctx.components[0].paginate);
        assert!(
            !ctx.components[1].fetch_list,
            "second list must not double-fetch"
        );
        assert!(ctx.components[2].fetch_item);
        assert_eq!(ctx.components[2].id_param.as_deref(), Some("customerId"));
    }

    fn field_def(name: &str, input: InputFieldType) -> codegraph_ifml_dsl::FieldDef {
        codegraph_ifml_dsl::FieldDef {
            name: name.to_string(),
            input,
            required: false,
            validations: Vec::new(),
            values: Vec::new(),
            messages: Vec::new(),
        }
    }

    #[test]
    fn form_save_event_gets_action_control_role() {
        let evt = |event_type: &str| IfmlEvent {
            name: format!("comp_form_{event_type}"),
            event_type: event_type.to_string(),
            params: Vec::new(),
            action: IfmlAction::Navigate {
                target: "CustomerList".to_string(),
                binding: HashMap::new(),
            },
        };
        for event_type in ["save", "submit", "cancel", "back", "click"] {
            let rendered = render_event(&evt(event_type), &HashSet::new());
            assert_eq!(
                rendered.role,
                Some(SemanticRole::ActionControl),
                "{event_type}"
            );
        }
        assert_eq!(render_event(&evt("select"), &HashSet::new()).role, None);
        assert_eq!(render_event(&evt("load"), &HashSet::new()).role, None);
    }

    #[test]
    fn dropdown_and_radio_inputs_carry_selection_field_role() {
        let spec = ComponentSpec::Form(FormSpec {
            fields: vec![
                field_def("tier", InputFieldType::Dropdown),
                field_def("channel", InputFieldType::RadioGroup),
                field_def("name", InputFieldType::Text),
            ],
        });
        let ctx = page_component_context_sync(&component_with_spec(Some(spec)));
        let form = ctx.form.expect("form render context");
        assert_eq!(
            form.fields[0].input_role,
            Some(SemanticRole::SelectionField)
        );
        assert_eq!(
            form.fields[1].input_role,
            Some(SemanticRole::SelectionField)
        );
        assert_eq!(form.fields[2].input_role, None);
    }

    #[test]
    fn component_roles_classify_slots() {
        let table = component_with_spec(Some(table_spec()));
        assert_eq!(
            page_component_context_sync(&table).role,
            Some(SemanticRole::Collection)
        );

        let mut details = component_with_spec(None);
        details.component_type = "details".to_string();
        assert_eq!(
            page_component_context_sync(&details).role,
            Some(SemanticRole::Display)
        );

        let mut dropdown = component_with_spec(None);
        dropdown.component_type = "dropdown".to_string();
        assert_eq!(
            page_component_context_sync(&dropdown).role,
            Some(SemanticRole::SelectionField)
        );

        let form = component_with_spec(Some(ComponentSpec::Form(FormSpec { fields: vec![] })));
        assert_eq!(page_component_context_sync(&form).role, None);
    }

    #[test]
    fn view_roles_mark_modal_landmark_and_xor_containers() {
        let vc = |is_modal: bool, is_landmark: bool, is_xor: bool| IfmlViewContainer {
            name: "CustomerDialog".to_string(),
            label: None,
            is_xor,
            is_default: false,
            is_landmark,
            is_modal,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(true, false, false),
            None,
            &HashSet::new(),
        ));
        assert_eq!(ctx.view_role, Some(SemanticRole::ModalView));
        assert_eq!(ctx.container_role, None);

        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(false, true, false),
            None,
            &HashSet::new(),
        ));
        assert_eq!(ctx.view_role, Some(SemanticRole::Shell));

        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(false, false, true),
            None,
            &HashSet::new(),
        ));
        assert_eq!(ctx.view_role, None);
        assert_eq!(
            ctx.container_role,
            Some(SemanticRole::PresentationContainer)
        );
    }

    #[test]
    fn paginated_table_gets_pagination_role() {
        let paged = TableSpec {
            columns: vec![],
            pagination: true,
        };
        assert_eq!(render_table(&paged).role, Some(SemanticRole::Pagination));
        let plain = TableSpec {
            columns: vec![],
            pagination: false,
        };
        assert_eq!(render_table(&plain).role, None);
    }

    #[test]
    fn role_mapping_resolves_for_collection_slot() {
        let mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
role = "collection"
path = "$lib/components/Collection.svelte"
"#,
        )
        .unwrap();
        let mut c = component_with_spec(Some(table_spec()));
        c.component_type = "list".to_string();
        let ctx = page_component_context_for_tests(&c, &mappings);
        let mapping = ctx.mapping.expect("role-mapped component");
        assert_eq!(mapping.import_name, "Collection");
    }

    fn form_component() -> IfmlComponent {
        let mut c = component_with_spec(Some(ComponentSpec::Form(FormSpec {
            fields: vec![field_def("name", InputFieldType::Text)],
        })));
        c.name = "editor".to_string();
        c.component_type = "form".to_string();
        c
    }

    fn save_event() -> IfmlEvent {
        IfmlEvent {
            name: "comp_editor_save".to_string(),
            event_type: "save".to_string(),
            params: Vec::new(),
            action: IfmlAction::Navigate {
                target: "CustomerList".to_string(),
                binding: HashMap::new(),
            },
        }
    }

    fn form_view(is_modal: bool) -> IfmlViewContainer {
        IfmlViewContainer {
            name: if is_modal {
                "CustomerDialog".to_string()
            } else {
                "CustomerEdit".to_string()
            },
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![form_component()],
            events: Vec::new(),
            containers: Vec::new(),
        }
    }

    fn button_mappings() -> IfmlComponentMappings {
        toml::from_str(
            r#"
[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
testids = { root = "ui-button" }
"#,
        )
        .unwrap()
    }

    #[test]
    fn mapped_save_event_renders_button_invocation() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = form_view(false);
        vc.components[0].events.push(save_event());
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&button_mappings()),
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import Button from '$lib/components/Button.svelte';"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "<Button onclick={submit_editor} disabled={submitting} testid=\"ui-button\">Save</Button>"
            ),
            "{rendered}"
        );
        assert!(
            !rendered.contains("<button type=\"submit\""),
            "mapped button must replace the hardcoded fallback: {rendered}"
        );
    }

    #[test]
    fn mapped_cancel_event_renders_secondary_button() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = form_view(false);
        vc.components[0].events.push(IfmlEvent {
            name: "comp_editor_cancel".to_string(),
            event_type: "cancel".to_string(),
            params: Vec::new(),
            action: IfmlAction::Navigate {
                target: "CustomerList".to_string(),
                binding: HashMap::new(),
            },
        });
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&button_mappings()),
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "<Button onclick={comp_editor_cancel} testid=\"editor-cancel\">Cancel</Button>"
            ),
            "secondary buttons must not share the submit button's root testid: {rendered}"
        );
    }

    #[test]
    fn unmapped_form_keeps_hardcoded_submit_button() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = form_view(false);
        vc.components[0].events.push(save_event());
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&IfmlComponentMappings::default()),
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>"
            ),
            "{rendered}"
        );
        assert!(!rendered.contains("<Button"), "{rendered}");
    }

    #[test]
    fn modal_view_with_mapping_renders_dialog_wrapper() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = form_view(true);
        vc.components[0].events.push(save_event());
        let mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
role = "modal-view"
path = "$lib/components/Dialog.svelte"
export = "Dialog"
testids = { root = "customer-modal" }

[[component]]
role = "action-control"
path = "$lib/components/Button.svelte"
export = "Button"
"#,
        )
        .unwrap();
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&mappings),
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import Dialog from '$lib/components/Dialog.svelte';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("<Dialog bind:open={dialog_open} testid=\"customer-modal\">"),
            "{rendered}"
        );
        assert!(rendered.contains("</Dialog>"), "{rendered}");
        assert!(
            rendered.contains("let dialog_open = $state(true);"),
            "{rendered}"
        );
        assert!(rendered.contains("history.back();"), "{rendered}");
        assert!(
            rendered.contains("data-testid=\"customerdialog-modal-close\" onclick={close_dialog}"),
            "{rendered}"
        );
    }

    #[test]
    fn modal_view_without_modal_mapping_renders_builtin_div_fallback() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = form_view(true);
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&button_mappings()),
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "<div class=\"modal\" role=\"dialog\" data-testid=\"customerdialog-modal\">"
            ),
            "{rendered}"
        );
        assert!(rendered.contains("</div>"), "{rendered}");
        assert!(!rendered.contains("bind:open"), "{rendered}");
        assert!(
            rendered.contains("let dialog_open = $state(true);"),
            "{rendered}"
        );
    }

    #[test]
    fn modal_view_without_pack_renders_plain_page() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = form_view(true);
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            None,
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("dialog_open"), "{rendered}");
        assert!(!rendered.contains("class=\"modal\""), "{rendered}");
        assert!(
            rendered.contains("<form data-testid=\"editor-form\""),
            "no-mapping modal views must render exactly as before: {rendered}"
        );
    }

    #[test]
    fn navigation_into_modal_target_appends_dialog_param() {
        let targets: HashSet<String> = ["CustomerDialog".to_string()].into_iter().collect();
        let evt = IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: vec!["row".to_string()],
            action: IfmlAction::Navigate {
                target: "CustomerDialog".to_string(),
                binding: HashMap::new(),
            },
        };
        assert_eq!(
            render_event(&evt, &targets).url_expr,
            "\"/customerdialog?dialog=open\""
        );

        let mut binding = HashMap::new();
        binding.insert("customerId".to_string(), "row.id".to_string());
        let bound = IfmlEvent {
            action: IfmlAction::Navigate {
                target: "CustomerDialog".to_string(),
                binding,
            },
            ..evt
        };
        assert_eq!(
            render_event(&bound, &targets).url_expr,
            "`/customerdialog?customerId=${row.id}&dialog=open`"
        );
        assert_eq!(
            render_event(&bound, &HashSet::new()).url_expr,
            "`/customerdialog?customerId=${row.id}`",
            "non-modal targets must keep byte-identical URLs"
        );
    }

    fn xor_view() -> IfmlViewContainer {
        IfmlViewContainer {
            name: "Checkout".to_string(),
            label: Some("Checkout".to_string()),
            is_xor: true,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        }
    }

    fn container_mappings() -> IfmlComponentMappings {
        toml::from_str(
            r#"
[[component]]
role = "presentation-container"
path = "$lib/components/Card.svelte"
export = "Card"
testids = { root = "card" }
"#,
        )
        .unwrap()
    }

    #[test]
    fn xor_view_with_mapping_renders_card_wrapper_around_children() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = xor_view();
        vc.components.push(form_component());
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&container_mappings()),
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import Card from '$lib/components/Card.svelte';"),
            "{rendered}"
        );
        assert!(rendered.contains("<Card testid=\"card\">"), "{rendered}");
        assert!(rendered.contains("</Card>"), "{rendered}");
        assert!(
            rendered.contains("<form data-testid=\"editor-form\""),
            "children must render inside the wrapper: {rendered}"
        );
        let open = rendered.find("<Card testid=\"card\">").expect("open");
        let child = rendered.find("<form").expect("form");
        let close = rendered.rfind("</Card>").expect("close");
        assert!(open < child && child < close, "{rendered}");
    }

    #[test]
    fn xor_view_without_container_mapping_renders_section_fallback() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = xor_view();
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&button_mappings()),
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("<section data-testid=\"checkout-container\">"),
            "{rendered}"
        );
        assert!(rendered.contains("</section>"), "{rendered}");
        assert!(!rendered.contains("<Card"), "{rendered}");
    }

    #[test]
    fn xor_view_without_pack_renders_plain_page() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = xor_view();
        vc.components.push(form_component());
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            None,
            &HashSet::new(),
        ));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("<section"), "{rendered}");
        assert!(!rendered.contains("<Card"), "{rendered}");
        assert!(
            rendered.contains("<form data-testid=\"editor-form\""),
            "no-pack xor views must render exactly as before: {rendered}"
        );
    }

    #[test]
    fn mapped_container_testid_resolves_only_for_xor_with_mapping() {
        assert_eq!(
            mapped_container_testid("Checkout", true, Some(&container_mappings())),
            Some("card".to_string())
        );
        assert_eq!(
            mapped_container_testid("Checkout", true, Some(&button_mappings())),
            None
        );
        assert_eq!(
            mapped_container_testid("Checkout", false, Some(&container_mappings())),
            None
        );
    }

    #[test]
    fn shell_nav_builds_items_from_landmark_navigate_events() {
        let mut list = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: Some("Customers".to_string()),
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        list.events.push(IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: vec!["row".to_string()],
            action: IfmlAction::Navigate {
                target: "CustomerDetail".to_string(),
                binding: HashMap::new(),
            },
        });
        let detail = IfmlViewContainer {
            name: "CustomerDetail".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let shell_mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
role = "shell"
path = "$lib/components/Nav.svelte"
export = "Nav"
testids = { root = "side-nav" }
"#,
        )
        .unwrap();
        let nav = shell_nav(&[list, detail], Some(&shell_mappings)).expect("shell nav");
        assert_eq!(nav.import.export_name, "Nav");
        assert_eq!(nav.testid.as_deref(), Some("side-nav"));
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].label, "CustomerDetail");
        assert_eq!(nav.items[0].href_attr, "href={\"/customerdetail\"}");
    }

    #[test]
    fn shell_nav_is_none_without_landmark_or_shell_mapping() {
        let plain = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        assert!(shell_nav(std::slice::from_ref(&plain), Some(&container_mappings())).is_none());
        assert!(shell_nav(&[plain], None).is_none());
    }

    #[test]
    fn layout_template_renders_shell_navigation_and_children() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let ctx = LayoutSvelteContext {
            shell: RenderShellNav {
                import: RenderImport {
                    export_name: "NavigationMenu".to_string(),
                    import_path: "$lib/components/ui/navigation-menu/navigation-menu.svelte"
                        .to_string(),
                },
                testid: Some("navigation-menu".to_string()),
                items: vec![
                    RenderNavItem {
                        label: "Customers".to_string(),
                        href_attr: "href=\"/customerlist\"".to_string(),
                    },
                    RenderNavItem {
                        label: "CustomerDetail".to_string(),
                        href_attr: "href={`?customerId=${row.id}`}".to_string(),
                    },
                ],
            },
        };
        let rendered = render_template(&tera, "ifml/svelte/layout.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "import NavigationMenu from '$lib/components/ui/navigation-menu/navigation-menu.svelte';"
            ),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("let { children }: { children: import('svelte').Snippet } = $props();"),
            "{rendered}"
        );
        assert!(
            rendered.contains("<NavigationMenu testid=\"navigation-menu\">"),
            "{rendered}"
        );
        assert!(
            rendered.contains("<a href=\"/customerlist\">Customers</a>"),
            "{rendered}"
        );
        assert!(
            rendered.contains("<a href={`?customerId=${row.id}`}>CustomerDetail</a>"),
            "{rendered}"
        );
        assert!(rendered.contains("</NavigationMenu>"), "{rendered}");
        assert!(rendered.contains("{@render children()}"), "{rendered}");
    }
}
