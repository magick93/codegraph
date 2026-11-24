//! Machine kernel decisioning (issue #816 — Lane 2 of epic #812).
//!
//! The restingstate `Interpreter` is the ONE transition decision-maker for
//! both planes. These KATs pin the `MachineRuntime` contract:
//!
//! - a legal transition advances and reports its terminality;
//! - an illegal transition is a named conflict (the wire 409);
//! - a registered guard is consulted (allow and deny);
//! - **an UNREGISTERED guard is a named 501 error** — the library's
//!   missing-guard-ALLOWS default must never govern backend lifecycles;
//! - guard `params` carried on the machine's transitions close over into
//!   the source callback (host-side data);
//! - the drift hash is FNV-1a 64 over the CANONICAL machine JSON.

use codegraph_workflow::machine_runtime::{
    GuardDecision, MachineError, MachineGuards, MachineRuntime,
};
use serde_json::{Value, json};

/// The canonical payrun machine: Draft → (CALCULATE) → Calculating, where
/// Calculating is final. Same shape the Lane-1 artifact embeds.
fn payrun_machine() -> Value {
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

/// A guard source that records every consultation.
struct RecordingGuards {
    verdict: GuardDecision,
    consulted: std::sync::Mutex<Vec<(String, Value)>>,
}

impl RecordingGuards {
    fn allowing() -> Self {
        Self {
            verdict: GuardDecision::Allow,
            consulted: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn denying(message: &str) -> Self {
        Self {
            verdict: GuardDecision::Deny(message.to_string()),
            consulted: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl MachineGuards for RecordingGuards {
    fn evaluate(
        &self,
        name: &str,
        context: &Value,
        event: &Value,
        params: &Value,
    ) -> GuardDecision {
        self.consulted
            .lock()
            .unwrap()
            .push((name.to_string(), params.clone()));
        let _ = (context, event);
        match &self.verdict {
            GuardDecision::Allow => GuardDecision::Allow,
            GuardDecision::Deny(m) => GuardDecision::Deny(m.clone()),
            GuardDecision::NotImplemented => GuardDecision::NotImplemented,
            GuardDecision::Failed(m) => GuardDecision::Failed(m.clone()),
        }
    }
}

struct NoGuards;
impl MachineGuards for NoGuards {
    fn evaluate(&self, _name: &str, _c: &Value, _e: &Value, _p: &Value) -> GuardDecision {
        GuardDecision::NotImplemented
    }
}

#[test]
fn a_valid_transition_advances_through_the_kernel() {
    let rt = MachineRuntime::from_json(&payrun_machine()).unwrap();
    let guards = RecordingGuards::allowing();
    let outcome = rt
        .decide("Draft", "Calculating", &json!({}), &guards)
        .expect("Draft → Calculating is the machine's declared edge");
    assert_eq!(outcome.from, "Draft");
    assert_eq!(outcome.to, "Calculating");
    assert_eq!(outcome.event, "CALCULATE");
    assert!(outcome.is_terminal, "Calculating is a final state");
    assert!(
        guards.consulted.lock().unwrap().is_empty(),
        "an unguarded transition consults no guard"
    );
}

#[test]
fn an_invalid_transition_is_a_named_conflict() {
    let rt = MachineRuntime::from_json(&payrun_machine()).unwrap();
    let err = rt
        .decide("Calculating", "Draft", &json!({}), &NoGuards)
        .expect_err("a final state has no outbound edges");
    match &err {
        MachineError::InvalidTransition { current, target } => {
            assert_eq!(current, "Calculating");
            assert_eq!(target, "Draft");
        }
        other => panic!("expected InvalidTransition, got: {other:?}"),
    }
    // And the unknown-target shape is the same named conflict.
    let err = rt
        .decide("Draft", "Voided", &json!({}), &NoGuards)
        .expect_err("no edge targets Voided");
    assert!(
        matches!(&err, MachineError::InvalidTransition { target, .. } if target == "Voided"),
        "{err:?}"
    );
}

/// A machine whose transition names a guard.
fn guarded_machine() -> Value {
    let mut m = payrun_machine();
    m["states"][0]["transitions"][0]["guard"] = json!({
        "name": "amount_is_positive",
        "params": {"floor": 1}
    });
    m
}

#[test]
fn a_registered_guard_is_consulted_and_can_deny() {
    let rt = MachineRuntime::from_json(&guarded_machine()).unwrap();

    // Allow: the transition fires.
    let allow = RecordingGuards::allowing();
    let outcome = rt
        .decide("Draft", "Calculating", &json!({}), &allow)
        .expect("an allowing guard lets the edge fire");
    assert_eq!(outcome.to, "Calculating");
    let consulted = allow.consulted.lock().unwrap();
    assert_eq!(consulted.len(), 1, "the guard is consulted exactly once");
    assert_eq!(consulted[0].0, "amount_is_positive");

    // Deny: the kernel does not advance and names the guard.
    let deny = RecordingGuards::denying("amount must be positive");
    let err = rt
        .decide("Draft", "Calculating", &json!({}), &deny)
        .expect_err("a denying guard blocks the edge");
    assert!(
        matches!(&err, MachineError::GuardDenied { guard, target, .. }
            if guard == "amount_is_positive" && target == "Calculating"),
        "the denial names the guard and the target: {err:?}"
    );
}

#[test]
fn guard_params_close_over_the_machine_definition() {
    let rt = MachineRuntime::from_json(&guarded_machine()).unwrap();
    let guards = RecordingGuards::allowing();
    rt.decide("Draft", "Calculating", &json!({}), &guards)
        .expect("the guarded edge fires");
    let consulted = guards.consulted.lock().unwrap();
    assert_eq!(
        consulted[0].1,
        json!({"floor": 1}),
        "the transition's guard params ride into the source callback"
    );
}

#[test]
fn an_unregistered_guard_is_a_named_501_error() {
    // The machine names `amount_is_positive` but the source implements
    // nothing: the missing-guard-ALLOWS default must NOT govern the
    // decision — the runtime fails closed with the guard named.
    let rt = MachineRuntime::from_json(&guarded_machine()).unwrap();
    let err = rt
        .decide("Draft", "Calculating", &json!({}), &NoGuards)
        .expect_err("an unimplemented guard must not silently allow");
    match &err {
        MachineError::UnimplementedGuard(name) => {
            assert_eq!(name, "amount_is_positive");
        }
        other => panic!("expected UnimplementedGuard, got: {other:?}"),
    }
    let text = err.to_string();
    assert!(
        text.contains("amount_is_positive"),
        "the error names the guard: {text}"
    );
    assert!(
        text.contains("not implemented"),
        "the error carries the 501 semantics: {text}"
    );
}

#[test]
fn a_guard_implementation_failure_is_a_named_evaluation_error() {
    let rt = MachineRuntime::from_json(&guarded_machine()).unwrap();
    let broken = RecordingGuards {
        verdict: GuardDecision::Failed("boom".to_string()),
        consulted: std::sync::Mutex::new(Vec::new()),
    };
    let err = rt
        .decide("Draft", "Calculating", &json!({}), &broken)
        .expect_err("a broken guard evaluation must surface");
    assert!(
        matches!(&err, MachineError::GuardFailed { guard, message }
            if guard == "amount_is_positive" && message == "boom"),
        "{err:?}"
    );
}

#[test]
fn the_drift_hash_is_fnv1a_over_the_canonical_machine_json() {
    let rt = MachineRuntime::from_json(&payrun_machine()).unwrap();
    let hash = rt.content_hash();
    // FNV-1a 64 over the canonical serialization — the same digest the
    // ops harness and the seed migration record.
    let canonical = codegraph_workflow::machine_runtime::canonical_json(&payrun_machine());
    let expected = codegraph_workflow::machine_runtime::fnv1a_64_hex(canonical.as_bytes());
    assert_eq!(hash, expected);
    assert_eq!(hash.len(), 16, "lowercase hex, 64-bit");

    // Key order does not move the hash: the canonicalization is the
    // determinism contract.
    let reordered = json!({
        "states": payrun_machine()["states"],
        "setup": null,
        "initial": "Draft",
        "id": "payrun",
        "context": {"fields": []},
    });
    let rt2 = MachineRuntime::from_json(&reordered).unwrap();
    assert_eq!(rt.content_hash(), rt2.content_hash());
}

#[test]
fn the_runtime_reports_states_and_terminality() {
    let rt = MachineRuntime::from_json(&payrun_machine()).unwrap();
    assert_eq!(rt.machine_id(), "payrun");
    assert_eq!(rt.state_names(), vec!["Draft", "Calculating"]);
    assert!(rt.is_terminal_state("Calculating"));
    assert!(!rt.is_terminal_state("Draft"));
    // Available transitions mirror the engine's wire surface.
    assert_eq!(rt.available_transitions("Draft"), vec!["Calculating"]);
    assert!(rt.available_transitions("Calculating").is_empty());
}

#[test]
fn a_structurally_broken_machine_refuses_the_runtime() {
    let mut broken = payrun_machine();
    broken["states"][0]["transitions"][0]["target"] = json!("Nowhere");
    let err = match MachineRuntime::from_json(&broken) {
        Err(e) => e,
        Ok(_) => panic!("an unknown transition target must refuse the runtime"),
    };
    let text = err.to_string();
    assert!(text.contains("Nowhere"), "names the defect: {text}");
}

#[test]
fn the_persisted_snapshot_round_trips_through_the_runtime() {
    let rt = MachineRuntime::from_json(&payrun_machine()).unwrap();
    let snapshot = rt.persisted_snapshot_from_state("Draft", &json!({"total": 42}));
    let (state, context) = rt.restore_persisted(&snapshot).unwrap();
    assert_eq!(state, "Draft");
    assert_eq!(context, json!({"total": 42}));
}
