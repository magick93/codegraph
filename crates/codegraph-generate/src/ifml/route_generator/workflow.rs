use codegraph_config::DomainConfig;

use crate::ifml::context::IfmlComponent;

use super::context::{RenderTransition, RenderWorkflow};
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
pub(super) fn component_workflow(
    config: &DomainConfig,
    c: &IfmlComponent,
) -> Option<RenderWorkflow> {
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
