use crate::ProjectConfig;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::ux::{Align, Dimension, Display, UxRules};
use codegraph_config::{DomainConfig, IfmlComponentMappings, SemanticRole};
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_type_contracts::RefClassificationKind;
use rex_ifml::{ComponentSpec, FormSpec};

use crate::error::Result;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::ux::plan::{build_ux_plan, CollectionPlan, UxPlan, UxPlanInput};
use crate::GenerationEntry;

use super::api_paths::{id_param_from, resolve_entity_api, ResolvedApi};
use super::context::{
    IfmlAction, IfmlComponent, IfmlEvent, IfmlModel, IfmlViewContainer, PolicyContext,
};
use super::querier::{IfmlGraphQuerier, IfmlQuerier};
use super::route_generator::{
    denial_target, mapped_container_testid, modal_wrapper_active, modal_wrapper_testid, shell_nav,
    workflow_for_entity, RenderWorkflow,
};

/// Global generator emitting IFML-driven Playwright E2E specs.
///
/// One spec file per view container under `{fw}/tests/ifml/{view-kebab}.spec.ts`,
/// with test kinds selected by what the model supports:
///
/// - render (always): goto the view route, expect the heading + primary
///   component testid.
/// - click-through (schema-backed API required): seed a fixture entity via the
///   API, click a row, assert the navigation-flow target URL with the
///   parameter bindings substituted.
/// - form validation negative (typed form spec with required fields): submit
///   empty, assert the input is in the invalid state.
/// - form CRUD round trip (schema-backed API + save navigation): create a
///   fixture, load the edit form, submit a modified value, assert the save
///   navigation and API persistence.
///
/// API-dependent kinds are skipped when the bound entity has no schema in the
/// graph (e.g. `ifml-generate` without `--schemas`), so a schema-less run
/// emits render tests only.
pub struct IfmlE2eTestGenerator {
    output_dir: PathBuf,
    framework: String,
    mappings: Option<IfmlComponentMappings>,
}

impl IfmlE2eTestGenerator {
    pub fn new(output_dir: &Path, framework: &str) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            framework: framework.to_string(),
            mappings: None,
        }
    }

    pub fn with_mappings(mut self, mappings: Option<IfmlComponentMappings>) -> Self {
        self.mappings = mappings;
        self
    }

    fn selectors(&self, vc: &IfmlViewContainer, c: &IfmlComponent) -> ComponentSelectors {
        let kind = component_kind(c);
        if let Some(mapped) = self
            .mappings
            .as_ref()
            .and_then(|m| m.resolve(&vc.name, &c.name, &c.component_type, &kind))
        {
            return ComponentSelectors {
                root: mapped
                    .testid("root")
                    .or_else(|| mapped.testid("form"))
                    .map(str::to_string),
                row: mapped.testid("row").map(str::to_string),
                form: mapped.testid("form").map(str::to_string),
                submit: mapped.testid("submit").map(str::to_string),
            };
        }
        if is_collection(c) {
            ComponentSelectors {
                root: Some(format!("{}-table", c.name)),
                row: Some(format!("{}-row", c.name)),
                form: None,
                submit: None,
            }
        } else if is_form(c) {
            let submit = self
                .mappings
                .as_ref()
                .and_then(|m| {
                    m.resolve_slot(
                        &vc.name,
                        &c.name,
                        &c.component_type,
                        &kind,
                        Some(SemanticRole::ActionControl),
                    )
                })
                .and_then(|m| m.testid("root").map(str::to_string));
            ComponentSelectors {
                root: Some(format!("{}-form", c.name)),
                row: None,
                form: Some(format!("{}-form", c.name)),
                submit: Some(submit.unwrap_or_else(|| format!("{}-submit", c.name))),
            }
        } else if is_details(c) {
            ComponentSelectors {
                root: Some(format!("{}-details", c.name)),
                row: None,
                form: None,
                submit: None,
            }
        } else {
            ComponentSelectors::default()
        }
    }

    async fn build_view_spec(
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

    fn build_render_test(
        &self,
        vc: &IfmlViewContainer,
        nav_testid: Option<String>,
    ) -> Option<RenderTest> {
        let (_component, selectors, is_collection) = primary_component(vc, self)?;
        Some(RenderTest {
            label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
            route: view_route(&vc.name),
            primary_testid: selectors.root?,
            assert_heading: is_collection,
            nav_testid,
            container_testid: mapped_container_testid(&vc.name, vc.is_xor, self.mappings.as_ref()),
        })
    }

    async fn build_click_through(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        model: &IfmlModel,
        vc: &IfmlViewContainer,
        edge: &super::context::NavigationEdge,
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
            row_testid,
            fixture: Fixture {
                base_path: api.base_path.clone(),
                entries: fixture_entries(component),
            },
            target_pattern,
            modal,
        })
    }

    async fn build_validation_test(
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
            route: view_route(&vc.name),
            form_testid,
            submit_testid,
            field: first_required,
        })
    }

    async fn build_round_trip_test(
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
    async fn build_view_workflow_tests(
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

    async fn build_workflow_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
        id_param: Option<&str>,
    ) -> Option<WorkflowTest> {
        let entity = c.entity.as_deref()?;
        // Collections are skipped outright (issue #303, exposed by the
        // gate fixture's unmapped fallback list): their per-row badge reads
        // the entity's status column, which API creates leave unset (the
        // create DTO carries no status field — the workflow plane owns it),
        // so no seeded row can back the strict initial-state assertion.
        // Details/forms keep their workflow spec — their badge reads the
        // `{id}/workflow` endpoint, which materializes the instance at the
        // initial state on first read.
        if is_collection(c) {
            return None;
        }
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
    /// values for constrained columns — the workflow status field seeds the
    /// initial state, and other codelist-backed fields use their first
    /// enum value (the create would otherwise violate the column's FK to
    /// the codelist table).
    async fn valid_fixture_entries(
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
    fn build_persona_tests(
        &self,
        policy: &PolicyContext,
        human_actors: &[String],
        vc: &IfmlViewContainer,
        denial: &str,
    ) -> Vec<PersonaTest> {
        if vc.roles.is_empty() && vc.requires.is_empty() {
            return Vec::new();
        }
        let (assert_heading, primary_testid) = match primary_component(vc, self) {
            Some((_, selectors, is_collection)) => (is_collection, selectors.root),
            None => (true, None),
        };
        // The page markup gates the submit control behind the same guard
        // checks, so a permitted persona can assert it is visible. Mapped
        // form components own their internals — no control assertion there.
        let control_testid = vc
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
            .and_then(|c| self.selectors(vc, c).submit);
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
    async fn build_view_ux_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        rules: &UxRules,
        plans: &HashMap<String, UxPlan>,
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
            // The workflow status column is never a chip exercise (issue
            // #303): API creates leave it unset (the create DTO carries no
            // status field), so a seeded override cannot reach the page.
            let status_field = c
                .entity
                .as_deref()
                .and_then(|entity| workflow_for_entity(config, entity))
                .map(|wf| wf.status_field);
            let status_field = status_field.as_deref();
            let chip_checks = ux_chip_checks(db, c, plan, &entries, status_field).await;
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
                CollectionPlan::Timeline { order_by, .. } => Some(UxTimelineCheck {
                    root_testid: format!("{}-timeline", c.name),
                    item_testid: format!("{}-timeline-item", c.name),
                    order_literal: entries
                        .iter()
                        .find(|(field, _)| field == order_by)
                        .map(|(_, value)| js_literal_inner(value)),
                }),
                CollectionPlan::Table => None,
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
    fn is_mapped(&self, vc: &IfmlViewContainer, c: &IfmlComponent) -> bool {
        let kind = component_kind(c);
        self.mappings
            .as_ref()
            .and_then(|m| m.resolve(&vc.name, &c.name, &c.component_type, &kind))
            .is_some()
    }
}

#[async_trait]
impl GlobalGenerator for IfmlE2eTestGenerator {
    fn name(&self) -> &str {
        "ifml-e2e-test"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        _tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if self.framework != "svelte" {
            return Ok(vec![]);
        }

        let querier = IfmlGraphQuerier::new(db);
        let model = querier
            .get_ifml_model()
            .await
            .map_err(crate::error::Error::Graph)?;
        if model.view_containers.is_empty() {
            return Ok(vec![]);
        }

        let mut specs = Vec::new();
        let mut workflow_specs: Vec<(String, Vec<WorkflowTest>)> = Vec::new();
        // ux-rules plane (issue #303): one plan per bound entity plus one
        // ux spec per view with an eligible fallback list. Flag off → no
        // plans, no ux specs (byte-identical output).
        let ux_rules = project.ux.as_ref();
        let ux_plans = match ux_rules {
            Some(rules) => build_ux_plans(db, config, &model, rules).await?,
            None => HashMap::new(),
        };
        let mut ux_specs: Vec<(String, UxViewTest)> = Vec::new();
        let human_actors: Vec<String> = match &model.policy {
            Some(_) => db
                .get_actors()
                .await
                .map_err(crate::error::Error::Graph)?
                .into_iter()
                .filter(|actor| actor.kind.as_deref() != Some("agent"))
                .map(|actor| actor.name)
                .collect(),
            None => Vec::new(),
        };
        let denial = denial_target(&model);
        for vc in &model.view_containers {
            let mut spec = self
                .build_view_spec(db, config, &project.api_version, &model, vc)
                .await;
            if let Some(policy) = &model.policy {
                spec.personas = self.build_persona_tests(policy, &human_actors, vc, &denial);
            }
            if has_tests(&spec) {
                specs.push(spec);
            }
            if let Some(rules) = ux_rules {
                if let Some(ux_test) = self
                    .build_view_ux_test(db, config, &project.api_version, vc, rules, &ux_plans)
                    .await
                {
                    ux_specs.push((vc.name.clone(), ux_test));
                }
            }
            let workflow_tests = self
                .build_view_workflow_tests(db, config, &project.api_version, vc)
                .await;
            if !workflow_tests.is_empty() {
                workflow_specs.push((vc.name.clone(), workflow_tests));
            }
        }
        if specs.is_empty() && workflow_specs.is_empty() {
            return Ok(vec![]);
        }

        // Remove stale per-view spec files for views no longer in the model,
        // stale workflow specs whose view no longer carries workflow
        // tests (e.g. the workflow config or a mapped collection changed
        // across regenerations into the same root), and stale ux specs
        // (issue #303) whose view lost its ux eligibility (flag off, view
        // removed, or a mapping now replaces the fallback).
        let active_specs: std::collections::HashSet<String> = model
            .view_containers
            .iter()
            .map(|vc| format!("{}.spec.ts", codegraph_naming::to_kebab_case(&vc.name)))
            .collect();
        let active_workflow_specs: std::collections::HashSet<String> = workflow_specs
            .iter()
            .map(|(view_name, _)| {
                format!(
                    "{}.workflow.spec.ts",
                    codegraph_naming::to_kebab_case(view_name)
                )
            })
            .collect();
        let active_ux_specs: std::collections::HashSet<String> = ux_specs
            .iter()
            .map(|(view_name, _)| {
                format!("{}.ux.spec.ts", codegraph_naming::to_kebab_case(view_name))
            })
            .collect();
        let specs_dir = self.output_dir.join("tests").join("ifml");
        if specs_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&specs_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if !name.ends_with(".spec.ts") {
                        continue;
                    }
                    if name.strip_suffix(".workflow.spec.ts").is_some() {
                        if !active_workflow_specs.contains(name) {
                            let _ = std::fs::remove_file(&path);
                        }
                        continue;
                    }
                    if name.strip_suffix(".ux.spec.ts").is_some() {
                        if !active_ux_specs.contains(name) {
                            let _ = std::fs::remove_file(&path);
                        }
                        continue;
                    }
                    if !active_specs.contains(name) {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }

        let mut files: Vec<GeneratedFile> = specs
            .iter()
            .map(|spec| GeneratedFile {
                path: self.output_dir.join("tests").join("ifml").join(format!(
                    "{}.spec.ts",
                    codegraph_naming::to_kebab_case(&spec.view_name)
                )),
                content: render_spec(spec),
            })
            .collect();

        for (view_name, workflow_tests) in &workflow_specs {
            if let Some(vc) = model
                .view_containers
                .iter()
                .find(|vc| &vc.name == view_name)
            {
                files.push(GeneratedFile {
                    path: self.output_dir.join("tests").join("ifml").join(format!(
                        "{}.workflow.spec.ts",
                        codegraph_naming::to_kebab_case(view_name)
                    )),
                    content: render_workflow_spec(vc, workflow_tests),
                });
            }
        }

        for (view_name, ux_test) in &ux_specs {
            files.push(GeneratedFile {
                path: self.output_dir.join("tests").join("ifml").join(format!(
                    "{}.ux.spec.ts",
                    codegraph_naming::to_kebab_case(view_name)
                )),
                content: render_ux_spec(ux_test),
            });
        }

        let config_path = self.output_dir.join("playwright.config.ts");
        if !config_path.exists() {
            files.push(GeneratedFile {
                path: config_path,
                content: PLAYWRIGHT_CONFIG.to_string(),
            });
        }
        let package_path = self.output_dir.join("package.json");
        if !package_path.exists() {
            files.push(GeneratedFile {
                path: package_path,
                content: PACKAGE_JSON.to_string(),
            });
        }

        Ok(files)
    }
}

// ── Test models ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ViewTestSpec {
    pub view_name: String,
    pub label: String,
    pub route: String,
    pub render: Option<RenderTest>,
    pub click_throughs: Vec<ClickThroughTest>,
    pub validations: Vec<ValidationTest>,
    pub round_trips: Vec<RoundTripTest>,
    /// Actor-persona guard tests (policy-gated): per human actor, one test
    /// asserting the view renders when permitted or redirects to the denial
    /// target when not.
    pub personas: Vec<PersonaTest>,
}

/// One actor-persona guard test: seeds `__USER_ROLES__` (and
/// `__USER_CAPABILITIES__` for capability-only views) before navigating,
/// then asserts the permitted outcome (page renders) or the denied one
/// (redirect to the denial target).
#[derive(Debug)]
pub struct PersonaTest {
    pub actor: String,
    pub permitted: bool,
    pub route: String,
    pub label: String,
    pub assert_heading: bool,
    pub primary_testid: Option<String>,
    pub denial_target: String,
    /// Capabilities seeded into `__USER_CAPABILITIES__` for capability-only
    /// views (requires without roles); `None` skips the seeding.
    pub capabilities: Option<Vec<String>>,
    /// Submit-control testid asserted visible for permitted personas when
    /// the view carries an unmapped form (the page markup gates it behind
    /// the same guard checks); `None` skips the assertion.
    pub control_testid: Option<String>,
}

#[derive(Debug)]
pub struct RenderTest {
    pub label: String,
    pub route: String,
    pub primary_testid: String,
    pub assert_heading: bool,
    /// Landmark shell nav testid (resolved `shell` mapping) asserted visible
    /// on every page that renders inside the layout.
    pub nav_testid: Option<String>,
    /// Mapped presentation-container wrapper testid for xor view containers.
    pub container_testid: Option<String>,
}

#[derive(Debug)]
pub struct ClickThroughTest {
    pub title: String,
    pub source_route: String,
    pub row_testid: String,
    pub fixture: Fixture,
    pub target_pattern: String,
    /// Assertions for navigation into a `modal: true` target: wrapper
    /// visibility, close-button click, and the URL pattern after close.
    pub modal: Option<ModalCloseAssertions>,
}

#[derive(Debug)]
pub struct ModalCloseAssertions {
    pub wrapper_testid: String,
    pub close_testid: String,
    pub back_pattern: String,
}

#[derive(Debug)]
pub struct ValidationTest {
    pub route: String,
    pub form_testid: String,
    pub submit_testid: String,
    pub field: String,
}

#[derive(Debug)]
pub struct RoundTripTest {
    pub route: String,
    pub id_param: String,
    pub form_testid: String,
    pub submit_testid: String,
    pub field: String,
    pub original: String,
    pub updated: String,
    pub fixture: Fixture,
    pub target_pattern: String,
}

/// One workflow state assertion inside a view's `{view}.workflow.spec.ts`:
/// create a fixture via the API, open the view, and assert the state badge
/// shows the configured initial state (mirrors the non-IFML
/// `{entity}.workflow.test.ts` convention). Details/form components also
/// carry a transition round trip when a valid (from → to) edge exists.
#[derive(Debug)]
pub struct WorkflowTest {
    /// Human-readable component name used in the test title.
    pub component_name: String,
    pub route: String,
    pub id_param: Option<String>,
    pub state_testid: String,
    pub initial_state: String,
    /// Collection badges render per row (`{#each}`): state assertions use
    /// `.first()` to stay strict-mode-safe.
    pub is_collection: bool,
    /// Transition round trip (details/form only): initial state → first
    /// valid target via the `{component}-transition-{target}` button.
    pub transition: Option<TransitionStep>,
    pub fixture: Fixture,
}

/// The transition a workflow spec exercises: click the enabled
/// `{component}-transition-{target}` button, assert the badge shows the
/// target state, and confirm persistence via a GET.
#[derive(Debug)]
pub struct TransitionStep {
    pub from: String,
    pub to: String,
    pub to_testid: String,
}

/// The ux-rules rendering tests for one view (issue #303), rendered into
/// `{view}.ux.spec.ts`. Every check is generated only for markup the
/// fallback template actually renders (chips, numeric alignment, Intl
/// formatting, copy-chip, overflow menu, timeline) — no vacuous tests.
#[derive(Debug)]
pub struct UxViewTest {
    /// The fallback list component carrying the asserted markup.
    pub component_name: String,
    pub route: String,
    pub fixture: Fixture,
    pub table_testid: String,
    pub row_testid: String,
    /// Chip columns: seeded value, its rendered text, and the expected
    /// `data-chip-variant` (the column's ToneMap lookup).
    pub chip_checks: Vec<UxChipCheck>,
    /// Numeric/date column cells: nth-child position, alignment class, and
    /// the Intl formatter the page runs for the column's dimension.
    pub column_checks: Vec<UxColumnCheck>,
    /// Copy-chip column: click writes the cell value to the clipboard.
    pub copy_check: Option<UxCopyCheck>,
    /// Overflow menu (second+ navigate events disclosed by tiering).
    pub menu: Option<UxMenuCheck>,
    /// Timeline layout entries (explicit `[[collection]]` rule).
    pub timeline: Option<UxTimelineCheck>,
    /// Locale/currency baseline feeding the in-spec Intl computations.
    pub locale: String,
    /// Ready-to-render money options object literal (`{}` when the pack
    /// carries no currency).
    pub money_options: String,
}

/// One chip assertion: create a row overriding `field` to `override_literal`,
/// then expect the row's `{comp}-chip` carrying `text` to show `variant`.
#[derive(Debug)]
pub struct UxChipCheck {
    pub field: String,
    pub override_literal: String,
    pub text: String,
    pub variant: String,
}

/// One numeric/date cell assertion: the cell at `cell_index` (0-based within
/// the row) renders `literal` through the Intl formatter of `format`; the
/// `text-right` header/cell class asserts fire only on right-aligned
/// columns (mirroring the template).
#[derive(Debug)]
pub struct UxColumnCheck {
    /// Header name when the column is right-aligned (asserts `text-right`
    /// on the `<th>`).
    pub header: Option<String>,
    pub cell_index: usize,
    pub format: UxCellFormat,
    pub assert_right: bool,
}

/// The Intl formatter a column's dimension renders through (mirroring the
/// page's `formatMoney`/`formatNumber`/`formatDate`).
#[derive(Debug)]
pub enum UxCellFormat {
    Money { literal: String },
    Quantity { literal: String },
    Date { literal: String },
}

/// One copy-chip assertion: clicking `{comp}-copy` writes the created
/// entity's `field` value to the clipboard.
#[derive(Debug)]
pub struct UxCopyCheck {
    pub field: String,
}

/// Overflow-menu assertions: open the trigger, expect the menu, and follow
/// its first item to the navigation target.
#[derive(Debug)]
pub struct UxMenuCheck {
    pub trigger_testid: String,
    pub menu_testid: String,
    pub target_pattern: String,
}

/// Timeline assertions: the `{comp}-timeline` root renders items newest-first
/// with real `<time>` content matching the fixture's order value.
#[derive(Debug)]
pub struct UxTimelineCheck {
    pub root_testid: String,
    pub item_testid: String,
    /// The fixture's value for the timeline `order_by` field, when present.
    pub order_literal: Option<String>,
}

/// The (from → to) edge a spec can safely exercise: with a populated
/// transitions map, the initial state's first (sorted) target; with an
/// empty map, the first non-terminal state other than the initial one.
fn pick_transition(workflow: &RenderWorkflow) -> Option<(String, String)> {
    if workflow.transition_map.is_empty() {
        return workflow
            .states
            .iter()
            .find(|s| !workflow.terminal_states.contains(s) && **s != workflow.initial_state)
            .map(|s| (workflow.initial_state.clone(), s.clone()));
    }
    let targets = workflow.transition_map.get(&workflow.initial_state)?;
    let mut sorted: Vec<&String> = targets.iter().collect();
    sorted.sort();
    sorted
        .first()
        .map(|to| (workflow.initial_state.clone(), (*to).clone()))
}

#[derive(Debug)]
pub struct Fixture {
    pub base_path: String,
    pub entries: Vec<(String, String)>,
}

impl Fixture {
    fn data_literal(&self) -> String {
        let inner = self
            .entries
            .iter()
            .map(|(k, v)| format!("'{k}': {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {inner} }}")
    }
}

#[derive(Debug, Default)]
pub struct ComponentSelectors {
    root: Option<String>,
    row: Option<String>,
    form: Option<String>,
    submit: Option<String>,
}

// ── Spec rendering ───────────────────────────────────────────────────────────

const PLAYWRIGHT_CONFIG: &str = r#"// Generated by codegraph. DO NOT EDIT.

import { defineConfig } from '@playwright/test';

// Env-gated auth: when IFML_API_KEY is set, every page/request fixture sends
// `Authorization: Bearer <key>` so the generated axum server's api-key
// middleware (public.verify_api_key) accepts the specs' calls. Unset keeps
// the config inert (header omitted).
export default defineConfig({
	testDir: './tests',
	use: {
		baseURL: process.env.BASE_URL ?? 'http://localhost:5173',
		extraHTTPHeaders: process.env.IFML_API_KEY
			? { authorization: `Bearer ${process.env.IFML_API_KEY}` }
			: undefined,
	},
});
"#;

const PACKAGE_JSON: &str = r#"{
  "name": "ifml-e2e-tests",
  "private": true,
  "scripts": {
    "test:e2e": "playwright test"
  },
  "devDependencies": {
    "@playwright/test": "^1.49.1",
    "typescript": "^5.7.2"
  }
}
"#;

/// Render a view's `{view}.workflow.spec.ts`: one test per workflow
/// component, mirroring the non-IFML workflow test's "initial state"
/// assertion (`toContainText` on the state badge).
fn render_workflow_spec(vc: &IfmlViewContainer, tests: &[WorkflowTest]) -> String {
    let label = vc.label.clone().unwrap_or_else(|| vc.name.clone());
    let mut s = String::new();
    s.push_str("// Generated by codegraph. DO NOT EDIT.\n");
    s.push_str(&format!(
        "// IFML Playwright E2E workflow tests for view {}.\n\n",
        vc.name
    ));
    s.push_str("import { test, expect } from '@playwright/test';\n\n");
    s.push_str(&format!(
        "test.describe('{} workflow', () => {{\n",
        js_string(&label)
    ));

    for workflow in tests {
        s.push_str(&format!(
            "\ttest('shows the initial workflow state for {}', async ({{ page, request }}) => {{\n",
            js_string(&workflow.component_name)
        ));
        s.push_str(&format!(
            "\t\tconst created = await (await request.post('{}', {{ data: {} }})).json();\n",
            workflow.fixture.base_path,
            workflow.fixture.data_literal()
        ));
        match &workflow.id_param {
            Some(id_param) => s.push_str(&format!(
                "\t\tawait page.goto(`{}?{}=${{created.data?.id ?? created.id}}`);\n",
                workflow.route, id_param
            )),
            None => s.push_str(&format!("\t\tawait page.goto('{}');\n", workflow.route)),
        }
        s.push_str(&format!(
            "\t\tawait expect(page.getByTestId('{}'){}).toContainText('{}');\n",
            workflow.state_testid,
            if workflow.is_collection {
                ".first()"
            } else {
                ""
            },
            js_string(&workflow.initial_state)
        ));
        s.push_str("\t});\n\n");

        if let Some(transition) = &workflow.transition {
            s.push_str(&format!(
                "\ttest('transitions {} from {} to {}', async ({{ page, request }}) => {{\n",
                js_string(&workflow.component_name),
                js_string(&transition.from),
                js_string(&transition.to)
            ));
            s.push_str(&format!(
                "\t\tconst created = await (await request.post('{}', {{ data: {} }})).json();\n",
                workflow.fixture.base_path,
                workflow.fixture.data_literal()
            ));
            let id_expr = "created.data?.id ?? created.id";
            match &workflow.id_param {
                Some(id_param) => s.push_str(&format!(
                    "\t\tawait page.goto(`{}?{}=${{{}}}`);\n",
                    workflow.route, id_param, id_expr
                )),
                None => s.push_str(&format!("\t\tawait page.goto('{}');\n", workflow.route)),
            }
            s.push_str(&format!(
                "\t\tawait page.getByTestId('{}').click();\n",
                transition.to_testid
            ));
            s.push_str(&format!(
                "\t\tawait expect(page.getByTestId('{}')).toContainText('{}');\n",
                workflow.state_testid,
                js_string(&transition.to)
            ));
            // Persistence check via the generated workflow-state endpoint
            // (`current_state` is authoritative; the entity status column is
            // the fallback for pipelines that sync it).
            s.push_str(&format!(
                "\t\tconst fetched = await (await request.get(`{}/${{{}}}/workflow`)).json();\n",
                workflow.fixture.base_path, id_expr
            ));
            s.push_str(&format!(
                "\t\texpect(fetched.data?.current_state ?? fetched.data?.status).toBe('{}');\n",
                js_string(&transition.to)
            ));
            s.push_str("\t});\n\n");
        }
    }

    while s.ends_with("\n\n") {
        s.pop();
    }
    s.push_str("});\n");
    s
}

/// Render a view's `{view}.ux.spec.ts` (issue #303): ux-rules fallback
/// markup assertions computed from the SAME resolution the route generator
/// renders from — chip tones via the ToneMap lookup, cell text via the
/// Intl formatters the page runs (`UX_LOCALE`/`UX_MONEY_OPTS`).
fn render_ux_spec(test: &UxViewTest) -> String {
    let mut s = String::new();
    s.push_str("// Generated by codegraph. DO NOT EDIT.\n");
    s.push_str(&format!(
        "// IFML Playwright E2E ux-rules tests for view {} component {} (#303).\n\n",
        test.route.trim_start_matches('/'),
        test.component_name
    ));
    s.push_str("import { test, expect } from '@playwright/test';\n\n");
    s.push_str(&format!(
        "test.describe('{} ux', () => {{\n",
        js_string(&test.component_name)
    ));
    s.push_str(&format!(
        "\tconst UX_BASE = '{}';\n",
        test.fixture.base_path
    ));
    s.push_str(&format!(
        "\tconst UX_DATA = {};\n",
        test.fixture.data_literal()
    ));
    s.push_str(&format!(
        "\tconst UX_LOCALE = '{}';\n",
        js_string(&test.locale)
    ));
    s.push_str(&format!(
        "\tconst UX_MONEY_OPTS = {};\n\n",
        test.money_options
    ));
    s.push_str(
        "\tasync function createUxRow(request: any, overrides: Record<string, unknown> = {}) {\n",
    );
    s.push_str("\t\tconst response = await request.post(UX_BASE, { data: { ...UX_DATA, ...overrides } });\n");
    s.push_str("\t\texpect(response.ok(), 'ux fixture entity should be created').toBeTruthy();\n");
    s.push_str("\t\tconst body = await response.json();\n");
    s.push_str("\t\treturn (body.data ?? body) as Record<string, unknown>;\n\t}\n");

    if !test.chip_checks.is_empty() {
        s.push_str(
            "\n\ttest('ux chips render with tone variants', async ({ page, request }) => {\n",
        );
        for (index, chip) in test.chip_checks.iter().enumerate() {
            s.push_str(&format!(
                "\t\tawait createUxRow(request, {{ 'title': 'Ux chips {index}', {}: {} }});\n",
                js_string(&chip.field),
                chip.override_literal
            ));
        }
        s.push_str(&format!("\t\tawait page.goto('{}');\n", test.route));
        s.push_str(&format!(
            "\t\tconst rows = page.getByTestId('{}');\n",
            test.row_testid
        ));
        for (index, chip) in test.chip_checks.iter().enumerate() {
            s.push_str(&format!("\t\t{{\n\t\t\tconst chip = rows.filter({{ hasText: 'Ux chips {index}' }}).first().getByTestId('{comp}-chip').filter({{ hasText: '{text}' }}).first();\n", index = index, comp = test.component_name, text = js_string(&chip.text)));
            s.push_str("\t\t\tawait expect(chip).toBeVisible();\n");
            s.push_str(&format!(
                "\t\t\tawait expect(chip).toHaveText('{}');\n",
                js_string(&chip.text)
            ));
            s.push_str(&format!(
                "\t\t\tawait expect(chip).toHaveAttribute('data-chip-variant', '{}');\n\t\t}}\n",
                js_string(&chip.variant)
            ));
        }
        s.push_str("\t});\n");
    }

    if !test.column_checks.is_empty() {
        s.push_str("\n\ttest('ux numeric columns align and format through Intl', async ({ page, request }) => {\n");
        s.push_str("\t\tawait createUxRow(request, { 'title': 'Ux formats 0' });\n");
        s.push_str(&format!("\t\tawait page.goto('{}');\n", test.route));
        s.push_str(&format!(
            "\t\tconst row = page.getByTestId('{}').filter({{ hasText: 'Ux formats 0' }}).first();\n",
            test.row_testid
        ));
        for check in &test.column_checks {
            if let Some(header) = &check.header {
                s.push_str("\t\t{\n");
                s.push_str(&format!(
                    "\t\t\tconst th = page.getByTestId('{}').locator('thead th').filter({{ hasText: '{}' }});\n",
                    test.table_testid,
                    js_string(header)
                ));
                s.push_str("\t\t\tawait expect(th).toHaveClass(/text-right/);\n\t\t}\n");
            }
            s.push_str("\t\t{\n");
            s.push_str(&format!(
                "\t\t\tconst td = row.locator('td').nth({});\n",
                check.cell_index
            ));
            match &check.format {
                UxCellFormat::Money { literal } => {
                    if check.assert_right {
                        s.push_str("\t\t\tawait expect(td).toHaveClass(/text-right/);\n");
                        s.push_str("\t\t\tawait expect(td).toHaveClass(/tabular-nums/);\n");
                    }
                    s.push_str(
                        "\t\t\tconst fmt = new Intl.NumberFormat(UX_LOCALE, UX_MONEY_OPTS);\n",
                    );
                    s.push_str(&format!(
                        "\t\t\tawait expect(td).toHaveText(fmt.format({literal}));\n"
                    ));
                }
                UxCellFormat::Quantity { literal } => {
                    if check.assert_right {
                        s.push_str("\t\t\tawait expect(td).toHaveClass(/text-right/);\n");
                        s.push_str("\t\t\tawait expect(td).toHaveClass(/tabular-nums/);\n");
                    }
                    s.push_str("\t\t\tconst fmt = new Intl.NumberFormat(UX_LOCALE);\n");
                    s.push_str(&format!(
                        "\t\t\tawait expect(td).toHaveText(fmt.format({literal}));\n"
                    ));
                }
                UxCellFormat::Date { literal } => {
                    if check.assert_right {
                        s.push_str("\t\t\tawait expect(td).toHaveClass(/text-right/);\n");
                    }
                    s.push_str("\t\t\tconst fmt = new Intl.DateTimeFormat(UX_LOCALE, { dateStyle: 'medium' });\n");
                    s.push_str(&format!(
                        "\t\t\tawait expect(td).toHaveText(fmt.format(new Date('{literal}')));\n"
                    ));
                }
            }
            s.push_str("\t\t}\n");
        }
        s.push_str("\t});\n");
    }

    if let Some(copy) = &test.copy_check {
        s.push_str("\n\ttest('ux copy chips write the clipboard', async ({ page, request, context }) => {\n");
        s.push_str("\t\tconst created = await createUxRow(request, { 'title': 'Ux copies 0' });\n");
        s.push_str(&format!("\t\tawait page.goto('{}');\n", test.route));
        s.push_str("\t\tawait context.grantPermissions(['clipboard-read', 'clipboard-write']);\n");
        s.push_str(&format!(
            "\t\tconst copy = page.getByTestId('{}').filter({{ hasText: 'Ux copies 0' }}).first().getByTestId('{comp}-copy').first();\n",
            test.row_testid,
            comp = test.component_name
        ));
        s.push_str("\t\tawait copy.click();\n");
        s.push_str("\t\tawait expect\n\t\t\t.poll(() => page.evaluate(() => navigator.clipboard.readText()))\n");
        s.push_str(&format!(
            "\t\t\t.toBe(String(created['{field}'] ?? ''));\n\t}});\n",
            field = js_string(&copy.field)
        ));
    }

    if let Some(menu) = &test.menu {
        s.push_str(
            "\n\ttest('ux row actions open the overflow menu', async ({ page, request }) => {\n",
        );
        s.push_str("\t\tawait createUxRow(request, { 'title': 'Ux menu 0' });\n");
        s.push_str(&format!("\t\tawait page.goto('{}');\n", test.route));
        s.push_str(&format!(
            "\t\tawait page.getByTestId('{}').first().click();\n",
            menu.trigger_testid
        ));
        s.push_str(&format!(
            "\t\tconst menu = page.getByTestId('{}').first();\n",
            menu.menu_testid
        ));
        s.push_str("\t\tawait expect(menu).toBeVisible();\n");
        s.push_str("\t\tawait menu.getByRole('button').first().click();\n");
        s.push_str(&format!(
            "\t\tawait page.waitForURL(new RegExp('{}'));\n\t}});\n",
            js_string(&menu.target_pattern)
        ));
    }

    if let Some(timeline) = &test.timeline {
        s.push_str(
            "\n\ttest('ux timeline renders entries newest-first', async ({ page, request }) => {\n",
        );
        s.push_str("\t\tawait createUxRow(request, { 'title': 'Ux timeline 0' });\n");
        s.push_str("\t\tawait createUxRow(request, { 'title': 'Ux timeline 1' });\n");
        s.push_str(&format!("\t\tawait page.goto('{}');\n", test.route));
        s.push_str(&format!(
            "\t\tconst timeline = page.getByTestId('{}');\n",
            timeline.root_testid
        ));
        s.push_str("\t\tawait expect(timeline).toBeVisible();\n");
        s.push_str(&format!(
            "\t\tconst items = page.getByTestId('{}');\n",
            timeline.item_testid
        ));
        s.push_str("\t\texpect(await items.count()).toBeGreaterThan(0);\n");
        s.push_str("\t\tconst times = await items.locator('time').allTextContents();\n");
        s.push_str("\t\tfor (const text of times) {\n\t\t\texpect(text.trim().length).toBeGreaterThan(0);\n\t\t}\n");
        if let Some(literal) = &timeline.order_literal {
            s.push_str(&format!(
                "\t\t{{\n\t\t\tconst fmt = new Intl.DateTimeFormat(UX_LOCALE, {{ dateStyle: 'medium' }});\n\t\t\texpect(times).toContain(fmt.format(new Date('{literal}')));\n\t\t}}\n"
            ));
        }
        s.push_str(
            "\t\tconst parsed = times.map((t) => Date.parse(t)).filter((v) => !Number.isNaN(v));\n",
        );
        s.push_str("\t\tfor (let i = 1; i < parsed.length; i++) {\n\t\t\texpect(parsed[i]).toBeLessThanOrEqual(parsed[i - 1]);\n\t\t}\n");
        s.push_str("\t});\n");
    }

    s.push_str("});\n");
    s
}

// ── ux spec helpers (issue #303) ─────────────────────────────────────────────

/// The second navigate event of a collection component — the first one the
/// route generator keeps inline (the row click), any further one it
/// discloses into the per-row overflow menu. `None` means the page renders
/// no menu, so the ux spec asserts none.
fn ux_menu_event(events: &[IfmlEvent]) -> Option<&IfmlEvent> {
    events
        .iter()
        .filter(|e| matches!(e.action, IfmlAction::Navigate { .. }))
        .nth(1)
}

/// Numeric/date cell checks for a fallback list: per displayed field, the
/// Intl formatter its dimension renders through, plus alignment assertions
/// when the resolved align is right. Skips id/FK fields and fields without
/// a fixture value.
fn ux_column_checks(
    c: &IfmlComponent,
    plan: &UxPlan,
    entries: &[(String, String)],
) -> Vec<UxColumnCheck> {
    let mut out = Vec::new();
    for (index, field) in c.fields.iter().enumerate() {
        if field == "id" || field.ends_with("_id") {
            continue;
        }
        let Some(col) = plan.columns.get(field) else {
            continue;
        };
        if col.display != Display::Raw {
            continue;
        }
        let Some(literal) = entry_inner(entries, field) else {
            continue;
        };
        let format = match col.dimension {
            Dimension::Money => UxCellFormat::Money { literal },
            Dimension::Quantity => UxCellFormat::Quantity { literal },
            Dimension::TimePoint => UxCellFormat::Date { literal },
            _ => continue,
        };
        let right = col.align == Align::Right;
        out.push(UxColumnCheck {
            header: right.then(|| field.clone()),
            cell_index: index,
            format,
            assert_right: right,
        });
    }
    out
}

/// Chip checks for a fallback list: per chip column, up to two seeded
/// values with DISTINCT expected variants (exercising the ToneMap). The
/// candidates are the backing codelist's values when there is one, else
/// the field's fixture value. The workflow status field is skipped: API
/// creates leave it unset (the create DTO carries no status field), so a
/// seeded override cannot reach the rendered chip.
async fn ux_chip_checks(
    db: &dyn GraphQuerier,
    c: &IfmlComponent,
    plan: &UxPlan,
    entries: &[(String, String)],
    workflow_status_field: Option<&str>,
) -> Vec<UxChipCheck> {
    let mut out = Vec::new();
    for field in &c.fields {
        if field == "id" || field.ends_with("_id") {
            continue;
        }
        if workflow_status_field == Some(field.as_str()) {
            continue;
        }
        let Some(col) = plan.columns.get(field) else {
            continue;
        };
        if col.display != Display::Chip {
            continue;
        }
        let Some((_, literal)) = entries.iter().find(|(name, _)| name == field) else {
            continue;
        };
        let mut candidates: Vec<String> = codelist_values(db, c.entity.as_deref(), field)
            .await
            .unwrap_or_default();
        if candidates.is_empty() {
            candidates.push(js_literal_inner(literal));
        }
        let mut picked = 0usize;
        for value in candidates {
            let variant = col.tone.lookup(&value.to_lowercase()).to_string();
            if out
                .iter()
                .any(|check: &UxChipCheck| check.variant == variant)
            {
                continue;
            }
            let override_literal = if literal.starts_with('\'') {
                format!("'{}'", js_string(&value))
            } else {
                value.clone()
            };
            out.push(UxChipCheck {
                field: field.clone(),
                override_literal,
                text: value,
                variant,
            });
            picked += 1;
            if picked == 2 || out.len() == 4 {
                break;
            }
        }
    }
    out
}

/// Fixture payload for a ux spec: `fixture_entries` plus valid values for
/// codelist-backed fields (the create would otherwise reference a code
/// outside the codelist table).
async fn ux_fixture_entries(db: &dyn GraphQuerier, c: &IfmlComponent) -> Vec<(String, String)> {
    let mut entries = fixture_entries(c);
    for (field, value) in entries.iter_mut() {
        if let Some(code) = codelist_fixture_value(db, c.entity.as_deref(), field).await {
            *value = format!("'{}'", js_string(&code));
        }
    }
    entries
}

/// All values of the codelist backing `field` on `entity`'s schema, when
/// the property references one (`None` keeps the generic fixture value).
async fn codelist_values(
    db: &dyn GraphQuerier,
    entity: Option<&str>,
    field: &str,
) -> Option<Vec<String>> {
    let entity = entity?;
    let schema = match db.get_schema(entity).await {
        Ok(Some(schema)) => Some(schema),
        Ok(None) => db.get_schema(&format!("{entity}Type")).await.ok()?,
        Err(_) => None,
    }?;
    let props = db.get_properties(&schema.title).await.ok()?;
    let target = props
        .iter()
        .find(|p| p.name == field)?
        .ref_target
        .as_deref()?;
    let stem = target.rsplit('/').next()?.strip_suffix(".json")?;
    let codelist = db.get_schema(stem).await.ok()??;
    if !codelist.is_codelist {
        return None;
    }
    let values = db.get_enum_values(stem).await.ok()?;
    Some(values.into_iter().map(|v| v.value).collect())
}

/// The ready-to-render money options object literal for a rules document:
/// currency style when configured, grouped decimals otherwise (mirroring
/// the page context the route generator renders from).
fn ux_money_options(rules: &UxRules) -> String {
    match rules.format.currency.as_deref() {
        Some(code) => format!("{{ style: 'currency', currency: '{}' }}", js_string(code)),
        None => "{}".to_string(),
    }
}

/// The inner text of a JS literal (`'draft'` → `draft`, `42` → `42`).
fn js_literal_inner(literal: &str) -> String {
    let trimmed = literal.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn entry_inner(entries: &[(String, String)], field: &str) -> Option<String> {
    entries
        .iter()
        .find(|(name, _)| name == field)
        .map(|(_, value)| js_literal_inner(value))
        .filter(|value| !value.is_empty())
}

/// One ux plan per distinct bound entity (issue #303) — the same projection
/// the route generator's `resolve_generation_ux` builds (schema-backed
/// entities plan over ALL their properties, schema-less ones over the
/// component's display fields), minus the advisory diagnostics the e2e
/// generator never prints.
async fn build_ux_plans(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    model: &IfmlModel,
    rules: &UxRules,
) -> Result<HashMap<String, UxPlan>> {
    let mut plans: HashMap<String, UxPlan> = HashMap::new();
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
            if plans.contains_key(entity) {
                continue;
            }
            let props = ux_props_for_entity(db, entity).await;
            let status_field = workflow_for_entity(config, entity).map(|wf| wf.status_field);
            let status_field = status_field.as_deref();
            let names = ux_plan_field_names(c, &props);
            let mut fields: Vec<crate::ui::page::UiField> = Vec::with_capacity(names.len());
            let mut prop_by_name: BTreeMap<String, &PropertyNode> = BTreeMap::new();
            for name in &names {
                if let Some(prop) = props.get(name) {
                    prop_by_name.insert(name.clone(), prop);
                    fields.push(ux_ui_field(prop));
                } else {
                    fields.push(ux_synth_field(name, &c.fields_with_types));
                }
            }
            let input = UxPlanInput {
                entity_title: entity,
                fields: &fields,
                prop_by_name: prop_by_name
                    .iter()
                    .map(|(name, prop)| (name.as_str(), *prop))
                    .collect(),
                workflow_status_field: status_field,
                workflow_terminal_states: &[],
                has_soft_delete: false,
                user_pinned_list_order: false,
            };
            if let Some(plan) = build_ux_plan(Some(rules), &input)? {
                plans.insert(entity.to_string(), plan);
            }
        }
    }
    Ok(plans)
}

/// The plan-input display field names of a collection component: the
/// declared `fields` when present, else the typed-table column bindings
/// (expression columns carry no property and are skipped).
fn ux_plan_field_names(c: &IfmlComponent, props: &HashMap<String, PropertyNode>) -> Vec<String> {
    if !props.is_empty() {
        let mut names: Vec<String> = props
            .iter()
            .filter(|(key, prop)| prop.name == **key)
            .map(|(key, _)| key.clone())
            .collect();
        names.sort();
        return names;
    }
    if !c.fields.is_empty() {
        return c.fields.clone();
    }
    match &c.spec {
        Some(ComponentSpec::Table(spec)) => spec
            .columns
            .iter()
            .filter_map(|col| match col {
                rex_ifml::ColumnDef::Field { field, .. }
                | rex_ifml::ColumnDef::Lookup { field, .. } => Some(field.property.clone()),
                rex_ifml::ColumnDef::Expression { .. } => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The graph properties of an IFML component's bound entity keyed by
/// property name (mirroring the route generator's resolution).
async fn ux_props_for_entity(db: &dyn GraphQuerier, entity: &str) -> HashMap<String, PropertyNode> {
    let Some(title) = ux_schema_title(db, entity).await else {
        return HashMap::new();
    };
    let Ok(props) = db.get_properties(&title).await else {
        return HashMap::new();
    };
    props
        .into_iter()
        .map(|prop| (prop.name.clone(), prop))
        .collect()
}

/// Resolve the graph schema title for an IFML entity binding: exact match
/// first, then the `{entity}Type` shape.
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

/// The entity pipeline's UI-field projection of one graph property.
fn ux_ui_field(prop: &PropertyNode) -> crate::ui::page::UiField {
    crate::ui::form::ui_field_from_property(
        prop,
        column_is_entity_ref(prop),
        column_is_codelist(prop),
        &[],
        &[],
        &prop.pg_column_type,
        prop.pg_column_type.contains("RANGE"),
        false,
    )
}

fn column_is_codelist(prop: &PropertyNode) -> bool {
    matches!(
        prop.effective_kind(),
        Some(RefClassificationKind::CodelistReference) | Some(RefClassificationKind::CodelistCheck)
    )
}

fn column_is_entity_ref(prop: &PropertyNode) -> bool {
    prop.effective_kind() == Some(RefClassificationKind::EntityReference)
}

/// A signal-poor [`crate::ui::page::UiField`] for columns whose property is
/// absent from the graph.
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

fn render_spec(spec: &ViewTestSpec) -> String {
    let mut s = String::new();
    s.push_str("// Generated by codegraph. DO NOT EDIT.\n");
    s.push_str(&format!(
        "// IFML Playwright E2E tests for view {}.\n\n",
        spec.view_name
    ));
    s.push_str("import { test, expect } from '@playwright/test';\n\n");
    s.push_str(&format!(
        "test.describe('{}', () => {{\n",
        js_string(&spec.label)
    ));

    if let Some(render) = &spec.render {
        s.push_str(&format!(
            "\ttest('renders {}', async ({{ page }}) => {{\n",
            js_string(&render.label)
        ));
        s.push_str(&format!("\t\tawait page.goto('{}');\n", render.route));
        if render.assert_heading {
            s.push_str(&format!(
                "\t\tawait expect(page.getByRole('heading', {{ name: '{}' }})).toBeVisible();\n",
                js_string(&render.label)
            ));
        }
        s.push_str(&format!(
            "\t\tawait expect(page.getByTestId('{}')).toBeVisible();\n",
            render.primary_testid
        ));
        if let Some(nav_testid) = &render.nav_testid {
            s.push_str(&format!(
                "\t\tawait expect(page.getByTestId('{}')).toBeVisible();\n",
                nav_testid
            ));
        }
        if let Some(container_testid) = &render.container_testid {
            s.push_str(&format!(
                "\t\tawait expect(page.getByTestId('{}')).toBeVisible();\n",
                container_testid
            ));
        }
        s.push_str("\t});\n\n");
    }

    for persona in &spec.personas {
        let caps_lines = match &persona.capabilities {
            Some(caps) => format!(
                "\t\t\t(globalThis as any).__USER_CAPABILITIES__ = [{}];\n",
                caps.iter()
                    .map(|c| format!("'{}'", js_string(c)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => String::new(),
        };
        s.push_str(&format!(
            "\ttest('actor {} {} {}', async ({{ page }}) => {{\n",
            js_string(&persona.actor),
            if persona.permitted {
                "views"
            } else {
                "is redirected from"
            },
            js_string(&persona.label)
        ));
        s.push_str(&format!(
            "\t\tawait page.addInitScript(() => {{\n\t\t\t(globalThis as any).__USER_ROLES__ = ['{}'];\n{}\t\t}});\n",
            js_string(&persona.actor),
            caps_lines
        ));
        s.push_str(&format!("\t\tawait page.goto('{}');\n", persona.route));
        if persona.permitted {
            if persona.assert_heading {
                s.push_str(&format!(
                    "\t\tawait expect(page.getByRole('heading', {{ name: '{}' }})).toBeVisible();\n",
                    js_string(&persona.label)
                ));
            } else if let Some(testid) = &persona.primary_testid {
                s.push_str(&format!(
                    "\t\tawait expect(page.getByTestId('{}')).toBeVisible();\n",
                    testid
                ));
            } else {
                s.push_str(&format!(
                    "\t\tawait expect(page.getByRole('heading', {{ name: '{}' }})).toBeVisible();\n",
                    js_string(&persona.label)
                ));
            }
            if let Some(control_testid) = &persona.control_testid {
                s.push_str(&format!(
                    "\t\tawait expect(page.getByTestId('{}')).toBeVisible();\n",
                    control_testid
                ));
            }
        } else {
            s.push_str(&format!(
                "\t\tawait page.waitForURL('{}');\n",
                js_string(&persona.denial_target)
            ));
        }
        s.push_str("\t});\n\n");
    }

    for flow in &spec.click_throughs {
        s.push_str(&format!(
            "\ttest('{}', async ({{ page, request }}) => {{\n",
            js_string(&flow.title)
        ));
        s.push_str(&format!(
            "\t\tconst fixture = await request.post('{}', {{ data: {} }});\n",
            flow.fixture.base_path,
            flow.fixture.data_literal()
        ));
        s.push_str("\t\texpect(fixture.ok(), 'fixture entity should be created').toBeTruthy();\n");
        s.push_str(&format!("\t\tawait page.goto('{}');\n", flow.source_route));
        s.push_str(&format!(
            "\t\tawait page.getByTestId('{}').first().click();\n",
            flow.row_testid
        ));
        s.push_str(&format!(
            "\t\tawait page.waitForURL(new RegExp('{}'));\n",
            js_string(&flow.target_pattern)
        ));
        if let Some(modal) = &flow.modal {
            s.push_str(&format!(
                "\t\tawait expect(page.getByTestId('{}')).toBeVisible();\n",
                modal.wrapper_testid
            ));
            s.push_str(&format!(
                "\t\tawait page.getByTestId('{}').click();\n",
                modal.close_testid
            ));
            s.push_str(&format!(
                "\t\tawait page.waitForURL(new RegExp('{}'));\n",
                js_string(&modal.back_pattern)
            ));
        }
        s.push_str("\t});\n\n");
    }

    for validation in &spec.validations {
        s.push_str("\ttest('form validation blocks empty submit', async ({ page }) => {\n");
        s.push_str(&format!("\t\tawait page.goto('{}');\n", validation.route));
        s.push_str(&format!(
            "\t\tawait page.getByTestId('{}').click();\n",
            validation.submit_testid
        ));
        s.push_str(&format!(
            "\t\tawait expect(page.getByTestId('{}').locator('[name=\"{}\"]')).toHaveJSProperty('validity.valid', false);\n",
            validation.form_testid, validation.field
        ));
        s.push_str("\t});\n\n");
    }

    for round_trip in &spec.round_trips {
        s.push_str("\ttest('form round trip persists changes', async ({ page, request }) => {\n");
        s.push_str(&format!(
            "\t\tconst created = await (await request.post('{}', {{ data: {} }})).json();\n",
            round_trip.fixture.base_path,
            round_trip.fixture.data_literal()
        ));
        s.push_str(&format!(
            "\t\tawait page.goto(`{}?{}=${{created.data?.id ?? created.id}}`);\n",
            round_trip.route, round_trip.id_param
        ));
        s.push_str(&format!(
            "\t\tconst form = page.getByTestId('{}');\n",
            round_trip.form_testid
        ));
        s.push_str(&format!(
            "\t\tawait expect(form.locator('[name=\"{}\"]')).not.toHaveValue('');\n",
            round_trip.field
        ));
        s.push_str(&format!(
            "\t\tawait form.locator('[name=\"{}\"]').fill('{}');\n",
            round_trip.field, round_trip.updated
        ));
        s.push_str(&format!(
            "\t\tawait page.getByTestId('{}').click();\n",
            round_trip.submit_testid
        ));
        s.push_str(&format!(
            "\t\tawait page.waitForURL(new RegExp('{}'));\n",
            js_string(&round_trip.target_pattern)
        ));
        s.push_str(&format!(
            "\t\tconst persisted = await (await request.get(`{}/${{created.data?.id ?? created.id}}`)).json();\n",
            round_trip.fixture.base_path
        ));
        s.push_str(&format!(
            "\t\texpect((persisted.data ?? persisted).{}).toBe('{}');\n",
            round_trip.field, round_trip.updated
        ));
        s.push_str("\t});\n\n");
    }

    while s.ends_with("\n\n") {
        s.pop();
    }
    s.push_str("});\n");
    s
}

// ── Model helpers ────────────────────────────────────────────────────────────

/// The URL route for a view, matching the route generator's svelte output
/// (`/{view-name-lowercase}`).
fn view_route(name: &str) -> String {
    format!("/{}", name.to_lowercase())
}

fn has_tests(spec: &ViewTestSpec) -> bool {
    spec.render.is_some()
        || !spec.personas.is_empty()
        || !spec.click_throughs.is_empty()
        || !spec.validations.is_empty()
        || !spec.round_trips.is_empty()
}

fn component_kind(c: &IfmlComponent) -> String {
    match c.spec {
        Some(ComponentSpec::Table(_)) => "table".to_string(),
        Some(ComponentSpec::Form(_)) => "form".to_string(),
        Some(ComponentSpec::Chart(_)) => "chart".to_string(),
        None => c.component_type.clone(),
    }
}

fn is_collection(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Table(_)))
        || c.component_type == "list"
        || c.component_type == "table"
}

fn is_form(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Form(_))) || c.component_type == "form"
}

fn is_details(c: &IfmlComponent) -> bool {
    c.component_type == "details"
}

fn form_spec(c: &IfmlComponent) -> Option<&FormSpec> {
    match &c.spec {
        Some(ComponentSpec::Form(form)) => Some(form),
        _ => None,
    }
}

/// First collection component, else first component with a testable root.
fn primary_component<'a>(
    vc: &'a IfmlViewContainer,
    gen: &IfmlE2eTestGenerator,
) -> Option<(&'a IfmlComponent, ComponentSelectors, bool)> {
    let mut fallback = None;
    for c in &vc.components {
        let selectors = gen.selectors(vc, c);
        if selectors.root.is_none() {
            continue;
        }
        if is_collection(c) {
            return Some((c, selectors, true));
        }
        if fallback.is_none() {
            fallback = Some((c, selectors, false));
        }
    }
    fallback
}

/// Resolve the API surface for an entity only when a schema for it exists in
/// the graph. Without schemas (`ifml-generate` without `--schemas`) this
/// returns `None`, which keeps API-dependent test kinds out of the output.
async fn schema_backed_api(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity: &str,
    api_version: &str,
) -> Option<ResolvedApi> {
    let in_graph = matches!(db.get_schema(entity).await, Ok(Some(_)))
        || matches!(db.get_schema(&format!("{entity}Type")).await, Ok(Some(_)));
    if !in_graph {
        return None;
    }
    resolve_entity_api(db, config, entity, api_version).await
}

/// Fixture payload entries for a component: `(field, js_literal)` pairs for
/// the component's fields, skipping primary-key and FK fields. Typed form
/// specs declare fields outside `fields`, so they are used as a fallback;
/// their per-field `values` supply the first valid value for
/// codelist/dropdown fields.
fn fixture_entries(c: &IfmlComponent) -> Vec<(String, String)> {
    let names: Vec<String> = if c.fields.is_empty() {
        form_spec(c)
            .map(|f| f.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default()
    } else {
        c.fields.clone()
    };
    let types: std::collections::HashMap<&str, &str> = c
        .fields_with_types
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    let form_values: std::collections::HashMap<&str, &[String]> = form_spec(c)
        .map(|spec| {
            spec.fields
                .iter()
                .filter(|f| !f.values.is_empty())
                .map(|f| (f.name.as_str(), f.values.as_slice()))
                .collect()
        })
        .unwrap_or_default();
    names
        .iter()
        .filter(|f| f.as_str() != "id" && !f.ends_with("_id"))
        .map(|f| {
            let rust_type = types.get(f.as_str()).copied().unwrap_or("String");
            let value = match form_values.get(f.as_str()) {
                Some(values) if !values.is_empty() => format!("'{}'", js_string(&values[0])),
                _ => js_value_for_type(f, rust_type),
            };
            (f.clone(), value)
        })
        .collect()
}

/// First form field eligible for a round-trip modification: a plain string
/// field (skipping id/FK fields), with its fixture and updated values.
fn modifiable_field(c: &IfmlComponent, form: &FormSpec) -> Option<(String, String, String)> {
    let types: std::collections::HashMap<&str, &str> = c
        .fields_with_types
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    form.fields
        .iter()
        .filter(|f| f.name != "id" && !f.name.ends_with("_id"))
        .find(|f| {
            let rust_type = types
                .get(f.name.as_str())
                .copied()
                .unwrap_or("String")
                .to_ascii_lowercase();
            !rust_type.contains("bool")
                && ![
                    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
                ]
                .iter()
                .any(|n| rust_type.contains(n))
        })
        .map(|f| {
            let original = js_value_for_type(&f.name, "String");
            let updated = format!("Updated {}", f.name);
            (f.name.clone(), original, updated)
        })
}

fn js_value_for_type(field: &str, rust_type: &str) -> String {
    let t = rust_type.to_ascii_lowercase();
    if t.contains("bool") {
        return "true".to_string();
    }
    if crate::ifml::route_generator::is_numeric_rust_type(&t) {
        return "42".to_string();
    }
    if t.contains("datetime") || t.contains("timestamp") {
        return "'2024-01-15T10:30:00Z'".to_string();
    }
    format!("'Test {field}'")
}

/// The first enum value of the codelist backing `field` on `entity`'s
/// schema, when the property references one. `None` keeps the generic
/// fixture value.
async fn codelist_fixture_value(
    db: &dyn GraphQuerier,
    entity: Option<&str>,
    field: &str,
) -> Option<String> {
    let entity = entity?;
    let schema = match db.get_schema(entity).await {
        Ok(Some(schema)) => Some(schema),
        Ok(None) => db.get_schema(&format!("{entity}Type")).await.ok()?,
        Err(_) => None,
    }?;
    let props = db.get_properties(&schema.title).await.ok()?;
    let target = props
        .iter()
        .find(|p| p.name == field)?
        .ref_target
        .as_deref()?;
    let stem = target.rsplit('/').next()?.strip_suffix(".json")?;
    let codelist = db.get_schema(stem).await.ok()??;
    if !codelist.is_codelist {
        return None;
    }
    let values = db.get_enum_values(stem).await.ok()?;
    values.into_iter().next().map(|v| v.value)
}

/// The view-level or component-level `on save`/`on submit` navigation target:
/// `(target_view, bindings)`.
fn save_navigation_target(
    vc: &IfmlViewContainer,
    c: &IfmlComponent,
) -> Option<(String, std::collections::HashMap<String, String>)> {
    let is_save =
        |e: &super::context::IfmlEvent| e.event_type == "save" || e.event_type == "submit";
    let target = |a: &IfmlAction| match a {
        IfmlAction::Navigate { target, binding } => Some((target.clone(), binding.clone())),
        _ => None,
    };
    c.events
        .iter()
        .chain(vc.events.iter())
        .find(|e| is_save(e))
        .and_then(|e| target(&e.action))
}

/// Build a `RegExp` source matching the target URL: the route plus query
/// params for each binding key (sorted, mirroring the page's `nav_url_expr`
/// ordering). Binding expressions substitute to `[^&]+`; quoted literals to
/// their escaped text.
fn url_pattern(route: &str, binding: &std::collections::HashMap<String, String>) -> String {
    let escaped_route = escape_regex(route);
    if binding.is_empty() {
        return format!("{escaped_route}$");
    }
    let mut pairs: Vec<(&String, &String)> = binding.iter().collect();
    pairs.sort();
    let params = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", escape_regex(k), binding_value_pattern(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{escaped_route}\\?{params}")
}

fn binding_value_pattern(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        escape_regex(&trimmed[1..trimmed.len() - 1])
    } else {
        "[^&]+".to_string()
    }
}

/// URL pattern for navigation into a modal view: the `dialog=open` marker
/// is appended last, mirroring the route generator's `nav_url_expr` ordering.
fn url_pattern_with_dialog(
    route: &str,
    binding: &std::collections::HashMap<String, String>,
) -> String {
    if binding.is_empty() {
        return format!("{}\\?dialog=open", escape_regex(route));
    }
    format!("{}&dialog=open", url_pattern(route, binding))
}

fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if "\\.+*?()|[]{}^$#".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn url_pattern_sorts_bindings_and_substitutes_expressions() {
        let mut binding = HashMap::new();
        binding.insert("region".to_string(), "\"eu\"".to_string());
        binding.insert("customerId".to_string(), "row.id".to_string());
        assert_eq!(
            url_pattern("/customerdetail", &binding),
            "/customerdetail\\?customerId=[^&]+&region=eu"
        );
        assert_eq!(
            url_pattern("/customerlist", &HashMap::new()),
            "/customerlist$"
        );
    }

    #[test]
    fn url_pattern_with_dialog_appends_marker_last() {
        let mut binding = HashMap::new();
        binding.insert("customerId".to_string(), "row.id".to_string());
        assert_eq!(
            url_pattern_with_dialog("/customerdialog", &binding),
            "/customerdialog\\?customerId=[^&]+&dialog=open"
        );
        assert_eq!(
            url_pattern_with_dialog("/customerdialog", &HashMap::new()),
            "/customerdialog\\?dialog=open"
        );
    }

    #[test]
    fn modal_click_through_renders_wrapper_and_close_assertions() {
        let spec = ViewTestSpec {
            view_name: "CustomerList".to_string(),
            label: "Customer Management".to_string(),
            route: "/customerlist".to_string(),
            render: None,
            click_throughs: vec![ClickThroughTest {
                title: "comp_grid_select navigates to CustomerDialog".to_string(),
                source_route: "/customerlist".to_string(),
                row_testid: "grid-row".to_string(),
                fixture: Fixture {
                    base_path: "/api/v1/sales/customer".to_string(),
                    entries: vec![],
                },
                target_pattern: "/customerdialog\\?dialog=open".to_string(),
                modal: Some(ModalCloseAssertions {
                    wrapper_testid: "customer-modal".to_string(),
                    close_testid: "customerdialog-modal-close".to_string(),
                    back_pattern: "/customerlist$".to_string(),
                }),
            }],
            validations: Vec::new(),
            round_trips: Vec::new(),
            personas: Vec::new(),
        };

        let rendered = render_spec(&spec);
        assert!(
            rendered.contains("waitForURL(new RegExp('/customerdialog\\\\?dialog=open'))"),
            "{rendered}"
        );
        assert!(
            rendered.contains("expect(page.getByTestId('customer-modal')).toBeVisible()"),
            "{rendered}"
        );
        assert!(
            rendered.contains("page.getByTestId('customerdialog-modal-close').click()"),
            "{rendered}"
        );
        assert!(
            rendered.contains("waitForURL(new RegExp('/customerlist$'))"),
            "{rendered}"
        );
    }

    #[test]
    fn binding_value_pattern_handles_literals_and_expressions() {
        assert_eq!(binding_value_pattern("\"eu\""), "eu");
        assert_eq!(binding_value_pattern("row.id"), "[^&]+");
        assert_eq!(binding_value_pattern("a.b(c)"), "[^&]+");
    }

    #[test]
    fn escape_regex_escapes_metacharacters() {
        assert_eq!(escape_regex("/customer-detail"), "/customer-detail");
        assert_eq!(escape_regex("a.b*c"), "a\\.b\\*c");
    }

    #[test]
    fn js_value_follows_rust_types() {
        assert_eq!(js_value_for_type("active", "bool"), "true");
        assert_eq!(js_value_for_type("age", "Option< i32 >"), "42");
        assert_eq!(js_value_for_type("age", "i32"), "42");
        assert_eq!(js_value_for_type("name", "String"), "'Test name'");
        assert_eq!(
            js_value_for_type("submittedAt", "Option< DateTime < Utc > >"),
            "'2024-01-15T10:30:00Z'"
        );
    }

    #[test]
    fn spec_render_includes_all_test_kinds() {
        let spec = ViewTestSpec {
            view_name: "CustomerList".to_string(),
            label: "Customer Management".to_string(),
            route: "/customerlist".to_string(),
            render: Some(RenderTest {
                label: "Customer Management".to_string(),
                route: "/customerlist".to_string(),
                primary_testid: "grid-table".to_string(),
                assert_heading: true,
                nav_testid: Some("navigation-menu".to_string()),
                container_testid: Some("card".to_string()),
            }),
            click_throughs: vec![ClickThroughTest {
                title: "comp_grid_select navigates to CustomerDetail".to_string(),
                source_route: "/customerlist".to_string(),
                row_testid: "grid-row".to_string(),
                fixture: Fixture {
                    base_path: "/api/v1/sales/customer".to_string(),
                    entries: vec![("name".to_string(), "'Test name'".to_string())],
                },
                target_pattern: "/customerdetail\\?customerId=[^&]+".to_string(),
                modal: None,
            }],
            validations: Vec::new(),
            round_trips: Vec::new(),
            personas: Vec::new(),
        };

        let rendered = render_spec(&spec);
        assert!(rendered.contains("import { test, expect } from '@playwright/test';"));
        assert!(rendered.contains("test.describe('Customer Management'"));
        assert!(rendered.contains("page.goto('/customerlist')"));
        assert!(
            rendered.contains("page.getByRole('heading', { name: 'Customer Management' })"),
            "{rendered}"
        );
        assert!(rendered.contains("page.getByTestId('grid-table')"));
        assert!(
            rendered.contains("page.getByTestId('navigation-menu')"),
            "{rendered}"
        );
        assert!(rendered.contains("page.getByTestId('card')"), "{rendered}");
        assert!(rendered.contains("request.post('/api/v1/sales/customer'"));
        assert!(rendered.contains("page.getByTestId('grid-row').first().click()"));
        assert!(rendered.contains("waitForURL(new RegExp('/customerdetail\\\\?customerId=[^&]+'))"));
        assert!(rendered.ends_with("});\n"));
    }

    fn ux_test_fixture() -> UxViewTest {
        UxViewTest {
            component_name: "grid".to_string(),
            route: "/refundrequestlist".to_string(),
            fixture: Fixture {
                base_path: "/api/v1/refunds/refund-request".to_string(),
                entries: vec![
                    ("title".to_string(), "'Test title'".to_string()),
                    ("status".to_string(), "'draft'".to_string()),
                    ("amount".to_string(), "42".to_string()),
                    ("urgent".to_string(), "true".to_string()),
                    (
                        "submittedAt".to_string(),
                        "'2024-01-15T10:30:00Z'".to_string(),
                    ),
                ],
            },
            table_testid: "grid-table".to_string(),
            row_testid: "grid-row".to_string(),
            chip_checks: vec![
                UxChipCheck {
                    field: "status".to_string(),
                    override_literal: "'draft'".to_string(),
                    text: "draft".to_string(),
                    variant: "secondary".to_string(),
                },
                UxChipCheck {
                    field: "status".to_string(),
                    override_literal: "'approved'".to_string(),
                    text: "approved".to_string(),
                    variant: "default".to_string(),
                },
            ],
            column_checks: vec![
                UxColumnCheck {
                    header: Some("amount".to_string()),
                    cell_index: 2,
                    format: UxCellFormat::Money {
                        literal: "42".to_string(),
                    },
                    assert_right: true,
                },
                UxColumnCheck {
                    header: None,
                    cell_index: 4,
                    format: UxCellFormat::Date {
                        literal: "2024-01-15T10:30:00Z".to_string(),
                    },
                    assert_right: false,
                },
            ],
            copy_check: None,
            menu: Some(UxMenuCheck {
                trigger_testid: "grid-actions".to_string(),
                menu_testid: "grid-actions-menu".to_string(),
                target_pattern: "/refundrequestdetail\\?id=[^&]+".to_string(),
            }),
            timeline: None,
            locale: "en-NZ".to_string(),
            money_options: "{ style: 'currency', currency: 'NZD' }".to_string(),
        }
    }

    #[test]
    fn ux_spec_renders_chip_tone_and_intl_assertions() {
        let rendered = render_ux_spec(&ux_test_fixture());
        assert!(rendered.ends_with("});\n"), "{rendered}");
        // Pinned title prefixes — the gate's `ux` category matches these.
        assert!(
            rendered.contains("test('ux chips render with tone variants'"),
            "{rendered}"
        );
        assert!(
            rendered.contains("test('ux numeric columns align and format through Intl'"),
            "{rendered}"
        );
        assert!(
            rendered.contains("test('ux row actions open the overflow menu'"),
            "{rendered}"
        );
        // Fixture + in-spec Intl computation.
        assert!(
            rendered.contains("const UX_BASE = '/api/v1/refunds/refund-request';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("const UX_LOCALE = 'en-NZ';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("const UX_MONEY_OPTS = { style: 'currency', currency: 'NZD' };"),
            "{rendered}"
        );
        assert!(
            rendered.contains("new Intl.NumberFormat(UX_LOCALE, UX_MONEY_OPTS)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("new Intl.DateTimeFormat(UX_LOCALE, { dateStyle: 'medium' })"),
            "{rendered}"
        );
        // Balanced parens on the Intl text assertions (regression pin).
        assert!(
            rendered.contains("await expect(td).toHaveText(fmt.format(42));"),
            "{rendered}"
        );
        // Chip variant + visibility within the fixture row.
        assert!(
            rendered.contains("toHaveAttribute('data-chip-variant', 'secondary')"),
            "{rendered}"
        );
        assert!(
            rendered.contains("toHaveAttribute('data-chip-variant', 'default')"),
            "{rendered}"
        );
        assert!(rendered.contains("getByTestId('grid-chip')"), "{rendered}");
        // Numeric alignment classes + nth cell addressing.
        assert!(rendered.contains("toHaveClass(/text-right/)"), "{rendered}");
        assert!(
            rendered.contains("toHaveClass(/tabular-nums/)"),
            "{rendered}"
        );
        assert!(rendered.contains("row.locator('td').nth(2)"), "{rendered}");
        assert!(rendered.contains("row.locator('td').nth(4)"), "{rendered}");
        // Overflow menu open + first item navigation.
        assert!(
            rendered.contains("getByTestId('grid-actions').first().click()"),
            "{rendered}"
        );
        assert!(
            rendered.contains("getByTestId('grid-actions-menu')"),
            "{rendered}"
        );
        assert!(
            rendered.contains("waitForURL(new RegExp('/refundrequestdetail\\\\?id=[^&]+'))"),
            "{rendered}"
        );
    }

    #[test]
    fn ux_spec_omits_absent_features_and_renders_timeline() {
        let mut test = ux_test_fixture();
        test.chip_checks.clear();
        test.column_checks.clear();
        test.menu = None;
        test.timeline = Some(UxTimelineCheck {
            root_testid: "grid-timeline".to_string(),
            item_testid: "grid-timeline-item".to_string(),
            order_literal: Some("2024-01-15T10:30:00Z".to_string()),
        });
        let rendered = render_ux_spec(&test);
        assert!(!rendered.contains("ux chips render"), "{rendered}");
        assert!(!rendered.contains("ux numeric columns"), "{rendered}");
        assert!(!rendered.contains("overflow menu"), "{rendered}");
        assert!(!rendered.contains("UX_MONEY_OPTS)"), "{rendered}");
        assert!(
            rendered.contains("test('ux timeline renders entries newest-first'"),
            "{rendered}"
        );
        assert!(
            rendered.contains("getByTestId('grid-timeline-item')"),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("expect(times).toContain(fmt.format(new Date('2024-01-15T10:30:00Z')))"),
            "{rendered}"
        );
        assert!(
            rendered.contains("expect(parsed[i]).toBeLessThanOrEqual(parsed[i - 1]);"),
            "{rendered}"
        );
    }

    #[test]
    fn ux_menu_event_requires_a_second_navigate() {
        let navigate = |target: &str| IfmlEvent {
            name: String::new(),
            event_type: "select".to_string(),
            params: Vec::new(),
            requires: Vec::new(),
            action: IfmlAction::Navigate {
                target: target.to_string(),
                binding: HashMap::new(),
            },
        };
        let single = vec![navigate("Detail")];
        assert!(ux_menu_event(&single).is_none());
        let mut second = navigate("Review");
        second.event_type = "review".to_string();
        let pair = vec![navigate("Detail"), second];
        let menu = ux_menu_event(&pair).expect("second navigate discloses into the menu");
        match &menu.action {
            IfmlAction::Navigate { target, .. } => assert_eq!(target, "Review"),
            _ => panic!("expected a navigate action"),
        }
    }

    #[test]
    fn ux_chip_checks_skip_the_workflow_status_field() {
        use codegraph_core::mock::MockEngine;
        use codegraph_core::types::{EnumValue, SchemaNode};

        fn schema(title: &str, is_codelist: bool) -> SchemaNode {
            SchemaNode {
                namespace: None,
                schema_id: format!("id:{title}"),
                title: title.to_string(),
                description: None,
                schema_type: "object".to_string(),
                classification: "entity".to_string(),
                domain: Some("refunds".to_string()),
                rel_path: format!("{title}.json"),
                pg_type: "UUID".to_string(),
                rust_type: "Uuid".to_string(),
                sea_orm_type: "Uuid".to_string(),
                rust_type_name: title.to_string(),
                pg_table_name: codegraph_naming::to_snake_case(title),
                api_path_segment: codegraph_naming::to_kebab_case(title),
                parent_schema: None,
                is_entity: !is_codelist,
                is_codelist,
                is_primitive_wrapper: false,
                has_all_of: false,
                has_one_of: false,
                has_any_of: false,
                has_definitions: false,
                custom_annotations: Default::default(),
            }
        }

        let rules = codegraph_config::builtin_ux_rules().unwrap().rules;
        let mut status_prop = ux_test_prop("status");
        status_prop.classification_kind = Some(RefClassificationKind::CodelistReference);
        status_prop.ref_target = Some("codelist/RefundStatusCodeList.json".to_string());
        let field = ux_ui_field(&status_prop);
        let input = UxPlanInput {
            entity_title: "RefundRequest",
            fields: std::slice::from_ref(&field),
            prop_by_name: BTreeMap::from([("status", &status_prop)]),
            workflow_status_field: None,
            workflow_terminal_states: &[],
            has_soft_delete: false,
            user_pinned_list_order: false,
        };
        let plan = build_ux_plan(Some(&rules), &input)
            .unwrap()
            .expect("codelist column planned");
        assert!(
            matches!(plan.columns.get("status"), Some(col) if col.display == Display::Chip),
            "precondition: the codelist column resolves to a chip"
        );

        let c = IfmlComponent {
            name: "grid".to_string(),
            component_type: "list".to_string(),
            mode: None,
            entity: Some("RefundRequest".to_string()),
            fields: vec!["status".to_string()],
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec: None,
        };
        let entries = vec![("status".to_string(), "'draft'".to_string())];
        let db = MockEngine::builder()
            .with_schema(schema("RefundRequestType", false))
            .with_properties("RefundRequestType", vec![status_prop])
            .with_schema(schema("RefundStatusCodeList", true))
            .with_enum_values(
                "RefundStatusCodeList",
                vec![
                    EnumValue {
                        value: "draft".to_string(),
                        display_name: None,
                        sort_order: 0,
                    },
                    EnumValue {
                        value: "approved".to_string(),
                        display_name: None,
                        sort_order: 1,
                    },
                ],
            )
            .build();

        // Control: without a workflow the codelist chip is exercised with
        // distinct expected variants.
        let control = futures::executor::block_on(ux_chip_checks(&db, &c, &plan, &entries, None));
        assert!(
            !control.is_empty(),
            "control: the codelist chip column is exercised without a workflow"
        );
        // The workflow status column reads NULL on API-created rows (the
        // create DTO carries no status field), so a seeded override could
        // never reach the rendered chip — it must be skipped (issue #303).
        let skipped =
            futures::executor::block_on(ux_chip_checks(&db, &c, &plan, &entries, Some("status")));
        assert!(
            skipped.is_empty(),
            "the workflow status column must never be chip-exercised"
        );
    }

    fn ux_test_prop(name: &str) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: "string".into(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: name.into(),
            pg_column_type: "TEXT".into(),
            rust_field_name: name.into(),
            rust_field_type: "String".into(),
            sea_orm_type: "String".into(),
            render_strategy: "scalar".into(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
        }
    }

    #[test]
    fn ux_column_checks_pin_dimensions_and_alignment() {
        let rules = codegraph_config::parse_ux_rules_str(
            "[[column]]\nname_pattern = \"amount\"\ndimension = \"money\"\n",
        )
        .unwrap()
        .rules;
        let c = IfmlComponent {
            name: "grid".to_string(),
            component_type: "list".to_string(),
            mode: None,
            entity: Some("RefundRequest".to_string()),
            fields: vec!["title".to_string(), "amount".to_string()],
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec: None,
        };
        let field = crate::ui::page::UiField {
            name: "amount".to_string(),
            label: String::new(),
            ts_type: "number".to_string(),
            input_type: String::new(),
            is_required: false,
            is_array: false,
            is_entity_ref: false,
            is_immutable: false,
            is_codelist: false,
            is_range: false,
            codelist_values: Vec::new(),
            description: String::new(),
            pg_type: "BIGINT".to_string(),
            open_end: false,
            ref_api_path: None,
            structured_sub_fields: Vec::new(),
            nested_type_name: None,
        };
        let input = UxPlanInput {
            entity_title: "RefundRequest",
            fields: std::slice::from_ref(&field),
            prop_by_name: BTreeMap::new(),
            workflow_status_field: None,
            workflow_terminal_states: &[],
            has_soft_delete: false,
            user_pinned_list_order: false,
        };
        let plan = build_ux_plan(Some(&rules), &input)
            .unwrap()
            .expect("rules on → plan");
        let checks = ux_column_checks(&c, &plan, &[("amount".to_string(), "42".to_string())]);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].cell_index, 1);
        assert_eq!(checks[0].header.as_deref(), Some("amount"));
        assert!(checks[0].assert_right);
        assert!(matches!(checks[0].format, UxCellFormat::Money { .. }));
    }

    #[test]
    fn stale_cleanup_removes_ux_specs_by_suffix() {
        // Mirrors the suffix ordering in generate(): ux specs must be
        // matched BEFORE the generic `.spec.ts` fallback or active ux files
        // would be deleted as stale plain specs.
        let name = "refund-request-list.ux.spec.ts";
        assert!(name.strip_suffix(".workflow.spec.ts").is_none());
        assert!(name.strip_suffix(".ux.spec.ts").is_some());
        let plain = "refund-request-list.spec.ts";
        assert!(plain.strip_suffix(".ux.spec.ts").is_none());
        assert!(plain.strip_suffix(".workflow.spec.ts").is_none());
    }

    #[test]
    fn js_literal_inner_strips_quotes() {
        assert_eq!(js_literal_inner("'draft'"), "draft");
        assert_eq!(js_literal_inner("42"), "42");
        assert_eq!(js_literal_inner("true"), "true");
        assert_eq!(
            entry_inner(&[("a".into(), "'x y'".into())], "a"),
            Some("x y".into())
        );
        assert_eq!(entry_inner(&[("a".into(), "''".into())], "a"), None);
    }
}
