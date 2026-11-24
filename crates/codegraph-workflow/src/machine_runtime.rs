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
}
