//! The machine read-model spec family (issue #817), split out of
//! `assembly` for the #751 file-length budget: the `{view}.machine.spec.ts`
//! planner, the generation-time edge projections the specs click, and the
//! action-legs helpers the journey/action planner shares. Same module
//! tree, same visibility rules (`super::` sees the parent's private
//! items).

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use super::super::super::api_paths::id_param_from;
use super::super::super::context::{IfmlComponent, IfmlViewContainer};
use super::super::super::route_generator::workflow_for_entity;
use super::super::super::selectors::{is_details, is_form};
use super::super::fixtures::{
    Fixture, codelist_fixture_value, fixture_entries, js_string, schema_backed_api,
};
use super::super::generator::IfmlE2eTestGenerator;

impl IfmlE2eTestGenerator {
    /// The machine read-model spec family (issue #817): one
    /// `{view}.machine.spec.ts` per view carrying a machine-bearing
    /// details/form component. The same shape the workflow tests walk —
    /// schema-backed, seedable, unguarded (the spec navigates without
    /// persona seeding; access is the actor-persona tests' job) — but the
    /// assertions derive from the READ MODEL, not fixture knowledge.
    pub(in crate::ifml::e2e_test) async fn build_view_machine_tests(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
    ) -> Vec<super::super::spec_payload::MachineTest> {
        if !vc.roles.is_empty() || !vc.requires.is_empty() {
            return Vec::new();
        }
        let id_param = id_param_from(&vc.params);
        let mut has_item = false;
        let mut has_form = false;
        let mut tests = Vec::new();
        for c in &vc.components {
            let is_details = is_details(c);
            let is_form = is_form(c);
            let fetch_item = is_details && !has_item && id_param.is_some();
            let fetch_form = is_form && !has_form && id_param.is_some();
            has_item |= fetch_item;
            has_form |= fetch_form;
            if !fetch_item && !fetch_form {
                continue;
            }
            if let Some(test) = self
                .build_machine_test(db, config, api_version, c, id_param.as_deref())
                .await
            {
                tests.push(test);
            }
        }
        tests
    }

    async fn build_machine_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        c: &IfmlComponent,
        id_param: Option<&str>,
    ) -> Option<super::super::spec_payload::MachineTest> {
        use super::super::spec_payload::{MachineTest, MachineTransitionLeg};
        use crate::machines::machine_read_model_surface_for_title;

        let entity = c.entity.as_deref()?;
        // The surface chain mirrors the route generator (plain, then
        // `{entity}Type`) — the UI and the specs cannot disagree about
        // which entities have a machine read model (issue #824: the SAME
        // loader rule the api plane emits by, so no spec drives a route
        // that was withheld). The workflow scope rides along (issue #818):
        // a dual entity specs the machine bound to the workflow's own
        // status field.
        let scope =
            |title: &str| workflow_for_entity(config, title).map(|wf| wf.status_field.clone());
        let surface = match machine_read_model_surface_for_title(
            db,
            config,
            entity,
            scope(entity).as_deref(),
        )
        .await
        {
            Some(s) => s,
            None => {
                let suffixed = format!("{entity}{}", config.defaults.type_suffix);
                machine_read_model_surface_for_title(
                    db,
                    config,
                    &suffixed,
                    scope(&suffixed).as_deref(),
                )
                .await?
            }
        };
        let api = schema_backed_api(db, config, entity, api_version).await?;
        if !api.has_create || !api.has_read {
            return None;
        }
        let ir: serde_json::Value = serde_json::from_str(&surface.ir_json).ok()?;
        let initial = surface.initial_state.clone();
        let offered = machine_edges_from(&ir, &initial);
        // The conflict target: provably NOT offered on the seeded row
        // (FreshFixtureGuard — never a guessed 409).
        let conflict_target = machine_conflict_target(&ir, &initial, &offered);
        // The transition leg needs a POST endpoint: dual entities only
        // (machine-only entities carry no transition surface).
        let dual = workflow_for_entity(config, entity).is_some();
        let transition = dual
            .then(|| offered.first())
            .flatten()
            .map(|edge| MachineTransitionLeg {
                to: edge.to.clone(),
            });

        let workflow = workflow_for_entity(config, entity);
        let status_field = workflow
            .as_ref()
            .map(|wf| wf.status_field.clone())
            .unwrap_or_else(|| surface.status_field.clone());
        let entries = fixture_entries(c);
        let entries = super::super::fixtures::strip_workflow_status(entries, Some(&status_field));
        let mut entries = entries;
        for (field, value) in entries.iter_mut() {
            if let Some(code) = codelist_fixture_value(db, c.entity.as_deref(), field).await {
                *value = format!("'{}'", js_string(&code));
            }
        }
        let entries = super::super::fixtures::with_required_entries(
            db,
            config,
            Some(entity),
            entries,
            self.dto_casing,
        )
        .await;

        Some(MachineTest {
            component_name: c.name.clone(),
            id_param: id_param.map(str::to_string),
            read_model_route: if dual { "workflow" } else { "machine" }.to_string(),
            initial_state: initial,
            transition,
            conflict_target,
            // The required-FK closure rides the SAME seeder the workflow
            // specs use — the specs must drive the entity through the API.
            fixture: self
                .attach_required_refs(
                    db,
                    config,
                    api_version,
                    Some(entity),
                    Fixture {
                        casing: self.dto_casing,
                        base_path: api.base_path,
                        entries,
                        deps: Vec::new(),
                    },
                )
                .await,
        })
    }
}

/// One offered edge of the machine's initial state (issue #817): the
/// generation-time projection the machine spec clicks.
pub(super) struct MachineEdge {
    pub event: String,
    pub to: String,
}

/// Projects the machine IR's edges FROM `state` (the kernel's own
/// derivation shape — target-resolved, declaration order, deduped).
pub(super) fn machine_edges_from(ir: &serde_json::Value, state: &str) -> Vec<MachineEdge> {
    let mut out: Vec<MachineEdge> = Vec::new();
    let Some(states) = ir.get("states").and_then(serde_json::Value::as_array) else {
        return out;
    };
    for s in states {
        if s.get("name").and_then(serde_json::Value::as_str) != Some(state) {
            continue;
        }
        for t in s
            .get("transitions")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let event = t
                .get("event")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let to = t
                .get("target")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    t.get("targets")
                        .and_then(serde_json::Value::as_array)
                        .and_then(|targets| targets.first())
                        .and_then(|first| first.as_str())
                        .map(|first| first.split('.').next().unwrap_or(first))
                })
                .unwrap_or_default()
                .to_string();
            if !out.iter().any(|e| e.event == event && e.to == to) {
                out.push(MachineEdge { event, to });
            }
        }
    }
    out
}

/// The not-offered state the 409 leg drives: a state the machine declares
/// but the current state's offered set does not target. Falls back to the
/// CURRENT state itself when no self-edge offers it (posting the current
/// state is then provably refused). `None` when every declared state is
/// offered — the leg omits (never a guessed conflict).
pub(super) fn machine_conflict_target(
    ir: &serde_json::Value,
    current: &str,
    offered: &[MachineEdge],
) -> Option<String> {
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
    let unoffered = |s: &str| !offered.iter().any(|e| e.to == s);
    states
        .iter()
        .find(|s| *s != current && unoffered(s))
        .cloned()
        .or_else(|| unoffered(current).then(|| current.to_string()))
}

/// The entity's machine, whole (the same rehydration the #449 surface
/// uses). `None` when the entity carries no `.ddd` design machine.
pub(super) async fn machine_for_entity(
    db: &dyn GraphQuerier,
    entity_title: &str,
) -> Option<phero_ir::ddd::StateMachine> {
    let surface = crate::ddd::design::DddDesignSurface::from_graph(db)
        .await
        .ok()?;
    surface.machine_for(entity_title)
}

/// The op-level `when` condition text for one op (the DDD design mirror).
pub(super) async fn operation_condition(
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
///
/// This is NOT the transition selector and must not become one (issue
/// #660): the op's transitions come from the route plane
/// ([`ExecutionKind::transition_state`]), which since #660 is the authored
/// `transition <Trigger>` binding or — for unbound models only — the
/// documented heuristics. This scan answers the narrower guard question
/// ("does the op-level `when` admit the initial state, so the act can flip
/// it for the 403 leg?"), which the binding does not answer.
pub(super) fn machine_condition_source_states(
    condition: &str,
    machine_field: &str,
    machine: &phero_ir::ddd::StateMachine,
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
pub(super) async fn entity_domain(
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
