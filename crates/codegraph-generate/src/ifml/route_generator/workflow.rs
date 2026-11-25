use codegraph_config::DomainConfig;

use crate::ifml::context::IfmlComponent;
use crate::machines::{EntityMachineSurface, machine_read_model_surface_for_title};

use std::collections::HashMap;

use super::context::{RenderMachine, RenderTransition, RenderWorkflow};
use super::render::sanitize_ident;
use super::roles::{is_collection, is_form_component};

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
                    machine: None,
                    transitions: Vec::new(),
                    badge_html: String::new(),
                    each: false,
                });
        }
    }
    None
}

/// The machine read-model surface for a component's bound entity (issue
/// #817): graph-driven, so machine-ONLY entities (an authored TypeScript
/// machine, no domains.toml workflow config) resolve too. The db-backed
/// helper is the SAME one the api plane uses to EMIT the read model — the
/// UI and the routes cannot disagree about which entities have one. Issue
/// #824: a machine-bearing design without a loader (no repository, or a
/// parent-scoped member) gets no read model on either plane, so the UI
/// never fetches what the api plane withheld. The title chain mirrors
/// `workflow_for_entity` (plain, then `{entity}Type`). The workflow scope
/// rides along (issue #818): a dual entity surfaces the machine bound to
/// the workflow's own status field — a machine on another field drives a
/// different transition plane and never renders through the workflow's
/// buttons.
pub(super) async fn machine_surface_for(
    db: &dyn codegraph_core::traits::GraphQuerier,
    config: &DomainConfig,
    entity: &str,
) -> Option<EntityMachineSurface> {
    if entity.is_empty() {
        return None;
    }
    let scope = |title: &str| workflow_for_entity(config, title).map(|wf| wf.status_field.clone());
    if let Some(surface) =
        machine_read_model_surface_for_title(db, config, entity, scope(entity).as_deref()).await
    {
        return Some(surface);
    }
    let suffixed = format!("{entity}{}", config.defaults.type_suffix);
    machine_read_model_surface_for_title(db, config, &suffixed, scope(&suffixed).as_deref()).await
}

/// Workflow context for a component: resolution plus the badge markup
/// rendered for the component's markup context. Only collection, details,
/// and form components carry a badge. `machine` is the graph-resolved
/// surface (issue #817): a machine-bearing entity renders its buttons
/// from the read model's `available_transitions` (a runtime each-loop),
/// never from the declared-edge list.
pub(super) fn component_workflow(
    config: &DomainConfig,
    c: &IfmlComponent,
    machine: Option<&EntityMachineSurface>,
) -> Option<RenderWorkflow> {
    if !is_form_component(c) && c.component_type != "details" && !is_collection(c) {
        return None;
    }
    let entity = c.entity.as_deref()?;
    // A machine-bearing entity synthesizes the surface from the KERNEL
    // when no config workflow exists (machine-only entities): badge from
    // `current_state`, terminal states from the IR's final nodes, and the
    // read-model route marker.
    let mut wf = match machine {
        Some(m) if workflow_for_entity(config, entity).is_none() => {
            let ir: serde_json::Value = serde_json::from_str(&m.ir_json).ok()?;
            let states: Vec<String> = ir
                .get("states")
                .and_then(serde_json::Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.get("name").and_then(serde_json::Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            RenderWorkflow {
                status_field: m.status_field.clone(),
                states,
                terminal_states: m.terminal_states.clone(),
                initial_state: m.initial_state.clone(),
                transition_map: HashMap::new(),
                // Machine-only entities have no transition POST endpoint:
                // the buttons stay off (fail closed) until the machine
                // plane grows its own action surface.
                generate_action_endpoints: false,
                machine: Some(render_machine(m, "machine", c)),
                transitions: Vec::new(),
                badge_html: String::new(),
                each: false,
            }
        }
        _ => {
            let mut wf = workflow_for_entity(config, entity)?;
            if let Some(m) = machine {
                // Dual entity (config workflow + kernel machine): the
                // buttons switch to the read model — the UI can never
                // offer a transition the backend would 409.
                wf.machine = Some(render_machine(m, "workflow", c));
            }
            wf
        }
    };
    let value_path = workflow_value_path(c, &wf.status_field);
    wf.badge_html = workflow_badge_html(&c.name, &value_path, &wf.terminal_states);
    wf.each = is_collection(c);
    // Static per-declared-edge buttons render ONLY for the machine-less
    // workflow twin; machine-bearing components render the runtime loop.
    if wf.machine.is_none() && (is_form_component(c) || c.component_type == "details") {
        wf.transitions = render_transitions(&wf, &c.name, &value_path);
    }
    Some(wf)
}

/// The read-model facts the page template renders from (issue #817):
/// the drift hash the UI can detect a stale render with, and the route
/// segment the read model lives under (`machine` route vs the merged
/// `workflow` payload). The transitions expression defaults to the
/// component's markup-context paths (details read the load's `data.item`,
/// forms the form-state derived); the remote plane's substitution pass
/// rewrites it to the data var + live stream.
fn render_machine(m: &EntityMachineSurface, route: &str, c: &IfmlComponent) -> RenderMachine {
    let transitions_expr = if is_form_component(c) {
        format!(
            "{}_form_state.workflow_state?.machine?.available_transitions ?? []",
            sanitize_ident(&c.name)
        )
    } else {
        "data.item?.workflow_state?.machine?.available_transitions ?? []".to_string()
    };
    RenderMachine {
        machine_id: m.machine_id.clone(),
        machine_hash: m.machine_hash.clone(),
        route: route.to_string(),
        transitions_expr,
    }
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

pub(super) fn js_quote(value: &str) -> String {
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
