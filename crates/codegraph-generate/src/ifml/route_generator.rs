use crate::ProjectConfig;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::ux::{Align, Dimension, Display, ToneMap, UxRules};
use codegraph_config::{DomainConfig, IfmlComponentMapping, IfmlComponentMappings, SemanticRole};
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use rex_ifml::{
    BinOp, ChartKind, ChartSpec, ColumnDef, ComponentSpec, Expression, FormSpec, TableSpec, UnaryOp,
};
use serde::Serialize;

use crate::error::Result;
use crate::render_template;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;

use super::api_paths::{id_param_from, resolve_entity_api, ResolvedApi};
use super::context::{IfmlAction, IfmlComponent, IfmlEvent, IfmlViewContainer};
use super::control_core;
use super::querier::*;

/// The shared control-inference entry over `(rust_type, field_name)` pairs
/// (`fields_with_types`), re-exported for conformance testing (issue #201).
pub use super::control_core::control_for_field;

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

        // Issue #301: resolve the ux plane ONCE per generation — one plan
        // per distinct bound entity drives the timeline layouts and the
        // advisory diagnostics (printed through the stderr warning channel,
        // deduped like the entity pipeline's ui-page generator). Flag off ⇒
        // no plans, no lines, byte-identical output.
        let (ux_plans, ux_diag_lines) =
            resolve_generation_ux(db, config, &model, project.ux.as_ref()).await?;
        for line in &ux_diag_lines {
            eprintln!("warning: ux-rules: {line}");
        }
        let ux_generation = project.ux.as_ref().map(|rules| UxGeneration {
            rules,
            plans: &ux_plans,
        });

        for vc in ordered_view_containers(&model) {
            let ctx = build_page_context(
                db,
                config,
                &project.api_version,
                vc,
                self.mappings.as_ref(),
                &modal_targets,
                ux_generation.as_ref(),
                project,
            )
            .await?;

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

#[derive(Debug, Clone, Serialize)]
pub struct PageSvelteContext {
    pub api_version: String,
    name: String,
    label: String,
    components: Vec<PageComponentContext>,
    /// Body render groups: the view's own components first, then one group
    /// per nested view container. Each xor group renders its container label
    /// heading before its content (only when the presentation wrapper is
    /// active — no-pack output stays byte-identical).
    groups: Vec<RenderGroup>,
    params: Vec<super::context::ParameterDef>,
    view_events: Vec<RenderEvent>,
    imports: Vec<RenderImport>,
    needs_goto: bool,
    needs_on_mount: bool,
    /// Whether any component carries a transition handler (`invalidateAll`
    /// import).
    needs_invalidate: bool,
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
    /// View-level guard lowered from the persisted expression AST
    /// (issue #278). `None` — and therefore no rendered guard — unless the
    /// `expr_ir` feature flag is ON and the condition lowers cleanly.
    guard_expr: Option<String>,
    /// ux-rules page-level formatting baseline (issue #300): locale + money
    /// `Intl.NumberFormat` options for the fallback-table script helpers.
    /// `None` when the `ux_rules` plane is off — the key is skipped so
    /// flag-off output stays byte-identical. IFML page templates render
    /// with the component context only (no `project` in scope — see
    /// `render_template`), so the format baseline is threaded explicitly.
    #[serde(skip_serializing_if = "Option::is_none")]
    ux: Option<PageUxContext>,
}

/// Locale/currency baseline for the fallback-table formatting helpers
/// (issue #300), threaded from `ProjectConfig.ux.format`.
#[derive(Debug, Clone, Serialize)]
pub struct PageUxContext {
    /// BCP-47 locale for the page's `Intl` formatters.
    pub locale: String,
    /// Ready-to-render `Intl.NumberFormat` options object literal for money
    /// cells (`{ style: 'currency', currency: 'NZD' }`), or `{}` when no
    /// currency is configured (money degrades to grouped decimals).
    pub money_options: String,
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

    /// This gate AND-composed with an event-level `requires` check:
    /// `{#if viewChecks && ['Cap'].some((c) => can(c))}`. An empty requires
    /// list clones the gate unchanged; an inactive view gate yields the
    /// event check alone.
    fn compose_with_event(&self, requires: &[String]) -> Self {
        if requires.is_empty() {
            return self.clone();
        }
        let list = requires
            .iter()
            .map(|c| js_quote(c))
            .collect::<Vec<_>>()
            .join(", ");
        let check = format!("[{list}].some((c) => can(c))");
        let inner = if self.open.is_empty() {
            check
        } else {
            let view_expr = self
                .open
                .strip_prefix("{#if ")
                .and_then(|s| s.strip_suffix('}'))
                .unwrap_or_default();
            format!("{view_expr} && {check}")
        };
        let mut imports = self.imports.clone();
        if !imports.iter().any(|name| name == "can") {
            imports.push("can".to_string());
        }
        Self {
            imports,
            consts: self.consts.clone(),
            open: format!("{{#if {inner}}}"),
            close: "{/if}".to_string(),
        }
    }
}

/// One body render group: an optional container label heading followed by
/// the group's components. Group 0 is the view's own components (no
/// heading); further groups are the view's nested containers in
/// declaration order.
#[derive(Debug, Clone, Serialize)]
pub struct RenderGroup {
    pub heading: Option<String>,
    pub components: Vec<PageComponentContext>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderImport {
    pub export_name: String,
    pub import_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageComponentContext {
    pub(crate) name: String,
    /// Sanitized JS identifier (const/handler names).
    pub(crate) js_name: String,
    pub(crate) component_type: String,
    /// Semantic slot role of the whole component (`collection`, `display`,
    /// `selection-field`); forms keep `None` — their inputs carry roles.
    pub(crate) role: Option<SemanticRole>,
    pub(crate) entity: String,
    pub(crate) fields: Vec<String>,
    pub(crate) fields_with_types: Vec<(String, String)>,
    pub(crate) filter: String,
    pub(crate) table: Option<RenderTable>,
    pub(crate) form: Option<RenderForm>,
    pub(crate) chart: Option<RenderChart>,
    pub(crate) mapping: Option<RenderMapping>,
    pub(crate) events: Vec<RenderEvent>,
    /// Ready-to-render event callback props for mapped components,
    /// e.g. `onselect={comp_grid_select}` (Svelte 5 event-property form —
    /// `on:select` directives are not forwarded to components).
    pub(crate) event_props: Vec<String>,
    /// Ready-to-render data prop for mapped components, e.g. `data={data.items}`.
    pub(crate) data_prop: String,
    /// Ready-to-render fields prop for mapped components.
    pub(crate) fields_prop: String,
    /// Ready-to-render submit callback prop for mapped form components.
    pub(crate) submit_prop: Option<String>,
    /// Joined validation expressions for mapped form components
    /// (`data-validate` attribute).
    pub(crate) data_validate: Option<String>,
    /// First validation message for mapped form components
    /// (`data-validate-message` attribute).
    pub(crate) data_validate_message: Option<String>,
    pub(crate) api: Option<ResolvedApi>,
    /// View parameter carrying the entity id (edit mode), when any.
    pub(crate) id_param: Option<String>,
    /// Fields passed to a mapped component: declared fields when present,
    /// else derived from the typed spec (table columns / form fields).
    pub(crate) mapped_fields: Vec<String>,
    /// Handler for row-click navigation on list/table fallback markup.
    pub(crate) row_handler: Option<String>,
    pub(crate) submit: Option<RenderSubmit>,
    pub(crate) submit_handler: Option<String>,
    /// Mapped submit button replacing the hardcoded fallback `<button>`.
    pub(crate) submit_button: Option<RenderButton>,
    /// Mapped cancel/back/click buttons rendered after the form.
    pub(crate) buttons: Vec<RenderButton>,
    /// Workflow state display for the bound entity, resolved from the
    /// owning domain's config; `None` renders no badge (byte-identical).
    pub(crate) workflow: Option<RenderWorkflow>,
    /// Load fetch wiring for this component (mirrors the `+page.ts`
    /// contract: first list/details/form wins, details/form need an id
    /// param). Gates mapped-branch badges and transition buttons on the
    /// component's value path actually being fetch-backed.
    pub(crate) fetch_list: bool,
    pub(crate) fetch_item: bool,
    pub(crate) fetch_form: bool,
    /// Fetch URL (JS template literal) for the component's transition
    /// handler: `{base}/${id}/actions/transition`; `None` when the component
    /// carries no transition buttons.
    pub(crate) transition_url_expr: Option<String>,
    /// Markup gate for the submit control: the view gate AND-composed with
    /// the primary save/submit event's `requires` check. Equals the view
    /// gate (byte-identical) when no event-level requires exist.
    pub(crate) submit_gate: ControlGateContext,
    /// Whether the fallback form branch renders: emits the typed
    /// `{js_name}_form_state` const used by value bindings and the
    /// workflow badge.
    pub(crate) form_state: bool,
    /// Ready-to-render typed payload construction lines for the submit
    /// handler (`const payload: ...` + per-field coercions); empty keeps
    /// the untyped `formData` body (byte-identical).
    pub(crate) form_payload: String,
    /// Whether the component belongs to a nested view container: suppresses
    /// the page-level `<h1>` heading inside the group.
    pub(crate) in_container: bool,
    /// ux-resolved columns for a spec-less list component (issue #300):
    /// the same [`RenderColumn`] shape as typed tables, keyed aligned with
    /// `fields`. Empty (and skipped from the context) when the ux plane is
    /// off — the template keeps its `comp.fields` loop, byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) ux_list_columns: Vec<RenderColumn>,
    /// Uniform timeline view of the collection layout (issue #301):
    /// `Some` exactly when the active fallback branch renders a timeline —
    /// typed tables project their `RenderTable::layout` timeline here,
    /// spec-less lists resolve theirs directly. Mapped components never
    /// resolve one (whole-component mappings replace the fallback by
    /// design). `None` (skipped) renders the table — flag-off and
    /// rule-less pages stay byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) timeline: Option<RenderTimeline>,
    /// Secondary navigate events (issue #301) for fallback collections
    /// carrying more than one: the first navigate event stays inline (the
    /// row/item click handler), the rest disclose into the per-row actions
    /// menu. Empty (and skipped from the context) when the component has
    /// at most one navigate event, is mapped, or the ux plane is off —
    /// single-event markup stays byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) row_menu_events: Vec<RenderRowMenuEvent>,
}

/// Workflow config for a component's bound entity, pre-rendered into the
/// state badge markup for the component's markup context.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderWorkflow {
    pub status_field: String,
    pub states: Vec<String>,
    pub terminal_states: Vec<String>,
    pub initial_state: String,
    /// Raw from → [targets] edges from the config; `None` targets resolve
    /// per the empty-map rule (any non-terminal state). Skipped in output.
    #[serde(skip_serializing)]
    pub transition_map: HashMap<String, Vec<String>>,
    /// Whether the config generates workflow action endpoints: gates the
    /// transition buttons and handler (the POST target must exist).
    #[serde(skip_serializing)]
    pub generate_action_endpoints: bool,
    /// Ready-to-render transition buttons for details/form components:
    /// one per valid (from → to) edge, disabled client-side unless the
    /// current state matches the from-state (empty map: unless terminal).
    pub transitions: Vec<RenderTransition>,
    /// Ready-to-render badge: `<span class="workflow-state"
    /// data-testid="{component}-state" data-workflow-state={...}>...</span>`
    /// plus the terminal marker attribute when terminal states are known.
    pub badge_html: String,
    /// Whether the badge reads a per-row `item` binding (collections): the
    /// mapped-component sibling renders inside `{#each data.items as item}`.
    pub each: bool,
}

/// One workflow transition button: humanized target label, e2e testid
/// (`{component}-transition-{target}`), the (from → to) edge, the
/// client-side disabled expression over the component's state value path,
/// and the ready-to-render `<button>` markup.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderTransition {
    pub label: String,
    pub testid: String,
    pub from: String,
    pub to: String,
    pub disabled_expr: String,
    pub html: String,
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
                    transition_map: wf.transitions.clone(),
                    generate_action_endpoints: wf.generate_action_endpoints,
                    transitions: Vec::new(),
                    badge_html: String::new(),
                    each: false,
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
    wf.each = is_collection(c);
    if is_form_component(c) || c.component_type == "details" {
        wf.transitions = render_transitions(&wf, &c.name, &value_path);
    }
    Some(wf)
}

/// One transition button per valid (from → to) edge. A populated
/// `transitions` map enumerates its edges sorted for deterministic output;
/// an empty map targets every non-terminal state with no from-restriction
/// (buttons disable once the current state is terminal).
fn render_transitions(
    wf: &RenderWorkflow,
    component: &str,
    value_path: &str,
) -> Vec<RenderTransition> {
    let edges: Vec<(String, String)> = if wf.transition_map.is_empty() {
        wf.states
            .iter()
            .filter(|s| !wf.terminal_states.contains(s))
            .map(|s| (String::new(), s.clone()))
            .collect()
    } else {
        let mut edges: Vec<(String, String)> = wf
            .transition_map
            .iter()
            .flat_map(|(from, tos)| tos.iter().map(move |to| (from.clone(), to.clone())))
            .collect();
        edges.sort();
        edges
    };
    edges
        .into_iter()
        .map(|(from, to)| {
            let kebab = codegraph_naming::to_kebab_case(&to);
            let testid = format!("{component}-transition-{kebab}");
            let label = humanize_state_label(&to);
            let disabled_expr = if from.is_empty() {
                let terminals = wf
                    .terminal_states
                    .iter()
                    .map(|s| js_quote(s))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{terminals}].includes({value_path} as string)")
            } else {
                format!("{value_path} !== {}", js_quote(&from))
            };
            let handler = sanitize_ident(&format!("transition_{component}"));
            let disabled_binding = format!("({disabled_expr})");
            let html = format!(
                "<button type=\"button\" data-testid=\"{testid}\" data-transition-from=\"{from}\" data-transition-to=\"{to}\" disabled={{{disabled_binding}}} onclick={{() => {handler}('{to}')}}>{label}</button>"
            );
            RenderTransition {
                label,
                testid,
                from,
                to,
                disabled_expr,
                html,
            }
        })
        .collect()
}

/// Humanized state name for a transition button label: `submitted` →
/// `Submitted`, `awaiting_review` → `Awaiting review`.
fn humanize_state_label(state: &str) -> String {
    let spaced = state.replace(['_', '-'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
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
#[derive(Debug, Clone, Serialize)]
pub struct RenderSubmit {
    handler_name: String,
    /// URL expression (JS literal) passed to fetch. Edit views PUT here;
    /// create views POST here directly.
    url_expr: String,
    method: String,
    /// Create-mode collection URL expression. Views carrying an id param
    /// branch at runtime: `isEdit` PUTs [`Self::url_expr`] (item), otherwise
    /// POSTs this (collection). `None` for views without an id param, which
    /// always POST the collection.
    create_url_expr: Option<String>,
    /// Name of the id param gating the edit branch (`!!viewParams.<name>`).
    edit_param: Option<String>,
    /// Navigation target URL expression from the view's save event.
    navigate_url: Option<String>,
    /// Emit the conservative client-side check that surfaces the first
    /// failing validation message before the network request.
    client_validate: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderMapping {
    import_name: String,
    import_path: String,
    testid: Option<String>,
    row_testid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderEvent {
    pub handler_name: String,
    pub event_type: String,
    /// "navigate" | "refresh" | "action" | "stay"
    pub action_kind: String,
    pub target: String,
    /// JS string expression evaluating to the navigation target URL.
    pub url_expr: String,
    /// Capability requirements on the event (`requires: [Cap]`); empty when
    /// unguarded. Gates the event's control behind `can(...)`.
    pub requires: Vec<String>,
    /// Semantic slot role: `action-control` for save/submit/cancel/back/click
    /// button-style events.
    pub role: Option<SemanticRole>,
}

/// Modal wrapper for a `modal: true` view: a mapped `modal-view` component
/// (`<Dialog bind:open={dialog_open}>`) or the built-in div fallback.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
    /// Ready-to-render container label heading rendered right after the
    /// opening line (`<h2 class="container-label" …>`); `None` when the
    /// wrapper groups nested containers that carry their own headings.
    pub label_heading: Option<String>,
}

/// One navigation entry in the landmark shell layout: `href_attr` is the
/// ready-to-render `href={...}` attribute using the same URL expression the
/// page goto handlers emit.
#[derive(Debug, Clone, Serialize)]
pub struct RenderNavItem {
    pub label: String,
    pub href_attr: String,
}

/// Landmark shell nav context: a mapped `shell` component wrapping nav
/// items built from the landmark views' navigate events.
#[derive(Debug, Clone, Serialize)]
pub struct RenderShellNav {
    pub import: RenderImport,
    pub testid: Option<String>,
    pub items: Vec<RenderNavItem>,
}

/// Render context for the framework layout shell
/// (`ifml/{fw}/layout.tera` → `src/routes/+layout.svelte`).
#[derive(Debug, Clone, Serialize)]
pub struct LayoutSvelteContext {
    pub shell: RenderShellNav,
}

/// A mapped action-control button replacing the hardcoded fallback `<button>`
/// (form submit) or rendering a cancel/back/click action.
#[derive(Debug, Clone, Serialize)]
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
    /// Capability requirements of the source event; composed with the view
    /// gate into `gate_open`/`gate_close` by the page post-pass.
    pub event_requires: Vec<String>,
    /// Ready-to-render gate markers around the button: the view gate
    /// AND-composed with the event's `requires` check; empty when no gate
    /// applies.
    pub gate_open: String,
    pub gate_close: String,
}

/// Typed-table render context derived from a `ComponentSpec::Table`
#[derive(Debug, Clone, Serialize)]
pub struct RenderTable {
    pagination: bool,
    /// `pagination` when the list spec enables pagination.
    role: Option<SemanticRole>,
    columns: Vec<RenderColumn>,
    /// Collection layout (issue #301): the default `Table`, or a timeline
    /// resolved ONLY from an explicit matching `[[collection]]`
    /// `display = "timeline"` rule through the shared plan machinery
    /// ([`crate::ux::plan`]). The `Table` variant is skipped from the
    /// serialized context, so flag-off / rule-less rendering never sees
    /// the key and stays byte-identical (a single template gate).
    #[serde(skip_serializing_if = "TableLayout::is_table")]
    layout: TableLayout,
}

/// Collection layout for a fallback table/list (issue #301).
///
/// Timeline is strictly opt-in: only an explicit matching `[[collection]]`
/// rule produces it. The plan output is reused verbatim — order_by was
/// already validated against the entity's TimePoint fields by
/// [`crate::ux::plan::build_ux_plan`], so this type never re-validates.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "layout", rename_all = "kebab-case")]
pub enum TableLayout {
    /// The default table rendering (skipped from the serialized context).
    Table,
    /// Chronological rendering replacing the table block: newest-first
    /// `<ol>` items with a `<time>` head, title, preview fields and the
    /// workflow badge per item.
    Timeline {
        /// Data path each item sorts by (the rule's `order_by`).
        order_binding: String,
        /// Title field per item (rule value or the plan's column-order
        /// first field); `None` renders no title line.
        title_binding: Option<String>,
        /// Extra fields shown per item, resolved through the SAME
        /// per-column ux tier as #300 table cells.
        preview: Vec<RenderColumn>,
    },
}

impl TableLayout {
    /// True for the default table layout — the skip-serialization gate.
    pub fn is_table(&self) -> bool {
        matches!(self, TableLayout::Table)
    }
}

/// Template-facing timeline payload for a fallback collection (issue
/// #301). Typed tables project their [`RenderTable::layout`] timeline
/// here; spec-less lists resolve theirs directly — one uniform component
/// field renders both branches.
#[derive(Debug, Clone, Serialize)]
pub struct RenderTimeline {
    /// Data path each item sorts by (newest first).
    pub order_binding: String,
    /// Title field per item; `None` renders no title line.
    pub title_binding: Option<String>,
    /// Preview fields rendered through the same per-column ux resolution
    /// as #300 cells (chips/copy/money/quantity/date helpers).
    pub preview: Vec<RenderColumn>,
}

/// One secondary event disclosed into the per-row actions menu (issue
/// #301, IFML Pass-3 flavor). The menu item calls the SAME named handler
/// function the inline placement would call — parity by construction.
#[derive(Debug, Clone, Serialize)]
pub struct RenderRowMenuEvent {
    /// The shared handler fn name (`comp_grid_archive`).
    pub handler_name: String,
    /// Humanized event-type label (`Archive`).
    pub label: String,
}

/// One typed table column; `binding` is the ready-to-emit data path
/// (`property` for field/lookup columns, the rendered expression for
/// expression columns)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderColumn {
    label: String,
    kind: String,
    binding: String,
    lookup: String,
    expr: String,
    /// ux-rules resolved presentation (issue #300). `None` when the plane
    /// is off — the key is skipped so flag-off contexts and rendered
    /// markup stay byte-identical (the template's ux branches are then
    /// never taken).
    #[serde(skip_serializing_if = "Option::is_none")]
    ux: Option<RenderColumnUx>,
}

/// ux-rules resolved presentation for one fallback-table column (issue
/// #300), carried as the shared ux vocabulary (`codegraph_config::ux`)
/// serialized kebab-case for the template's cell branches.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderColumnUx {
    /// Resolved dimension, kebab-case (`text`, `quantity`, `money`,
    /// `time-point`, `status-category`, `identifier`, `reference`, `flag`).
    pub dimension: String,
    /// Resolved display, kebab-case (`raw`, `chip`, `copy-chip`, `link`).
    pub display: String,
    /// Cell/header alignment, kebab-case (`left`, `right`).
    pub align: String,
    /// Status keyword → badge-variant map driving `toneFor`; skipped when
    /// empty so templates treat absence as "no tone entries".
    #[serde(skip_serializing_if = "ToneMap::is_empty")]
    pub tone: ToneMap,
}

impl RenderColumnUx {
    /// The fixed presentation for a `lookup` column (tier 0): the DSL
    /// names the rendering explicitly (`via status_labels`), so the column
    /// is a StatusCategory chip with the pack keyword tone map — closing
    /// the gap where lookup columns rendered raw. Deliberately ABOVE the
    /// rule tier: no `[[column]]` rule downgrades an explicit lookup.
    fn for_lookup() -> Self {
        Self {
            dimension: Dimension::StatusCategory.as_str().to_string(),
            display: "chip".to_string(),
            align: "left".to_string(),
            tone: ToneMap::default(),
        }
    }

    /// Project a resolved [`ColumnPlan`] onto the render context.
    fn from_plan(plan: &crate::ux::plan::ColumnPlan) -> Self {
        Self {
            dimension: plan.dimension.as_str().to_string(),
            display: match plan.display {
                Display::Raw => "raw",
                Display::Chip => "chip",
                Display::CopyChip => "copy-chip",
                Display::Link => "link",
            }
            .to_string(),
            align: match plan.align {
                Align::Left => "left",
                Align::Right => "right",
            }
            .to_string(),
            tone: plan.tone.clone(),
        }
    }
}

/// Typed-form render context derived from a `ComponentSpec::Form`
#[derive(Debug, Clone, Serialize)]
pub struct RenderForm {
    fields: Vec<RenderInputField>,
}

#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct RenderChart {
    kind: String,
    label_field: Option<String>,
    value_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageLoadContext {
    pub api_version: String,
    name: String,
    components: Vec<PageLoadComponentContext>,
    has_fetch: bool,
    /// Whether any component carries a workflow: the load result types its
    /// values `any` instead of `unknown` so workflow value paths in the
    /// page markup typecheck. Absent for byte-identical non-workflow loads.
    has_workflow: bool,
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
#[derive(Debug, Clone, Serialize)]
pub struct RenderViewParam {
    name: String,
    /// JS literal emitted as the final `??` fallback.
    default: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
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

/// The lowered TypeScript guard for a view's `condition:`, or `None`.
///
/// Gated on the `expr_ir` feature flag: OFF (default) always yields `None`
/// so generated output stays byte-identical. ON, the guard is lowered from
/// the persisted expression AST (`expr_json`); a condition outside the
/// closed subset is a hard error naming the view (rexlang philosophy —
/// no silent passthrough).
pub(crate) fn page_guard_expr(
    vc: &IfmlViewContainer,
    project: &ProjectConfig,
) -> crate::error::Result<Option<String>> {
    if !project.expr_ir {
        return Ok(None);
    }
    let Some(expr_json) = vc.conditional_expr_json.as_deref() else {
        return Ok(None);
    };
    super::expr_ts::lower_ifml_json(expr_json, &vc.name)
        .map(Some)
        .map_err(|e| crate::error::Error::Config(e.to_string()))
}

#[allow(clippy::too_many_arguments)]

async fn build_page_context(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
    modal_targets: &HashSet<String>,
    ux: Option<&UxGeneration<'_>>,
    project: &ProjectConfig,
) -> crate::error::Result<PageSvelteContext> {
    let id_param = id_param_from(&vc.params);
    let mut api_cache: HashMap<String, Option<ResolvedApi>> = HashMap::new();
    let container = container_context(vc, mappings);

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
            ux,
        )
        .await;
        components.push(ctx);
    }

    // Body render groups: the view's own components, then one group per
    // nested container in declaration order. Headings (and the in-container
    // h1 suppression) are gated on the presentation wrapper being active —
    // without a mapping pack the page stays plain (byte-identical with
    // pre-nesting output).
    let mut groups = vec![RenderGroup {
        heading: None,
        components,
    }];
    if container.is_some() {
        for group_decl in &vc.containers {
            let heading = group_decl.is_xor.then(|| {
                let lower = group_decl.name.to_lowercase();
                let label = group_decl
                    .label
                    .clone()
                    .unwrap_or_else(|| group_decl.name.clone());
                format!("<h2 class=\"container-label\" data-testid=\"{lower}-label\">{label}</h2>")
            });
            let mut group_components = Vec::new();
            for c in &group_decl.components {
                let mut ctx = page_component_context(
                    db,
                    config,
                    api_version,
                    vc,
                    c,
                    id_param.as_deref(),
                    mappings,
                    &mut api_cache,
                    modal_targets,
                    ux,
                )
                .await;
                ctx.in_container = true;
                group_components.push(ctx);
            }
            groups.push(RenderGroup {
                heading,
                components: group_components,
            });
        }
    }
    let components: Vec<PageComponentContext> = groups
        .iter()
        .flat_map(|group| group.components.iter().cloned())
        .collect();

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
    let mut control_gate = ControlGateContext::for_view(vc, &components);

    // Load fetch wiring (same rules the load context applies) plus
    // event-level capability gates. Fetch flags gate the mapped-branch
    // badge and transition buttons on the component's value path actually
    // being fetch-backed; the transition handler URL resolves from the
    // component's API path and the view's id param. Event gates compose
    // the view gate with each component's save/submit requires (submit
    // control) and each secondary button's event requires; buttons without
    // requires reuse the view gate verbatim (byte-identical).
    let id_param = id_param_from(&vc.params);
    let mut fetch_state = FetchState::default();
    let mut needs_can_import = false;
    for group in &mut groups {
        for comp in &mut group.components {
            let is_list = comp.table.is_some() || comp.component_type == "list";
            let is_details = comp.component_type == "details";
            let is_form = comp.form.is_some() || comp.component_type == "form";
            let has_api = comp.api.is_some() && !comp.entity.is_empty();
            let (fetch_list, fetch_item, fetch_form) =
                fetch_state.next(is_list, is_details, is_form, has_api, id_param.as_deref());
            comp.fetch_list = fetch_list;
            comp.fetch_item = fetch_item;
            comp.fetch_form = fetch_form;

            if let Some(workflow) = &comp.workflow {
                let fetch_backed = match comp.component_type.as_str() {
                    "form" => fetch_form,
                    "details" => fetch_item,
                    _ => fetch_list,
                };
                if fetch_backed
                    && workflow.generate_action_endpoints
                    && !workflow.transitions.is_empty()
                {
                    if let Some(api) = &comp.api {
                        if let Some(param) = &id_param {
                            comp.transition_url_expr = Some(format!(
                                "`{}/${{viewParams.{param}}}/actions/transition`",
                                api.base_path
                            ));
                        }
                    }
                }
            }

            let primary = primary_event_requires(&comp.events);
            comp.submit_gate = control_gate.compose_with_event(&primary);
            if !primary.is_empty() {
                needs_can_import = true;
            }
            for btn in &mut comp.buttons {
                if btn.event_requires.is_empty() {
                    btn.gate_open = control_gate.open.clone();
                    btn.gate_close = control_gate.close.clone();
                } else {
                    needs_can_import = true;
                    let gate = control_gate.compose_with_event(&btn.event_requires);
                    btn.gate_open = gate.open;
                    btn.gate_close = gate.close;
                }
            }
        }
    }
    if needs_can_import && !control_gate.imports.iter().any(|name| name == "can") {
        control_gate.imports.push("can".to_string());
    }
    let components: Vec<PageComponentContext> = groups
        .iter()
        .flat_map(|group| group.components.iter().cloned())
        .collect();
    let needs_invalidate = components
        .iter()
        .any(|comp| comp.transition_url_expr.is_some());

    Ok(PageSvelteContext {
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
        needs_invalidate,
        has_submit,
        groups,
        view_role: semantic_view_role(vc),
        container_role: semantic_container_role(vc),
        roles: vc.roles.clone(),
        requires: vc.requires.clone(),
        control_gate,
        modal,
        container,
        ux: ux.map(|uxg| page_ux_context(uxg.rules)),
        guard_expr: page_guard_expr(vc, project)?,
    })
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
    ux: Option<&UxGeneration<'_>>,
) -> PageComponentContext {
    let (mut table, form, chart) = match c.spec {
        Some(ComponentSpec::Table(ref spec)) => (Some(render_table(spec)), None, None),
        Some(ComponentSpec::Form(ref spec)) => (None, Some(render_form(spec)), None),
        Some(ComponentSpec::Chart(ref spec)) => (None, None, Some(render_chart(spec))),
        None => (None, None, None),
    };

    // ux-rules fallback-table resolution (issue #300): only when the plane
    // is active — otherwise the ux context keys stay absent and the
    // rendered page is byte-identical. Field columns resolve through the
    // bound property's graph metadata (mirroring
    // `ifml_control_inference.rs`), then the shared Pass-1 + rule tier in
    // `crate::ux::plan`; lookup columns pin StatusCategory + chip above
    // both tiers; expression columns stay Text (the IFML AST carries no
    // return type).
    //
    // Issue #301 adds the collection layout: the plan for the bound entity
    // (resolved ONCE per generation, see `resolve_generation_ux`)
    // contributes a Timeline layout ONLY from an explicit matching
    // `[[collection]] display = "timeline"` rule. Plan output is reused
    // verbatim — never re-validated here.
    let (ux_list_columns, timeline) = match ux {
        Some(uxg) => {
            let rules = uxg.rules;
            let props = ux_props_for_entity(db, c.entity.as_deref().unwrap_or_default()).await;
            let status_field = c
                .entity
                .as_deref()
                .and_then(|entity| workflow_for_entity(config, entity))
                .map(|wf| wf.status_field);
            let status_field = status_field.as_deref();
            if let Some(table) = table.as_mut() {
                apply_table_ux(table, rules, &props, &c.fields_with_types, status_field);
            }
            let ux_list_columns = if c.spec.is_none() && c.component_type == "list" {
                c.fields
                    .iter()
                    .map(|field| RenderColumn {
                        label: field.clone(),
                        kind: "field".to_string(),
                        binding: field.clone(),
                        lookup: String::new(),
                        expr: String::new(),
                        ux: Some(resolve_column_ux(
                            rules,
                            "field",
                            field,
                            &props,
                            &c.fields_with_types,
                            status_field,
                        )),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let timeline = timeline_from_plan(
                uxg,
                c.entity.as_deref().unwrap_or_default(),
                rules,
                &props,
                &c.fields_with_types,
                status_field,
            );
            if let Some(payload) = &timeline {
                if let Some(table) = table.as_mut() {
                    table.layout = TableLayout::Timeline {
                        order_binding: payload.order_binding.clone(),
                        title_binding: payload.title_binding.clone(),
                        preview: payload.preview.clone(),
                    };
                }
            }
            (ux_list_columns, timeline)
        }
        None => (Vec::new(), None),
    };

    let kind = kind_of(c);
    let slot_role = component_role(c);
    let mapping = mappings
        .and_then(|m| m.resolve_slot(&vc.name, &c.name, &c.component_type, &kind, slot_role))
        .map(mapping_context);
    // Mapped components replace the fallback wholesale (issue #301): no
    // timeline layout, no row-actions menu — the mapped component owns its
    // presentation.
    let mut timeline = timeline;
    if mapping.is_some() {
        if let Some(table) = table.as_mut() {
            table.layout = TableLayout::Table;
        }
        timeline = None;
    }
    let events: Vec<RenderEvent> = c
        .events
        .iter()
        .map(|e| render_event(e, modal_targets))
        .collect();
    // IFML Pass-3 event tiering (issue #301): the first navigate event
    // stays inline (the row/item click handler, exactly as before);
    // additional secondary events disclose into the per-row actions menu.
    // Handler functions are emitted once per event regardless, so every
    // menu item calls the identical body its inline placement would.
    let row_menu_events = if ux.is_some() && mapping.is_none() && is_collection(c) {
        events
            .iter()
            .filter(|e| e.action_kind == "navigate")
            .skip(1)
            .map(|e| RenderRowMenuEvent {
                handler_name: e.handler_name.clone(),
                label: humanize_event_label(&e.event_type),
            })
            .collect()
    } else {
        Vec::new()
    };
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
        fetch_list: false,
        fetch_item: false,
        fetch_form: false,
        transition_url_expr: None,
        submit_gate: ControlGateContext::default(),
        form_state,
        form_payload,
        in_container: false,
        table,
        form,
        chart,
        ux_list_columns,
        timeline,
        row_menu_events,
    }
}

/// Running view-level fetch state: which component kinds have already
/// claimed the load's single fetch per kind.
#[derive(Default)]
struct FetchState {
    has_list: bool,
    has_details: bool,
    has_form: bool,
}

impl FetchState {
    /// The load fetch flags for one component: the first list/details/form
    /// contributes a fetch; details and form fetches additionally require
    /// an id param.
    fn next(
        &mut self,
        is_list: bool,
        is_details: bool,
        is_form: bool,
        has_api: bool,
        id_param: Option<&str>,
    ) -> (bool, bool, bool) {
        let mut fetch_list = false;
        let mut fetch_item = false;
        let mut fetch_form = false;
        if has_api {
            if is_list && !self.has_list {
                fetch_list = true;
            } else if is_details && !self.has_details && id_param.is_some() {
                fetch_item = true;
            } else if is_form && !self.has_form && id_param.is_some() {
                fetch_form = true;
            }
        }
        self.has_list |= fetch_list;
        self.has_details |= fetch_item;
        self.has_form |= fetch_form;
        (fetch_list, fetch_item, fetch_form)
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
        } else if control_core::is_temporal_rust_type(&rust_type)
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
    control_core::is_numeric_rust_type(rust_type)
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
    control_core::input_field_role(input_type)
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
    let (url_expr, method, create_url_expr, edit_param) = match id_param {
        Some(param) if api.has_update => (
            format!("`{}/${{viewParams.{param}}}`", api.base_path),
            "PUT".to_string(),
            Some(format!("\"{}\"", api.base_path)),
            Some(param.to_string()),
        ),
        _ => (
            format!("\"{}\"", api.base_path),
            "POST".to_string(),
            None,
            None,
        ),
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
        create_url_expr,
        edit_param,
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
        requires: evt.requires.clone(),
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
/// renders the plain page. Also active (without a label heading) when the
/// view nests xor containers that share the one wrapper.
fn container_context(
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
) -> Option<RenderContainer> {
    let has_xor_children = vc.containers.iter().any(|c| c.is_xor);
    if !container_wrapper_active(vc.is_xor || has_xor_children, mappings) {
        return None;
    }
    let view_lower = vc.name.to_lowercase();
    let section_testid = format!("{view_lower}-container");
    let label_heading = vc.is_xor.then(|| {
        let label = vc.label.clone().unwrap_or_else(|| vc.name.clone());
        format!("<h2 class=\"container-label\" data-testid=\"{section_testid}-label\">{label}</h2>")
    });
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
                label_heading,
            })
        }
        None => Some(RenderContainer {
            open_line: format!("<section data-testid=\"{section_testid}\">"),
            close_line: "</section>".to_string(),
            testid: section_testid,
            import: None,
            label_heading,
        }),
    }
}

/// Root identifier of a binding value expression (`row` in `row.id`);
/// `None` for literals and empty expressions.
fn binding_root_ident(expr: &str) -> Option<&str> {
    let root: String = expr
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if root.is_empty() || root.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(&expr[..root.len()])
    }
}

/// Whether a binding value expression is rooted in an event parameter —
/// such identifiers are out of scope in the landmark layout, so the nav
/// link must drop the pair instead of rendering a dead reference.
fn references_event_param(expr: &str, params: &[String]) -> bool {
    binding_root_ident(expr).is_some_and(|root| params.iter().any(|p| p == root))
}

/// Shell nav context for the landmark layout: resolves the `shell` mapping
/// against the landmark views and builds nav items from their navigate
/// events (same URL resolution as the page goto handlers, minus bindings
/// rooted in event params). `None` when no landmark views exist or no
/// `shell` mapping resolves — no layout is emitted then.
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
                let scoped: HashMap<String, String> = binding
                    .iter()
                    .filter(|(_, expr)| !references_event_param(expr, &evt.params))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let url_expr = nav_url_expr(target, &scoped);
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
        event_requires: primary_event_requires(events),
        gate_open: String::new(),
        gate_close: String::new(),
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
                event_requires: e.requires.clone(),
                gate_open: String::new(),
                gate_close: String::new(),
            }
        })
        .collect();
    (Some(submit_button), buttons)
}

/// Capability requirements of the save/submit event driving the primary
/// form button; empty when unguarded.
fn primary_event_requires(events: &[RenderEvent]) -> Vec<String> {
    events
        .iter()
        .find(|e| e.event_type == "save" || e.event_type == "submit")
        .map(|e| e.requires.clone())
        .unwrap_or_default()
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
        layout: TableLayout::Table,
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
            ux: None,
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
            ux: None,
        },
        ColumnDef::Expression { label, expr } => RenderColumn {
            label: label.clone(),
            kind: "expr".to_string(),
            binding: render_expression(expr),
            lookup: String::new(),
            expr: render_expression(expr),
            ux: None,
        },
    }
}

/// Resolve the graph schema title for an IFML entity binding: exact match
/// first, then the `{entity}Type` shape (mirrors the IFML querier's
/// `resolve_schema_title_by_name`). `None` when the entity is not
/// schema-backed — ux columns then resolve from the component's
/// `(field, rust_type)` pairs alone.
async fn ux_schema_title(db: &dyn GraphQuerier, entity: &str) -> Option<String> {
    if entity.is_empty() {
        return None;
    }
    if matches!(db.get_schema(entity).await, Ok(Some(_))) {
        return Some(entity.to_string());
    }
    let suffixed = format!("{entity}Type");
    matches!(db.get_schema(&suffixed).await, Ok(Some(_))).then_some(suffixed)
}

/// Graph properties for an IFML component's bound entity, keyed by both
/// the property name and the `r#`-stripped rust field name. Empty when the
/// entity is not schema-backed (columns fall back to the component's
/// `(field, rust_type)` pairs — an honest, signal-poor inference).
async fn ux_props_for_entity(db: &dyn GraphQuerier, entity: &str) -> HashMap<String, PropertyNode> {
    let Some(title) = ux_schema_title(db, entity).await else {
        return HashMap::new();
    };
    let Ok(props) = db.get_properties(&title).await else {
        return HashMap::new();
    };
    let mut by_name: HashMap<String, PropertyNode> = HashMap::with_capacity(props.len() * 2);
    for prop in props {
        let stripped = prop
            .rust_field_name
            .strip_prefix("r#")
            .unwrap_or(&prop.rust_field_name)
            .to_string();
        by_name.insert(prop.name.clone(), prop.clone());
        by_name.insert(stripped, prop);
    }
    by_name
}

/// The codelist half of the `ifml_control_inference.rs` mirror: graph
/// classification kinds whose value sets live in the graph. (InlineEnum
/// properties are detected through their projected `select` input type
/// instead — the same signal Pass 1 reads.)
fn column_is_codelist(prop: &PropertyNode) -> bool {
    matches!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference) | Some(RefClassificationKind::CodelistCheck)
    )
}

/// The entity-ref half of the `ifml_control_inference.rs` mirror.
fn column_is_entity_ref(prop: &PropertyNode) -> bool {
    prop.effective_kind() == Some(RefClassificationKind::EntityReference)
}

/// A signal-poor [`UiField`] for columns whose property is absent from the
/// graph: the component's `(field, rust_type)` pair is projected through
/// the entity pipeline's rust→ts mapping. No pg type exists here, so the
/// pg-backed branches of Pass 1 (money/quantity/time-point) honestly
/// cannot fire.
fn ux_synth_field(
    binding: &str,
    fields_with_types: &[(String, String)],
) -> crate::ui::page::UiField {
    let rust_type = fields_with_types
        .iter()
        .find(|(name, _)| name == binding)
        .map(|(_, t)| t.as_str())
        .unwrap_or("String");
    crate::ui::page::UiField {
        name: binding.to_string(),
        label: String::new(),
        ts_type: crate::ui::form::rust_type_to_ts(rust_type, false),
        input_type: String::new(),
        is_required: false,
        is_array: rust_type.starts_with("Vec<"),
        is_entity_ref: false,
        is_immutable: false,
        is_codelist: false,
        is_range: false,
        codelist_values: Vec::new(),
        description: String::new(),
        pg_type: String::new(),
        open_end: false,
        ref_api_path: None,
        structured_sub_fields: Vec::new(),
        nested_type_name: None,
    }
}

/// ux-rules resolution for ONE fallback-table column (issue #300).
///
/// Precedence, highest first (the future DSL `dimension:` key slots above
/// rules; then rules > pack defaults > inference):
///
/// 1. `kind == "lookup"` — the DSL names the presentation explicitly
///    (`via status_labels`): StatusCategory + chip with the pack tone map
///    (see [`RenderColumnUx::for_lookup`]). No rule can downgrade an
///    explicit lookup.
/// 2. Everything else resolves through the shared Pass-1 + rule tier
///    ([`crate::ux::plan::resolve_column`] folded by `into_column_plan`):
///    Pass 1 infers from the bound property's GRAPH metadata — the entity
///    pipeline's exact projection ([`crate::ui::form::ui_field_from_property`])
///    over classification kind, pg type, and input type — then the
///    first-match `[[column]]` rule (project rules ahead of pack rules)
///    overrides the inference and pins display/align/tone/sortable
///    payloads.
/// 3. `kind == "expr"` infers from what the model actually has — nothing:
///    the IFML AST carries no expression return type, so the bound
///    property (if any) is NOT consulted and Pass 1 runs signal-poor over
///    the rendered name (no inference theater). Rules may still pin
///    display/align payloads on the column's name.
///
/// When the bound property is absent from the graph (schema-less IFML
/// runs), a signal-poor [`UiField`] is synthesized from the component's
/// `(field, rust_type)` pairs ([`ux_synth_field`]).
fn resolve_column_ux(
    rules: &UxRules,
    kind: &str,
    binding: &str,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) -> RenderColumnUx {
    if kind == "lookup" {
        return RenderColumnUx::for_lookup();
    }
    let prop = if kind == "expr" {
        None
    } else {
        props.get(binding)
    };
    let field = match prop {
        Some(prop) => crate::ui::form::ui_field_from_property(
            prop,
            column_is_entity_ref(prop),
            column_is_codelist(prop),
            &[],
            &[],
            &prop.pg_column_type,
            prop.pg_column_type.contains("RANGE"),
            false,
        ),
        None => ux_synth_field(binding, fields_with_types),
    };
    let resolution = crate::ux::plan::resolve_column(rules, prop, &field, workflow_status_field);
    RenderColumnUx::from_plan(&resolution.into_column_plan(rules.format.clone()))
}

/// Apply the ux resolution to every column of a typed fallback table.
fn apply_table_ux(
    table: &mut RenderTable,
    rules: &UxRules,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) {
    for col in &mut table.columns {
        col.ux = Some(resolve_column_ux(
            rules,
            &col.kind,
            &col.binding,
            props,
            fields_with_types,
            workflow_status_field,
        ));
    }
}

/// Generation-scoped ux-rules context (issue #301): the resolved rules
/// plus one plan per distinct bound entity, built ONCE per generation by
/// [`resolve_generation_ux`]. Page contexts reuse the plan output verbatim
/// (timeline layout) instead of re-resolving — no second validation path.
pub(crate) struct UxGeneration<'a> {
    /// The project's resolved rules (`project.ux`).
    pub rules: &'a UxRules,
    /// Bound entity name → built plan. Flag-off generations carry an
    /// empty map.
    pub plans: &'a HashMap<String, crate::ux::plan::UxPlan>,
}

/// Resolve the ux-rules plane for a whole IFML generation (issue #301).
///
/// One plan per distinct bound entity over its collection components'
/// display fields — the same projection #300 uses for columns: the graph
/// property when the entity is schema-backed, a synthesized
/// `(field, rust_type)` pair otherwise. Advisory diagnostics are collected
/// over each plan and returned as DEDUPLICATED report lines for the
/// caller's stderr warning channel (mirroring the entity pipeline's
/// ui-page generator). Flag off ⇒ an empty map and no lines.
///
/// Errors when an explicit timeline rule's `order_by` does not resolve to
/// one of the entity's TimePoint fields — the error names the candidates
/// (a hard generation error, surfacing through the global phase's `?`).
async fn resolve_generation_ux(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    model: &super::context::IfmlModel,
    rules: Option<&UxRules>,
) -> Result<(HashMap<String, crate::ux::plan::UxPlan>, Vec<String>)> {
    let Some(rules) = rules else {
        return Ok((HashMap::new(), Vec::new()));
    };
    let mut plans: HashMap<String, crate::ux::plan::UxPlan> = HashMap::new();
    let mut reported: HashSet<String> = HashSet::new();
    let mut lines: Vec<String> = Vec::new();
    for vc in &model.view_containers {
        for c in vc
            .components
            .iter()
            .chain(vc.containers.iter().flat_map(|g| g.components.iter()))
        {
            if !is_collection(c) {
                continue;
            }
            let Some(entity) = c.entity.as_deref().filter(|e| !e.is_empty()) else {
                continue;
            };
            // Resolve once per entity, not per component: the first
            // collection bound to the entity fixes its plan for the run.
            if plans.contains_key(entity) {
                continue;
            }
            let props = ux_props_for_entity(db, entity).await;
            let status_field = workflow_for_entity(config, entity).map(|wf| wf.status_field);
            let entity_input = ux_entity_input(c, &props, status_field.as_deref());
            let input = entity_input.plan_input(entity);
            if let Some(plan) = crate::ux::plan::build_ux_plan(Some(rules), &input)? {
                let diag = crate::ux::diagnostics::collect_diagnostics(rules, &input, &plan);
                for line in crate::ux::diagnostics::report(&diag) {
                    if reported.insert(line.clone()) {
                        lines.push(line);
                    }
                }
                plans.insert(entity.to_string(), plan);
            }
        }
    }
    Ok((plans, lines))
}

/// The plan-input display fields of a collection component: the declared
/// `fields` when present, else the typed-table column bindings
/// (expression columns carry no property and are skipped).
fn ux_plan_fields(c: &IfmlComponent) -> Vec<String> {
    if !c.fields.is_empty() {
        return c.fields.clone();
    }
    match &c.spec {
        Some(ComponentSpec::Table(spec)) => spec
            .columns
            .iter()
            .filter_map(|col| match col {
                ColumnDef::Field { field, .. } | ColumnDef::Lookup { field, .. } => {
                    Some(field.property.clone())
                }
                ColumnDef::Expression { .. } => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Owned plan-input holder for one collection component ([`UxPlanInput`]
/// borrows, so the owner must outlive the plan build — see
/// [`UxEntityInput::plan_input`]).
struct UxEntityInput<'a> {
    fields: Vec<crate::ui::page::UiField>,
    prop_by_name: BTreeMap<String, &'a PropertyNode>,
    workflow_status_field: Option<&'a str>,
}

impl UxEntityInput<'_> {
    /// Borrow into the plan input [`crate::ux::plan::build_ux_plan`]
    /// consumes.
    fn plan_input<'b>(&'b self, entity: &'b str) -> crate::ux::plan::UxPlanInput<'b> {
        crate::ux::plan::UxPlanInput {
            entity_title: entity,
            fields: &self.fields,
            prop_by_name: self
                .prop_by_name
                .iter()
                .map(|(name, prop)| (name.as_str(), *prop))
                .collect(),
            workflow_status_field: self.workflow_status_field,
            workflow_terminal_states: &[],
            has_soft_delete: false,
            user_pinned_list_order: false,
        }
    }
}

/// The plan-input projection of one collection component. Schema-backed
/// entities plan over ALL their properties — the timeline `order_by` must
/// resolve to a TimePoint property of the bound entity, whether or not a
/// component displays it — with names sorted for deterministic output.
/// Schema-less projections fall back to the component's display fields
/// ([`ux_plan_fields`], signal-poor synthesized [`UiField`] pairs).
fn ux_entity_input<'a>(
    c: &IfmlComponent,
    props: &'a HashMap<String, PropertyNode>,
    workflow_status_field: Option<&'a str>,
) -> UxEntityInput<'a> {
    let names: Vec<String> = if props.is_empty() {
        ux_plan_fields(c)
    } else {
        let mut names: Vec<String> = props
            .iter()
            .filter(|(key, prop)| prop.name == **key)
            .map(|(key, _)| key.clone())
            .collect();
        names.sort();
        names
    };
    let mut fields: Vec<crate::ui::page::UiField> = Vec::with_capacity(names.len());
    let mut prop_by_name: BTreeMap<String, &'a PropertyNode> = BTreeMap::new();
    for name in &names {
        if let Some(prop) = props.get(name) {
            prop_by_name.insert(name.clone(), prop);
            fields.push(crate::ui::form::ui_field_from_property(
                prop,
                column_is_entity_ref(prop),
                column_is_codelist(prop),
                &[],
                &[],
                &prop.pg_column_type,
                prop.pg_column_type.contains("RANGE"),
                false,
            ));
        } else {
            fields.push(ux_synth_field(name, &c.fields_with_types));
        }
    }
    UxEntityInput {
        fields,
        prop_by_name,
        workflow_status_field,
    }
}

/// Project the entity plan's collection shape onto the component (issue
/// #301): a Timeline plan yields the render payload, with preview columns
/// resolved through the SAME per-column ux tier as #300 cells. Table
/// plans (and entities without a plan) yield `None`. The plan output is
/// reused verbatim — never re-validated.
fn timeline_from_plan(
    uxg: &UxGeneration<'_>,
    entity: &str,
    rules: &UxRules,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) -> Option<RenderTimeline> {
    let plan = uxg.plans.get(entity)?;
    match &plan.collection {
        crate::ux::plan::CollectionPlan::Timeline {
            order_by,
            title_field,
            preview,
        } => Some(RenderTimeline {
            order_binding: order_by.clone(),
            title_binding: title_field.clone(),
            preview: preview
                .iter()
                .map(|name| {
                    preview_column(rules, name, props, fields_with_types, workflow_status_field)
                })
                .collect(),
        }),
        crate::ux::plan::CollectionPlan::Table => None,
    }
}

/// One timeline preview column: the shared #300 column resolution over the
/// field's graph property (or its synthesized `(field, rust_type)` pair).
fn preview_column(
    rules: &UxRules,
    name: &str,
    props: &HashMap<String, PropertyNode>,
    fields_with_types: &[(String, String)],
    workflow_status_field: Option<&str>,
) -> RenderColumn {
    RenderColumn {
        label: name.to_string(),
        kind: "field".to_string(),
        binding: name.to_string(),
        lookup: String::new(),
        expr: String::new(),
        ux: Some(resolve_column_ux(
            rules,
            "field",
            name,
            props,
            fields_with_types,
            workflow_status_field,
        )),
    }
}

/// The page-level ux formatting context: locale plus a ready-to-render
/// money options object literal (`Intl.NumberFormat`). Currency-less packs
/// degrade money cells to grouped decimals (`{}` options).
fn page_ux_context(rules: &UxRules) -> PageUxContext {
    PageUxContext {
        locale: rules.format.locale.clone(),
        money_options: match rules.format.currency.as_deref() {
            Some(code) => format!("{{ style: 'currency', currency: {} }}", js_quote(code)),
            None => "{}".to_string(),
        },
    }
}

fn render_form(spec: &FormSpec) -> RenderForm {
    RenderForm {
        fields: spec
            .fields
            .iter()
            .map(|field| {
                let html = control_core::html_input_for_dsl(&field.input);
                let validations: Vec<String> =
                    field.validations.iter().map(render_expression).collect();
                let message = if validations.is_empty() {
                    None
                } else {
                    field.messages.first().cloned()
                };
                RenderInputField {
                    name: field.name.clone(),
                    input_role: input_field_role(&html.input_type),
                    input_type: html.input_type,
                    is_textarea: html.is_textarea,
                    is_select: html.is_select,
                    is_radio: html.is_radio,
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
    let mut fetch_state = FetchState::default();
    let mut has_fetch = false;

    for comp in components {
        let is_list = comp.table.is_some() || comp.component_type == "list";
        let is_details = comp.component_type == "details";
        let is_form = comp.form.is_some() || comp.component_type == "form";

        let (fetch_list, fetch_item, fetch_form) = fetch_state.next(
            is_list,
            is_details,
            is_form,
            comp.api.is_some() && !comp.entity.is_empty(),
            id_param.as_deref(),
        );
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

    let has_workflow = load_components.iter().any(|comp| comp.workflow);
    PageLoadContext {
        api_version: api_version.to_string(),
        name: vc.name.clone(),
        components: load_components,
        has_fetch,
        has_workflow,
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
    use rex_ifml::InputFieldType;
    use rex_ifml::PropertyRef;

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
            conditional_expression: None,
            conditional_expr_json: None,
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
            None,
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            None,
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

    fn workflow_transitions_config() -> DomainConfig {
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
initial_state = "draft"
states = ["draft", "submitted", "approved", "rejected"]
terminal_states = ["approved", "rejected"]
generate_action_endpoints = true

[domains.sales.entity_config.CustomerType.workflow.transitions]
draft = ["submitted"]
submitted = ["approved", "rejected"]
"#,
        )
        .unwrap()
    }

    fn details_component_named(name: &str) -> IfmlComponent {
        let mut c = component_with_spec(None);
        c.name = name.to_string();
        c.component_type = "details".to_string();
        c
    }

    fn context_json(c: &IfmlComponent, config: &DomainConfig) -> serde_json::Value {
        let ctx = page_component_context_with_config(c, config);
        serde_json::to_value(&ctx).expect("serialize page component context")
    }

    // ── Transition buttons (issue #198 workflow UI v2, RED) ─────────
    //
    // Pin: a details/form component whose entity has a workflow WITH
    // transitions carries one RenderTransition per (from → to) edge in the
    // `transitions` map, each labeled by the humanized target state with
    // testid `{component}-transition-{target_kebab}`, a client-side
    // disabled expression (current state ≠ from), and ready-to-render
    // button markup carrying `data-transition-from`/`data-transition-to`.
    // The assertions read the context through serde so they compile before
    // `RenderWorkflow.transitions` exists (RED: the key is missing).

    #[test]
    fn transition_context_built_from_transitions_map_for_details() {
        let c = details_component_named("info");
        let json = context_json(&c, &workflow_transitions_config());

        let transitions = &json["workflow"]["transitions"];
        assert!(
            transitions.is_array(),
            "details components need a transition collection: {json}"
        );
        let transitions = transitions.as_array().unwrap();
        assert_eq!(
            transitions.len(),
            3,
            "one entry per transition edge (draft→submitted, submitted→approved, submitted→rejected): {json}"
        );

        let first = &transitions[0];
        assert_eq!(first["from"], serde_json::json!("draft"));
        assert_eq!(first["to"], serde_json::json!("submitted"));
        assert_eq!(
            first["testid"],
            serde_json::json!("info-transition-submitted"),
            "testid contract: {{component}}-transition-{{target_kebab}}: {json}"
        );
        assert_eq!(
            first["label"],
            serde_json::json!("Submitted"),
            "labels are the humanized target state: {json}"
        );

        for entry in transitions {
            let disabled = entry["disabled_expr"].as_str().unwrap_or_default();
            assert!(
                disabled.contains("data.item?.workflow_state?.current_state"),
                "the disabled expression reads the merged workflow state: {entry}"
            );
            let from = entry["from"].as_str().unwrap_or_default();
            assert!(
                disabled.contains(&format!("'{from}'")),
                "the button is disabled unless the current state equals the from-state: {entry}"
            );
            let html = entry["html"].as_str().unwrap_or_default();
            let to = entry["to"].as_str().unwrap_or_default();
            let testid = entry["testid"].as_str().unwrap_or_default();
            assert!(
                html.contains(&format!("data-testid=\"{testid}\"")),
                "button markup carries the testid: {entry}"
            );
            assert!(
                html.contains(&format!("data-transition-from=\"{from}\"")),
                "button markup exposes the from-state for e2e hooks: {entry}"
            );
            assert!(
                html.contains(&format!("data-transition-to=\"{to}\"")),
                "button markup exposes the target state for e2e hooks: {entry}"
            );
            assert!(
                html.contains("disabled={"),
                "the disabled state must be a client-side binding: {entry}"
            );
            assert!(
                html.contains("onclick="),
                "clicking posts to the transition endpoint: {entry}"
            );
        }

        let rejected = transitions
            .iter()
            .find(|e| e["to"] == serde_json::json!("rejected"))
            .expect("rejected is a valid target from submitted");
        assert_eq!(rejected["from"], serde_json::json!("submitted"));
        assert_eq!(
            rejected["label"],
            serde_json::json!("Rejected"),
            "humanized target label: {rejected}"
        );
    }

    #[test]
    fn transition_disabled_expr_follows_form_value_path() {
        let mut c = details_component_named("editor");
        c.component_type = "form".to_string();
        let json = context_json(&c, &workflow_transitions_config());

        let transitions = json["workflow"]["transitions"]
            .as_array()
            .expect("form components carry transition buttons");
        let first = &transitions[0];
        let disabled = first["disabled_expr"].as_str().unwrap_or_default();
        assert!(
            disabled.contains("editor_form_state.workflow_state?.current_state"),
            "form transition buttons read the typed form state's merged workflow state: {first}"
        );
    }

    #[test]
    fn empty_transitions_map_targets_all_non_terminal_states() {
        let c = details_component_named("info");
        let json = context_json(&c, &workflow_config());

        let transitions = json["workflow"]["transitions"]
            .as_array()
            .expect("an empty transitions map still yields buttons");
        let targets: Vec<&str> = transitions
            .iter()
            .filter_map(|e| e["to"].as_str())
            .collect();
        assert_eq!(
            targets,
            vec!["received", "review"],
            "every non-terminal state is a valid target when the map is empty: {json}"
        );
        for entry in transitions {
            assert_eq!(
                entry["from"],
                serde_json::json!(""),
                "no from-state restriction: any non-terminal state may transition: {entry}"
            );
            let disabled = entry["disabled_expr"].as_str().unwrap_or_default();
            assert!(
                disabled.contains("'done'"),
                "buttons disable once the current state is terminal: {entry}"
            );
        }
    }

    #[test]
    fn list_components_carry_no_transition_buttons() {
        let mut c = component_with_spec(None);
        c.component_type = "list".to_string();
        let json = context_json(&c, &workflow_transitions_config());

        assert!(
            json["workflow"]["transitions"]
                .as_array()
                .map(|t| t.is_empty())
                .unwrap_or(true),
            "list components render state badges only — no transition buttons: {json}"
        );
    }

    // ── Event-level capability gating (issue #208 slice, RED) ───────
    //
    // Pin: events carrying `requires` gate their control behind `can(...)`
    // exactly like the view-level control gate, AND-composed with the view
    // gate when the view is also guarded. The IfmlEvent literal below must
    // gain `requires: vec!["RaiseRefund".to_string()]` when the field lands
    // on the context type (Wave B struct evolution) — until then the event
    // is indistinguishable from an ungated one and the gates do not render,
    // which is the RED failure mode.

    fn gated_save_event() -> IfmlEvent {
        IfmlEvent {
            name: "save".to_string(),
            event_type: "save".to_string(),
            params: Vec::new(),
            requires: vec!["RaiseRefund".to_string()],
            action: IfmlAction::Navigate {
                target: "CustomerList".to_string(),
                binding: HashMap::new(),
            },
        }
    }

    fn action_control_mappings() -> IfmlComponentMappings {
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
    fn event_requires_gate_the_fallback_submit_control() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_view(
            "RefundEdit",
            Vec::new(),
            Vec::new(),
            vec![form_component_named("editor", vec![gated_save_event()])],
        );
        let ctx = page_context_for(&vc, None);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "{#if ['RaiseRefund'].some((c) => can(c))}<button type=\"submit\" data-testid=\"editor-submit\" disabled={submitting}>Submit</button>{/if}"
            ),
            "an event-level requires gates the submit button via can() even in unguarded views:\n{rendered}"
        );
        assert!(
            rendered.contains("import { can } from '$lib/roles';"),
            "the gate needs the can() helper import: {rendered}"
        );
    }

    #[test]
    fn event_requires_and_compose_with_the_view_gate() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_view(
            "RefundEdit",
            Vec::new(),
            vec!["ViewRefunds".to_string()],
            vec![form_component_named("editor", vec![gated_save_event()])],
        );
        let ctx = page_context_for(&vc, None);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "{#if viewRequires.some((c) => can(c)) && ['RaiseRefund'].some((c) => can(c))}"
            ),
            "event and view gates AND-compose, mirroring the load guard:\n{rendered}"
        );
    }

    #[test]
    fn event_requires_gate_mapped_action_control_buttons_too() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = guarded_view(
            "RefundEdit",
            Vec::new(),
            Vec::new(),
            vec![form_component_named("editor", vec![gated_save_event()])],
        );
        let ctx = page_context_for(&vc, Some(&action_control_mappings()));
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "{#if ['RaiseRefund'].some((c) => can(c))}<Button onclick={submit_editor} disabled={submitting} testid=\"ui-button\">Save</Button>{/if}"
            ),
            "mapped action-control buttons carry the event-level gate:\n{rendered}"
        );
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

    // ── ux-rules fallback-table resolution (issue #300) ──

    fn ux_rules(toml: &str) -> UxRules {
        codegraph_config::parse_ux_rules_str(toml).unwrap().rules
    }

    fn ux_prop(name: &str, pg_type: &str, kind: Option<RefClassificationKind>) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: "string".into(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: false,
            type_expr: None,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: name.into(),
            pg_column_type: pg_type.into(),
            rust_field_name: name.into(),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "scalar".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: kind,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    fn ux_props(props: Vec<PropertyNode>) -> HashMap<String, PropertyNode> {
        let mut by_name = HashMap::new();
        for prop in props {
            by_name.insert(prop.name.clone(), prop);
        }
        by_name
    }

    fn pack() -> UxRules {
        codegraph_config::builtin_ux_rules().unwrap().rules
    }

    #[test]
    fn lookup_column_pins_status_category_chip_above_inference_and_rules() {
        // An EntityReference property would infer Reference via Pass 1; the
        // explicit DSL lookup (`via status_labels`) still wins (tier 0).
        let props = ux_props(vec![ux_prop(
            "status",
            "UUID",
            Some(RefClassificationKind::EntityReference),
        )]);
        // A project rule that would pin the same name raw must NOT reach a
        // lookup column either.
        let rules = ux_rules("[[column]]\nname_pattern = \"status\"\ndisplay = \"raw\"\n");
        let ux = resolve_column_ux(&rules, "lookup", "status", &props, &[], None);
        assert_eq!(ux.dimension, "status-category");
        assert_eq!(ux.display, "chip");
        assert_eq!(ux.align, "left");
        assert_eq!(ux.tone.lookup("active"), "default", "pack tone map");
    }

    #[test]
    fn rule_beats_inference_for_field_columns() {
        // `total_amount` on NUMERIC infers Money (keyword + numeric pg); the
        // project rule pins Quantity + raw instead (rules > pack > inference).
        let props = ux_props(vec![ux_prop("total_amount", "NUMERIC(10,2)", None)]);
        let rules = ux_rules(
            "[[column]]\nname_pattern = \"*_amount\"\ndimension = \"quantity\"\ndisplay = \"raw\"\nalign = \"left\"\n",
        );
        let ux = resolve_column_ux(&rules, "field", "total_amount", &props, &[], None);
        assert_eq!(ux.dimension, "quantity", "rule pins quantity");
        assert_eq!(ux.display, "raw");
        assert_eq!(ux.align, "left", "rule align replaces the pack default");
    }

    #[test]
    fn expression_columns_stay_text() {
        // The IFML AST carries no return type for `tenure_years(...)` — the
        // column stays Text regardless of the name.
        let props = ux_props(vec![ux_prop("tenure_years", "NUMERIC(10,2)", None)]);
        let ux = resolve_column_ux(&pack(), "expr", "tenure_years", &props, &[], None);
        assert_eq!(ux.dimension, "text");
        assert_eq!(ux.display, "raw");
        assert_eq!(ux.align, "left");
    }

    #[test]
    fn graph_metadata_drives_pass1_inference() {
        let props = ux_props(vec![
            ux_prop(
                "status",
                "TEXT",
                Some(RefClassificationKind::CodelistReference),
            ),
            ux_prop(
                "assignee",
                "UUID",
                Some(RefClassificationKind::EntityReference),
            ),
            ux_prop("id", "UUID", None),
            ux_prop("total_amount", "NUMERIC(10,2)", None),
            ux_prop("created_at", "TIMESTAMPTZ", None),
        ]);
        let status = resolve_column_ux(&pack(), "field", "status", &props, &[], None);
        assert_eq!(status.dimension, "status-category");
        assert_eq!(status.display, "chip");

        let reference = resolve_column_ux(&pack(), "field", "assignee", &props, &[], None);
        assert_eq!(reference.dimension, "reference");
        assert_eq!(reference.display, "link");

        let id = resolve_column_ux(&pack(), "field", "id", &props, &[], None);
        assert_eq!(id.dimension, "identifier");
        assert_eq!(id.display, "copy-chip");

        let money = resolve_column_ux(&pack(), "field", "total_amount", &props, &[], None);
        assert_eq!(money.dimension, "money");
        assert_eq!(money.align, "right");

        let time = resolve_column_ux(&pack(), "field", "created_at", &props, &[], None);
        assert_eq!(time.dimension, "time-point");
    }

    #[test]
    fn workflow_status_field_lifts_to_status_category() {
        let props = ux_props(vec![ux_prop("state", "TEXT", None)]);
        let ux = resolve_column_ux(&pack(), "field", "state", &props, &[], Some("state"));
        assert_eq!(ux.dimension, "status-category");
        assert_eq!(ux.display, "chip");
    }

    #[test]
    fn schemaless_columns_fall_back_to_component_type_pairs() {
        // No graph props: `id` with a Uuid rust type still infers Identifier
        // through the (field, rust_type) projection; a Decimal money name
        // stays Text (no pg type — honest degradation).
        let pairs = vec![
            ("id".to_string(), "Uuid".to_string()),
            ("total_amount".to_string(), "Decimal".to_string()),
        ];
        let id = resolve_column_ux(&pack(), "field", "id", &HashMap::new(), &pairs, None);
        assert_eq!(id.dimension, "identifier");
        assert_eq!(id.display, "copy-chip");
        let money = resolve_column_ux(
            &pack(),
            "field",
            "total_amount",
            &HashMap::new(),
            &pairs,
            None,
        );
        assert_eq!(money.dimension, "text");
    }

    #[test]
    fn apply_table_ux_resolves_every_column_by_tier() {
        let mut table = render_table(&TableSpec {
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
                    expr: Expression::Ident("Customer".to_string()),
                },
            ],
            pagination: false,
        });
        let props = ux_props(vec![
            ux_prop("name", "TEXT", None),
            ux_prop(
                "status",
                "TEXT",
                Some(RefClassificationKind::CodelistReference),
            ),
        ]);
        apply_table_ux(&mut table, &pack(), &props, &[], Some("status"));
        assert_eq!(table.columns[0].ux.as_ref().unwrap().dimension, "text");
        assert_eq!(
            table.columns[1].ux.as_ref().unwrap().display,
            "chip",
            "lookup pins the chip"
        );
        assert_eq!(table.columns[2].ux.as_ref().unwrap().dimension, "text");
    }

    #[test]
    fn flag_off_leaves_render_contexts_ux_free() {
        let ctx = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let table = ctx.table.expect("table render context");
        assert!(
            table.columns.iter().all(|col| col.ux.is_none()),
            "flag off ⇒ no ux payloads in the context"
        );
        assert!(ctx.ux_list_columns.is_empty());

        let mut list = component_with_spec(None);
        list.component_type = "list".to_string();
        list.fields = vec!["name".to_string(), "status".to_string()];
        let ctx = page_component_context_sync(&list);
        assert!(ctx.ux_list_columns.is_empty(), "no ux columns when off");
    }

    #[test]
    fn ux_on_resolves_specless_list_columns() {
        let mut c = component_with_spec(None);
        c.component_type = "list".to_string();
        c.fields = vec!["status".to_string(), "name".to_string()];
        let rules = pack();
        let props = ux_props(vec![
            ux_prop(
                "status",
                "TEXT",
                Some(RefClassificationKind::CodelistReference),
            ),
            ux_prop("name", "TEXT", None),
        ]);
        let status = resolve_column_ux(&rules, "field", "status", &props, &[], None);
        assert_eq!(status.display, "chip");
        assert!(status.tone.lookup("draft") == "secondary");
        let name = resolve_column_ux(&rules, "field", "name", &props, &[], None);
        assert_eq!(name.display, "raw");
    }

    #[test]
    fn page_ux_context_formats_locale_and_money_options() {
        let rules = ux_rules("[format]\nlocale = \"de-DE\"\ncurrency = \"EUR\"\n");
        let ctx = page_ux_context(&rules);
        assert_eq!(ctx.locale, "de-DE");
        assert_eq!(ctx.money_options, "{ style: 'currency', currency: 'EUR' }");

        let ctx = page_ux_context(&pack());
        assert_eq!(ctx.locale, "en-NZ");
        assert_eq!(ctx.money_options, "{ style: 'currency', currency: 'NZD' }");

        let ctx = page_ux_context(&ux_rules("[format]\nlocale = \"de-DE\"\n"));
        assert_eq!(ctx.money_options, "{}", "no currency ⇒ plain decimals");
    }

    #[test]
    fn page_ux_rides_the_page_context_only_when_enabled() {
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            conditional_expression: None,
            conditional_expr_json: None,
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
            &vc,
            None,
            &HashSet::new(),
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
        assert!(ctx.ux.is_none(), "flag off ⇒ no ux key on the page context");

        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            None,
            &HashSet::new(),
            Some(&ux_generation_for_tests(&pack())),
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
        let ux = ctx.ux.expect("ux context");
        assert_eq!(ux.locale, "en-NZ");
        assert_eq!(ux.money_options, "{ style: 'currency', currency: 'NZD' }");
    }

    /// A flag-on [`UxGeneration`] over an empty plan map — the shape test
    /// contexts use when the timeline/layout plane itself isn't the
    /// subject.
    fn ux_generation_for_tests<'a>(rules: &'a UxRules) -> UxGeneration<'a> {
        static EMPTY_PLANS: std::sync::OnceLock<HashMap<String, crate::ux::plan::UxPlan>> =
            std::sync::OnceLock::new();
        UxGeneration {
            rules,
            plans: EMPTY_PLANS.get_or_init(HashMap::new),
        }
    }

    // ── Timeline layout + event tiering + diagnostics (issue #301) ──

    fn ux_schema(title: &str) -> codegraph_core::types::SchemaNode {
        codegraph_core::types::SchemaNode {
            namespace: None,
            schema_id: format!("id:{title}"),
            title: title.to_string(),
            access: None,
            annotations: None,
            description: None,
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some("sales".to_string()),
            rel_path: format!("{title}.json"),
            pg_type: "UUID".to_string(),
            rust_type: "Uuid".to_string(),
            sea_orm_type: "Uuid".to_string(),
            rust_type_name: title.to_string(),
            pg_table_name: codegraph_naming::to_snake_case(title),
            api_path_segment: codegraph_naming::to_kebab_case(title),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
        }
    }

    /// Graph fixture for the Customer entity: a name, a codelist status,
    /// a numeric money-named amount and a timestamp — one field per
    /// dimension the timeline layout touches.
    fn ux_customer_db() -> MockEngine {
        MockEngine::builder()
            .with_schema(ux_schema("CustomerType"))
            .with_properties(
                "CustomerType",
                vec![
                    ux_prop("name", "TEXT", None),
                    ux_prop(
                        "status",
                        "TEXT",
                        Some(RefClassificationKind::CodelistReference),
                    ),
                    ux_prop("total_amount", "NUMERIC(10,2)", None),
                    ux_prop("created_at", "TIMESTAMPTZ", None),
                ],
            )
            .build()
    }

    fn nav_event(name: &str, event_type: &str, target: &str) -> IfmlEvent {
        IfmlEvent {
            name: name.to_string(),
            event_type: event_type.to_string(),
            params: vec!["row".to_string()],
            requires: Vec::new(),
            action: IfmlAction::Navigate {
                target: target.to_string(),
                binding: HashMap::new(),
            },
        }
    }

    fn timeline_rules() -> UxRules {
        ux_rules(
            "[[collection]]\nentity_pattern = \"Customer*\"\ndisplay = \"timeline\"\n\
             order_by = \"created_at\"\ntitle_field = \"name\"\npreview = [\"status\", \"total_amount\"]\n",
        )
    }

    /// A typed fallback table over the Customer fixture carrying THREE
    /// navigate events (select inline + archive/delete disclosed).
    fn timeline_table_component() -> IfmlComponent {
        let mut c = component_with_spec(Some(ComponentSpec::Table(TableSpec {
            columns: vec![
                ColumnDef::Field {
                    label: "Name".to_string(),
                    field: PropertyRef {
                        entity: "Customer".to_string(),
                        property: "name".to_string(),
                    },
                },
                ColumnDef::Field {
                    label: "Created".to_string(),
                    field: PropertyRef {
                        entity: "Customer".to_string(),
                        property: "created_at".to_string(),
                    },
                },
            ],
            pagination: false,
        })));
        c.events = vec![
            nav_event("comp_grid_select", "select", "CustomerDetail"),
            nav_event("comp_grid_archive", "archive", "CustomerArchive"),
            nav_event("comp_grid_delete", "delete", "CustomerDelete"),
        ];
        c
    }

    /// Resolve the generation plans for a single-view fixture.
    fn resolve_plans_for(
        db: &MockEngine,
        vc: &IfmlViewContainer,
        rules: &UxRules,
    ) -> HashMap<String, crate::ux::plan::UxPlan> {
        let model = model_of(vec![vc.clone()]);
        let (plans, _lines) = futures::executor::block_on(resolve_generation_ux(
            db,
            &test_config(),
            &model,
            Some(rules),
        ))
        .expect("generation ux resolution");
        plans
    }

    fn page_component_context_with_ux(
        db: &MockEngine,
        config: &DomainConfig,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
        uxgen: Option<&UxGeneration<'_>>,
    ) -> PageComponentContext {
        let mut cache = HashMap::new();
        futures::executor::block_on(page_component_context(
            db,
            config,
            "v1",
            vc,
            c,
            None,
            None,
            &mut cache,
            &HashSet::new(),
            uxgen,
        ))
    }

    #[test]
    fn timeline_rule_resolves_layout_with_resolved_bindings() {
        let db = ux_customer_db();
        let rules = timeline_rules();
        let c = timeline_table_component();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c.clone());
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };

        let ctx = page_component_context_with_ux(&db, &test_config(), &vc, &c, Some(&uxgen));

        // RenderTable.layout carries the timeline with the rule's
        // resolved bindings (reused plan output — no re-validation).
        let table = ctx.table.expect("table context");
        let TableLayout::Timeline {
            order_binding,
            title_binding,
            preview,
        } = &table.layout
        else {
            panic!(
                "explicit matching rule must resolve a timeline: {:?}",
                table.layout
            );
        };
        assert_eq!(order_binding, "created_at");
        assert_eq!(title_binding.as_deref(), Some("name"));
        assert_eq!(
            preview
                .iter()
                .map(|col| col.binding.as_str())
                .collect::<Vec<_>>(),
            vec!["status", "total_amount"]
        );

        // The uniform component view agrees (single template path).
        let timeline = ctx.timeline.as_ref().expect("uniform timeline view");
        assert_eq!(timeline.order_binding, "created_at");
        assert_eq!(timeline.title_binding.as_deref(), Some("name"));
        assert_eq!(timeline.preview[0].binding, "status");
        assert_eq!(
            timeline.preview[0].ux.as_ref().unwrap().display,
            "chip",
            "preview renders through the shared #300 chip tier"
        );
        assert_eq!(timeline.preview[1].binding, "total_amount");
        assert_eq!(
            timeline.preview[1].ux.as_ref().unwrap().dimension,
            "money",
            "preview renders through the shared #300 money tier"
        );
    }

    #[test]
    fn timeline_unresolvable_order_by_errors_naming_candidates() {
        let db = ux_customer_db();
        let rules = ux_rules(
            "[[collection]]\nentity_pattern = \"Customer*\"\ndisplay = \"timeline\"\norder_by = \"deleted_at\"\n",
        );
        let mut vc = plain_vc("CustomerList");
        vc.components.push(timeline_table_component());
        let model = model_of(vec![vc]);
        let err = futures::executor::block_on(resolve_generation_ux(
            &db,
            &test_config(),
            &model,
            Some(&rules),
        ))
        .expect_err("unresolvable order_by must fail generation");
        let message = err.to_string();
        assert!(message.contains("deleted_at"), "{message}");
        assert!(message.contains("Customer"), "{message}");
        assert!(
            message.contains("created_at"),
            "the error names the candidate time-point fields: {message}"
        );
    }

    #[test]
    fn mapped_component_beats_timeline_layout() {
        let db = ux_customer_db();
        let rules = timeline_rules();
        let c = timeline_table_component();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c.clone());
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };
        let mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
"#,
        )
        .unwrap();

        let mut cache = HashMap::new();
        let ctx = futures::executor::block_on(page_component_context(
            &db,
            &test_config(),
            "v1",
            &vc,
            &c,
            None,
            Some(&mappings),
            &mut cache,
            &HashSet::new(),
            Some(&uxgen),
        ));

        assert!(ctx.mapping.is_some(), "whole-component mapping resolves");
        assert!(
            ctx.timeline.is_none(),
            "mapped components replace the fallback — no timeline"
        );
        assert!(
            ctx.table.as_ref().unwrap().layout.is_table(),
            "mapped components reset the table layout"
        );
        assert!(
            ctx.row_menu_events.is_empty(),
            "mapped components keep their own event surface — no actions menu"
        );
    }

    #[test]
    fn no_timeline_rule_stays_table_and_single_event_has_no_menu() {
        let db = ux_customer_db();
        let rules = pack();
        let mut c = timeline_table_component();
        c.events = vec![nav_event("comp_grid_select", "select", "CustomerDetail")];
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c.clone());
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };

        let ctx = page_component_context_with_ux(&db, &test_config(), &vc, &c, Some(&uxgen));
        assert!(ctx.table.unwrap().layout.is_table(), "no rule ⇒ table");
        assert!(ctx.timeline.is_none());
        assert!(
            ctx.row_menu_events.is_empty(),
            "single navigate event ⇒ no menu, no actions testid"
        );
    }

    #[test]
    fn event_partition_first_inline_rest_menu_with_shared_handlers() {
        let db = ux_customer_db();
        let rules = pack();
        let c = timeline_table_component();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c.clone());
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };

        let ctx = page_component_context_with_ux(&db, &test_config(), &vc, &c, Some(&uxgen));
        assert_eq!(
            ctx.row_handler.as_deref(),
            Some("comp_grid_select"),
            "the FIRST navigate event stays inline"
        );
        let menu = &ctx.row_menu_events;
        assert_eq!(menu.len(), 2, "the other two events disclose");
        assert_eq!(menu[0].handler_name, "comp_grid_archive");
        assert_eq!(menu[0].label, "Archive");
        assert_eq!(menu[1].handler_name, "comp_grid_delete");
        assert_eq!(menu[1].label, "Delete");

        // Handler parity: menu items call the same named functions the
        // inline placement would call (the page template emits one
        // function per event, referenced from both placements).
        let emitted: Vec<&str> = ctx.events.iter().map(|e| e.handler_name.as_str()).collect();
        for evt in menu {
            assert!(
                emitted.contains(&evt.handler_name.as_str()),
                "menu handler {} must be an emitted event handler",
                evt.handler_name
            );
        }
    }

    #[test]
    fn flag_off_context_json_omits_issue301_keys() {
        let c = timeline_table_component();
        let ctx = page_component_context_sync(&c);
        let json = serde_json::to_value(&ctx).unwrap();
        assert!(json.get("timeline").is_none(), "{json}");
        assert!(json.get("row_menu_events").is_none(), "{json}");
        assert!(
            json.get("table").and_then(|t| t.get("layout")).is_none(),
            "flag off ⇒ no layout key on the serialized table: {json}"
        );

        // Flag ON but no matching rule ⇒ same absence.
        let db = ux_customer_db();
        let rules = pack();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c.clone());
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };
        let ctx = page_component_context_with_ux(&db, &test_config(), &vc, &c, Some(&uxgen));
        let json = serde_json::to_value(&ctx).unwrap();
        assert!(json.get("timeline").is_none(), "{json}");
        assert!(
            json.get("table").and_then(|t| t.get("layout")).is_none(),
            "table plan ⇒ layout key skipped: {json}"
        );
    }

    #[test]
    fn specless_list_resolves_the_uniform_timeline_view() {
        let db = ux_customer_db();
        let rules = timeline_rules();
        let mut c = component_with_spec(None);
        c.component_type = "list".to_string();
        c.fields = vec![
            "name".to_string(),
            "status".to_string(),
            "created_at".to_string(),
        ];
        c.events = vec![nav_event("comp_grid_select", "select", "CustomerDetail")];
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c.clone());
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };

        let ctx = page_component_context_with_ux(&db, &test_config(), &vc, &c, Some(&uxgen));
        let timeline = ctx.timeline.as_ref().expect("spec-less list timeline");
        assert_eq!(timeline.order_binding, "created_at");
        assert_eq!(timeline.title_binding.as_deref(), Some("name"));
        assert_eq!(timeline.preview[0].binding, "status");
    }

    fn render_page_with_ux(
        db: &MockEngine,
        config: &DomainConfig,
        vc: &IfmlViewContainer,
        uxgen: Option<&UxGeneration<'_>>,
    ) -> String {
        let tera = create_tera(Path::new(".")).expect("tera");
        let ctx = futures::executor::block_on(build_page_context(
            db,
            config,
            "v1",
            vc,
            None,
            &HashSet::new(),
            uxgen,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
        render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render")
    }

    #[test]
    fn timeline_render_replaces_the_table_block() {
        let db = ux_customer_db();
        let rules = timeline_rules();
        let c = timeline_table_component();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c);
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };

        let rendered = render_page_with_ux(&db, &test_config(), &vc, Some(&uxgen));
        assert!(
            rendered.contains("<ol class=\"timeline\" data-testid=\"grid-timeline\">"),
            "{rendered}"
        );
        assert!(
            rendered.contains("{#each grid_timeline_items as item}"),
            "{rendered}"
        );
        assert!(
            rendered.contains("timeOf(b['created_at']) - timeOf(a['created_at'])"),
            "items sort newest first on the order binding:\n{rendered}"
        );
        assert!(
            rendered
                .contains("<li class=\"timeline-item\" data-testid=\"grid-timeline-item\" onclick={() => comp_grid_select(item)}>"),
            "the row onclick fires from the item body:\n{rendered}"
        );
        assert!(
            rendered.contains(
                "<time class=\"timeline-time\" datetime={item.created_at}>{formatDate(item.created_at)}</time>"
            ),
            "the time head renders the date-only Intl helper:\n{rendered}"
        );
        assert!(
            rendered.contains(
                "<button type=\"button\" class=\"timeline-title\" data-testid=\"grid-timeline-title\" onclick={() => comp_grid_select(item)}>{item.name}</button>"
            ),
            "the title carries the row-handler target convention:\n{rendered}"
        );
        assert!(
            rendered.contains(
                "<span class=\"timeline-meta\" data-testid=\"grid-timeline-meta\"><span class=\"chip\" data-chip={item.status} data-chip-variant={toneFor('grid.status', item.status)} data-testid=\"grid-chip\">{item.status}</span></span>"
            ),
            "preview fields render through the shared #300 chip formatter:\n{rendered}"
        );
        assert!(
            rendered.contains("{formatMoney(item.total_amount)}"),
            "{rendered}"
        );
        assert!(
            rendered.contains("'grid.status': {"),
            "preview chips join the page tone map:\n{rendered}"
        );
        assert!(
            !rendered.contains("<table data-testid=\"grid-table\""),
            "the timeline replaces the table block:\n{rendered}"
        );
    }

    #[test]
    fn timeline_item_renders_workflow_badge_and_actions_menu() {
        let db = ux_customer_db();
        let rules = timeline_rules();
        let c = timeline_table_component();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c);
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };

        let rendered = render_page_with_ux(&db, &workflow_config(), &vc, Some(&uxgen));
        assert!(
            rendered.contains("data-workflow-state={item.status}"),
            "the workflow badge renders per item:\n{rendered}"
        );
        assert!(
            rendered.contains("let grid_actions_open = $state(false);"),
            "{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"grid-actions\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"grid-actions-menu\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("onclick={(e) => { e.stopPropagation(); comp_grid_archive(item); }}>Archive</button>"),
            "menu items carry the identical handler invocation:\n{rendered}"
        );
        // Handler parity: each handler body is emitted exactly once and
        // referenced from both the item click and the menu.
        assert_eq!(
            rendered
                .matches("function comp_grid_archive(row: Record<string, unknown>) {")
                .count(),
            1,
            "the secondary handler body is emitted once:\n{rendered}"
        );
        let def = rendered
            .find("function comp_grid_archive(row: Record<string, unknown>) {")
            .expect("handler definition");
        let menu_use = rendered.find("comp_grid_archive(item);").expect("menu use");
        assert!(menu_use > def, "{rendered}");
    }

    #[test]
    fn secondary_event_menu_renders_on_plain_tables_with_inline_parity() {
        let db = ux_customer_db();
        let rules = pack();
        let c = timeline_table_component();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c);
        let plans = resolve_plans_for(&db, &vc, &rules);
        let uxgen = UxGeneration {
            rules: &rules,
            plans: &plans,
        };

        let rendered = render_page_with_ux(&db, &test_config(), &vc, Some(&uxgen));
        // The table stays; the first event remains the inline row handler.
        assert!(
            rendered
                .contains("<tr data-testid=\"grid-row\" onclick={() => comp_grid_select(item)}>"),
            "first event stays inline byte-equal:\n{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"grid-actions\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"grid-actions-menu\""),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "onclick={(e) => { e.stopPropagation(); comp_grid_archive(item); }}>Archive</button>"
            ),
            "{rendered}"
        );
        assert!(
            !rendered.contains("grid-timeline"),
            "no timeline without a rule: {rendered}"
        );
        assert_eq!(
            rendered
                .matches("function comp_grid_archive(row: Record<string, unknown>) {")
                .count(),
            1,
            "menu handlers are the shared named functions:\n{rendered}"
        );
    }

    #[test]
    fn generation_diagnostics_are_deduped_per_entity_and_quiet_when_clean() {
        // A spec-less list whose declared fields carry the finding shapes:
        // a money keyword, a time-named field. Both hint classes fire once,
        // and a second view binding the same entity adds nothing.
        let db = ux_customer_db();
        let rules = pack();
        let mut c = component_with_spec(None);
        c.component_type = "list".to_string();
        c.fields = vec![
            "name".to_string(),
            "status".to_string(),
            "total_amount".to_string(),
            "created_at".to_string(),
        ];
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c);
        let mut vc2 = plain_vc("CustomerList2");
        vc2.components.push({
            let mut c = component_with_spec(None);
            c.component_type = "list".to_string();
            c.fields = vec![
                "name".to_string(),
                "status".to_string(),
                "total_amount".to_string(),
                "created_at".to_string(),
            ];
            c
        });
        let model = model_of(vec![vc, vc2]);
        let (plans, lines) = futures::executor::block_on(resolve_generation_ux(
            &db,
            &test_config(),
            &model,
            Some(&rules),
        ))
        .expect("resolution");
        assert_eq!(plans.len(), 1, "one plan per distinct bound entity");
        let money = lines
            .iter()
            .find(|l| l.contains("total_amount"))
            .expect("money hint line");
        assert!(money.contains("total_amount"), "{money}");
        let suggestion = lines
            .iter()
            .find(|l| l.contains("timeline"))
            .expect("a time-dominated collection without a rule earns a suggestion");
        assert!(suggestion.contains("created_at"), "{suggestion}");

        // Clean projection — an entity without money keywords or
        // time-shaped fields: only the shared actions accounting line
        // remains, once.
        let mut c = component_with_spec(None);
        c.component_type = "list".to_string();
        c.fields = vec!["name".to_string(), "quantity".to_string()];
        let mut vc = plain_vc("CustomerList");
        vc.components.push(c);
        let model = model_of(vec![vc]);
        let empty_db = MockEngine::new();
        let (_plans, lines) = futures::executor::block_on(resolve_generation_ux(
            &empty_db,
            &test_config(),
            &model,
            Some(&rules),
        ))
        .expect("resolution");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("2 row action(s)"), "{lines:?}");
    }

    #[test]
    fn flag_off_generation_diagnostics_stay_silent() {
        let db = ux_customer_db();
        let mut vc = plain_vc("CustomerList");
        vc.components.push(timeline_table_component());
        let model = model_of(vec![vc]);
        let (plans, lines) =
            futures::executor::block_on(resolve_generation_ux(&db, &test_config(), &model, None))
                .expect("resolution");
        assert!(plans.is_empty());
        assert!(lines.is_empty());
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
                rex_ifml::FieldDef {
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
                rex_ifml::FieldDef {
                    name: "start".to_string(),
                    input: InputFieldType::DateTime,
                    required: false,
                    validations: Vec::new(),
                    values: Vec::new(),
                    messages: Vec::new(),
                },
                rex_ifml::FieldDef {
                    name: "tier".to_string(),
                    input: InputFieldType::Dropdown,
                    required: false,
                    validations: Vec::new(),
                    values: vec!["gold".to_string(), "silver".to_string()],
                    messages: Vec::new(),
                },
                rex_ifml::FieldDef {
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
            fields: vec![rex_ifml::FieldDef {
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
            fields: vec![rex_ifml::FieldDef {
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx")
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
                requires: Vec::new(),
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
            requires: Vec::new(),
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
            groups: vec![RenderGroup {
                heading: None,
                components: components.clone(),
            }],
            components,
            params: Vec::new(),
            view_params: Vec::new(),
            view_events: Vec::new(),
            imports: Vec::new(),
            needs_goto: false,
            needs_on_mount: false,
            needs_invalidate: false,
            has_submit: false,
            view_role: None,
            container_role: None,
            roles: Vec::new(),
            requires: Vec::new(),
            control_gate: ControlGateContext::default(),
            modal: None,
            container: None,
            ux: None,
            guard_expr: None,
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
            requires: Vec::new(),
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");

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
            conditional_expression: None,
            conditional_expr_json: None,
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

    fn field_def(name: &str, input: InputFieldType) -> rex_ifml::FieldDef {
        rex_ifml::FieldDef {
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
            requires: Vec::new(),
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
        assert_eq!(ctx.view_role, Some(SemanticRole::ModalView));
        assert_eq!(ctx.container_role, None);

        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(false, true, false),
            None,
            &HashSet::new(),
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
        assert_eq!(ctx.view_role, Some(SemanticRole::Shell));

        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(false, false, true),
            None,
            &HashSet::new(),
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            requires: Vec::new(),
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            requires: Vec::new(),
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            requires: Vec::new(),
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
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

    fn tabs_mappings() -> IfmlComponentMappings {
        toml::from_str(
            r#"
[[component]]
role = "presentation-container"
path = "$lib/components/ui/tabs/tabs.svelte"
export = "Tabs"
testids = { root = "tabs" }
"#,
        )
        .unwrap()
    }

    fn xor_container_with_form(name: &str, label: &str, form_name: &str) -> IfmlViewContainer {
        IfmlViewContainer {
            name: name.to_string(),
            label: Some(label.to_string()),
            is_xor: true,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            conditional_expression: None,
            conditional_expr_json: None,
            roles: Vec::new(),
            requires: Vec::new(),
            params: Vec::new(),
            components: vec![form_component_named(form_name, Vec::new())],
            events: Vec::new(),
            containers: Vec::new(),
        }
    }

    #[test]
    fn sibling_xor_containers_render_inside_one_mapped_presentation_container() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = plain_vc("Checkout");
        vc.label = Some("Checkout".to_string());
        vc.containers = vec![
            xor_container_with_form("Shipping", "Shipping", "shipping_form"),
            xor_container_with_form("Payment", "Payment", "payment_form"),
        ];
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&tabs_mappings()),
            &HashSet::new(),
            None,
            &ProjectConfig::default(),
        ))
        .expect("page ctx");
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import Tabs from '$lib/components/ui/tabs/tabs.svelte';"),
            "{rendered}"
        );
        assert_eq!(
            rendered.matches("<Tabs testid=\"tabs\">").count(),
            1,
            "sibling xor containers must share ONE mapped presentation-container wrapper: {rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"shipping-label\">Shipping<"),
            "each group renders its container label with a stable testid: {rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"payment-label\">Payment<"),
            "{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"shipping_form-form\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"payment_form-form\""),
            "{rendered}"
        );
        let open = rendered.find("<Tabs testid=\"tabs\">").expect("open");
        let shipping = rendered
            .find("data-testid=\"shipping-label\"")
            .expect("shipping group");
        let payment = rendered
            .find("data-testid=\"payment-label\"")
            .expect("payment group");
        let close = rendered.rfind("</Tabs>").expect("close");
        assert!(
            open < shipping && shipping < payment && payment < close,
            "both groups render inside the wrapper in container order: {rendered}"
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            requires: Vec::new(),
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
            conditional_expression: None,
            conditional_expr_json: None,
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
            conditional_expression: None,
            conditional_expr_json: None,
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

    fn nav_shell_mappings() -> IfmlComponentMappings {
        toml::from_str(
            r#"
[[component]]
role = "shell"
path = "$lib/components/Nav.svelte"
export = "Nav"
testids = { root = "side-nav" }
"#,
        )
        .unwrap()
    }

    fn landmark_with_select_event(
        binding: HashMap<String, String>,
    ) -> super::super::context::IfmlViewContainer {
        let mut list = plain_vc("CustomerList");
        list.is_landmark = true;
        list.events.push(IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: vec!["row".to_string()],
            requires: Vec::new(),
            action: IfmlAction::Navigate {
                target: "CustomerDetail".to_string(),
                binding,
            },
        });
        list
    }

    #[test]
    fn shell_nav_drops_event_scoped_bindings_from_nav_hrefs() {
        let list = landmark_with_select_event(HashMap::from([(
            "customerId".to_string(),
            "row.id".to_string(),
        )]));
        let nav = shell_nav(&[list], Some(&nav_shell_mappings())).expect("shell nav");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(
            nav.items[0].href_attr, "href={\"/customerdetail\"}",
            "identifiers bound only in event params (row) are not in layout scope; \
             the nav link must be a plain route link: {:?}",
            nav.items[0].href_attr
        );
    }

    #[test]
    fn shell_nav_keeps_bindings_that_do_not_reference_event_params() {
        let list = landmark_with_select_event(HashMap::from([(
            "tab".to_string(),
            "'overview'".to_string(),
        )]));
        let nav = shell_nav(&[list], Some(&nav_shell_mappings())).expect("shell nav");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(
            nav.items[0].href_attr, "href={`/customerdetail?tab=${'overview'}`}",
            "static bindings stay on the nav link: {:?}",
            nav.items[0].href_attr
        );
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

    fn id_param() -> super::super::context::ParameterDef {
        super::super::context::ParameterDef {
            name: "customerId".to_string(),
            type_ref: "Uuid".to_string(),
            default: None,
        }
    }

    #[test]
    fn create_view_submit_posts_to_collection_url() {
        let ctx = page_context_for(&form_view(false), None);
        let submit = ctx.components[0].submit.as_ref().expect("form submit");
        assert_eq!(submit.method, "POST");
        assert_eq!(submit.url_expr, "\"/api/v1/customer\"");
        assert!(
            !submit.url_expr.contains("${"),
            "create submit URL must not interpolate an id: {}",
            submit.url_expr
        );
    }

    #[test]
    fn edit_view_submit_puts_to_item_url() {
        let mut vc = form_view(false);
        vc.params = vec![id_param()];
        let ctx = page_context_for(&vc, None);
        let submit = ctx.components[0].submit.as_ref().expect("form submit");
        assert_eq!(submit.method, "PUT");
        assert_eq!(
            submit.url_expr, "`/api/v1/customer/${viewParams.customerId}`",
            "edit submit must target the item URL through viewParams"
        );
    }

    #[test]
    fn edit_view_page_branches_to_post_collection_in_create_mode() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut vc = form_view(false);
        vc.params = vec![id_param()];
        vc.components[0].events.push(save_event());
        let ctx = page_context_for(&vc, None);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("method: 'POST'"),
            "opening an id-param form view without ?id (create mode) must POST the \
             collection URL; rendered submit:\n{rendered}"
        );
        assert!(
            rendered.contains("\"/api/v1/customer\"") || rendered.contains("`/api/v1/customer`"),
            "create mode must target the collection URL literal (no trailing \
             empty item id):\n{rendered}"
        );
        assert!(
            rendered.contains("method: 'PUT'"),
            "edit mode (?id present) must keep the item PUT:\n{rendered}"
        );
    }

    // ── expr_ir gated page guard (issue #278) ───────────────────────────

    fn conditional_vc() -> IfmlViewContainer {
        let expr = serde_json::json!({
            "type": "binOp",
            "value": {
                "left": {"type": "fieldExpr", "value": {
                    "object": {"type": "ident", "value": "row"},
                    "field": "active"
                }},
                "op": "eq",
                "right": {"type": "boolLit", "value": true}
            }
        });
        let mut vc = plain_vc("Promo");
        vc.conditional_expression = Some("row.active == true".to_string());
        vc.conditional_expr_json = Some(expr.to_string());
        vc
    }

    #[test]
    fn page_guard_expr_only_when_expr_ir_flag_on() {
        let vc = conditional_vc();
        let off = ProjectConfig::default();
        assert_eq!(
            page_guard_expr(&vc, &off).expect("flag OFF cannot fail"),
            None,
            "expr_ir OFF must never lower a guard"
        );
        let on = ProjectConfig {
            expr_ir: true,
            ..ProjectConfig::default()
        };
        assert_eq!(
            page_guard_expr(&vc, &on)
                .expect("lowering must succeed")
                .as_deref(),
            Some("(row.active === true)"),
            "expr_ir ON must lower the persisted AST"
        );
    }

    #[test]
    fn page_guard_is_none_without_expr_json_or_condition() {
        let project = ProjectConfig {
            expr_ir: true,
            ..ProjectConfig::default()
        };
        let mut unconditioned = plain_vc("Plain");
        unconditioned.conditional_expr_json = None;
        assert_eq!(
            page_guard_expr(&unconditioned, &project).expect("lowerable"),
            None
        );
        let mut source_only = plain_vc("Plain");
        source_only.conditional_expression = Some("row.active == true".to_string());
        source_only.conditional_expr_json = None;
        assert_eq!(
            page_guard_expr(&source_only, &project).expect("lowerable"),
            None,
            "without a persisted AST there is nothing to lower"
        );
    }

    #[test]
    fn guard_template_renders_from_ast_gated() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let vc = conditional_vc();

        let off_ctx = page_context_for(&vc, None);
        assert_eq!(off_ctx.guard_expr, None);
        let off = render_template(&tera, "ifml/svelte/page.tera", &off_ctx).expect("render");
        assert!(
            !off.contains("view_guard"),
            "expr_ir OFF must render no guard: {off}"
        );

        let on = ProjectConfig {
            expr_ir: true,
            ..ProjectConfig::default()
        };
        let mut on_ctx = page_context_for(&vc, None);
        on_ctx.guard_expr = page_guard_expr(&vc, &on).expect("lowering must succeed");
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &on_ctx).expect("render");
        assert!(
            rendered.contains("const view_guard = $derived(Boolean((row.active === true)));"),
            "expr_ir ON must render the lowered guard: {rendered}"
        );
    }
}
