//! Per-view spec assembly for the IFML e2e generator (#317 module
//! split): the `build_*` methods that turn the graph model into
//! [`ViewTestSpec`](super::spec_payload::ViewTestSpec) and the sibling
//! workflow/ux payloads. Resolution lives here; emission (specs + POM +
//! stale cleanup) stays in [`super::generator`].

use std::collections::HashMap;

use codegraph_config::DomainConfig;
use codegraph_config::ux::{Display, UxRules};
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::IfmlManifestNode;
use codegraph_naming::{to_kebab_case, to_pascal_case, to_snake_case};

use super::super::api_paths::id_param_from;
use super::super::context::{
    IfmlAction, IfmlComponent, IfmlModel, IfmlViewContainer, NavigationEdge, PolicyContext,
};
use super::super::route_generator::{
    RenderWorkflow, mapped_container_testid, modal_wrapper_active, modal_wrapper_testid, shell_nav,
    workflow_for_entity,
};
use super::super::selectors::{component_kind, is_collection, is_details, is_form};
use super::fixtures::{
    Fixture, codelist_fixture_value, escape_regex, fixture_entries, form_spec, js_string,
    modifiable_field, save_navigation_target, schema_backed_api, schema_for_entity, url_pattern,
    url_pattern_with_dialog, view_route,
};
use super::generator::IfmlE2eTestGenerator;
use super::pom::primary_component;
use super::render::{
    ux_chip_checks, ux_column_checks, ux_fixture_entries, ux_menu_event, ux_money_options,
};
use super::spec_payload::{
    ClickThroughTest, JourneyHop, JourneyHopKind, JourneyPath, JourneyStep, ModalCloseAssertions,
    PersonaTest, RenderTest, RoundTripTest, TransitionStep, UxCopyCheck, UxMenuCheck,
    UxTimelineCheck, UxViewTest, ValidationTest, ViewJourney, ViewTestSpec, WorkflowTest,
    pick_transition,
};
use crate::ddd::service_ops::{self, ExecutionKind, ServiceOpRoute};

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
            fixture: self
                .attach_required_refs(
                    db,
                    config,
                    api_version,
                    component.entity.as_deref(),
                    Fixture {
                        casing: self.dto_casing,
                        base_path: api.base_path.clone(),
                        entries: self.typed_fixture_entries(db, config, component).await,
                        deps: Vec::new(),
                    },
                )
                .await,
            target_pattern,
            modal,
        })
    }

    /// Fixture payload with type-valid values (issue #558): codelist-backed
    /// fields seed their first enum value — prose would violate the
    /// column's FK to the codelist table.
    pub(super) async fn typed_fixture_entries(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        c: &IfmlComponent,
    ) -> Vec<(String, String)> {
        let mut entries = fixture_entries(c);
        for (field, value) in entries.iter_mut() {
            if let Some(code) = codelist_fixture_value(db, c.entity.as_deref(), field).await {
                *value = format!("'{}'", js_string(&code));
            }
        }
        let status = c
            .entity
            .as_deref()
            .and_then(|entity| workflow_for_entity(config, entity))
            .map(|w| w.status_field);
        let entries = super::fixtures::strip_workflow_status(entries, status.as_deref());
        super::fixtures::with_required_entries(
            db,
            config,
            c.entity.as_deref(),
            entries,
            self.dto_casing,
        )
        .await
    }

    /// Attach the required-FK parent seeds to a fixture (P1): parents are
    /// created through their own API before the child and their id is wired
    /// into the child body.
    async fn attach_required_refs(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        entity: Option<&str>,
        fixture: Fixture,
    ) -> Fixture {
        super::fixtures::add_required_refs(db, config, api_version, entity, fixture).await
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
        let first_required = form.fields.iter().find(|f| f.required)?;
        let field = first_required.name.clone();
        // The first required field's positional validation message drives
        // the TanStack per-field visibility assertion (issue #580).
        let message = first_required.messages.first().cloned();

        Some(ValidationTest {
            component: c.name.clone(),
            route: view_route(&vc.name),
            form_testid,
            submit_testid,
            field,
            message,
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
            fixture: self
                .attach_required_refs(
                    db,
                    config,
                    api_version,
                    c.entity.as_deref(),
                    Fixture {
                        casing: self.dto_casing,
                        base_path: api.base_path,
                        entries: self.typed_fixture_entries(db, config, c).await,
                        deps: Vec::new(),
                    },
                )
                .await,
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
            fixture: self
                .attach_required_refs(
                    db,
                    config,
                    api_version,
                    c.entity.as_deref(),
                    Fixture {
                        casing: self.dto_casing,
                        base_path: api.base_path,
                        entries: self.valid_fixture_entries(db, config, c, &workflow).await,
                        deps: Vec::new(),
                    },
                )
                .await,
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
        config: &DomainConfig,
        c: &IfmlComponent,
        workflow: &RenderWorkflow,
    ) -> Vec<(String, String)> {
        let entries = fixture_entries(c);
        let entries = super::fixtures::strip_workflow_status(entries, Some(&workflow.status_field));
        let mut entries = entries;
        for (field, value) in entries.iter_mut() {
            if let Some(code) = codelist_fixture_value(db, c.entity.as_deref(), field).await {
                *value = format!("'{}'", js_string(&code));
            }
        }
        super::fixtures::with_required_entries(
            db,
            config,
            c.entity.as_deref(),
            entries,
            self.dto_casing,
        )
        .await
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
            // View role tokens may be authored as the actor name verbatim
            // or as its snake_case form (crewbase's `roles: [member]` vs
            // actor `Member`) — match either, mirroring the hydration
            // convention.
            let roles_ok = vc.roles.is_empty()
                || vc
                    .roles
                    .iter()
                    .any(|role| super::super::route_generator::role_token_matches(role, actor));
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
    /// Required-FK entities keep their spec with the parent seeds attached
    /// (`add_required_refs`, the click-through family's machinery) — the
    /// assertion checks computed below see only the PRE-dep entries, so
    /// seeded FK columns never leak into chip/column/copy/timeline pins.
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
            let status_field = workflow_for_entity(config, entity).map(|w| w.status_field);
            let entries =
                ux_fixture_entries(db, config, c, status_field.as_deref(), self.dto_casing).await;
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
            let fixture = self
                .attach_required_refs(
                    db,
                    config,
                    api_version,
                    c.entity.as_deref(),
                    Fixture {
                        casing: self.dto_casing,
                        base_path: api.base_path,
                        entries,
                        deps: Vec::new(),
                    },
                )
                .await;
            return Some(UxViewTest {
                component_name: c.name.clone(),
                route,
                fixture,
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

    // ── Navigation journeys (issue #468 W5) ──────────────────────────────

    /// The `{view}.journey.spec.ts` payloads for one model (`ifml_e2e_auth`
    /// gated — the caller passes personas only when the flag is on): for
    /// every view with drivable outgoing navigation, the maximal navigate()
    /// paths as multi-hop journeys. A hop is drivable when the event's
    /// control exists in the generated markup AND the bound entity is
    /// schema-backed (rows to click, a submit that navigates): collections'
    /// first navigate events drive their row, forms' save/submit events
    /// drive the submit (fixture fills first). Row-menu disclosures,
    /// cancel/back buttons, and control-less events (details/chart) render
    /// no fallback control and are skipped. Paths a persona cannot walk
    /// (a guarded view it lacks) are dropped; a view whose paths all drop
    /// emits no journey.
    pub(super) async fn build_view_journeys(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        model: &IfmlModel,
        personas: &[super::auth::PersonaSpec],
    ) -> Vec<ViewJourney> {
        if personas.is_empty() {
            return Vec::new();
        }
        // The hop graph: per view, the drivable outgoing hops in model order.
        let mut hops_by_view: HashMap<String, Vec<JourneyHop>> = HashMap::new();
        for vc in &model.view_containers {
            let hops = self
                .journey_hops_for_view(db, config, api_version, model, vc)
                .await;
            hops_by_view.insert(vc.name.clone(), hops);
        }

        let mut journeys = Vec::new();
        for vc in &model.view_containers {
            let empty = Vec::new();
            let hops = hops_by_view.get(&vc.name).unwrap_or(&empty);
            if hops.is_empty() {
                continue;
            }
            let Some((open_param, open_seed)) =
                self.journey_open_seed(db, config, api_version, vc).await
            else {
                // A param'd source view without a seedable entity cannot be
                // opened reliably — no journey.
                continue;
            };
            let mut paths = journey_paths(&vc.name, &hops_by_view);
            // Persona gating: keep only paths some persona may walk end to
            // end, then resolve the driving persona per path.
            paths.retain(|(views, _)| path_persona(personas, views, model).is_some());
            if paths.is_empty() {
                continue;
            }
            // Collision-suffix the hop methods per source view (the same
            // scheme the flow methods use) and freeze them into the steps.
            let mut seen_targets: HashMap<String, usize> = HashMap::new();
            let mut seen_pairs: HashMap<(String, String), usize> = HashMap::new();
            let mut seeds: Vec<Fixture> = Vec::new();
            let paths = paths
                .into_iter()
                .map(|(views, hops)| {
                    let steps = hops
                        .into_iter()
                        .map(|hop| {
                            let count = seen_targets.entry(hop.target_view.clone()).or_default();
                            *count += 1;
                            let suffix = if *count == 1 {
                                String::new()
                            } else {
                                let pair = seen_pairs
                                    .entry((hop.target_view.clone(), hop.source_component.clone()))
                                    .or_default();
                                *pair += 1;
                                if *pair == 1 {
                                    format!("From{}", to_pascal_case(&hop.source_component))
                                } else {
                                    format!("From{}{}", to_pascal_case(&hop.source_component), pair)
                                }
                            };
                            if let Some(seed) = &hop.seed {
                                let already = seeds
                                    .iter()
                                    .any(|s: &Fixture| s.base_path == seed.base_path);
                                if !already {
                                    seeds.push(seed.clone());
                                }
                            }
                            let target_assert_heading = view_primary_is_collection(
                                model,
                                &hop.target_view,
                                self.mappings.as_ref(),
                            );
                            JourneyStep {
                                method: format!(
                                    "journeyTo{}{}",
                                    to_pascal_case(&hop.target_view),
                                    suffix
                                ),
                                hop,
                                arrival: false,
                                arrival_heading: target_assert_heading,
                                target_steps: Vec::new(),
                            }
                        })
                        .collect();
                    JourneyPath { views, steps }
                })
                .collect();
            journeys.push(ViewJourney {
                view_name: vc.name.clone(),
                label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
                persona: None,
                open_param,
                open_seed,
                seeds,
                paths,
            });
        }
        // One persona per JOURNEY (all its paths must share the asPersona
        // describe): the first persona permitted on every view of every
        // path; journeys without one are dropped.
        journeys.retain_mut(|journey| {
            let all_views: Vec<String> = journey
                .paths
                .iter()
                .flat_map(|path| path.views.iter().cloned())
                .collect();
            journey.persona = path_persona(personas, &all_views, model);
            journey.persona.is_some()
        });
        journeys
    }

    /// The drivable outgoing hops for one view (see
    /// [`IfmlE2eTestGenerator::build_view_journeys`]).
    async fn journey_hops_for_view(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        model: &IfmlModel,
        vc: &IfmlViewContainer,
    ) -> Vec<JourneyHop> {
        let mut hops = Vec::new();
        for c in walk_components(vc) {
            let selectors = self.selectors(vc, c);
            // The fallback markup wires a collection's FIRST navigate event
            // (the row click); later ones disclose into the row menu.
            let mut first_navigate_used = false;
            for event in &c.events {
                let IfmlAction::Navigate { target, binding } = &event.action else {
                    continue;
                };
                let Some(target_vc) = model.view_containers.iter().find(|v| &v.name == target)
                else {
                    continue;
                };
                if target_vc.is_modal {
                    // Modal flows stay click-through territory (close-flow
                    // assertions live there).
                    continue;
                }
                let save_or_submit = matches!(event.event_type.as_str(), "save" | "submit");
                let pattern = url_pattern(&view_route(target), binding);
                let hop = if is_collection(c) && !save_or_submit {
                    if first_navigate_used {
                        continue;
                    }
                    first_navigate_used = true;
                    if selectors.row.is_none() {
                        // No row slot on the component — nothing to click.
                        continue;
                    }
                    let Some(entity) = c.entity.as_deref() else {
                        continue;
                    };
                    let Some(api) = schema_backed_api(db, config, entity, api_version).await else {
                        continue;
                    };
                    if !api.has_create {
                        continue;
                    }
                    JourneyHop {
                        source_component: c.name.clone(),
                        kind: JourneyHopKind::Row,
                        target_view: target.clone(),
                        target_pattern: pattern,
                        seed: Some(
                            self.attach_required_refs(
                                db,
                                config,
                                api_version,
                                c.entity.as_deref(),
                                Fixture {
                                    casing: self.dto_casing,
                                    base_path: api.base_path,
                                    entries: self.valid_journey_entries(db, config, c).await,
                                    deps: Vec::new(),
                                },
                            )
                            .await,
                        ),
                        fills: Vec::new(),
                    }
                } else if is_form(c) && save_or_submit {
                    if selectors.submit.is_none() {
                        // No submit control on the component — nothing to
                        // click.
                        continue;
                    }
                    let Some(entity) = c.entity.as_deref() else {
                        continue;
                    };
                    let Some(api) = schema_backed_api(db, config, entity, api_version).await else {
                        continue;
                    };
                    if !api.has_create && !api.has_update {
                        continue;
                    }
                    // The submit control only renders once the page's form
                    // handler exists (api-backed); required fields must be
                    // filled first — one fill per typed field with a fixture
                    // value. The POM fill methods type the DOM value as
                    // `string`, so the fixture's typed JS literals (`42`,
                    // `true`, the random-UUID template) become their string
                    // forms here (the svelte-check leg of the gate caught
                    // the raw literals — the journey plane never typechecked
                    // before the gate threaded the auth flag).
                    let entries = self.valid_journey_entries(db, config, c).await;
                    let fills = super::fixtures::form_spec(c)
                        .map(|form| {
                            form.fields
                                .iter()
                                .filter_map(|field| {
                                    let literal = entries
                                        .iter()
                                        .find(|(name, _)| name == &field.name)
                                        .map(|(_, value)| pom_fill_literal(value))?;
                                    Some((
                                        format!(
                                            "fill{}{}",
                                            to_pascal_case(&c.name),
                                            to_pascal_case(&field.name)
                                        ),
                                        literal,
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    JourneyHop {
                        source_component: c.name.clone(),
                        kind: JourneyHopKind::Submit,
                        target_view: target.clone(),
                        target_pattern: pattern,
                        seed: None,
                        fills,
                    }
                } else {
                    // Row-menu disclosure, cancel/back button (mapped markup
                    // only), or a control-less component's event.
                    continue;
                };
                hops.push(hop);
            }
        }
        hops
    }

    /// The open seed for a journey source view: the view's id param plus a
    /// fixture from its first schema-backed component (the param'd view
    /// fetches by id before anything is drivable). `None` for param-less
    /// views (opened plain) and unseedable param'd views.
    pub(super) async fn journey_open_seed(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
    ) -> Option<(Option<String>, Option<Fixture>)> {
        let Some(param) = id_param_from(&vc.params) else {
            return Some((None, None));
        };
        for c in walk_components(vc) {
            let Some(entity) = c.entity.as_deref() else {
                continue;
            };
            let Some(api) = schema_backed_api(db, config, entity, api_version).await else {
                continue;
            };
            if !api.has_create {
                continue;
            }
            return Some((
                Some(param.to_string()),
                Some(
                    self.attach_required_refs(
                        db,
                        config,
                        api_version,
                        c.entity.as_deref(),
                        Fixture {
                            casing: self.dto_casing,
                            base_path: api.base_path,
                            entries: self.valid_journey_entries(db, config, c).await,
                            deps: Vec::new(),
                        },
                    )
                    .await,
                ),
            ));
        }
        None
    }

    /// Fixture payload for a journey seed/fill: `fixture_entries` plus
    /// valid first-enum values for codelist-backed fields (a seed create
    /// must not violate a codelist FK). Unlike the workflow pinning, no
    /// status special-case: the create DTO ignores it anyway.
    pub(super) async fn valid_journey_entries(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        c: &IfmlComponent,
    ) -> Vec<(String, String)> {
        let mut entries = fixture_entries(c);
        for (field, value) in entries.iter_mut() {
            if let Some(code) = codelist_fixture_value(db, c.entity.as_deref(), field).await {
                *value = format!("'{}'", js_string(&code));
            }
        }
        let status = c
            .entity
            .as_deref()
            .and_then(|entity| workflow_for_entity(config, entity))
            .map(|w| w.status_field);
        let entries = super::fixtures::strip_workflow_status(entries, status.as_deref());
        super::fixtures::with_required_entries(
            db,
            config,
            c.entity.as_deref(),
            entries,
            self.dto_casing,
        )
        .await
    }
}

/// The POM fill methods type the DOM value as `string`: a typed JS fixture
/// literal (`42`, `true`) becomes its quoted form, and the random-UUID
/// template literal (an API-payload nicety) collapses to a static valid
/// UUID — a form fill needs *a* valid value, not a fresh one.
fn pom_fill_literal(literal: &str) -> String {
    if literal.starts_with('\'') {
        literal.to_string()
    } else if literal.starts_with('`') {
        "'00000000-0000-4000-8000-000000000000'".to_string()
    } else {
        format!("'{literal}'")
    }
}

/// Resolve each journey step's arrival assertions against the target view's
/// POM plan (which surfaces the page class actually exposes) and collect
/// the xor wizard step names of xor targets. Called after the POM plans are
/// built; journeys render last.
pub(super) fn resolve_journey_arrivals(
    journeys: &mut [ViewJourney],
    pom_by_view: &HashMap<String, &super::pom::ViewPom>,
) {
    for journey in journeys {
        for path in &mut journey.paths {
            for step in &mut path.steps {
                let kebab = to_kebab_case(&step.hop.target_view);
                let Some(pom) = pom_by_view.get(&kebab) else {
                    continue;
                };
                // The render-spec convention: collection targets assert
                // their heading, everything else the primary root — but only
                // surfaces the target page class actually exposes.
                if step.arrival_heading && pom.heading.is_some() {
                    step.arrival = true;
                } else if pom.primary_root.is_some() {
                    step.arrival = true;
                    step.arrival_heading = false;
                } else if pom.heading.is_some() {
                    step.arrival = true;
                    step.arrival_heading = true;
                } else {
                    step.arrival = false;
                    step.arrival_heading = false;
                }
                step.target_steps = pom.steps.iter().map(|s| s.name.clone()).collect();
            }
        }
    }
}

/// The view container named `name`, if it is a top-level view.
fn target_of<'m>(model: &'m IfmlModel, name: &str) -> Option<&'m IfmlViewContainer> {
    model.view_containers.iter().find(|vc| vc.name == name)
}

/// Whether the view's primary component is a collection — the render-spec
/// heading convention for arrival assertions.
fn view_primary_is_collection(
    model: &IfmlModel,
    name: &str,
    mappings: Option<&codegraph_config::IfmlComponentMappings>,
) -> bool {
    let Some(vc) = target_of(model, name) else {
        return false;
    };
    let Some((component, _)) = primary_component(vc, mappings) else {
        return false;
    };
    vc.components
        .iter()
        .find(|c| c.name == component)
        .map(is_collection)
        .unwrap_or(false)
}

/// A view's components including those nested in its containers (their
/// events wire the same controls).
fn walk_components(vc: &IfmlViewContainer) -> Vec<&IfmlComponent> {
    fn walk<'a>(vc: &'a IfmlViewContainer, out: &mut Vec<&'a IfmlComponent>) {
        out.extend(&vc.components);
        for nested in &vc.containers {
            walk(nested, out);
        }
    }
    let mut out = Vec::new();
    walk(vc, &mut out);
    out
}

/// Maximal chain depth for a journey path (issue #468 W5): deeper walks
/// explode combinatorially and read badly.
const JOURNEY_MAX_DEPTH: usize = 4;

/// The maximal navigate() paths from `source` over the hop graph: a DFS
/// that only records leaves, so prefixes are deduped by construction (A→B
/// never renders beside A→B→C), cycles never repeat a view, and chains cap
/// at [`JOURNEY_MAX_DEPTH`] hops.
fn journey_paths(
    source: &str,
    hops_by_view: &HashMap<String, Vec<JourneyHop>>,
) -> Vec<(Vec<String>, Vec<JourneyHop>)> {
    fn walk(
        views: &mut Vec<String>,
        steps: &mut Vec<JourneyHop>,
        out: &mut Vec<(Vec<String>, Vec<JourneyHop>)>,
        hops_by_view: &HashMap<String, Vec<JourneyHop>>,
    ) {
        let current = views.last().cloned().unwrap_or_default();
        let mut extended = false;
        if steps.len() < JOURNEY_MAX_DEPTH
            && let Some(hops) = hops_by_view.get(&current)
        {
            for hop in hops {
                if views.contains(&hop.target_view) {
                    continue;
                }
                extended = true;
                views.push(hop.target_view.clone());
                steps.push(hop.clone());
                walk(views, steps, out, hops_by_view);
                views.pop();
                steps.pop();
            }
        }
        if !extended && !steps.is_empty() {
            out.push((views.clone(), steps.clone()));
        }
    }
    let mut out = Vec::new();
    walk(
        &mut vec![source.to_string()],
        &mut Vec::new(),
        &mut out,
        hops_by_view,
    );
    out
}

/// The first persona permitted on EVERY view of the walk (roles and
/// capabilities, the same check the route guards render from).
fn path_persona(
    personas: &[super::auth::PersonaSpec],
    views: &[String],
    model: &IfmlModel,
) -> Option<String> {
    personas
        .iter()
        .find(|p| {
            views.iter().all(|name| {
                let Some(vc) = target_of(model, name) else {
                    return true;
                };
                let roles_ok = vc.roles.is_empty()
                    || vc.roles.iter().any(|role| {
                        super::super::route_generator::role_token_matches(role, &p.actor)
                    });
                let caps_ok = vc.requires.is_empty()
                    || vc.requires.iter().any(|cap| p.capabilities.contains(cap));
                roles_ok && caps_ok
            })
        })
        .map(|p| p.id.clone())
}

// ── WS6: the action-op plane (manifest → POM actions + action specs) ────────

impl IfmlE2eTestGenerator {
    /// Build one view's action spec from the folded manifest (WS6): every
    /// `kind: action` edge whose component binds a schema-backed entity AND
    /// whose action target resolves to a MOUNTED service-op route
    /// (`routes_for_entity`, the same surface the API plane mounts) yields
    /// one `ActionTest` with the legs the route provably supports.
    ///
    /// Resolution gates (each miss drops the edge silently — no invented
    /// bindings): no manifest node (pre-WS6 graph), edge source not this
    /// view, component not in the graph model (pattern-module-contributed
    /// controls), entity/schema/API unresolved, no view id param, no
    /// matching route, unplannable legs. `None` when no edge survives —
    /// the spec file is not emitted (payload gating).
    pub(super) async fn build_action_spec(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        manifest: Option<&IfmlManifestNode>,
    ) -> Option<(Vec<super::pom::PomAction>, super::spec_payload::ActionSpec)> {
        use super::spec_payload::{ActionAct, ActionTest};
        use rex_ir::ifml_manifest::{ManifestEdgeKind, ManifestEdgeSource};

        let manifest = manifest?;
        let manifest: rex_ir::ifml_manifest::IfmlManifest =
            serde_json::from_str(&manifest.model_json).ok()?;
        let id_param = id_param_from(&vc.params)?;

        // The view's action edges, deterministic (manifest order — the
        // derivation walks the expanded tree in declaration order).
        let mut tests: Vec<ActionTest> = Vec::new();
        let mut pom_actions: Vec<super::pom::PomAction> = Vec::new();
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for edge in &manifest.edges {
            if edge.kind != ManifestEdgeKind::Action {
                continue;
            }
            let ManifestEdgeSource::View { view, component } = &edge.source else {
                continue;
            };
            if view != &vc.name {
                continue;
            }
            let Some(component_name) = component else {
                continue;
            };
            let Some(component) = vc.components.iter().find(|c| &c.name == component_name) else {
                continue;
            };
            let entity = component.entity.as_deref()?;
            let Some(api) = schema_backed_api(db, config, entity, api_version).await else {
                continue;
            };
            if !api.has_read && !api.has_update && !api.has_delete {
                continue;
            }
            let Some(schema) = schema_for_entity(db, entity).await else {
                continue;
            };
            let Some(domain) = entity_domain(db, config, &schema.title).await else {
                continue;
            };
            let op_snake = to_snake_case(&edge.target);
            if !seen.insert((component_name.clone(), op_snake.clone())) {
                continue;
            }
            let routes =
                service_ops::routes_for_entity(db, config, &domain, &schema.title, self.dto_casing)
                    .await
                    .unwrap_or_default();
            let Some(route) = routes.iter().find(|r| r.snake == op_snake) else {
                // The action names no mounted op — the dead-trigger scan
                // (Ask 1) owns the warning; here it stays unbuilt.
                continue;
            };
            let legs = plan_action_legs(db, &schema.title, route).await;
            if legs.is_empty() {
                continue;
            }
            let acts: Vec<ActionAct> = legs
                .into_iter()
                .map(|(kind, round_trip)| ActionAct { kind, round_trip })
                .collect();
            let statuses = acts
                .iter()
                .map(|a| a.kind.status())
                .collect::<Vec<_>>()
                .join(" then ");
            tests.push(ActionTest {
                title: format!("action {op_snake} on {}: {statuses}", component_name),
                component: component_name.clone(),
                op_snake,
                fixture: self
                    .attach_required_refs(
                        db,
                        config,
                        api_version,
                        component.entity.as_deref(),
                        Fixture {
                            casing: self.dto_casing,
                            base_path: api.base_path.clone(),
                            entries: self.typed_fixture_entries(db, config, component).await,
                            deps: Vec::new(),
                        },
                    )
                    .await,
                acts,
            });
            pom_actions.push(super::pom::PomAction {
                component: component_name.clone(),
                pascal: to_pascal_case(component_name),
                op_snake: route.snake.clone(),
                op_kebab: to_kebab_case(&edge.target),
            });
        }
        if tests.is_empty() {
            return None;
        }
        Some((
            pom_actions,
            super::spec_payload::ActionSpec {
                view_name: vc.name.clone(),
                label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
                route: view_route(&vc.name),
                id_param: Some(id_param),
                tests,
            },
        ))
    }
}

/// The action legs one route provably supports (WS6). Every leg must be
/// SOUND — the drive sequence must deterministically produce the asserted
/// outcome for any conforming app. Unplannable shapes return an empty vec
/// (the action stays unbuilt rather than guessed):
///
/// - `Seam` → `[501]` (the seam answers 501 naming the gap, any state).
/// - `Read`/`Delete` → `[200]` (read returns the record; delete removes it).
/// - `Update` without a `when` guard → `[200]`, plus `[409]` when the op's
///   transitions all land on machine TERMINAL states (re-acting from the
///   target state then provably conflicts).
/// - `Update` WITH a `when` guard over the machine field → the guard admits
///   the machine's initial state, the first act passes the guard and moves
///   the record → `[200, 403]` (the re-act denies; authorize runs before
///   the conflict check). A guard that doesn't admit the initial state, or
///   whose shape the conservative scan cannot read, stays unbuilt.
async fn plan_action_legs(
    db: &dyn GraphQuerier,
    entity_title: &str,
    route: &ServiceOpRoute,
) -> Vec<(super::spec_payload::ActionActKind, Option<(String, String)>)> {
    use super::spec_payload::ActionActKind;

    match &route.execution {
        ExecutionKind::Seam { .. } => {
            return vec![(ActionActKind::NotImplemented, None)];
        }
        ExecutionKind::Read | ExecutionKind::Delete => {
            return vec![(ActionActKind::Ok, None)];
        }
        ExecutionKind::Update { .. } => {}
    }

    let Some(machine) = machine_for_entity(db, entity_title).await else {
        // No machine: an unbound patch answers 200 (an empty body is an
        // idempotent read), deterministically.
        if route.authorize_fn.is_none() {
            return vec![(ActionActKind::Ok, None)];
        }
        // A `when` guard without a machine cannot be driven to flip — skip.
        return Vec::new();
    };
    let initial = machine.initial.clone();
    let terminals: std::collections::HashSet<&str> =
        machine.terminals.iter().map(String::as_str).collect();

    // The op-level `when` guard's source states, read conservatively off
    // the DSL condition text (the same `field == State` shape the rexlang
    // guard typecheck admits): equality comparisons against declared
    // machine states. Anything else refuses to plan.
    let condition = operation_condition(db, entity_title, &route.op_name).await;
    let guard_sources: Option<Vec<String>> = condition
        .as_ref()
        .map(|condition| machine_condition_source_states(condition, &machine.field, &machine));

    // The op's transitions (the route plane already selected them).
    let transitions = match &route.execution {
        ExecutionKind::Update { state } => state
            .as_ref()
            .map(|s| s.op.as_slice())
            .unwrap_or_default()
            .to_vec(),
        _ => Vec::new(),
    };

    if route.authorize_fn.is_some() {
        // 200 + 403 requires: the guard admits the initial state (the
        // first act passes), and the act MOVES the machine (the re-act
        // then fails the guard).
        let admits_initial = match &guard_sources {
            Some(sources) => sources.contains(&initial),
            None => false,
        };
        if admits_initial && !transitions.is_empty() {
            let round_trip = ok_round_trip_check(route);
            return vec![
                (ActionActKind::Ok, round_trip),
                (ActionActKind::Forbidden, None),
            ];
        }
        return Vec::new();
    }

    // Unguarded update: 200 always; 409 when re-acting provably conflicts.
    let mut legs = vec![(ActionActKind::Ok, ok_round_trip_check(route))];
    if !transitions.is_empty()
        && transitions
            .iter()
            .all(|t| terminals.contains(t.to.as_str()))
    {
        legs.push((ActionActKind::Conflict, None));
    }
    legs
}

/// The `field, expected` round-trip the ok leg asserts through the seeded
/// record's GET: the machine field moving to the op's transition target
/// (`Some` only when every transition shares one target — a single
/// deterministic landing state).
fn ok_round_trip_check(route: &ServiceOpRoute) -> Option<(String, String)> {
    let ExecutionKind::Update { state } = &route.execution else {
        return None;
    };
    let state = state.as_ref()?;
    let mut targets = state.op.iter().map(|t| t.to.as_str().to_string());
    let target = targets.next()?;
    if targets.any(|other| other != target) {
        return None;
    }
    Some((state.wire_key.as_str().to_string(), target))
}

/// The entity's machine, whole (the same rehydration the #449 surface
/// uses). `None` when the entity carries no `.ddd` design machine.
async fn machine_for_entity(
    db: &dyn GraphQuerier,
    entity_title: &str,
) -> Option<rex_ir::ddd::StateMachine> {
    let surface = crate::ddd::design::DddDesignSurface::from_graph(db)
        .await
        .ok()?;
    surface.machine_for(entity_title)
}

/// The op-level `when` condition text for one op (the DDD design mirror).
async fn operation_condition(
    db: &dyn GraphQuerier,
    entity_title: &str,
    op_name: &str,
) -> Option<String> {
    let models = db.get_ddd_models().await.ok()?;
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.name.eq_ignore_ascii_case(op_name) {
                    return op.condition.clone();
                }
            }
        }
    }
    let _ = entity_title;
    None
}

/// The machine states a `when` condition reads, conservatively: only
/// `field == State` / `State == field` comparisons against declared
/// machine states, AND/OR-combined. `None` when the shape isn't
/// recognized (the leg planner then refuses to plan a 403 drive).
fn machine_condition_source_states(
    condition: &str,
    machine_field: &str,
    machine: &rex_ir::ddd::StateMachine,
) -> Vec<String> {
    let mut sources = Vec::new();
    for clause in condition.split("&&").chain(condition.split("||")) {
        let clause = clause.trim();
        let Some((lhs, rhs)) = clause.split_once("==") else {
            continue;
        };
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        let field_matches = |side: &str| {
            side == machine_field
                || side.eq_ignore_ascii_case(machine_field)
                || codegraph_naming::to_snake_case(side)
                    == codegraph_naming::to_snake_case(machine_field)
        };
        let state_names = machine.states();
        let state_at = |side: &str| state_names.iter().any(|s| s.eq_ignore_ascii_case(side));
        let state_name = if field_matches(lhs) && state_at(rhs) {
            machine
                .states()
                .into_iter()
                .find(|s| s.eq_ignore_ascii_case(rhs))
                .map(str::to_string)
        } else if field_matches(rhs) && state_at(lhs) {
            machine
                .states()
                .into_iter()
                .find(|s| s.eq_ignore_ascii_case(lhs))
                .map(str::to_string)
        } else {
            None
        };
        if let Some(name) = state_name
            && !sources.contains(&name)
        {
            sources.push(name);
        }
    }
    sources
}

/// The domain key one entity title belongs to (schema-declared domain
/// first, then the config scan — the `resolve_schema_with_domain`
/// convention, mirrored locally to keep the api-paths module untouched).
async fn entity_domain(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity_title: &str,
) -> Option<String> {
    if let Ok(Some(schema)) = db.get_schema(entity_title).await
        && let Some(domain) = schema.domain
    {
        return Some(domain);
    }
    config
        .domains
        .iter()
        .find(|(_, entry)| {
            [entity_title, &format!("{entity_title}Type")]
                .iter()
                .any(|candidate| entry.entities.iter().any(|e| e == candidate))
        })
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod journey_tests {
    use std::collections::HashMap;

    use super::super::super::context::{IfmlAction, IfmlModel, IfmlViewContainer};
    use super::super::auth::PersonaSpec;
    use super::super::spec_payload::{JourneyHop, JourneyHopKind};
    use super::{journey_paths, path_persona};

    fn hop(target: &str) -> JourneyHop {
        JourneyHop {
            source_component: "grid".to_string(),
            kind: JourneyHopKind::Row,
            target_view: target.to_string(),
            target_pattern: format!("{}$", target.to_lowercase()),
            seed: None,
            fills: Vec::new(),
        }
    }

    fn view(name: &str, roles: &[&str]) -> IfmlViewContainer {
        IfmlViewContainer {
            name: name.to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: false,
            is_modal: false,
            conditional_expr_json: None,
            conditional_expression: None,
            roles: roles.iter().map(|r| r.to_string()).collect(),
            requires: Vec::new(),
            params: Vec::new(),
            components: Vec::new(),
            events: Vec::new(),
            containers: Vec::new(),
        }
    }

    fn persona(id: &str) -> PersonaSpec {
        // The real shape: the id is the lowercased identity, the actor keeps
        // the policy's casing (the roles check matches the actor).
        let mut actor = id.to_string();
        if let Some(first) = actor.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        PersonaSpec {
            id: id.to_string(),
            actor: actor.clone(),
            roles: vec![actor],
            capabilities: Vec::new(),
        }
    }

    /// The DFS records leaves only: a 2-hop walk suppresses its 1-hop
    /// prefix (one journey per path, prefixes deduped).
    #[test]
    fn journey_paths_dedupe_prefixes() {
        let mut hops: HashMap<String, Vec<JourneyHop>> = HashMap::new();
        hops.insert("A".to_string(), vec![hop("B")]);
        hops.insert("B".to_string(), vec![hop("C")]);
        let paths = journey_paths("A", &hops);
        assert_eq!(paths.len(), 1, "A→B must not render beside A→B→C");
        assert_eq!(paths[0].0, vec!["A", "B", "C"]);
        assert_eq!(paths[0].1.len(), 2);
    }

    /// Cycles never repeat a view: A→B→(A|C) records A→B→C only.
    #[test]
    fn journey_paths_break_cycles() {
        let mut hops: HashMap<String, Vec<JourneyHop>> = HashMap::new();
        hops.insert("A".to_string(), vec![hop("B")]);
        hops.insert("B".to_string(), vec![hop("A"), hop("C")]);
        hops.insert("C".to_string(), vec![hop("A")]);
        let paths = journey_paths("A", &hops);
        let views: Vec<Vec<String>> = paths.iter().map(|(views, _)| views.clone()).collect();
        assert_eq!(
            views,
            vec![vec!["A".to_string(), "B".to_string(), "C".to_string()]],
            "the A→B→A cycle must be cut: {views:?}"
        );
    }

    /// Chains cap at 4 hops: a 5-edge chain truncates at the cap instead of
    /// exploding.
    #[test]
    fn journey_paths_cap_depth() {
        let mut hops: HashMap<String, Vec<JourneyHop>> = HashMap::new();
        for (from, to) in [("A", "B"), ("B", "C"), ("C", "D"), ("D", "E"), ("E", "F")] {
            hops.insert(from.to_string(), vec![hop(to)]);
        }
        let paths = journey_paths("A", &hops);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].1.len(), super::JOURNEY_MAX_DEPTH);
        assert_eq!(paths[0].0.len(), super::JOURNEY_MAX_DEPTH + 1);
    }

    /// A walk through a guarded view only drives for personas permitted
    /// there; the roles check matches the persona actor.
    #[test]
    fn path_persona_requires_access_to_every_view() {
        let model = IfmlModel {
            view_containers: vec![view("Open", &[]), view("AdminOnly", &["Admin"])],
            actions: Vec::new(),
            navigation_edges: Vec::new(),
            data_flows: Vec::new(),
            generation_order: Vec::new(),
            policy: None,
        };
        let personas = vec![persona("admin"), persona("agent")];
        let open_walk = vec!["Open".to_string()];
        assert_eq!(
            path_persona(&personas, &open_walk, &model).as_deref(),
            Some("admin"),
            "the first persona wins on an unguarded walk"
        );
        let guarded_walk = vec!["Open".to_string(), "AdminOnly".to_string()];
        assert_eq!(
            path_persona(&personas, &guarded_walk, &model).as_deref(),
            Some("admin")
        );
        let denied = vec![persona("agent")];
        assert_eq!(
            path_persona(&denied, &guarded_walk, &model),
            None,
            "a walk no persona may complete drops the journey"
        );
    }

    /// A navigate action to a view-less target (a nested container) never
    /// becomes a hop — hop resolution is view-scoped.
    #[test]
    fn journey_hops_skip_non_view_targets() {
        // Covered by build_view_journeys' `target_vc` lookup; pinned here by
        // the payload contract: hops only exist with a resolvable view.
        let action = IfmlAction::Navigate {
            target: "Step2".to_string(),
            binding: HashMap::new(),
        };
        assert!(matches!(action, IfmlAction::Navigate { .. }));
    }
}
