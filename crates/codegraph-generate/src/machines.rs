//! Machine kernel (issue #815 — Lane 1 of epic #812).
//!
//! The compiled `.ddd` artifact embeds, per referenced machine, the
//! canonical `xstate_ir::Machine` JSON (`machine_ir`, keys sorted
//! recursively by the pherolang compiler) plus its provenance
//! (`machine_provenance`: authored path, machine id, content SHA-256).
//! This plane makes machines FIRST-CLASS serialized artifacts of a
//! generated app:
//!
//! - **Mirror** (Commit B): the blobs ride `DddDesignNode` through the
//!   graph (Mock + Grafeo), `GRAPH_FORMAT_VERSION` 14 → 15.
//! - **Fail-closed validation** (`machine_kernel_refusals`): a
//!   structurally broken machine (`xstate_core::validate_machine_json` —
//!   the restingstate fail-closed boundary) or a machine state outside
//!   its bound codelist enum (the enum correspondence is OURS — the
//!   artifact validates machine-internal structure only) refuses the run
//!   by name BEFORE any tree is written (`ValidationPass::check_machine_
//!   kernel`, the DD9 `query_refusals` posture).
//! - **Emitter** (`MachineKernelGenerator`, the `evt_events`/
//!   `media_storage` precedent — registers unconditionally,
//!   presence-gated inside): one
//!   `{domain_types}/src/machines/{domain}/{machine_id}.ir.json` per
//!   machine + a generated `machines/mod.rs` registry embedding each via
//!   `include_str!` with parse-once accessors. Static data only — no DB
//!   handle, no AppState, no async — so the module compiles on wasm32 and
//!   BOTH topologies reach it through the shared domain-types crate (the
//!   mox_ops placement precedent; worker crates already depend on
//!   domain-types).
//! - **Determinism**: the emitted JSON is the canonical serialization
//!   ([`canonical_ir_json`] — keys sorted recursively). The artifact's IR
//!   arrives canonical already, so embedding is byte-stable run to run
//!   even though the IR carries `HashMap`-borne structures (`SetupConfig`
//!   maps); Stage 2 drift checks must project through the same helper.
//!
//! Provenance (`machine_id`, authored source path, `content_sha256`) is
//! surfaced on the emitted module as consts — the drift-detection seam
//! Stages 2/3 consume.
//!
//! **Inline machines (issue #818 — Lane 4 of epic #812)**: a DSL machine
//! (`machine <field>: <Enum> { … }`) carries no artifact IR — only the raw
//! `machine_json` StateMachine mirror. The kernel covers it too through a
//! PURE, deterministic generation-time lowering
//! ([`inline_machine_to_ir`]): the same canonical-IR form a referenced
//! machine embeds, derived from the mirror. The emitter emits the lowered
//! IR like any other (`machines/{domain}/{id}.ir.json` + registry entry),
//! the same fail-closed validation doors apply (`validate_machine_json` +
//! the enum correspondence), and the route plane renders kernel probes
//! ([`codegraph_workflow::MachineRuntime::transition_for`]) against the
//! registry id — so inline machines execute through the MachineRuntime
//! exactly like referenced machines and the hand-rendered match arms are
//! gone. The raw mirror stays on the node for provenance/debug.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::DddDesignNode;
use heck::ToLowerCamelCase;
use serde_json::Value;
use xstate_core::xstate_ir::{ContextSchema, Machine, StateNode, StateType, Transition};

use crate::ProjectConfig;
use crate::error::{Error, Result};
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};

/// The canonical serialization of a machine IR: keys sorted recursively,
/// compact separators — the pherolang `machineIr` embed form (upstream
/// canonicalizes before embedding; re-projecting ANY machine JSON through
/// this helper is the determinism contract, because the IR carries
/// `HashMap`-borne structures whose iteration order varies run to run).
/// Stage 2 drift checks hash/compare THROUGH this helper, never raw
/// `serde_json` serialization.
pub fn canonical_ir_json(value: &Value) -> String {
    phero_ir::canonical_json(value)
}

/// Whether ANY machine in the graph names a guard — the scaffold's
/// `has_machine_guards` flag (the AppState `with_guards` wiring).
pub async fn graph_has_machine_guards(db: &dyn GraphQuerier) -> bool {
    db.get_ddd_models()
        .await
        .map(|models| {
            models.iter().any(|m| {
                m.designs.iter().any(|d| {
                    d.machine_ir_json
                        .as_ref()
                        .is_some_and(|ir| !machine_guard_names(ir).is_empty())
                })
            })
        })
        .unwrap_or(false)
}

/// Guard names a machine IR references: every transition's authored guard
/// (`{ name, params }`) plus every `setup.guards` declaration. Sorted and
/// deduped — the seam's stable vocabulary (issue #816).
pub fn machine_guard_names(ir: &Value) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    let visit_states = |states: &Vec<Value>, names: &mut BTreeSet<String>| {
        for state in states {
            if let Some(transitions) = state.get("transitions").and_then(Value::as_array) {
                for t in transitions {
                    if let Some(name) = t
                        .get("guard")
                        .and_then(|g| g.get("name"))
                        .and_then(Value::as_str)
                    {
                        names.insert(name.to_string());
                    }
                }
            }
            if let Some(children) = state.get("children").and_then(Value::as_array) {
                for child in children {
                    if let Some(transitions) = child.get("transitions").and_then(Value::as_array) {
                        for t in transitions {
                            if let Some(name) = t
                                .get("guard")
                                .and_then(|g| g.get("name"))
                                .and_then(Value::as_str)
                            {
                                names.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    };
    if let Some(states) = ir.get("states").and_then(Value::as_array) {
        visit_states(states, &mut names);
    }
    if let Some(setup_guards) = ir
        .get("setup")
        .and_then(|s| s.get("guards"))
        .and_then(Value::as_object)
    {
        for name in setup_guards.keys() {
            names.insert(name.clone());
        }
    }
    names.into_iter().collect()
}

/// Lower a `domains.toml` `[domains.x.entities.Y.workflow]` config into the
/// canonical xstate-ir Machine (issue #816, Decision 2 — Plane B seeding).
/// PURE and deterministic: states are the config's declared union (declared
/// `states` + `terminal_states` + every transition endpoint), sorted; an
/// empty `transitions` map lowers the documented default ("any non-terminal
/// state can transition to any other state"); terminal states lower as
/// final nodes; the machine's own data-guard/dual-status layers attach as
/// NAMED kernel guards per target state (`data_guard:{to}`, `dual_status:{to}`,
/// and `guards:{to}` when a target carries both) whose bodies the workflow
/// engine registers. The event of a lowered edge is its target state name —
/// the engine's API is target-addressed, and the kernel finds the edge by
/// (from, to).
pub fn workflow_config_to_machine(
    workflow: &codegraph_config::WorkflowConfig,
    machine_id: &str,
) -> Machine {
    // State universe: declared states + terminals + every endpoint, sorted.
    let mut states: BTreeSet<String> = BTreeSet::new();
    states.extend(workflow.states.iter().cloned());
    states.extend(workflow.terminal_states.iter().cloned());
    for (from, tos) in &workflow.transitions {
        states.insert(from.clone());
        states.extend(tos.iter().cloned());
    }
    states.insert(workflow.initial_state.clone());

    let terminal: BTreeSet<&str> = workflow
        .terminal_states
        .iter()
        .map(|s| s.as_str())
        .collect();

    // The (from → [to]) edge map: the config's own, or the documented
    // complete-graph default when the map is empty.
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if workflow.transitions.is_empty() {
        for from in &states {
            if terminal.contains(from.as_str()) {
                continue;
            }
            let tos: Vec<String> = states.iter().filter(|to| *to != from).cloned().collect();
            edges.insert(from.clone(), tos);
        }
    } else {
        for (from, tos) in &workflow.transitions {
            let mut sorted: Vec<String> = tos.clone();
            sorted.sort();
            edges.insert(from.clone(), sorted);
        }
    }

    // Deterministic guard naming per target state.
    let mut data_targets: BTreeSet<&str> = workflow
        .data_guards
        .iter()
        .map(|g| g.transition_to.as_str())
        .collect();
    let dual_targets: BTreeSet<&str> = workflow
        .dual_status_guards
        .keys()
        .map(|s| s.as_str())
        .collect();
    data_targets.extend(dual_targets.iter().copied());

    let guard_name_for = |target: &str| -> Option<String> {
        let has_data = workflow
            .data_guards
            .iter()
            .any(|g| g.transition_to == target);
        let has_dual = workflow.dual_status_guards.contains_key(target);
        match (has_data, has_dual) {
            (true, true) => Some(format!("guards:{target}")),
            (true, false) => Some(format!("data_guard:{target}")),
            (false, true) => Some(format!("dual_status:{target}")),
            (false, false) => None,
        }
    };

    let mut state_nodes: Vec<StateNode> = Vec::new();
    for name in &states {
        let transitions: Vec<Transition> = edges
            .get(name)
            .map(|tos| {
                tos.iter()
                    .map(|to| Transition {
                        event: to.clone(),
                        target: Some(to.clone()),
                        targets: Vec::new(),
                        guard: guard_name_for(to).map(|guard_name| xstate_core::xstate_ir::Guard {
                            name: guard_name,
                            params: None,
                        }),
                        actions: Vec::new(),
                        description: None,
                        delay_ms: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        state_nodes.push(StateNode {
            name: name.clone(),
            description: None,
            initial: None,
            entry: Vec::new(),
            exit: Vec::new(),
            transitions,
            children: Vec::new(),
            node_type: if terminal.contains(name.as_str()) {
                StateType::Final
            } else {
                StateType::Atomic
            },
            tags: Vec::new(),
            invoke: None,
            on_done: Vec::new(),
            output: None,
        });
    }

    Machine {
        id: machine_id.to_string(),
        initial: workflow.initial_state.clone(),
        context: ContextSchema { fields: Vec::new() },
        states: state_nodes,
        setup: None,
    }
}

/// The two-representation cross-check (issue #816, Decision 2): when BOTH
/// an artifact machine and a `WorkflowConfig` bind the same entity, their
/// state/transition/initial surfaces must AGREE. Disagreement is a hard
/// generation error naming BOTH sources — the two-representation drift
/// class dies here. Comparison is by (from, to) edges and state names,
/// never by event spelling (authored triggers differ from the lowered
/// target-name events by design) or guards (the config's engine-level
/// layers attach only to lowered edges).
pub fn compare_machine_surfaces(
    artifact_ir: &Value,
    lowered_ir: &Value,
    artifact_source: &str,
    config_source: &str,
) -> std::result::Result<(), String> {
    let parse = |value: &Value, source: &str| -> std::result::Result<Machine, String> {
        serde_json::from_value::<Machine>(value.clone()).map_err(|e| {
            format!("machine from {source} does not parse as an xstate-ir Machine: {e}")
        })
    };
    let artifact = parse(artifact_ir, artifact_source)?;
    let lowered = parse(lowered_ir, config_source)?;

    fn state_set(m: &Machine) -> BTreeSet<&str> {
        m.states.iter().map(|s| s.name.as_str()).collect()
    }
    fn edge_set(m: &Machine) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for state in &m.states {
            for t in &state.transitions {
                if let Some(to) = &t.target {
                    out.insert(format!("{}->{}", state.name, to));
                }
            }
        }
        out
    }

    let mut disagreements: Vec<String> = Vec::new();
    let artifact_states = state_set(&artifact);
    let lowered_states = state_set(&lowered);
    let artifact_only: Vec<&str> = artifact_states
        .difference(&lowered_states)
        .copied()
        .collect();
    let lowered_only: Vec<&str> = lowered_states
        .difference(&artifact_states)
        .copied()
        .collect();
    if !artifact_only.is_empty() {
        disagreements.push(format!(
            "states only the artifact machine declares: [{}]",
            artifact_only.join(", ")
        ));
    }
    if !lowered_only.is_empty() {
        disagreements.push(format!(
            "states only the config declares: [{}]",
            lowered_only.join(", ")
        ));
    }

    let artifact_edges = edge_set(&artifact);
    let lowered_edges = edge_set(&lowered);
    let artifact_only_edges: Vec<&String> = artifact_edges.difference(&lowered_edges).collect();
    let lowered_only_edges: Vec<&String> = lowered_edges.difference(&artifact_edges).collect();
    if !artifact_only_edges.is_empty() {
        disagreements.push(format!(
            "transitions only the artifact machine declares: [{}]",
            artifact_only_edges
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !lowered_only_edges.is_empty() {
        disagreements.push(format!(
            "transitions only the config declares: [{}]",
            lowered_only_edges
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if artifact.initial != lowered.initial {
        disagreements.push(format!(
            "initial state: artifact says `{}`, config says `{}`",
            artifact.initial, lowered.initial
        ));
    }

    if disagreements.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the artifact machine and the WorkflowConfig disagree for the same entity — the \
             two representations must agree (fix the model or the config; the artifact machine \
             is authoritative for the state surface):\n  artifact: {artifact_source}\n  config: \
             {config_source}\n{}",
            disagreements
                .iter()
                .map(|d| format!("  - {d}"))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

// ── Inline machines: the generation-time lowering (issue #818) ────────

/// The registry id of an INLINE machine (a DSL mirror with no kernel IR):
/// `{module}_{field}`, both snake_cased — deterministic, a safe file stem,
/// and unique per (module, field). The kernel emitter and the route plane
/// derive the SAME id through this one helper, so the emitted embed and
/// the rendered probe can never disagree.
#[must_use]
pub fn inline_machine_id(module: &str, field: &str) -> String {
    format!(
        "{}_{}",
        codegraph_naming::to_snake_case(module),
        codegraph_naming::to_snake_case(field)
    )
}

/// The kernel machine id of a design: the artifact's authored id for a
/// REFERENCED machine (the provenance `machineId`, else the IR's own
/// `id`), the derived [`inline_machine_id`] for a DSL mirror. `None` when
/// the design carries no machine, or a referenced IR whose id is
/// unresolved (the refusals walker names the defect).
#[must_use]
pub fn machine_kernel_id(design: &DddDesignNode) -> Option<String> {
    if design.machine_ir_json.is_some() {
        return machine_id_of(design);
    }
    let mirror = design.machine_json.as_ref()?;
    let field = mirror.get("field").and_then(Value::as_str)?;
    Some(inline_machine_id(&design.module, field))
}

/// The kernel IR JSON of a design: the artifact embed when present, else
/// the deterministic lowering of the DSL mirror ([`inline_machine_to_ir`]
/// through [`canonical_ir_json`]). `Ok(None)` when the design carries no
/// machine; `Err` carries the named refusal (a mirror that does not
/// deserialize — the refusals walker and the emitter both surface it).
///
/// # Errors
/// The mirror fails to deserialize as a `phero_ir::ddd::StateMachine`, or
/// the lowered machine fails to serialize (a defect seam).
pub fn machine_ir_for_design(design: &DddDesignNode) -> std::result::Result<Option<Value>, String> {
    if let Some(ir) = &design.machine_ir_json {
        return Ok(Some(ir.clone()));
    }
    let Some(mirror) = &design.machine_json else {
        return Ok(None);
    };
    let machine: phero_ir::ddd::StateMachine =
        serde_json::from_value(mirror.clone()).map_err(|e| {
            format!(
                "the machine mirror of `{}` (module `{}`) does not deserialize as a \
                 StateMachine ({e}) — the artifact is stale or hand-edited; recompile the model",
                design.class, design.module
            )
        })?;
    let id = machine_kernel_id(design).ok_or_else(|| {
        format!(
            "the machine mirror of `{}` (module `{}`) names no state field — the kernel \
             id cannot be derived",
            design.class, design.module
        )
    })?;
    let lowered = inline_machine_to_ir(&machine, &id);
    serde_json::to_value(&lowered).map(Some).map_err(|e| {
        format!(
            "the lowered machine `{id}` (module `{}`) does not serialize: {e}",
            design.module
        )
    })
}

/// The PURE, deterministic lowering of a DSL machine mirror
/// (`phero_ir::ddd::StateMachine` — the `machine_json` node blob) into the
/// canonical xstate-ir [`Machine`], so an INLINE machine executes through
/// the kernel exactly like a referenced one (issue #818, Option A plane).
///
/// Determinism contract: states in the mirror's canonical order
/// ([`phero_ir::ddd::StateMachine::states`] — initial, terminals,
/// transition endpoints by first appearance), each state's out-edges in
/// declaration order, and the event of an edge is the authored `via`
/// trigger verbatim or the derived lowerCamel of the target state — the
/// SAME vocabulary `ddd::service_ops::transitions::declared_triggers`
/// resolves the #660 bindings against, so the kernel's events and the
/// binding plane's triggers cannot drift. Terminal states lower as final
/// nodes. Inline `when` conditions on transitions are NOT lowered (no
/// kernel guard form exists for them) — they stay mirror-provenance,
/// exactly as the pre-#818 match arms ignored them (recorded deferral).
#[must_use]
pub fn inline_machine_to_ir(machine: &phero_ir::ddd::StateMachine, machine_id: &str) -> Machine {
    let terminals: BTreeSet<&str> = machine.terminals.iter().map(|s| s.as_str()).collect();
    let mut state_nodes: Vec<StateNode> = Vec::new();
    for name in machine.states() {
        let transitions: Vec<Transition> = machine
            .transitions
            .iter()
            .filter(|t| t.from == name)
            .map(|t| Transition {
                event: t
                    .trigger
                    .clone()
                    .unwrap_or_else(|| t.to.to_lower_camel_case()),
                target: Some(t.to.clone()),
                targets: Vec::new(),
                guard: None,
                actions: Vec::new(),
                description: None,
                delay_ms: None,
            })
            .collect();
        state_nodes.push(StateNode {
            name: name.to_string(),
            description: None,
            initial: None,
            entry: Vec::new(),
            exit: Vec::new(),
            transitions,
            children: Vec::new(),
            node_type: if terminals.contains(name) {
                StateType::Final
            } else {
                StateType::Atomic
            },
            tags: Vec::new(),
            invoke: None,
            on_done: Vec::new(),
            output: None,
        });
    }
    Machine {
        id: machine_id.to_string(),
        initial: machine.initial.clone(),
        context: ContextSchema { fields: Vec::new() },
        states: state_nodes,
        setup: None,
    }
}

/// The machine read-model surface of ONE entity (issue #817): the
/// entity→machine binding the api and IFML planes consume. Resolved from
/// the graph with the SAME predicate the workflow seed uses
/// (`find_artifact_machine`: `resolved_title` equality), requiring BOTH
/// the artifact IR (the kernel registry's embed) and the DSL mirror (the
/// status field the read model reads). `machine_hash` is the FNV-1a
/// digest over the canonical IR — the SAME digest the workflow-definition
/// envelope records, so a UI render can detect drift against either
/// plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMachineSurface {
    /// The authored `createMachine` id — the registry key.
    pub machine_id: String,
    /// The declaring design's module (the kernel's `machines/{domain}/`
    /// segment).
    pub domain: String,
    /// The state feature the machine binds (the DSL mirror's `field`) —
    /// the entity column the read model's `current_state` reads.
    pub status_field: String,
    /// The bound enum type name, when the mirror records one.
    pub enum_name: Option<String>,
    /// The canonical IR JSON (the registry's embedded bytes).
    pub ir_json: String,
    /// FNV-1a 64 hex over the canonical IR (the drift hash).
    pub machine_hash: String,
    /// The machine's final (terminal) states, sorted.
    pub terminal_states: Vec<String>,
    /// The machine's initial state.
    pub initial_state: String,
}

/// Resolves the machine surface bound to `title` (the schema title), if
/// any. Graph-driven: the entity's `.ddd` design must carry a machine —
/// the artifact IR of a referenced machine OR the lowered IR of an inline
/// DSL machine (issue #818; the lowering is total, so a mirror is always
/// enough), PLUS the DSL mirror (the status field the read model reads).
/// `None` when the entity is machine-less — the caller's presence gate
/// (no machine ⇒ no route, byte-identical).
///
/// `workflow_status_field` scopes the pick (issue #818): an entity whose
/// workflow config binds `status` must surface THE MACHINE BOUND TO THAT
/// FIELD — a machine on another field (an `escalation` machine beside a
/// `status` workflow) drives a different transition plane, and rendering
/// its edges through the workflow's buttons/payload would promise
/// transitions the backend 409s. `Some(field)` therefore requires the
/// match (no fallback to a foreign field's machine); `None` takes the
/// entity's first machine (no workflow config — every machine is the
/// transition plane).
pub async fn machine_surface_for_entity(
    db: &dyn GraphQuerier,
    title: &str,
    workflow_status_field: Option<&str>,
) -> Option<EntityMachineSurface> {
    let models = db.get_ddd_models().await.ok()?;
    let mut first: Option<EntityMachineSurface> = None;
    for model in &models {
        for design in &model.designs {
            if design.resolved_title.as_deref() != Some(title) {
                continue;
            }
            let Some(ir) = machine_ir_for_design(design).ok().flatten() else {
                continue;
            };
            // The status field comes from the DSL mirror; a design with
            // only the IR (a stale/hand-built artifact) names no column to
            // read — skipped, never guessed (the Lane-1 posture).
            let Some(mirror) = &design.machine_json else {
                continue;
            };
            let Some(status_field) = mirror.get("field").and_then(Value::as_str) else {
                continue;
            };
            // The workflow-field scope: a machine on another field never
            // qualifies (see the doc comment).
            if let Some(field) = workflow_status_field
                && status_field != field
            {
                continue;
            }
            let Some(machine_id) = machine_kernel_id(design) else {
                continue;
            };
            let canonical = canonical_ir_json(&ir);
            let terminal_states: Vec<String> = ir
                .get("states")
                .and_then(Value::as_array)
                .map(|states| {
                    states
                        .iter()
                        .filter(|s| s.get("type").and_then(Value::as_str) == Some("final"))
                        .filter_map(|s| s.get("name").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let mut terminal_states = terminal_states;
            terminal_states.sort();
            let surface = Some(EntityMachineSurface {
                machine_id,
                domain: design.module.clone(),
                status_field: status_field.to_string(),
                enum_name: bound_enum_of(design),
                machine_hash: fnv1a_64_hex(canonical.as_bytes()),
                ir_json: canonical,
                terminal_states,
                initial_state: ir
                    .get("initial")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            });
            if workflow_status_field.is_some() {
                // The scoped pick: the first matching field wins.
                return surface;
            }
            if first.is_none() {
                first = surface;
            }
        }
    }
    first
}

/// Whether the machine read model has a LOADER (issue #824). The read
/// model reads the status column off the row the query handler's UNSCOPED
/// `find_by_id` returns — the method `ddd/query.tera` emits exactly for an
/// entity with a `read` op and no parent reference. A parent-scoped member
/// gets `find_by_id_scoped` instead (it loads through its aggregate, a
/// different route shape — the recorded follow-up), and a design without a
/// repository resolves to an EMPTY op set with no loader at all.
/// `machine_read.tera` used to render for every machine-bearing design
/// regardless, and both shapes failed a consumer's build (`E0599`: no
/// method named `find_by_id`).
///
/// A machine-bearing entity outside the shape keeps its kernel — the
/// registry embed, the workflow seed, the guards. Only the read model is
/// withheld, and the run says so
/// ([`machine_read_model_withheld_warnings`]).
#[must_use]
pub const fn machine_read_model_has_loader(has_read: bool, has_parent_ref: bool) -> bool {
    has_read && !has_parent_ref
}

/// The machine read-model surface of `title` (issues #817 + #824):
/// [`machine_surface_for_entity`] narrowed to what the read model can
/// actually read — the entity binds a machine AND its query plane exposes
/// the loader ([`machine_read_model_has_loader`]). `None` when either is
/// missing, so the route module, the `/workflow` merge, the router and
/// OpenAPI registrations, the API contract and the UI's read-model fetches
/// are withheld TOGETHER: every consumer asks this one function, and no
/// plane can promise a read model another plane did not emit.
///
/// Topology is the caller's fact, not this function's. The api plane
/// mounts the `/machine` ROUTE on the monolith only (the #703 posture);
/// the `/workflow` merge and the IFML plane read the model on both
/// topologies.
pub async fn machine_read_model_surface_for_entity(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    title: &str,
    workflow_status_field: Option<&str>,
) -> Option<EntityMachineSurface> {
    let surface = machine_surface_for_entity(db, title, workflow_status_field).await?;
    if machine_read_model_loader_facts(db, config, domain, title)
        .await
        .is_some_and(|(has_read, has_parent_ref)| {
            machine_read_model_has_loader(has_read, has_parent_ref)
        })
    {
        Some(surface)
    } else {
        None
    }
}

/// [`machine_read_model_surface_for_entity`] for a caller that knows the
/// title but not its domain (the IFML planes): the schema's own domain
/// resolves it. A title that resolves to no schemaed domain has no loader.
pub async fn machine_read_model_surface_for_title(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    title: &str,
    workflow_status_field: Option<&str>,
) -> Option<EntityMachineSurface> {
    let domain = schema_domain(db, title).await?;
    machine_read_model_surface_for_entity(db, config, &domain, title, workflow_status_field).await
}

/// The machine-bearing entity titles whose read model has a loader (issue
/// #824) — [`machine_bearing_titles`] narrowed by
/// [`machine_read_model_has_loader`]. The set every e2e plane that keys on
/// "this entity has a read model" walks.
pub async fn machine_read_model_titles(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> HashSet<String> {
    let mut titles = HashSet::new();
    for title in machine_bearing_titles(db).await {
        if let Some(domain) = schema_domain(db, &title).await
            && machine_read_model_loader_facts(db, config, &domain, &title)
                .await
                .is_some_and(|(has_read, has_parent_ref)| {
                    machine_read_model_has_loader(has_read, has_parent_ref)
                })
        {
            titles.insert(title);
        }
    }
    titles
}

/// One Warning per machine-bearing design whose read model is withheld for
/// want of a loader (issue #824) — never a silent omission. Each names the
/// class and the reason: no `read` op (a design without a repository
/// resolves to an empty op set), or a parent-scoped member (the read model
/// has no scoped loader yet — the recorded follow-up).
pub async fn machine_read_model_withheld_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut titles: Vec<String> = machine_bearing_titles(db).await.into_iter().collect();
    titles.sort();
    for title in titles {
        let facts = match schema_domain(db, &title).await {
            Some(domain) => machine_read_model_loader_facts(db, config, &domain, &title).await,
            None => None,
        };
        let (servable, reason) = match facts {
            None => (false, "resolves to no schema".to_string()),
            Some((has_read, has_parent_ref)) => {
                // The SAME predicate the emitters withhold by — the Warning
                // cannot disagree with the emission.
                let reason = if !has_read {
                    [
                        "its query plane serves no `read` op (a design without a repository",
                        "resolves to an empty op set), so there is no `find_by_id` for the read",
                        "model to load through",
                    ]
                    .join(" ")
                } else if has_parent_ref {
                    [
                        "it is a parent-scoped member: its query handler loads through",
                        "`find_by_id_scoped`, and the read model has no scoped loader yet (the",
                        "scoped loader is the recorded follow-up)",
                    ]
                    .join(" ")
                } else {
                    String::new()
                };
                (
                    machine_read_model_has_loader(has_read, has_parent_ref),
                    reason,
                )
            }
        };
        if servable {
            continue;
        }
        warnings.push(format!(
            "design `{title}` carries a machine but {reason} — the machine read model (route, \
             `/workflow` merge, UI) is withheld (#824)"
        ));
    }
    warnings
}

/// The schema's own domain, when the graph knows the title.
async fn schema_domain(db: &dyn GraphQuerier, title: &str) -> Option<String> {
    db.get_schema(title)
        .await
        .ok()
        .flatten()
        .and_then(|schema| schema.domain)
}

/// The loader facts resolved from the graph — the same two facts
/// `ddd/query.tera` reads: the op set the query plane serves (the `read`
/// op gates `find_by_id`) and the parent reference (which switches the
/// handler to the scoped loader). `None` when the graph carries no schema
/// for the title in `domain`.
async fn machine_read_model_loader_facts(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    machine_title: &str,
) -> Option<(bool, bool)> {
    let schema = db
        .get_schema_in_domain(machine_title, domain)
        .await
        .ok()
        .flatten()?;
    let entity_name = schema.rust_type_name;
    let design_surface = crate::ddd::design::DddDesignSurface::from_graph(db)
        .await
        .ok()?;
    let has_read = crate::ddd::query::effective_operations(
        db,
        &design_surface,
        config,
        domain,
        machine_title,
        &entity_name,
    )
    .await
    .iter()
    .any(|op| op == "read");
    if !has_read {
        // No loader either way; the parent-candidate read is not needed.
        return Some((false, false));
    }
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(&entity_name));
    let parent_candidates = db.get_parent_candidates().await.ok()?;
    let has_parent_ref = crate::resolve_parent_fk_column_same_domain(
        machine_title,
        &parent_candidates,
        entity_cfg,
        domain,
        config,
        db,
    )
    .await
    .is_some();
    Some((has_read, has_parent_ref))
}

/// FNV-1a 64-bit lowercase hex — the inline digest every plane uses (the
/// ops harness, the workflow seed's envelope, the kernel). No new
/// dependency.
fn fnv1a_64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Whether ANY entity in the generation order is machine-bearing — a
/// bulk fact the e2e generators use to skip whole passes. One graph walk.
/// Effective presence: the artifact IR of a referenced machine or the
/// mirror of an inline one (the lowering is total — issue #818).
pub async fn graph_has_machine_entities(db: &dyn GraphQuerier) -> bool {
    db.get_ddd_models()
        .await
        .map(|models| {
            models
                .iter()
                .any(|m| m.designs.iter().any(design_has_machine))
        })
        .unwrap_or(false)
}

/// The machine-bearing entity titles (issue #817): the `resolved_title`
/// of every design carrying a machine — effective presence (artifact IR
/// or inline mirror, issue #818). One graph walk (the caller reuses it
/// per view).
pub async fn machine_bearing_titles(db: &dyn GraphQuerier) -> std::collections::HashSet<String> {
    db.get_ddd_models()
        .await
        .map(|models| {
            models
                .iter()
                .flat_map(|m| m.designs.iter())
                .filter_map(|d| {
                    design_has_machine(d)
                        .then(|| d.resolved_title.clone())
                        .flatten()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a design carries a machine under the EFFECTIVE presence rule
/// (issue #818): the artifact IR of a referenced machine or the DSL
/// mirror of an inline one (whose lowered IR always exists — the
/// lowering is total). Every bulk fact keys on this one predicate so the
/// emitter, the scaffold declaration and the route plane cannot disagree.
#[must_use]
pub fn design_has_machine(design: &DddDesignNode) -> bool {
    design.machine_ir_json.is_some() || design.machine_json.is_some()
}

/// One machine to emit, collected from the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineEntry {
    /// The authored `createMachine` id — the registry key and the file
    /// stem. Application-unique (duplicates refuse).
    pub machine_id: String,
    /// The declaring design's module — the `.ddd` module = domain
    /// convention, and the `machines/{domain}/…` directory segment.
    pub domain: String,
    /// The provenance path (the authored TS file as written in the
    /// `.ddd`), when the design carries provenance.
    pub source_path: Option<String>,
    /// The provenance SHA-256 over the canonical IR.
    pub content_sha256: Option<String>,
    /// The canonical IR JSON (the embedded bytes).
    pub ir_json: String,
}

/// The machine id exactly as the design records it: the provenance's
/// `machineId` when present, else the IR's own `id`.
fn machine_id_of(design: &DddDesignNode) -> Option<String> {
    if let Some(id) = design
        .machine_provenance_json
        .as_ref()
        .and_then(|prov| prov.get("machineId"))
        .and_then(Value::as_str)
    {
        return Some(id.to_string());
    }
    design
        .machine_ir_json
        .as_ref()
        .and_then(|ir| ir.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The bound enum type name of the design's DSL machine mirror
/// (`machine <field>: <Enum>`), when the design carries one.
fn bound_enum_of(design: &DddDesignNode) -> Option<String> {
    let machine = design.machine_json.as_ref()?;
    let enum_type = machine.get("enumType")?;
    match enum_type {
        Value::String(name) => Some(name.clone()),
        Value::Object(map) => map
            .get("value")
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

/// A machine id must be a safe file stem: no separators, no `..`, no
/// control characters — it becomes `{machine_id}.ir.json` under
/// `machines/{domain}/`.
fn machine_id_is_file_safe(machine_id: &str) -> bool {
    !machine_id.is_empty()
        && machine_id != "."
        && machine_id != ".."
        && !machine_id.contains(['/', '\\'])
        && !machine_id.chars().any(char::is_control)
        && !machine_id.contains("..")
}

/// Collect the machine entries of the graph, sorted by (domain,
/// machine_id), refusing duplicates. Effective presence: a referenced
/// machine's artifact IR or an inline machine's lowered IR (issue #818).
/// Designs whose mirror does not lower are skipped — the refusals walker
/// names the defect and the run refuses before any tree.
async fn collect_machine_entries(
    db: &dyn GraphQuerier,
) -> std::result::Result<Vec<MachineEntry>, GraphError> {
    let models = db.get_ddd_models().await?;
    let mut entries: Vec<MachineEntry> = Vec::new();
    for model in &models {
        for design in &model.designs {
            let Some(ir) = machine_ir_for_design(design).ok().flatten() else {
                continue;
            };
            let Some(machine_id) = machine_kernel_id(design) else {
                // The refusals walker names the defect; collection skips.
                continue;
            };
            let provenance = design.machine_provenance_json.as_ref();
            entries.push(MachineEntry {
                machine_id,
                domain: design.module.clone(),
                source_path: provenance
                    .and_then(|p| p.get("path"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content_sha256: provenance
                    .and_then(|p| p.get("contentSha256"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                ir_json: canonical_ir_json(&ir),
            });
        }
    }
    entries.sort_by(|a, b| (&a.domain, &a.machine_id).cmp(&(&b.domain, &b.machine_id)));
    Ok(entries)
}

/// Fail-closed validation (issue #815): every referenced machine must be
/// structurally valid (the restingstate boundary), its provenance must
/// agree with the IR, its id must be a safe file stem, and every machine
/// state must be a value of the bound codelist enum. Returned messages
/// are HARD pre-generation errors (`ValidationPass::check_machine_kernel`)
/// AND the emitter's own gate — a broken machine never reaches a tree.
pub async fn machine_kernel_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> std::result::Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    let suffix = &config.defaults.type_suffix;
    let mut refusals = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut const_names: BTreeMap<String, String> = BTreeMap::new();
    let mut guard_methods: BTreeMap<String, String> = BTreeMap::new();

    for model in &models {
        for design in &model.designs {
            // Effective IR: the artifact embed, or the inline lowering
            // (issue #818). An unlowerable mirror refuses by name.
            let ir = match machine_ir_for_design(design) {
                Ok(Some(ir)) => ir,
                Ok(None) => continue,
                Err(reason) => {
                    refusals.push(format!(
                        "machine of class `{}` (module `{}`): {reason}",
                        design.class, design.module,
                    ));
                    continue;
                }
            };
            let where_ = format!(
                "machine `{}` of class `{}` (domain `{}`)",
                machine_kernel_id(design).unwrap_or_else(|| "<unresolved id>".to_string()),
                design.class,
                design.module,
            );

            // (a) The IR must be an xstate-ir `Machine` AND structurally
            // valid — `validate_machine_json` is the fail-closed boundary
            // restingstate ships (`Parse` + `Invalid` arms both named).
            let typed: std::result::Result<xstate_core::xstate_ir::Machine, _> =
                serde_json::from_value(ir.clone());
            let machine = match typed {
                Ok(machine) => machine,
                Err(e) => {
                    refusals.push(format!(
                        "{where_}: the embedded machine_ir is not a valid xstate-ir Machine \
                         ({e}) — the artifact is stale or hand-edited; recompile the model",
                    ));
                    continue;
                }
            };
            let canonical = canonical_ir_json(&ir);
            if let Err(e) = xstate_core::validate_machine_json(&canonical) {
                refusals.push(format!(
                    "{where_}: the machine is structurally invalid ({e}) — fix the authored \
                     machine (the TypeScript/XState file, or the inline DSL block) and \
                     recompile",
                ));
                continue;
            }

            // Provenance agreement: the recorded machineId must be the
            // IR's own id (a mismatch is a tampered/stale embed).
            if let Some(prov) = &design.machine_provenance_json {
                let prov_id = prov.get("machineId").and_then(Value::as_str);
                if prov_id != Some(machine.id.as_str()) {
                    refusals.push(format!(
                        "{where_}: the provenance machineId `{}` does not match the machine \
                         IR's id `{}` — the artifact is stale or tampered",
                        prov_id.unwrap_or("<absent>"),
                        machine.id,
                    ));
                    continue;
                }
            }

            // File-safety: the id becomes the data file's stem.
            if !machine_id_is_file_safe(&machine.id) {
                refusals.push(format!(
                    "{where_}: the machine id is not a safe file stem — use an identifier \
                     (letters, digits, `-`/`_`)",
                ));
                continue;
            }

            // Registry-key + const-name uniqueness.
            if !seen_ids.insert(machine.id.clone()) {
                refusals.push(format!(
                    "{where_}: duplicate machine id `{}` — machine ids are application-unique \
                     registry keys; rename one of the machines",
                    machine.id,
                ));
                continue;
            }
            let const_name = registry_const_name(&machine.id);
            if let Some(other) = const_names.get(&const_name) {
                refusals.push(format!(
                    "{where_}: machine id `{}` and `{}` both render the registry const \
                     `{const_name}` — rename one of the machines",
                    other, machine.id,
                ));
                continue;
            }
            const_names.insert(const_name, machine.id.clone());

            // Guard-method-name collisions: two guard names rendering the
            // same `guard_{snake}` method would make the seam trait
            // unimplementable.
            for guard in machine_guard_names(&ir) {
                let method = guard_method_name(&guard);
                if let Some(other) = guard_methods.get(&method) {
                    if other != &guard {
                        refusals.push(format!(
                            "{where_}: guard `{guard}` and guard `{other}` both render the seam \
                             method `{method}` — rename one of the guards",
                        ));
                    }
                } else {
                    guard_methods.insert(method, guard);
                }
            }

            // A machine may not live in a domain named after the kernel
            // home — the domain's entity modules and the kernel registry
            // would share one directory (`src/machines/`).
            if design.module == "machines" {
                refusals.push(format!(
                    "{where_}: the declaring module is named `machines` — that name is the \
                     machine kernel's directory inside the domain-types crate \
                     (src/machines/); rename the module",
                ));
                continue;
            }

            // (b) The enum correspondence (OUR check): every machine
            // state must be a value of the bound codelist enum. The bound
            // enum comes from the design's DSL mirror (`machine <field>:
            // <Enum>`); a design carrying only the IR has no enum to
            // check against — the arm is skipped, never guessed.
            let Some(enum_name) = bound_enum_of(design) else {
                continue;
            };
            let codelist = resolve_codelist(db, &enum_name, suffix).await?;
            let Some(codelist) = codelist else {
                refusals.push(format!(
                    "{where_}: the bound enum `{enum_name}` resolves to no codelist in the \
                     graph — declare the enum in the domain model (the machine's states are \
                     its literals)",
                ));
                continue;
            };
            let values = db.get_enum_values(&codelist).await?;
            let known: BTreeSet<&str> = values.iter().map(|v| v.value.as_str()).collect();
            for state in &machine.states {
                if !known.contains(state.name.as_str()) {
                    refusals.push(format!(
                        "{where_}: state `{}` is not a value of the bound enum `{}` — the \
                         machine's states must be literals of the enum the state field binds \
                         (declared values: {})",
                        state.name,
                        enum_name,
                        known.iter().copied().collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
    }
    refusals.sort();
    Ok(refusals)
}

/// Resolve the authored enum name to a codelist name: exact, then the
/// suffix-stripped spelling, then title + suffix, then the JSON-codelist
/// convention (`{Enum}CodeList` — the classifier's codelist schema
/// spelling, whose VALUES an enum-typed field reads; issue #818 makes the
/// door reachable for inline machines whose projects seed enum values
/// through a codelist schema).
async fn resolve_codelist(
    db: &dyn GraphQuerier,
    enum_name: &str,
    suffix: &str,
) -> std::result::Result<Option<String>, GraphError> {
    let candidates = [
        enum_name.to_string(),
        codegraph_naming::strip_suffix(enum_name, suffix),
        format!("{enum_name}{suffix}"),
        format!("{enum_name}CodeList"),
    ];
    for candidate in candidates {
        if db.get_codelist(&candidate).await?.is_some() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// The registry const prefix for a machine id: SCREAMING_SNAKE, forced
/// into a valid Rust identifier (a leading digit gains the `MACHINE_`
/// prefix).
fn registry_const_name(machine_id: &str) -> String {
    let mut name = codegraph_naming::to_snake_case(machine_id).to_uppercase();
    if name.is_empty() {
        name = "MACHINE".to_string();
    }
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name = format!("MACHINE_{name}");
    }
    name
}

/// Emits the machine kernel into the domain-types crate: one canonical IR
/// JSON per machine under `src/machines/{domain}/` plus the registry
/// module `src/machines/mod.rs` (static data, parse-once accessors,
/// provenance consts). Registers unconditionally (the `functions_sql`/
/// `media_storage` precedent) and is presence-gated inside: a graph with
/// no machine IR emits ZERO files.
pub struct MachineKernelGenerator {
    /// The domain-types crate root — `src/` is appended internally (the
    /// `dto_boundaries`/`mox_ops` placement: the domain-types crate is
    /// sea_orm-free and wasm32-safe, and worker crates depend on it, so
    /// one home serves both topologies).
    src_dir: PathBuf,
}

impl MachineKernelGenerator {
    /// Creates a generator that writes under `base_dir` (the domain-types
    /// crate root), appending `src/` internally.
    pub fn new_with_base(base_dir: PathBuf) -> Self {
        Self {
            src_dir: base_dir.join("src"),
        }
    }
}

#[async_trait]
impl GlobalGenerator for MachineKernelGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::MachineKernel
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[crate::GenerationEntry],
        _tera: &tera::Tera,
        _project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Fail closed BEFORE any tree: the same walk the pre-generation
        // validation pass runs. The domain-generator phase collects its
        // own errors non-fatally, so both gates are what make a broken
        // machine refuse the run.
        let refusals = machine_kernel_refusals(db, config).await?;
        if !refusals.is_empty() {
            return Err(Error::Config(format!(
                "machine kernel — {} referenced machine(s) refused:\n{}",
                refusals.len(),
                refusals
                    .iter()
                    .map(|r| format!("  - {r}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }

        let entries = collect_machine_entries(db).await?;
        let machines_dir = self.src_dir.join("machines");
        if entries.is_empty() {
            // Presence gating: no machine IR ⇒ nothing emitted. A stale
            // kernel from a previous run is swept — but ONLY kernel-shaped
            // files (mod.rs + guards.rs + *.ir.json), never a real domain
            // directory that happens to be named `machines`.
            sweep_stale_kernel(&machines_dir, &BTreeSet::new(), false);
            return Ok(Vec::new());
        }

        // The guard seam's vocabulary: every guard name the machines
        // reference, sorted (issue #816).
        let mut guard_names: Vec<String> = entries
            .iter()
            .flat_map(|e| {
                let ir: Value = serde_json::from_str(&e.ir_json).unwrap_or(Value::Null);
                machine_guard_names(&ir)
            })
            .collect();
        guard_names.sort();
        guard_names.dedup();
        let has_guards = !guard_names.is_empty();

        let mut keep: BTreeSet<String> = BTreeSet::new();
        let mut files = Vec::new();
        for entry in &entries {
            let rel = format!("machines/{}/{}.ir.json", entry.domain, entry.machine_id);
            keep.insert(rel.clone());
            files.push(GeneratedFile {
                path: self.src_dir.join(&rel),
                content: format!("{}\n", entry.ir_json),
            });
        }
        files.push(GeneratedFile {
            path: machines_dir.join("mod.rs"),
            content: registry_module(&entries, &guard_names),
        });
        keep.insert("machines/mod.rs".to_string());

        // The write-once guard impls (the ManualImpls precedent): emitted
        // ONLY when absent — hand edits survive regeneration; the trait
        // re-emits every run, so a renamed/removed guard fails the hand
        // file's compile (E0407/E0046). When NO machine carries guards any
        // more, the stale hand file is swept (it would not compile against
        // the vanished trait).
        let guards_path = machines_dir.join("guards.rs");
        if has_guards {
            if !guards_path.exists() {
                files.push(GeneratedFile {
                    path: guards_path.clone(),
                    content: guard_impls_module(&guard_names),
                });
            }
            keep.insert("machines/guards.rs".to_string());
        }
        sweep_stale_kernel(&machines_dir, &keep, has_guards);

        Ok(files)
    }
}

/// Sweeps kernel-shaped files from a previous run that the current model
/// no longer carries: `mod.rs`, `guards.rs` (only when the current model
/// carries no guards — a stale hand impl would not compile against the
/// vanished trait), and `**/*.ir.json` under the kernel dir. Anything else
/// (a real domain directory's entity modules) is never touched.
fn sweep_stale_kernel(machines_dir: &Path, keep: &BTreeSet<String>, has_guards: bool) {
    let Ok(entries) = fs::read_dir(machines_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let rel = format!("machines/{}", entry.file_name().to_string_lossy());
            let kernel_shaped =
                rel == "machines/mod.rs" || (rel == "machines/guards.rs" && !has_guards);
            if kernel_shaped && !keep.contains(&rel) {
                let _ = fs::remove_file(&path);
            }
            continue;
        }
        // A domain subdirectory: remove only when it carries nothing but
        // `.ir.json` files (and none of them are kept).
        let Ok(domain_entries) = fs::read_dir(&path) else {
            continue;
        };
        let mut ir_files = Vec::new();
        let mut foreign = false;
        for de in domain_entries.flatten() {
            let p = de.path();
            if p.is_file()
                && p.extension().and_then(|e| e.to_str()) == Some("json")
                && p.to_string_lossy().ends_with(".ir.json")
            {
                ir_files.push(p);
            } else {
                foreign = true;
                break;
            }
        }
        if foreign {
            continue;
        }
        for p in ir_files {
            let rel = format!(
                "machines/{}/{}",
                entry.file_name().to_string_lossy(),
                p.file_name().unwrap().to_string_lossy()
            );
            if !keep.contains(&rel) {
                let _ = fs::remove_file(&p);
            }
        }
        // Drop the domain dir when it is now empty.
        let _ = fs::remove_dir(&path); // fails harmlessly when non-empty
    }
}

/// Renders the registry module: provenance consts, `include_str!` embeds,
/// and parse-once accessors over a sorted `BTreeMap`. Static data only —
/// no DB handle, no AppState, no async — so it compiles on wasm32 and
/// both topologies reach it through the shared domain-types crate.
///
/// When the machines carry named guards, the module additionally emits the
/// `MachineGuardImpls` trait (one method per guard — the ManualImpls
/// precedent; the write-once `guards.rs` implements it) and the fail-closed
/// dispatch implementing the workflow crate's `MachineGuards` seam. A
/// guard-less model renders BYTE-IDENTICAL to the Lane-1 module.
fn registry_module(entries: &[MachineEntry], guard_names: &[String]) -> String {
    let mut out = String::new();
    out.push_str(
        "//! Machine kernel (issue #815): the compiled `.ddd` artifact's\n\
         //! machine IRs, embedded as static data with a parse-once registry.\n\
         //!\n\
         //! Generated by codegraph. DO NOT EDIT.\n\
         //!\n\
         //! Each embed is the CANONICAL xstate-ir `Machine` JSON the compiler\n\
         //! produced (`machineIr` — keys sorted recursively), validated at\n\
         //! generation time (`xstate_core::validate_machine_json`) and checked\n\
         //! against its bound codelist enum. Provenance consts are the\n\
         //! drift-detection seam: the authored source path and the content\n\
         //! SHA-256 over the canonical IR, recomputable from the artifact\n\
         //! alone.\n\
         //!\n\
         //! Static data only — no DB handle, no AppState, no async — so this\n\
         //! module compiles on wasm32 and both deployment topologies reach it\n\
         //! through the shared domain-types crate.\n\n\
         use std::collections::BTreeMap;\n\
         use std::sync::LazyLock;\n\n",
    );

    // Provenance consts (both emitted per machine — the accessor arm
    // below references them unconditionally; an absent provenance half
    // rides the empty string).
    out.push_str("// --- PROVENANCE (issue #815) — the drift-detection seam ---\n");
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        let path = entry.source_path.as_deref().unwrap_or_default();
        let hash = entry.content_sha256.as_deref().unwrap_or_default();
        if !path.is_empty() {
            out.push_str(&format!(
                "/// Machine `{id}` was authored at `{path}`.\n",
                id = entry.machine_id,
                path = path,
            ));
        }
        out.push_str(&format!(
            "pub const {name}_SOURCE_PATH: &str = {path_literal};\n",
            name = name,
            path_literal = rust_str_literal(path),
        ));
        if !hash.is_empty() {
            out.push_str(&format!(
                "/// Machine `{id}` — SHA-256 over the canonical IR JSON.\n",
                id = entry.machine_id,
            ));
        }
        out.push_str(&format!(
            "pub const {name}_CONTENT_SHA256: &str = {hash_literal};\n",
            name = name,
            hash_literal = rust_str_literal(hash),
        ));
    }

    // Embeds.
    out.push_str("\n// --- EMBEDS: canonical xstate-ir Machine JSON, verbatim ---\n");
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        let rel = format!("{}/{}.ir.json", entry.domain, entry.machine_id);
        out.push_str(&format!(
            "const {name}_IR_JSON: &str = include_str!({rel_literal});\n",
            name = name,
            rel_literal = rust_str_literal(&rel),
        ));
    }

    // Registry + accessors.
    out.push_str(
        "\n/// The machine registry: machine id → embedded canonical IR JSON.\n\
         static IR_JSONS: LazyLock<BTreeMap<&'static str, &'static str>> = LazyLock::new(|| {\n\
         \x20   BTreeMap::from([\n",
    );
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        out.push_str(&format!(
            "        ({id_literal}, {name}_IR_JSON),\n",
            id_literal = rust_str_literal(&entry.machine_id),
            name = name,
        ));
    }
    out.push_str(
        "    ])\n\
         });\n\n\
         /// Every machine id in the application, sorted.\n\
         pub fn machine_ids() -> Vec<&'static str> {\n\
         \x20   IR_JSONS.keys().copied().collect()\n\
         }\n\n\
         /// The embedded canonical IR JSON for `machine_id`, verbatim from\n\
         /// the compiled artifact.\n\
         pub fn machine_json(machine_id: &str) -> Option<&'static str> {\n\
         \x20   IR_JSONS.get(machine_id).copied()\n\
         }\n\n\
         /// The typed machine: the embedded IR deserialized into the\n\
         /// xstate-ir `Machine`. Generation validated the embed, so `Err`\n\
         /// is a defect seam, never an expected path.\n\
         pub fn machine(\n\
         \x20   machine_id: &str,\n\
         ) -> Option<Result<xstate_core::xstate_ir::Machine, serde_json::Error>> {\n\
         \x20   machine_json(machine_id).map(serde_json::from_str)\n\
         }\n\n\
         /// Provenance for `machine_id`: `(authored source path, content\n\
         /// sha256)` — the Stage 2/3 drift-detection seam.\n\
         pub fn machine_provenance(machine_id: &str) -> Option<(&'static str, &'static str)> {\n\
         \x20   match machine_id {\n",
    );
    for entry in entries {
        let name = registry_const_name(&entry.machine_id);
        out.push_str(&format!(
            "        {id_literal} => Some(({name}_SOURCE_PATH, {name}_CONTENT_SHA256)),\n",
            id_literal = rust_str_literal(&entry.machine_id),
            name = name,
        ));
    }
    out.push_str(
        "        _ => None,\n\
         \x20   }\n\
         }\n\n\
         /// Structural validation mirroring the generation-time gate —\n\
         /// re-runnable at runtime. `None` when the id is unknown; `Err`\n\
         /// carries the fail-closed diagnostic.\n\
         pub fn validate_machine(\n\
         \x20   machine_id: &str,\n\
         ) -> Option<Result<(), xstate_core::ValidateError>> {\n\
         \x20   machine_json(machine_id).map(xstate_core::validate_machine_json)\n\
         }\n",
    );

    // --- The guard seam (issue #816, the ManualImpls precedent) ---
    if !guard_names.is_empty() {
        out.push_str("\npub mod guards;\n\n/// The application's guard implementations (write-once `guards.rs`).\npub use guards::DefaultMachineGuards;\n");
        out.push_str(
            "\n/// Why a guard implementation answered the way it did.\n\
             #[derive(Debug, Clone, PartialEq, Eq)]\n\
             pub enum GuardImplError {\n\
             \x20   /// The guard is still a stub — the wire 501 names the guard;\n\
             \x20   /// never silently allowed.\n\
             \x20   NotImplemented,\n\
             \x20   /// The guard's implementation itself failed.\n\
             \x20   Failed(String),\n\
             }\n\n\
             /// The compile-time guard seam: ONE method per named guard the\n\
             /// application's machines reference. `machines/guards.rs`\n\
             /// (WRITE-ONCE — the generator emits it only when absent; hand\n\
             /// edits survive regeneration) implements this trait; this\n\
             /// module re-emits the trait every run, so a renamed or removed\n\
             /// guard fails the hand file's compile instead of drifting.\n\
             pub trait MachineGuardImpls: Send + Sync {\n",
        );
        for guard in guard_names {
            out.push_str(&format!(
                "    /// Guard `{guard}` — authored in the machine definition; host-side\n\
                 /// data arrives through `params` (the guard's `params` object).\n\
                 fn {method}(\n\
                 \x20   &self,\n\
                 \x20   context: &serde_json::Value,\n\
                 \x20   event: &serde_json::Value,\n\
                 \x20   params: &serde_json::Value,\n\
                 ) -> Result<bool, GuardImplError>;\n",
                method = guard_method_name(guard),
            ));
        }
        out.push_str("}\n\n");
        // The fail-closed dispatch to the workflow crate's seam.
        out.push_str(
            "/// The kernel-facing adapter: routes a guard NAME to its trait\n\
             /// method. A name no machine declared is NotImplemented — the\n\
             /// fail-closed 501, never silently allowed (issue #816).\n\
             impl codegraph_workflow::MachineGuards for DefaultMachineGuards {\n\
             \x20   fn evaluate(\n\
             \x20       &self,\n\
             \x20       name: &str,\n\
             \x20       context: &serde_json::Value,\n\
             \x20       event: &serde_json::Value,\n\
             \x20       params: &serde_json::Value,\n\
             \x20   ) -> codegraph_workflow::GuardDecision {\n\
             \x20       let mapped = |result: Result<bool, GuardImplError>| match result {\n\
             \x20           Ok(true) => codegraph_workflow::GuardDecision::Allow,\n\
             \x20           Ok(false) => {\n\
             \x20               codegraph_workflow::GuardDecision::Deny(\n\
             \x20                   \"denied by the guard implementation\".to_string(),\n\
             \x20               )\n\
             \x20           }\n\
             \x20           Err(GuardImplError::NotImplemented) => {\n\
             \x20               codegraph_workflow::GuardDecision::NotImplemented\n\
             \x20           }\n\
             \x20           Err(GuardImplError::Failed(message)) => {\n\
             \x20               codegraph_workflow::GuardDecision::Failed(message)\n\
             \x20           }\n\
             \x20       };\n\
             \x20       match name {\n",
        );
        for guard in guard_names {
            out.push_str(&format!(
                "            {name_literal} => mapped(self.{method}(context, event, params)),\n",
                name_literal = rust_str_literal(guard),
                method = guard_method_name(guard),
            ));
        }
        out.push_str(
            "            _ => codegraph_workflow::GuardDecision::NotImplemented,\n\
             \x20       }\n\
             \x20   }\n\
             }\n",
        );
    }
    out
}

/// The write-once guard impls module (`machines/guards.rs`): one stub per
/// named guard, failing closed with `GuardImplError::NotImplemented` —
/// the wire 501 names the guard until the app fills it in. Emitted only
/// when absent; the trait re-emits so signature drift fails this file.
fn guard_impls_module(guard_names: &[String]) -> String {
    let mut out = String::new();
    out.push_str(
        "//! Machine guard implementations (issue #816 — write-once).\n\
         //!\n\
         //! The generator emits this file ONLY when absent: fill in the stubs\n\
         //! below and your edits survive every regeneration. The trait lives in\n\
         //! `machines/mod.rs` (regenerated each run), so renaming or removing a\n\
         //! guard in the model fails THIS file's compile — update the impl to\n\
         //! match. Stubs answer `NotImplemented`, which the kernel maps to a\n\
         //! named 501 (never a silent allow).\n\n\
         use serde_json::Value;\n\n\
         use super::{GuardImplError, MachineGuardImpls};\n\n\
         /// The application's guard implementations, registered with the\n\
         /// workflow service (`with_guards`).\n\
         pub struct DefaultMachineGuards;\n\n\
         impl MachineGuardImpls for DefaultMachineGuards {\n",
    );
    for guard in guard_names {
        out.push_str(&format!(
            "    fn {method}(\n\
             \x20       &self,\n\
             \x20       _context: &Value,\n\
             \x20       _event: &Value,\n\
             \x20       _params: &Value,\n\
             \x20   ) -> Result<bool, GuardImplError> {{\n\
             \x20       Err(GuardImplError::NotImplemented)\n\
             \x20   }}\n\n",
            method = guard_method_name(guard),
        ));
    }
    out.push_str("}\n");
    out
}

/// The Rust method name for a guard: `guard_{snake}`, forced into a valid
/// identifier (a leading digit gains the `guard_` prefix twice is fine —
/// the snake form already carries it).
fn guard_method_name(guard: &str) -> String {
    let mut name = codegraph_naming::to_snake_case(guard);
    if name.is_empty() {
        name = "unnamed".to_string();
    }
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name = format!("guard_{name}");
    }
    format!("guard_{name}")
}

/// A string as a Rust literal (the `ts_string_literal` discipline —
/// escape backslash + quote; the literals here are machine ids, authored
/// paths, and hex hashes, all quote-free in practice).
fn rust_str_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ir_json_sorts_keys_recursively() {
        // The determinism contract: HashMap-borne structures (`setup`
        // maps) serialize byte-identically regardless of insertion order.
        let a = serde_json::json!({
            "z": 1,
            "a": { "y": { "n": true, "m": 1 }, "b": 3 },
        });
        let b = serde_json::json!({
            "a": { "b": 3, "y": { "m": 1, "n": true } },
            "z": 1,
        });
        assert_eq!(canonical_ir_json(&a), canonical_ir_json(&b));
        assert_eq!(
            canonical_ir_json(&a),
            r#"{"a":{"b":3,"y":{"m":1,"n":true}},"z":1}"#,
        );
    }

    #[test]
    fn registry_const_names_are_valid_identifiers() {
        assert_eq!(registry_const_name("payrun"), "PAYRUN");
        assert_eq!(registry_const_name("pay-run"), "PAY_RUN");
        assert_eq!(registry_const_name("2fa"), "MACHINE_2FA");
    }

    #[test]
    fn machine_ids_must_be_file_safe() {
        assert!(machine_id_is_file_safe("payrun"));
        assert!(machine_id_is_file_safe("pay-run_2"));
        assert!(!machine_id_is_file_safe("../escape"));
        assert!(!machine_id_is_file_safe("a/b"));
        assert!(!machine_id_is_file_safe(""));
        assert!(!machine_id_is_file_safe("a\nb"));
    }
}
