//! POM (Playwright Object Model) emission for IFML views (issue #317).
//!
//! Two layers, mirroring the entity POM (#316):
//!
//! 1. **Kernel** — `tests/pages/support/base-page.ts` + `ux-table.ts`
//!    ([`super::kernel`]): the `BasePage`/`UxTable` contract is IDENTICAL
//!    to the entity kernel (`templates/ui/test/_pom_kernel.tera`); parity
//!    is pinned there by a unit test against the entity template.
//!
//! 2. **Page classes** — `tests/pages/{view-kebab}-page.ts` per view
//!    container, built from the SAME resolution the route generator renders
//!    from: selectors via `ComponentSelectors`, workflow via
//!    `workflow_for_entity` (the same edge enumeration the transition
//!    buttons render), typed form fields via
//!    `control_core::html_input_for_dsl`, and the ux plan mirror for
//!    collection component objects. Handler parity: the first navigate
//!    event drives the row testid the markup wires to its handler; menu
//!    events drive the actions menu — a page method always exercises the
//!    same named handler function the markup invokes.
//!
//! Spec-driven flow methods (`navigateTo{Target}`, `close{Target}Modal`,
//! `save{Component}`, `expectDenied`) are gated on the view's spec
//! payloads, so schema-less runs emit page classes with NO navigation
//! surface (the render-only contract). Emission itself is spec-infra
//! gated: kernel and page classes are emitted whenever any IFML spec is
//! emitted — never ux-flag gated.
//!
//! Method naming: `open(params)` navigates (NOT `goto` — that would clash
//! with `BasePage`'s raw-path goto under the TS type checker).

use std::collections::HashMap;
use std::path::PathBuf;

use codegraph_config::ux::UxRules;
use codegraph_config::{DomainConfig, IfmlComponentMappings};
use codegraph_naming::{to_kebab_case, to_pascal_case};

use crate::ux::plan::UxPlan;

use super::super::context::{IfmlComponent, IfmlViewContainer};
use super::super::route_generator::workflow_for_entity;
use super::super::selectors::{is_collection, is_details, is_form, ComponentSelectors};
use super::fixtures::{form_spec, view_route};
use super::kernel::{BASE_PAGE_TS, UX_TABLE_TS};
use super::pom_render::render_pom_page;
use super::spec_payload::ViewTestSpec;

// ── Page-class plan ──────────────────────────────────────────────────────────

/// The per-view POM plan: everything
/// [`super::pom_render::render_pom_page`] needs, built from the model +
/// payloads so the builder stays pure and unit-testable.
#[derive(Debug, Default)]
pub(crate) struct ViewPom {
    pub view_name: String,
    pub class_name: String,
    pub route: String,
    /// Label when a heading assertion surface is needed.
    pub heading: Option<String>,
    pub primary_root: Option<String>,
    pub nav_testid: Option<String>,
    pub container_testid: Option<String>,
    /// Denial redirect target — emitted only when persona payloads cover
    /// the guarded view (`expectDenied`).
    pub denial_target: Option<String>,
    pub collections: Vec<PomCollection>,
    pub forms: Vec<PomForm>,
    pub details: Vec<PomDetails>,
    pub flows: Vec<PomFlow>,
}

/// One collection component: row accessor + optional workflow/ux surface.
#[derive(Debug)]
pub(crate) struct PomCollection {
    pub name: String,
    pub pascal: String,
    pub row_testid: String,
    pub workflow: Option<PomWorkflowMethods>,
    /// The ux plan component object (`{name} = new UxTable(...)`) — present
    /// only for unmapped fallback collections whose entity carries a plan.
    pub ux_table: Option<PomUxTable>,
}

#[derive(Debug)]
pub(crate) struct PomWorkflowMethods {
    pub is_collection: bool,
    /// `(state, button testid)` — the same edge enumeration the route
    /// generator's transition buttons render.
    pub transitions: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub(crate) struct PomUxTable {
    pub locale: String,
    pub currency: Option<String>,
}

/// One typed form field (fill mechanics mirror the fallback form markup).
#[derive(Debug)]
pub(crate) struct PomFormField {
    pub name: String,
    pub kind: PomFieldKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PomFieldKind {
    Text,
    Textarea,
    Checkbox,
    Select,
    Radio,
    DatetimeLocal,
}

/// One form component: accessors, typed fills, submit (+ save flow).
#[derive(Debug)]
pub(crate) struct PomForm {
    pub name: String,
    pub pascal: String,
    pub form_testid: String,
    pub submit_testid: String,
    pub fields: Vec<PomFormField>,
    /// Round-trip save navigation `RegExp` source — present only when the
    /// view's round-trip payload exercises it.
    pub save_pattern: Option<String>,
    /// Workflow badge + transition surface (model-derived, markup parity).
    pub workflow: Option<PomWorkflowMethods>,
}

/// One details component: root accessor + per-field getters (`dd` order).
#[derive(Debug)]
pub(crate) struct PomDetails {
    pub name: String,
    pub root_testid: String,
    pub fields: Vec<String>,
    /// Workflow badge + transition surface (model-derived, markup parity).
    pub workflow: Option<PomWorkflowMethods>,
}

/// One navigation flow: row click → `waitForURL` (+ modal close flow).
#[derive(Debug)]
pub(crate) struct PomFlow {
    pub method: String,
    pub row_testid: String,
    pub pattern: String,
    pub close_method: Option<String>,
    pub close_wrapper_testid: Option<String>,
    pub close_testid: Option<String>,
    pub back_pattern: Option<String>,
}

/// Build the POM plan for one view container.
///
/// `spec` is the view's payload (`None` for untested views); `rules` and
/// `plans` carry the resolved ux plane (`None`/empty keeps the ux surface
/// out). Flow methods derive from the click-through payloads, the save
/// method from the round-trip payload, and `expectDenied` from the persona
/// payloads — schema-less runs therefore carry no navigation surface at
/// all. The workflow and ux-object surfaces are MODEL-derived (markup
/// parity: badges/buttons/chips render whenever the config/plan renders
/// them).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_view_pom(
    config: &DomainConfig,
    mappings: Option<&IfmlComponentMappings>,
    vc: &IfmlViewContainer,
    spec: Option<&ViewTestSpec>,
    rules: Option<&UxRules>,
    plans: &HashMap<String, UxPlan>,
) -> ViewPom {
    let mut pom = ViewPom {
        view_name: vc.name.clone(),
        class_name: format!("{}Page", to_pascal_case(&vc.name)),
        route: view_route(&vc.name),
        ..Default::default()
    };
    let selectors_for =
        |c: &IfmlComponent| ComponentSelectors::for_component(mappings, &vc.name, c);

    // Structure surface — only what the render/persona payloads assert.
    if let Some(render) = spec.and_then(|s| s.render.as_ref()) {
        pom.nav_testid = render.nav_testid.clone();
        pom.container_testid = render.container_testid.clone();
        if render.assert_heading {
            pom.heading = Some(render.label.clone());
        }
        pom.primary_root = Some(render.primary_testid.clone());
    }
    if let Some(personas) = spec.map(|s| &s.personas) {
        if pom.heading.is_none() && personas.iter().any(|p| p.permitted && p.assert_heading) {
            pom.heading = Some(vc.label.clone().unwrap_or_else(|| vc.name.clone()));
        }
        if pom.primary_root.is_none() {
            pom.primary_root = personas
                .iter()
                .filter(|p| p.permitted)
                .find_map(|p| p.primary_testid.clone());
        }
        pom.denial_target = personas
            .iter()
            .find(|p| !p.permitted)
            .map(|p| p.denial_target.clone());
    }
    if pom.heading.is_none()
        && pom.primary_root.is_none()
        && spec.is_some_and(|s| !s.personas.is_empty())
    {
        // No testable root at all: persona assertions fall back to the
        // view heading.
        pom.heading = Some(vc.label.clone().unwrap_or_else(|| vc.name.clone()));
    }

    // Collections (model-derived surface + plan-gated ux object).
    for c in &vc.components {
        if !is_collection(c) {
            continue;
        }
        let selectors = selectors_for(c);
        let workflow = c
            .entity
            .as_deref()
            .and_then(|entity| workflow_for_entity(config, entity))
            .map(|wf| pom_workflow_methods(&wf, &c.name, is_collection(c)));
        let ux_table = match rules {
            Some(r) if !is_component_mapped(mappings, vc, c) => c
                .entity
                .as_deref()
                .and_then(|entity| plans.get(entity))
                .map(|plan| PomUxTable {
                    locale: plan.format.locale.clone(),
                    currency: r.format.currency.clone(),
                }),
            _ => None,
        };
        pom.collections.push(PomCollection {
            name: c.name.clone(),
            pascal: to_pascal_case(&c.name),
            row_testid: selectors.row.unwrap_or_default(),
            workflow,
            ux_table,
        });
    }

    // Forms.
    for c in &vc.components {
        if !is_form(c) {
            continue;
        }
        let selectors = selectors_for(c);
        let (Some(form_testid), Some(submit_testid)) =
            (selectors.form.clone(), selectors.submit.clone())
        else {
            continue;
        };
        let save_pattern = spec
            .and_then(|s| s.round_trips.iter().find(|rt| rt.component == c.name))
            .map(|rt| rt.target_pattern.clone());
        let workflow = c
            .entity
            .as_deref()
            .and_then(|entity| workflow_for_entity(config, entity))
            .map(|wf| pom_workflow_methods(&wf, &c.name, is_collection(c)));
        pom.forms.push(PomForm {
            name: c.name.clone(),
            pascal: to_pascal_case(&c.name),
            form_testid,
            submit_testid,
            fields: typed_form_fields(c),
            save_pattern,
            workflow,
        });
    }

    // Details.
    for c in &vc.components {
        if !is_details(c) {
            continue;
        }
        let Some(root_testid) = selectors_for(c).root.clone() else {
            continue;
        };
        let workflow = c
            .entity
            .as_deref()
            .and_then(|entity| workflow_for_entity(config, entity))
            .map(|wf| pom_workflow_methods(&wf, &c.name, is_collection(c)));
        pom.details.push(PomDetails {
            name: c.name.clone(),
            root_testid,
            fields: c.fields.clone(),
            workflow,
        });
    }

    // Flows — payload-gated so schema-less runs carry no navigation
    // surface. Targets appearing more than once disambiguate by source
    // component.
    let mut seen_targets: HashMap<String, usize> = HashMap::new();
    let mut seen_pairs: HashMap<(String, String), usize> = HashMap::new();
    for flow in spec.iter().flat_map(|s| &s.click_throughs) {
        let count = seen_targets.entry(flow.target_view.clone()).or_default();
        *count += 1;
        let suffix = if *count == 1 {
            String::new()
        } else {
            let pair = seen_pairs
                .entry((flow.target_view.clone(), flow.source_component.clone()))
                .or_default();
            *pair += 1;
            if *pair == 1 {
                format!("From{}", to_pascal_case(&flow.source_component))
            } else {
                format!("From{}{}", to_pascal_case(&flow.source_component), pair)
            }
        };
        let (close_method, close_wrapper_testid, close_testid, back_pattern) = match &flow.modal {
            Some(modal) => (
                Some(format!("close{}Modal", to_pascal_case(&flow.target_view))),
                Some(modal.wrapper_testid.clone()),
                Some(modal.close_testid.clone()),
                Some(modal.back_pattern.clone()),
            ),
            None => (None, None, None, None),
        };
        pom.flows.push(PomFlow {
            method: format!("navigateTo{}{}", to_pascal_case(&flow.target_view), suffix),
            row_testid: flow.row_testid.clone(),
            pattern: flow.target_pattern.clone(),
            close_method,
            close_wrapper_testid,
            close_testid,
            back_pattern,
        });
    }
    pom
}

/// Workflow methods for one component: the same edge enumeration the route
/// generator's `render_transitions` uses, so the POM's transition map
/// matches the rendered buttons exactly. The button testids embed the
/// component name (`{component}-transition-{target}`).
fn pom_workflow_methods(
    workflow: &super::super::route_generator::RenderWorkflow,
    component: &str,
    is_collection: bool,
) -> PomWorkflowMethods {
    let mut transitions: Vec<(String, String)> = if workflow.transition_map.is_empty() {
        workflow
            .states
            .iter()
            .filter(|s| !workflow.terminal_states.contains(s))
            .map(|s| {
                (
                    s.clone(),
                    format!("{component}-transition-{}", to_kebab_case(s)),
                )
            })
            .collect()
    } else {
        let mut edges: Vec<(String, String)> = workflow
            .transition_map
            .iter()
            .flat_map(|(from, tos)| tos.iter().map(move |to| (from.clone(), to.clone())))
            .collect();
        edges.sort();
        edges
            .into_iter()
            .map(|(_, to)| {
                (
                    to.clone(),
                    format!("{component}-transition-{}", to_kebab_case(&to)),
                )
            })
            .collect()
    };
    transitions.dedup_by(|a, b| a.0 == b.0);
    PomWorkflowMethods {
        is_collection,
        transitions,
    }
}

fn is_component_mapped(
    mappings: Option<&IfmlComponentMappings>,
    vc: &IfmlViewContainer,
    c: &IfmlComponent,
) -> bool {
    let kind = super::super::selectors::component_kind(c);
    mappings
        .and_then(|m| m.resolve(&vc.name, &c.name, &c.component_type, &kind))
        .is_some()
}

/// First collection component, else first component with a testable root —
/// the same walk the render/persona assembly uses (`(component, root)`).
pub(crate) fn primary_component(
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
) -> Option<(String, String)> {
    let mut fallback = None;
    for c in &vc.components {
        let selectors = ComponentSelectors::for_component(mappings, &vc.name, c);
        let Some(root) = selectors.root else {
            continue;
        };
        if is_collection(c) {
            return Some((c.name.clone(), root));
        }
        if fallback.is_none() {
            fallback = Some((c.name.clone(), root));
        }
    }
    fallback
}

/// Typed fill fields for a form component — the same `html_input_for_dsl`
/// resolution the fallback form markup renders from.
fn typed_form_fields(c: &IfmlComponent) -> Vec<PomFormField> {
    let Some(form) = form_spec(c) else {
        // Untyped form components carry no fields the POM can fill.
        return Vec::new();
    };
    form.fields
        .iter()
        .map(|field| {
            let html = crate::ifml::control_core::html_input_for_dsl(&field.input);
            let kind = if html.is_select {
                PomFieldKind::Select
            } else if html.is_radio {
                PomFieldKind::Radio
            } else if html.is_textarea {
                PomFieldKind::Textarea
            } else if html.input_type == "checkbox" {
                PomFieldKind::Checkbox
            } else if html.input_type == "datetime-local" {
                PomFieldKind::DatetimeLocal
            } else {
                PomFieldKind::Text
            };
            PomFormField {
                name: field.name.clone(),
                kind,
            }
        })
        .collect()
}

/// Kernel + page-class emission for one generation run: the file set the
/// generator pushes when any IFML spec is emitted. `views` carries every
/// view container with its payloads (any of which may be absent).
pub(crate) fn pom_file_set(
    config: &DomainConfig,
    mappings: Option<&IfmlComponentMappings>,
    views: &[(&IfmlViewContainer, Option<&ViewTestSpec>)],
    rules: Option<&UxRules>,
    plans: &HashMap<String, UxPlan>,
) -> Vec<(PathBuf, String)> {
    let mut files = vec![
        (
            PathBuf::from("tests/pages/support/base-page.ts"),
            BASE_PAGE_TS.to_string(),
        ),
        (
            PathBuf::from("tests/pages/support/ux-table.ts"),
            UX_TABLE_TS.to_string(),
        ),
    ];
    for (vc, spec) in views {
        let pom = build_view_pom(config, mappings, vc, *spec, rules, plans);
        let kebab = to_kebab_case(&vc.name);
        files.push((
            PathBuf::from(format!("tests/pages/{kebab}-page.ts")),
            render_pom_page(&pom),
        ));
    }
    files
}

#[cfg(test)]
#[cfg(test)]
mod surface_tests {
    use super::super::super::context::{
        IfmlAction, IfmlComponent, IfmlEvent, IfmlViewContainer, ParameterDef,
    };
    use super::*;
    use codegraph_config::DomainConfig;

    use super::super::fixtures::Fixture;
    use super::super::pom_render::render_pom_page;
    use super::super::spec_payload::{
        ClickThroughTest, PersonaTest, RenderTest, RoundTripTest, ValidationTest, ViewTestSpec,
    };

    fn list_view() -> IfmlViewContainer {
        let navigate = |target: &str| IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: Vec::new(),
            requires: Vec::new(),
            action: IfmlAction::Navigate {
                target: target.to_string(),
                binding: HashMap::new(),
            },
        };
        let grid = IfmlComponent {
            name: "grid".to_string(),
            component_type: "list".to_string(),
            mode: None,
            entity: Some("Customer".to_string()),
            fields: vec!["name".to_string()],
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: vec![navigate("CustomerDetail")],
            parts: Vec::new(),
            spec: None,
        };
        let editor = IfmlComponent {
            name: "editor".to_string(),
            component_type: "form".to_string(),
            mode: None,
            entity: Some("Customer".to_string()),
            fields: vec!["name".to_string()],
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec: Some(
                serde_json::from_str(
                    r#"{"type":"form","value":{"fields":[{"name":"name","input":{"type":"text"},"required":true,"validations":[],"values":[]}]}}"#,
                )
                .unwrap(),
            ),
        };
        let info = IfmlComponent {
            name: "info".to_string(),
            component_type: "details".to_string(),
            mode: None,
            entity: Some("Customer".to_string()),
            fields: vec!["name".to_string(), "email".to_string()],
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec: None,
        };
        IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: Some("Customer Management".to_string()),
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            conditional_expr_json: None,
            conditional_expression: None,
            roles: Vec::new(),
            requires: Vec::new(),
            params: vec![ParameterDef {
                name: "customerId".to_string(),
                type_ref: "Uuid".to_string(),
                default: None,
            }],
            components: vec![grid, editor, info],
            events: Vec::new(),
            containers: Vec::new(),
        }
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
generate_action_endpoints = true

[domains.sales.entity_config.CustomerType.workflow.transitions]
received = ["review"]
"#,
        )
        .unwrap()
    }

    /// The model-derived surface (route/goto, rows, workflow badges +
    /// transition maps, typed form fills, details getters) is present; the
    /// payload-gated surface (flows, save, denial) is ABSENT without
    /// payloads — schema-less runs carry no navigation surface.
    #[test]
    fn page_class_surface_is_plan_and_payload_driven() {
        let config = workflow_config();
        let vc = list_view();

        let bare = build_view_pom(&config, None, &vc, None, None, &HashMap::new());
        let rendered = render_pom_page(&bare);
        assert!(
            rendered.contains("export class CustomerListPage extends BasePage {"),
            "{rendered}"
        );
        assert!(
            rendered.contains("static readonly route = '/customerlist';"),
            "{rendered}"
        );
        assert!(rendered.contains("async open(params"), "{rendered}");
        assert!(!rendered.contains("navigateTo"), "{rendered}");
        assert!(!rendered.contains("waitForURL"), "{rendered}");
        assert!(rendered.contains("gridRow(): Locator"), "{rendered}");
        // Workflow badge + transition map (model-derived, markup parity).
        assert!(rendered.contains("gridStateBadge(): Locator"), "{rendered}");
        assert!(
            rendered.contains("async expectGridState(state: string)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("'review': 'grid-transition-review',"),
            "{rendered}"
        );
        // Form surface: typed fill per the html_input_for_dsl resolution.
        assert!(rendered.contains("editorForm(): Locator"), "{rendered}");
        assert!(
            rendered.contains("editorInput(field: string)"),
            "{rendered}"
        );
        assert!(rendered.contains("editorSubmit(): Locator"), "{rendered}");
        assert!(
            rendered.contains("async fillEditorName(value: string)"),
            "{rendered}"
        );
        assert!(rendered.contains("async submitEditor()"), "{rendered}");
        assert!(!rendered.contains("async saveEditor()"), "{rendered}");
        // Details getters per field (dd order).
        assert!(rendered.contains("infoDetails(): Locator"), "{rendered}");
        assert!(rendered.contains("infoName(): Locator"), "{rendered}");
        assert!(rendered.contains("infoEmail(): Locator"), "{rendered}");

        // Payload-driven surface: round trip → saveEditor; persona denial →
        // expectDenied; click-through → navigateToCustomerDetail.
        let spec = ViewTestSpec {
            view_name: "CustomerList".to_string(),
            label: "Customer Management".to_string(),
            route: "/customerlist".to_string(),
            render: None,
            click_throughs: vec![ClickThroughTest {
                title: "select navigates to CustomerDetail".to_string(),
                source_route: "/customerlist".to_string(),
                source_component: "grid".to_string(),
                target_view: "CustomerDetail".to_string(),
                row_testid: "grid-row".to_string(),
                fixture: Fixture {
                    base_path: "/api/v1/sales/customer".to_string(),
                    entries: vec![],
                },
                target_pattern: "/customerdetail\\?customerId=[^&]+".to_string(),
                modal: None,
            }],
            validations: vec![ValidationTest {
                component: "editor".to_string(),
                route: "/customerlist".to_string(),
                form_testid: "editor-form".to_string(),
                submit_testid: "editor-submit".to_string(),
                field: "name".to_string(),
            }],
            round_trips: vec![RoundTripTest {
                component: "editor".to_string(),
                route: "/customerlist".to_string(),
                id_param: "customerId".to_string(),
                form_testid: "editor-form".to_string(),
                submit_testid: "editor-submit".to_string(),
                field: "name".to_string(),
                original: "'Test name'".to_string(),
                updated: "Updated name".to_string(),
                fixture: Fixture {
                    base_path: "/api/v1/sales/customer".to_string(),
                    entries: vec![],
                },
                target_pattern: "/customerdetail$".to_string(),
            }],
            personas: vec![PersonaTest {
                actor: "Intern".to_string(),
                permitted: false,
                route: "/customerlist".to_string(),
                label: "Customer Management".to_string(),
                assert_heading: false,
                primary_testid: None,
                denial_target: "/customerlist".to_string(),
                capabilities: None,
                control_testid: None,
                control_component: None,
            }],
        };
        let full = build_view_pom(&config, None, &vc, Some(&spec), None, &HashMap::new());
        let rendered = render_pom_page(&full);
        assert!(
            rendered.contains("async navigateToCustomerDetail()"),
            "{rendered}"
        );
        assert!(rendered.contains("async saveEditor()"), "{rendered}");
        assert!(rendered.contains("async expectDenied()"), "{rendered}");
    }

    /// A render payload's structure surface lands on the page class as
    /// named methods (heading/primary/nav/container) — never raw testids.
    #[test]
    fn page_class_carries_the_structure_surface() {
        let config: DomainConfig = toml::from_str(
            r#"
[defaults]
api_version = "v1"

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
entities = []
"#,
        )
        .unwrap();
        let mut vc = list_view();
        vc.components.retain(|c| c.name == "grid");
        vc.params.clear();
        let spec = ViewTestSpec {
            view_name: "CustomerList".to_string(),
            label: "Customer Management".to_string(),
            route: "/customerlist".to_string(),
            render: Some(RenderTest {
                label: "Customer Management".to_string(),
                route: "/customerlist".to_string(),
                primary_testid: "grid-table".to_string(),
                assert_heading: true,
                nav_testid: Some("side-nav".to_string()),
                container_testid: None,
            }),
            click_throughs: Vec::new(),
            validations: Vec::new(),
            round_trips: Vec::new(),
            personas: Vec::new(),
        };
        let pom = build_view_pom(&config, None, &vc, Some(&spec), None, &HashMap::new());
        let rendered = render_pom_page(&pom);
        assert!(rendered.contains("heading(): Locator"), "{rendered}");
        assert!(rendered.contains("primaryRoot(): Locator"), "{rendered}");
        assert!(rendered.contains("navRoot(): Locator"), "{rendered}");
        assert!(!rendered.contains("containerRoot(): Locator"), "{rendered}");
        assert!(!rendered.contains("expectDenied"), "{rendered}");
    }

    /// The `expect` import is required by the workflow surface on ANY
    /// component kind — not just collections (#318 gate finding: a
    /// details-only workflow view rendered `expectInfoState` without the
    /// import and failed svelte-check + Playwright transpile).
    #[test]
    fn details_workflow_imports_expect() {
        let config = workflow_config();
        let mut vc = list_view();
        vc.components.retain(|c| c.component_type == "details");
        let pom = build_view_pom(&config, None, &vc, None, None, &HashMap::new());
        let rendered = render_pom_page(&pom);
        assert!(
            rendered
                .contains("import { expect, type Locator, type Page } from '@playwright/test';"),
            "details workflow must pull in expect:\n{rendered}"
        );
        assert!(rendered.contains("async expectInfoState"), "{rendered}");
    }
}
