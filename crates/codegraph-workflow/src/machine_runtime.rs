//! The machine kernel runtime (issue #816 — Lane 2 of epic #812).
//!
//! ONE machine definition, ONE decision-maker: the restingstate
//! `Interpreter` decides every transition for both planes (the entity
//! workflow engine and any direct kernel consumer). This module wraps the
//! pinned `xstate-core` surface:
//!
//! - **Fail-closed guards.** The library's `GuardRegistry` defaults a
//!   MISSING guard to ALLOW — that default must never govern backend
//!   lifecycles. [`MachineRuntime::decide`] wraps the guard lookup so an
//!   unregistered guard name is [`MachineError::UnimplementedGuard`] (the
//!   wire 501), a denied guard is [`MachineError::GuardDenied`], and a
//!   broken guard evaluation is [`MachineError::GuardFailed`].
//! - **Drift hash.** [`MachineRuntime::content_hash`] is FNV-1a 64 over
//!   the CANONICAL machine JSON ([`canonical_json`], keys sorted
//!   recursively — the pherolang `machineIr` embed form). Definition rows
//!   record it; the engine refuses to decide on a drifted machine.
//! - **wasm32 discipline.** Pure decision path: no sea_orm, no tokio, no
//!   DB handle. The clock is `MockClock` — delayed (`after`) transitions
//!   are not driven by the kernel (the workflow engine's timers remain an
//!   engine-level layer), so a static clock never mis-fires one.

use std::collections::BTreeMap;

use serde_json::Value;
use xstate_core::xstate_ir::{Machine, StateNode, StateType, Transition};
use xstate_core::{GuardError, Interpreter, MockClock, StateValue};

/// Why a guard source answered the way it did (the app-side seam's
/// vocabulary; mirrored by the generated machines module).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    /// The guard evaluated and allows the edge.
    Allow,
    /// The guard evaluated and denies the edge (`message` explains).
    Deny(String),
    /// The guard name is not implemented by this source — the fail-closed
    /// arm (the wire 501), never silently allowed.
    NotImplemented,
    /// The guard's implementation itself failed (`message` carries why).
    Failed(String),
}

/// The application-side guard seam: named guards a machine's transitions
/// reference, implemented by the generated app (the `MachineGuardImpls`
/// trait in the domain-types `machines` module implements THIS trait via
/// a name dispatch). The engine registers its own data/dual-status guard
/// bodies as named guards beside it.
pub trait MachineGuards: Send + Sync {
    fn evaluate(&self, name: &str, context: &Value, event: &Value, params: &Value)
    -> GuardDecision;
}

/// The kernel decision errors, each with its wire semantics.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MachineError {
    /// No transition from the current state targets the requested state —
    /// the wire 409 CONFLICT.
    #[error(
        "invalid transition from '{current}' to '{target}': no transition from the current state"
    )]
    InvalidTransition { current: String, target: String },

    /// The machine names a guard no source implements — the wire 501
    /// NOT_IMPLEMENTED. Fail-closed: never silently allowed.
    #[error(
        "guard '{0}' is not implemented: the machine names it but no guard implementation is \
         registered (implement MachineGuardImpls for it and register the source via with_guards)"
    )]
    UnimplementedGuard(String),

    /// A registered guard evaluated and denied the edge.
    #[error("guard '{guard}' denied the transition to '{target}': {message}")]
    GuardDenied {
        guard: String,
        target: String,
        message: String,
    },

    /// A guard implementation itself failed — the wire 500.
    #[error("guard '{guard}' evaluation failed: {message}")]
    GuardFailed { guard: String, message: String },

    /// A persisted snapshot could not be restored — persisted-state drift
    /// is an operator problem, named.
    #[error("persisted machine snapshot is unusable: {0}")]
    Snapshot(String),

    /// The machine JSON is not a structurally valid xstate-ir Machine.
    #[error("machine definition is invalid: {0}")]
    Machine(String),
}

/// The canonical serialization of a machine IR: keys sorted recursively,
/// compact separators (the pherolang `machineIr` embed form). Hash/compare
/// machine JSON THROUGH this helper — the IR carries `HashMap`-borne
/// structures whose iteration order varies run to run.
pub fn canonical_json(value: &Value) -> String {
    phero_json_canonical(value)
}

fn phero_json_canonical(value: &Value) -> String {
    // Same contract as `codegraph_generate::machines::canonical_ir_json`
    // (which delegates to phero_ir::canonical_json). Implemented inline so
    // the workflow crate carries no codegraph-generate dependency: keys
    // sorted recursively, compact separators.
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_else(|_| "\"\"".to_string()),
                        phero_json_canonical(&map[*k])
                    )
                })
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(phero_json_canonical).collect();
            format!("[{}]", body.join(","))
        }
        other => other.to_string(),
    }
}

/// FNV-1a 64-bit lowercase hex — the inline-hash pattern the ops harness
/// and the generated apps use (no new dependency).
pub fn fnv1a_64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// One edge of the machine read model (`available_transitions[i]`, issue
/// #817): the event that fires it, the state it leaves, the state it
/// lands on, and the authored guard name when the edge carries one. This
/// is what the UI renders — a button per edge — so the UI can never offer
/// a transition the kernel would refuse (the wire 409).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MachineTransitionEdge {
    /// The machine event that fires the edge.
    pub event: String,
    /// The state the edge leaves (always the read model's `current_state`).
    pub from: String,
    /// The state the edge lands on.
    pub to: String,
    /// The authored guard name, when the transition carries one. A guard
    /// edge rendered by the UI may still be DENIED at decision time
    /// (`GuardDenied`) — the read model publishes the name so consumers
    /// can explain a denial, never evaluate it.
    pub guard: Option<String>,
}

/// The machine read model (issue #817): ONE shape served by both planes —
/// the machine-only `GET …/{entity}/{id}/machine` route and the merged
/// Plane B `/workflow` payload's `machine` object. `current_state` reads
/// the entity column (which Stage 2's write-both keeps in sync with the
/// workflow instance); `machine_hash` is the kernel's FNV-1a drift hash
/// over the canonical machine IR, so a stale render is detectable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MachineReadModel {
    /// The machine's authored id (the registry key).
    pub machine_id: String,
    /// The row's current state as read from the entity column.
    pub current_state: String,
    /// Whether `current_state` is a final (terminal) state.
    pub is_terminal: bool,
    /// Whether `current_state` names a state the machine declares. A
    /// drifted column value (outside the bound enum) is VISIBLE here and
    /// the edge set is empty — never a silently-offered transition.
    pub state_known: bool,
    /// FNV-1a 64 hex over the CANONICAL machine IR — the drift hash the
    /// definition rows record; the UI detects a stale render with it.
    pub machine_hash: String,
    /// The edges the UI may render, in declaration order.
    pub available_transitions: Vec<MachineTransitionEdge>,
}

/// The outcome of one kernel decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionOutcome {
    /// The state the transition started from.
    pub from: String,
    /// The state the transition landed on (always `target` on `Ok`).
    pub to: String,
    /// The machine event that fired the edge.
    pub event: String,
    /// Whether the target state is final (the workflow's terminal flag).
    pub is_terminal: bool,
}

/// The kernel: one machine definition, one decision-maker.
pub struct MachineRuntime {
    machine: Machine,
    interpreter: Interpreter,
    /// Guard name → params, collected from the machine's transitions
    /// (authored `{ type, params }` guard objects) and its `setup.guards`
    /// declarations. Host-side data closed over by the guard lookup.
    guard_params: BTreeMap<String, Value>,
    clock: MockClock,
}

impl MachineRuntime {
    /// Builds the runtime over a machine IR value: parses into the typed
    /// xstate-ir `Machine` and runs the fail-closed structural validation.
    pub fn from_json(machine_json: &Value) -> Result<Self, MachineError> {
        let machine: Machine = serde_json::from_value(machine_json.clone())
            .map_err(|e| MachineError::Machine(format!("not an xstate-ir Machine: {e}")))?;
        Self::from_machine(machine)
    }

    /// Builds the runtime over a typed machine (validating structure).
    pub fn from_machine(machine: Machine) -> Result<Self, MachineError> {
        Interpreter::validate_machine(&machine)
            .map_err(|e| MachineError::Machine(e.to_string()))?;
        let guard_params = collect_guard_params(&machine);
        let interpreter = Interpreter::new(machine.clone());
        Ok(Self {
            machine,
            interpreter,
            guard_params,
            clock: MockClock::default(),
        })
    }

    /// The machine's authored id.
    pub fn machine_id(&self) -> &str {
        &self.machine.id
    }

    /// Every top-level state name, in declaration order.
    pub fn state_names(&self) -> Vec<&str> {
        self.machine
            .states
            .iter()
            .map(|s| s.name.as_str())
            .collect()
    }

    /// Whether `name` is a final (terminal) state.
    pub fn is_terminal_state(&self, name: &str) -> bool {
        self.top_state(name)
            .is_some_and(|s| s.node_type == StateType::Final)
    }

    /// Target states reachable from `current` (the engine's
    /// `available_transitions` wire surface).
    pub fn available_transitions(&self, current: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(state) = self.top_state(current) {
            for t in &state.transitions {
                if let Some(target) = &t.target
                    && !out.contains(target)
                {
                    out.push(target.clone());
                }
                for target in &t.targets {
                    if let Some(first) = target.split('.').next()
                        && !out.iter().any(|o| o == first)
                    {
                        out.push(first.to_string());
                    }
                }
            }
        }
        out
    }

    /// FNV-1a 64 hex over the CANONICAL machine JSON — the drift-hash the
    /// definition rows record and the engine verifies.
    pub fn content_hash(&self) -> String {
        let canonical = match serde_json::to_value(&self.machine) {
            Ok(v) => canonical_json(&v),
            Err(_) => return fnv1a_64_hex(b""),
        };
        fnv1a_64_hex(canonical.as_bytes())
    }

    /// The typed machine (a clone — the runtime owns its own).
    pub fn machine(&self) -> &Machine {
        &self.machine
    }

    /// The first declared transition from `current` targeting `target`,
    /// as `(event, guard name)`. The engine's API is target-addressed;
    /// the kernel thinks in events — this is the one place the two
    /// vocabularies meet.
    pub fn transition_for(&self, current: &str, target: &str) -> Option<(String, Option<String>)> {
        let state = self.top_state(current)?;
        for t in &state.transitions {
            let matches_target = t.target.as_deref() == Some(target)
                || t.targets
                    .iter()
                    .any(|tt| tt.split('.').next() == Some(target));
            if matches_target {
                return Some((t.event.clone(), t.guard.as_ref().map(|g| g.name.clone())));
            }
        }
        None
    }

    /// THE kernel decision: find the transition from `current` targeting
    /// `target`, restore the state, `send` the trigger event through the
    /// fail-closed guard lookup, and report the landed state.
    ///
    /// `context` is the caller's data (for the engine: the entity's
    /// serialized row) — guard implementations read it.
    pub fn decide(
        &self,
        current: &str,
        target: &str,
        context: &Value,
        guards: &dyn MachineGuards,
    ) -> Result<TransitionOutcome, MachineError> {
        // The transition must exist before any guard is consulted: an
        // unknown (current, target) pair is the named conflict, and no
        // guard evaluation ever runs for it.
        let Some((event, guard_name)) = self.transition_for(current, target) else {
            return Err(MachineError::InvalidTransition {
                current: current.to_string(),
                target: target.to_string(),
            });
        };

        // The kernel addresses top-level (atomic) states — the workflow
        // wire surface is a flat state string.
        let state = StateValue::Atomic(current.to_string());
        let event_value = serde_json::json!({ "type": event });
        let mut ctx = context.clone();
        // The interpreter's `Ok(false)` carries no reason, so the guard
        // denial message is recorded here and surfaced with the denial
        // (the engine maps it onto the wire error).
        let denial_message: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);

        let result = self.interpreter.send_fallible(
            &state,
            &event_value,
            &mut ctx,
            &|name: &str, c: &Value, e: &Value| {
                let params = self.guard_params.get(name).cloned().unwrap_or(Value::Null);
                match guards.evaluate(name, c, e, &params) {
                    GuardDecision::Allow => Ok(true),
                    GuardDecision::Deny(message) => {
                        *denial_message.borrow_mut() = Some(message);
                        Ok(false)
                    }
                    GuardDecision::NotImplemented => Err(GuardError {
                        guard_name: name.to_string(),
                        source: Box::new(MachineError::UnimplementedGuard(name.to_string())),
                    }),
                    GuardDecision::Failed(message) => Err(GuardError {
                        guard_name: name.to_string(),
                        source: Box::new(MachineError::GuardFailed {
                            guard: name.to_string(),
                            message,
                        }),
                    }),
                }
            },
            &mut |_name: &str, _args: &[Value], _evt: &Value| {
                // No kernel actions: side effects stay engine-level layers.
            },
            &self.clock,
        );

        match result {
            // The edge fired; report where it landed.
            Ok(Some(next)) => {
                let to = flatten_state(&next)?;
                Ok(TransitionOutcome {
                    from: current.to_string(),
                    to,
                    event,
                    is_terminal: self.is_terminal_state(target),
                })
            }
            // The transition exists but did not fire ⇒ the guard denied it.
            Ok(None) => {
                let message = denial_message
                    .into_inner()
                    .unwrap_or_else(|| "guard denied the transition".to_string());
                Err(MachineError::GuardDenied {
                    guard: guard_name.unwrap_or_else(|| "<none>".to_string()),
                    target: target.to_string(),
                    message,
                })
            }
            Err(ge) => {
                // Surface the fail-closed errors we wrapped; anything else
                // from the interpreter is a guard evaluation failure.
                if let Some(machine_err) = ge.source.downcast_ref::<MachineError>() {
                    return Err(machine_err.clone());
                }
                Err(MachineError::GuardFailed {
                    guard: ge.guard_name,
                    message: ge.source.to_string(),
                })
            }
        }
    }

    /// The machine read model (issue #817): the DERIVED projection the UI
    /// renders — never a machine it runs. `available_transitions` is the
    /// set of edges whose from-state matches `current` (the kernel's own
    /// `available_transitions`, one edge richer: event + from + to + the
    /// authored guard name). `current` is validated on the way: a value
    /// that names no machine state yields `state_known = false` and an
    /// EMPTY edge set — the UI can never offer a transition the backend
    /// would 409 — and the `machine_hash` rides every payload so a stale
    /// render is detectable client-side.
    pub fn read_model(&self, current: &str) -> MachineReadModel {
        let state_known = self.top_state(current).is_some();
        let mut available_transitions: Vec<MachineTransitionEdge> = Vec::new();
        if let Some(state) = self.top_state(current) {
            for t in &state.transitions {
                // The kernel addresses top-level (atomic) states: a
                // multi-target edge contributes its FIRST top-level segment.
                let targets: Vec<String> = match (&t.target, t.targets.first()) {
                    (Some(target), _) => vec![target.clone()],
                    (None, Some(first)) => first
                        .split('.')
                        .next()
                        .map(|s| s.to_string())
                        .into_iter()
                        .collect(),
                    (None, None) => Vec::new(),
                };
                for to in targets {
                    let edge = MachineTransitionEdge {
                        event: t.event.clone(),
                        from: current.to_string(),
                        to: to.clone(),
                        guard: t.guard.as_ref().map(|g| g.name.clone()),
                    };
                    if !available_transitions
                        .iter()
                        .any(|e| e.event == edge.event && e.to == edge.to)
                    {
                        available_transitions.push(edge);
                    }
                }
            }
        }
        MachineReadModel {
            machine_id: self.machine.id.clone(),
            current_state: current.to_string(),
            is_terminal: state_known && self.is_terminal_state(current),
            state_known,
            machine_hash: self.content_hash(),
            available_transitions,
        }
    }

    /// The persisted snapshot for a (state, context) pair — the
    /// interpreter's restore/persist surface over flat workflow states.
    pub fn persisted_snapshot_from_state(&self, state: &str, context: &Value) -> Value {
        self.interpreter
            .persisted_snapshot(&StateValue::Atomic(state.to_string()), context)
    }

    /// Restores a persisted snapshot, returning `(state name, context)`.
    /// Only flat (atomic) snapshots are supported — compound values are a
    /// named error (the workflow wire surface is a flat string).
    pub fn restore_persisted(&self, snapshot: &Value) -> Result<(String, Value), MachineError> {
        let (state, context) = self
            .interpreter
            .restore_persisted(snapshot)
            .map_err(|e| MachineError::Snapshot(e.to_string()))?;
        let name = flatten_state(&state)?;
        Ok((name, context))
    }

    fn top_state(&self, name: &str) -> Option<&StateNode> {
        self.machine.states.iter().find(|s| s.name == name)
    }
}

/// Flattens a StateValue to its workflow string. Compound/parallel values
/// have no flat wire spelling — a named error, never a guessed string.
fn flatten_state(state: &StateValue) -> Result<String, MachineError> {
    match state {
        StateValue::Atomic(name) => Ok(name.clone()),
        other => Err(MachineError::Snapshot(format!(
            "compound/parallel state values have no flat workflow spelling: {other:?}"
        ))),
    }
}

/// Collects guard params from the machine: each transition's authored
/// guard (`{ name, params }`) and every `setup.guards` declaration.
/// Transition-level params win (they are the authored call site).
fn collect_guard_params(machine: &Machine) -> BTreeMap<String, Value> {
    let mut out: BTreeMap<String, Value> = BTreeMap::new();
    let walk = |transitions: &[Transition], out: &mut BTreeMap<String, Value>| {
        for t in transitions {
            if let Some(guard) = &t.guard {
                let params = guard.params.clone().unwrap_or(Value::Null);
                out.entry(guard.name.clone()).or_insert(params);
            }
        }
    };
    for state in &machine.states {
        walk(&state.transitions, &mut out);
        walk(&state.on_done, &mut out);
        for child in &state.children {
            walk(&child.transitions, &mut out);
        }
    }
    if let Some(setup) = &machine.setup {
        for (name, def) in &setup.guards {
            let params = if def.params.is_empty() {
                Value::Null
            } else {
                let literals: Vec<Value> = def
                    .params
                    .iter()
                    .map(|p| match p {
                        xstate_core::xstate_ir::actors::GuardParam::Literal(v) => v.clone(),
                    })
                    .collect();
                Value::Array(literals)
            };
            out.entry(name.clone()).or_insert(params);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The two-state machine the workflow wire speaks: Draft --CALCULATE-->
    /// Calculating (final). A guard-bearing variant hangs off Calculating.
    fn machine_json() -> Value {
        json!({
            "context": {"fields": []},
            "id": "payrun",
            "initial": "Draft",
            "setup": null,
            "states": [
                {
                    "children": [], "description": null, "entry": [], "exit": [],
                    "initial": null, "invoke": null, "name": "Draft",
                    "on_done": [], "output": null, "tags": [],
                    "transitions": [{
                        "actions": [], "description": null, "event": "CALCULATE",
                        "guard": null, "target": "Calculating", "targets": []
                    }],
                    "type": "atomic"
                },
                {
                    "children": [], "description": null, "entry": [], "exit": [],
                    "initial": null, "invoke": null, "name": "Calculating",
                    "on_done": [], "output": null, "tags": [],
                    "transitions": [],
                    "type": "final"
                }
            ]
        })
    }

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let a = json!({"z": 1, "a": {"y": {"n": true, "m": 1}, "b": 3}});
        let b = json!({"a": {"b": 3, "y": {"m": 1, "n": true}}, "z": 1});
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_eq!(
            canonical_json(&a),
            r#"{"a":{"b":3,"y":{"m":1,"n":true}},"z":1}"#
        );
    }

    #[test]
    fn fnv1a_matches_the_ops_harness_known_answers() {
        // The #756 known answers — one inline FNV for every plane.
        assert_eq!(fnv1a_64_hex(b""), "cbf29ce484222325");
        assert_eq!(fnv1a_64_hex(b"a"), "af63dc4c8601ec8c");
    }

    // ── the machine read model (issue #817) ─────────────────────────────

    #[test]
    fn read_model_state_x_yields_exactly_the_declared_out_edges() {
        let runtime = MachineRuntime::from_json(&machine_json()).unwrap();
        let model = runtime.read_model("Draft");
        // The exact declared out-edge — event, from, to — in declaration
        // order, nothing more.
        assert_eq!(model.machine_id, "payrun");
        assert_eq!(model.current_state, "Draft");
        assert!(model.state_known);
        assert!(!model.is_terminal);
        assert_eq!(
            model.available_transitions,
            vec![MachineTransitionEdge {
                event: "CALCULATE".to_string(),
                from: "Draft".to_string(),
                to: "Calculating".to_string(),
                guard: None,
            }]
        );
        // The drift hash is present and matches the kernel's own.
        assert_eq!(model.machine_hash, runtime.content_hash());
        assert_ne!(model.machine_hash, "");
    }

    #[test]
    fn read_model_terminal_state_has_no_out_edges_and_is_terminal() {
        let runtime = MachineRuntime::from_json(&machine_json()).unwrap();
        let model = runtime.read_model("Calculating");
        assert!(model.is_terminal);
        assert!(model.available_transitions.is_empty());
        assert!(model.state_known);
    }

    #[test]
    fn read_model_unknown_state_is_empty_with_the_hash_present() {
        let runtime = MachineRuntime::from_json(&machine_json()).unwrap();
        let model = runtime.read_model("Bogus");
        // The kernel validates the row's value on the way: an unknown
        // state is VISIBLE (state_known = false), the edge set is EMPTY
        // (the UI can never offer a transition the backend would 409),
        // and the drift hash still rides so a stale render is detectable.
        assert!(!model.state_known);
        assert!(!model.is_terminal);
        assert!(model.available_transitions.is_empty());
        assert_eq!(model.machine_hash, runtime.content_hash());
    }

    #[test]
    fn read_model_carries_the_authored_guard_name_but_never_evaluates_it() {
        let mut machine_json = machine_json();
        machine_json["states"][0]["transitions"][0]["guard"] =
            json!({"name": "amount_positive", "params": {"min": 1}});
        let runtime = MachineRuntime::from_json(&machine_json).unwrap();
        let model = runtime.read_model("Draft");
        assert_eq!(
            model.available_transitions.first().map(|e| e.guard.clone()),
            Some(Some("amount_positive".to_string()))
        );
        // Deriving the read model runs NO guard: the edge is published
        // with its name, and evaluation stays the decision path's job.
    }

    #[test]
    fn read_model_edges_serialize_for_the_wire() {
        let runtime = MachineRuntime::from_json(&machine_json()).unwrap();
        let model = runtime.read_model("Draft");
        let wire = serde_json::to_string(&model).unwrap();
        assert!(wire.contains(r#""event":"CALCULATE""#), "{wire}");
        assert!(wire.contains(r#""machine_hash":"#), "{wire}");
        assert!(wire.contains(r#""state_known":true"#), "{wire}");
    }
}
