//! Per-view spec assembly for the IFML e2e generator (#317 module
//! split): the `build_*` methods that turn the graph model into
//! [`ViewTestSpec`](super::spec_payload::ViewTestSpec) and the sibling
//! workflow/ux payloads. Resolution lives here; emission (specs + POM +
//! stale cleanup) stays in [`super::generator`].

use std::collections::HashMap;

use codegraph_config::ux::{Display, UxRules};
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use super::super::api_paths::id_param_from;
use super::super::context::{
    IfmlAction, IfmlComponent, IfmlModel, IfmlViewContainer, NavigationEdge, PolicyContext,
};
use super::super::route_generator::{
    mapped_container_testid, modal_wrapper_active, modal_wrapper_testid, shell_nav,
    workflow_for_entity, RenderWorkflow,
};
use super::super::selectors::{component_kind, is_collection, is_details, is_form};
use super::fixtures::{
    codelist_fixture_value, escape_regex, fixture_entries, form_spec, js_string, modifiable_field,
    save_navigation_target, schema_backed_api, url_pattern, url_pattern_with_dialog, view_route,
    Fixture,
};
use super::generator::IfmlE2eTestGenerator;
use super::pom::primary_component;
use super::render::{
    ux_chip_checks, ux_column_checks, ux_fixture_entries, ux_menu_event, ux_money_options,
};
use super::spec_payload::{
    pick_transition, ClickThroughTest, ModalCloseAssertions, PersonaTest, RenderTest,
    RoundTripTest, TransitionStep, UxCopyCheck, UxMenuCheck, UxTimelineCheck, UxViewTest,
    ValidationTest, ViewTestSpec, WorkflowTest,
};

impl IfmlE2eTestGenerator {
    pub(super) async fn build_view_spec(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        model: &IfmlModel,
        vc: &IfmlViewContainer,
    ) -> ViewTestSpec {
        let id_param = id_param_from(&vc.params);
        let label = vc.label.clone().unwrap_or_else(|| vc.name.clone());
        let route = view_route(&vc.name);
        let mut spec = ViewTestSpec {
            view_name: vc.name.clone(),
            label,
            route: route.clone(),
            render: None,
            click_throughs: Vec::new(),
            validations: Vec::new(),
            round_trips: Vec::new(),
            personas: Vec::new(),
        };

        // Guarded views redirect unauthenticated visitors, so a plain render
        // test would assert against the denial target; persona tests cover
        // both outcomes with credentials seeded.
        let is_guarded = !vc.roles.is_empty() || !vc.requires.is_empty();
        if id_param.is_none() && !is_guarded {
            let nav_testid = shell_nav(&model.view_containers, self.mappings.as_ref())
                .and_then(|shell| shell.testid);
            spec.render = self.build_render_test(vc, nav_testid);
        }

        for edge in &model.navigation_edges {
            if edge.source_container != vc.name {
                continue;
            }
            if let Some(test) = self
                .build_click_through(db, config, api_version, model, vc, edge)
                .await
            {
                spec.click_throughs.push(test);
            }
        }

        for c in vc.components.iter().filter(|c| is_form(c)) {
            if let Some(test) = self
                .build_validation_test(db, config, api_version, vc, c, id_param.as_deref())
                .await
            {
                spec.validations.push(test);
            }
            if let Some(test) = self
                .build_round_trip_test(db, config, api_version, vc, c, id_param.as_deref())
                .await
            {
                spec.round_trips.push(test);
            }
        }

        spec
    }

    pub(super) fn build_render_test(
        &self,
        vc: &IfmlViewContainer,
        nav_testid: Option<String>,
    ) -> Option<RenderTest> {
        let (_component, root) = primary_component(vc, self.mappings.as_ref())?;
        let is_collection = vc
            .components
            .iter()
            .find(|c| c.name == _component)
            .map(is_collection)
            .unwrap_or(false);
        Some(RenderTest {
            label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
            route: view_route(&vc.name),
            primary_testid: root,
            assert_heading: is_collection,
            nav_testid,
            container_testid: mapped_container_testid(&vc.name, vc.is_xor, self.mappings.as_ref()),
        })
    }

    pub(super) async fn build_click_through(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        model: &IfmlModel,
        vc: &IfmlViewContainer,
        edge: &NavigationEdge,
    ) -> Option<ClickThroughTest> {
        let component_name = edge.source_component.as_deref()?;
        let component = vc
            .components
            .iter()
            .find(|c| c.name == component_name)
            .filter(|c| is_collection(c))?;
        let selectors = self.selectors(vc, component);
        let row_testid = selectors.row.clone()?;

        let entity = component.entity.as_deref()?;
        let api = schema_backed_api(db, config, entity, api_version).await?;
        if !api.has_create {
            return None;
        }

        let target_view = model
            .view_containers
            .iter()
            .find(|v| v.name == edge.target_container);
        let target_modal = target_view
            .is_some_and(|v| modal_wrapper_active(v.is_modal, &v.name, self.mappings.as_ref()));
        let target_pattern = if target_modal {
            url_pattern_with_dialog(&view_route(&edge.target_container), &edge.parameter_binding)
        } else {
            url_pattern(&view_route(&edge.target_container), &edge.parameter_binding)
        };
        let modal = target_modal.then(|| {
            let source_route = view_route(&vc.name);
            ModalCloseAssertions {
                wrapper_testid: modal_wrapper_testid(
                    &edge.target_container,
                    self.mappings.as_ref(),
                ),
                close_testid: format!("{}-modal-close", edge.target_container.to_lowercase()),
                back_pattern: format!("{}$", escape_regex(&source_route)),
            }
        });

        Some(ClickThroughTest {
            title: format!(
                "{} navigates to {}",
                edge.source_event, edge.target_container
            ),
            source_route: view_route(&vc.name),
            source_component: component_name.to_string(),
            target_view: edge.target_container.clone(),
            row_testid,
            fixture: Fixture {
                base_path: api.base_path.clone(),
                entries: fixture_entries(component),
            },
            target_pattern,
            modal,
        })
    }

    pub(super) async fn build_validation_test(
        &self,
        _db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _api_version: &str,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
        _id_param: Option<&str>,
    ) -> Option<ValidationTest> {
        let selectors = self.selectors(vc, c);
        let form_testid = selectors.form.clone()?;
        let submit_testid = selectors.submit.clone()?;
        let form = form_spec(c)?;
        let first_required = form
            .fields
            .iter()
            .find(|f| f.required)
            .map(|f| f.name.clone())?;

        Some(ValidationTest {
            component: c.name.clone(),
            route: view_route(&vc.name),
            form_testid,
            submit_testid,
            field: first_required,
        })
    }

    pub(super) async fn build_round_trip_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
        id_param: Option<&str>,
    ) -> Option<RoundTripTest> {
        let id_param = id_param?;
        let selectors = self.selectors(vc, c);
        let form_testid = selectors.form.clone()?;
        let submit_testid = selectors.submit.clone()?;
        let form = form_spec(c)?;

        let entity = c.entity.as_deref()?;
        let api = schema_backed_api(db, config, entity, api_version).await?;
        if !api.has_create || !api.has_read || !api.has_update {
            return None;
        }

        let (field, original, updated) = modifiable_field(c, form)?;
        let save = save_navigation_target(vc, c)?;

        Some(RoundTripTest {
            component: c.name.clone(),
            route: view_route(&vc.name),
            id_param: id_param.to_string(),
            form_testid,
            submit_testid,
            field,
            original,
            updated,
            fixture: Fixture {
                base_path: api.base_path,
                entries: fixture_entries(c),
            },
            target_pattern: url_pattern(&view_route(&save.0), &save.1),
        })
    }

    /// Workflow state tests for a view: one per fetch-wired component whose
    /// bound entity has a workflow in domain config. Mirrors the fetch
    /// wiring of the route generator's load context — only components that
    /// actually load entity data can show the current state. Schema-backed
    /// only. Guarded views are skipped: the workflow spec navigates without
    /// persona seeding, so the load guard would redirect (access is covered
    /// by the actor-persona tests).
    pub(super) async fn build_view_workflow_tests(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
    ) -> Vec<WorkflowTest> {
        if !vc.roles.is_empty() || !vc.requires.is_empty() {
            return Vec::new();
        }
        let id_param = id_param_from(&vc.params);
        let mut has_list = false;
        let mut has_details = false;
        let mut has_form = false;
        let mut tests = Vec::new();

        for c in &vc.components {
            let is_list = is_collection(c);
            let is_details = is_details(c);
            let is_form = is_form(c);
            let fetch_list = is_list && !has_list;
            let fetch_item = is_details && !has_details && id_param.is_some();
            let fetch_form = is_form && !has_form && id_param.is_some();
            has_list |= fetch_list;
            has_details |= fetch_item;
            has_form |= fetch_form;
            if !fetch_list && !fetch_item && !fetch_form {
                continue;
            }
            if let Some(test) = self
                .build_workflow_test(db, config, api_version, vc, c, id_param.as_deref())
                .await
            {
                tests.push(test);
            }
        }
        tests
    }

    pub(super) async fn build_workflow_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
        id_param: Option<&str>,
    ) -> Option<WorkflowTest> {
        let entity = c.entity.as_deref()?;
        let workflow = workflow_for_entity(config, entity)?;
        let api = schema_backed_api(db, config, entity, api_version).await?;
        if !api.has_create {
            return None;
        }
        if is_collection(c) && !api.has_list {
            return None;
        }
        if (is_details(c) || is_form(c)) && !api.has_read {
            return None;
        }

        // Transition round trip: details/form components carry transition
        // buttons (mapped components render them as siblings), lists stay
        // badge-only. The buttons POST to the generated transition
        // endpoint, which only exists with generate_action_endpoints.
        let transition = if is_collection(c) || !workflow.generate_action_endpoints {
            None
        } else {
            pick_transition(&workflow).map(|(from, to)| TransitionStep {
                from,
                to_testid: format!(
                    "{}-transition-{}",
                    c.name,
                    codegraph_naming::to_kebab_case(&to)
                ),
                to,
            })
        };

        Some(WorkflowTest {
            component_name: c.name.clone(),
            route: view_route(&vc.name),
            id_param: id_param.map(str::to_string),
            state_testid: format!("{}-state", c.name),
            initial_state: workflow.initial_state.clone(),
            is_collection: is_collection(c),
            transition,
            fixture: Fixture {
                base_path: api.base_path,
                entries: self.valid_fixture_entries(db, c, &workflow).await,
            },
        })
    }

    /// Fixture payload for a workflow spec: `fixture_entries` plus valid
    /// values for constrained columns — other codelist-backed fields use
    /// their first enum value (the create would otherwise violate the
    /// column's FK to the codelist table). The workflow status field pins
    /// its payload value to the initial state too; the key is inert on the
    /// wire (the create DTO excludes workflow-managed fields) — the state
    /// itself materializes via the DDL column DEFAULT (issue #311).
    pub(super) async fn valid_fixture_entries(
        &self,
        db: &dyn GraphQuerier,
        c: &IfmlComponent,
        workflow: &RenderWorkflow,
    ) -> Vec<(String, String)> {
        let mut entries = fixture_entries(c);
        for (field, value) in entries.iter_mut() {
            if *field == workflow.status_field {
                *value = format!("'{}'", js_string(&workflow.initial_state));
                continue;
            }
            if let Some(code) = codelist_fixture_value(db, c.entity.as_deref(), field).await {
                *value = format!("'{}'", js_string(&code));
            }
        }
        entries
    }

    /// Actor-persona guard tests for a view: one per human actor, asserting
    /// the permitted outcome (the page renders) or the denied one (redirect
    /// to the denial target). Capability-only views also seed
    /// `__USER_CAPABILITIES__` with the actor's effective capabilities.
    pub(super) fn build_persona_tests(
        &self,
        policy: &PolicyContext,
        human_actors: &[String],
        vc: &IfmlViewContainer,
        denial: &str,
    ) -> Vec<PersonaTest> {
        if vc.roles.is_empty() && vc.requires.is_empty() {
            return Vec::new();
        }
        let (assert_heading, primary_testid) = match primary_component(vc, self.mappings.as_ref()) {
            Some((component, root)) => {
                let is_collection = vc
                    .components
                    .iter()
                    .find(|c| c.name == component)
                    .map(is_collection)
                    .unwrap_or(false);
                (is_collection, Some(root))
            }
            None => (true, None),
        };
        // The page markup gates the submit control behind the same guard
        // checks, so a permitted persona can assert it is visible. Mapped
        // form components own their internals — no control assertion there.
        let control = vc
            .components
            .iter()
            .find(|c| is_form(c))
            .filter(|c| {
                let kind = component_kind(c);
                self.mappings
                    .as_ref()
                    .and_then(|m| m.resolve(&vc.name, &c.name, &c.component_type, &kind))
                    .is_none()
            })
            .and_then(|c| {
                let submit = self.selectors(vc, c).submit;
                submit.map(|testid| (c.name.clone(), testid))
            });
        let control_component = control.as_ref().map(|(name, _)| name.clone());
        let control_testid = control.map(|(_, testid)| testid);
        let capability_only = !vc.requires.is_empty() && vc.roles.is_empty();
        let mut tests = Vec::new();
        for actor in human_actors {
            let caps = policy
                .actors
                .iter()
                .find(|(name, _)| name == actor)
                .map(|(_, caps)| caps.clone())
                .unwrap_or_default();
            let roles_ok = vc.roles.is_empty() || vc.roles.iter().any(|role| role == actor);
            let caps_ok = vc.requires.is_empty() || vc.requires.iter().any(|c| caps.contains(c));
            tests.push(PersonaTest {
                actor: actor.clone(),
                permitted: roles_ok && caps_ok,
                route: view_route(&vc.name),
                label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
                assert_heading,
                primary_testid: primary_testid.clone(),
                denial_target: denial.to_string(),
                capabilities: capability_only.then(|| caps.clone()),
                control_testid: control_testid.clone(),
                control_component: control_component.clone(),
            });
        }
        tests
    }

    /// ux-rules rendering tests for a view (issue #303): one
    /// `{view}.ux.spec.ts` per view carrying an UNMAPPED fallback list
    /// component, asserting the chips / numeric alignment / Intl formatting
    /// / copy-chip / overflow-menu / timeline markup the fallback template
    /// renders when the `ux_rules` plane is active. Gated like the other
    /// API-dependent kinds: schema-backed entity with create+list, unguarded
    /// view, and a live ux plan. Mapped components replace the fallback
    /// markup wholesale, so they never carry ux assertions; typed tables
    /// resolve their columns through a different path and are deferred.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn build_view_ux_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        rules: &UxRules,
        plans: &HashMap<String, crate::ux::plan::UxPlan>,
    ) -> Option<UxViewTest> {
        // Guarded views redirect unauthenticated visitors; the ux spec
        // navigates without persona seeding (persona tests cover access).
        if !vc.roles.is_empty() || !vc.requires.is_empty() {
            return None;
        }
        for c in &vc.components {
            // Spec-less fallback lists only — mapped components replace the
            // fallback wholesale and typed tables project their columns
            // through render_table.
            if !is_collection(c) || c.spec.is_some() {
                continue;
            }
            if self.is_mapped(vc, c) {
                continue;
            }
            let entity = c.entity.as_deref()?;
            let api = schema_backed_api(db, config, entity, api_version).await?;
            if !api.has_create || !api.has_list {
                return None;
            }
            let plan = plans.get(entity)?;
            let entries = ux_fixture_entries(db, c).await;
            let route = view_route(&vc.name);
            let columns = ux_column_checks(c, plan, &entries);
            // The workflow status column pins a chip to the initial state
            // (issue #311): create materializes it via the DDL default on
            // the status column, so the chip is exercisable on created rows.
            let workflow = c
                .entity
                .as_deref()
                .and_then(|entity| workflow_for_entity(config, entity));
            let chip_checks = ux_chip_checks(db, c, plan, &entries, workflow.as_ref()).await;
            if columns.is_empty() && chip_checks.is_empty() {
                // Nothing the fallback markup renders differently under the
                // ux plane — no vacuous spec.
                continue;
            }
            let copy_check = plan.columns.iter().find_map(|(field, col)| {
                (col.display == Display::CopyChip && field != "id" && !field.ends_with("_id")).then(
                    || UxCopyCheck {
                        field: field.clone(),
                    },
                )
            });
            let menu = ux_menu_event(&c.events).and_then(|e| match &e.action {
                IfmlAction::Navigate { target, binding } => Some(UxMenuCheck {
                    trigger_testid: format!("{}-actions", c.name),
                    menu_testid: format!("{}-actions-menu", c.name),
                    target_pattern: url_pattern(&view_route(target), binding),
                }),
                _ => None,
            });
            let timeline = match &plan.collection {
                crate::ux::plan::CollectionPlan::Timeline { order_by, .. } => {
                    Some(UxTimelineCheck {
                        root_testid: format!("{}-timeline", c.name),
                        item_testid: format!("{}-timeline-item", c.name),
                        order_literal: entries
                            .iter()
                            .find(|(field, _)| field == order_by)
                            .map(|(_, value)| super::fixtures::js_literal_inner(value)),
                    })
                }
                crate::ux::plan::CollectionPlan::Table => None,
            };
            return Some(UxViewTest {
                component_name: c.name.clone(),
                route,
                fixture: Fixture {
                    base_path: api.base_path,
                    entries,
                },
                table_testid: format!("{}-table", c.name),
                row_testid: format!("{}-row", c.name),
                chip_checks,
                column_checks: columns,
                copy_check,
                menu,
                timeline,
                locale: rules.format.locale.clone(),
                money_options: ux_money_options(rules),
            });
        }
        None
    }

    /// Whether the component resolves to a whole-component mapping (the
    /// same resolution the route generator uses to replace the fallback).
    pub(super) fn is_mapped(&self, vc: &IfmlViewContainer, c: &IfmlComponent) -> bool {
        let kind = component_kind(c);
        self.mappings
            .as_ref()
            .and_then(|m| m.resolve(&vc.name, &c.name, &c.component_type, &kind))
            .is_some()
    }
}
