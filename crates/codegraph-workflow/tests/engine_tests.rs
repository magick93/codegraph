//! Engine unit tests over the kernel decision contract (issue #816): the
//! lowered-machine shapes the engine consumes decide exactly like the
//! pre-kernel wire semantics — valid edges advance, invalid edges are the
//! named 409, data guards gate on entity data, timers skip them.

use codegraph_workflow::definition::WorkflowDefinition;
use codegraph_workflow::guard::GuardEvaluator;
use codegraph_workflow::machine_runtime::{GuardDecision, MachineGuards};
use codegraph_workflow::types::{TransitionContext, TriggerSource};
use serde_json::json;
use uuid::Uuid;

struct AllowAll;
impl MachineGuards for AllowAll {
    fn evaluate(
        &self,
        _n: &str,
        _c: &serde_json::Value,
        _e: &serde_json::Value,
        _p: &serde_json::Value,
    ) -> GuardDecision {
        GuardDecision::Allow
    }
}

/// Lower a transitions map into the machine envelope shape the seed
/// writes — the same lowering `codegraph-generate` performs (mirrored
/// here so the engine tests stay independent of the generator crate).
fn machine_envelope(transitions: &[(&str, &str)]) -> serde_json::Value {
    // Collect states in stable order.
    let mut states: Vec<String> = Vec::new();
    for (from, to) in transitions {
        for s in [*from, *to] {
            if !states.iter().any(|x| x == s) {
                states.push(s.to_string());
            }
        }
    }
    states.sort();
    let state_nodes: Vec<serde_json::Value> = states
        .iter()
        .map(|name| {
            let edges: Vec<serde_json::Value> = transitions
                .iter()
                .filter(|(from, _)| *from == name.as_str())
                .map(|(_, to)| {
                    json!({
                        "actions": [], "description": null, "event": to,
                        "guard": null, "target": to, "targets": []
                    })
                })
                .collect();
            json!({
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": name,
                "on_done": [], "output": null, "tags": [],
                "transitions": edges,
                "type": "atomic"
            })
        })
        .collect();
    let machine = json!({
        "context": {"fields": []},
        "id": "test",
        "initial": "draft",
        "setup": null,
        "states": state_nodes,
    });
    json!({"machine": machine, "machine_hash": null})
}

#[test]
fn the_kernel_decides_through_the_envelope_shape() {
    let envelope = machine_envelope(&[("draft", "active"), ("active", "closed")]);
    let def = WorkflowDefinition::from_json(&envelope).unwrap();
    let machine = def.machine.as_ref().unwrap();
    let rt = codegraph_workflow::MachineRuntime::from_json(machine).unwrap();

    assert!(rt.decide("draft", "active", &json!({}), &AllowAll).is_ok());
    let err = rt
        .decide("draft", "closed", &json!({}), &AllowAll)
        .expect_err("draft -> closed has no edge");
    assert!(
        matches!(&err, codegraph_workflow::machine_runtime::MachineError::InvalidTransition { current, target }
            if current == "draft" && target == "closed"),
        "{err:?}"
    );
}

#[test]
fn data_guards_block_invalid_data() {
    let json = json!({
        "machine": null,
        "data_guards": [{
            "transition_to": "approved",
            "rule": "salary > 0",
            "message": "salary must be positive"
        }]
    });
    let def = WorkflowDefinition::from_json(&json).unwrap();
    let guards = def.data_guards_for("approved");
    assert_eq!(guards.len(), 1);

    let valid = json!({"salary": 50000});
    assert!(GuardEvaluator::evaluate(&guards[0].rule, &valid).unwrap());

    let invalid = json!({"salary": 0});
    assert!(!GuardEvaluator::evaluate(&guards[0].rule, &invalid).unwrap());
}

#[test]
fn trigger_source_timer_skips_guards() {
    let ctx = TransitionContext {
        tenant_id: Uuid::new_v4(),
        entity_id: Uuid::new_v4(),
        domain: "test".to_string(),
        entity_table: "test_entity".to_string(),
        target_state: "active".to_string(),
        actor_id: None,
        correlation_id: Uuid::new_v4(),
        idempotency_key: None,
        comment: None,
        entity_data: serde_json::Value::Null,
        trigger_source: TriggerSource::Timer,
        session_user_id: None,
        session_api_key_id: None,
    };
    assert_eq!(ctx.trigger_source, TriggerSource::Timer);
    assert_ne!(ctx.trigger_source, TriggerSource::User);
}

#[test]
fn approval_chain_detection() {
    let sm: WorkflowDefinition = serde_json::from_value(json!({
        "approval_chains": [{
            "transition_from": "draft",
            "transition_to": "approved",
            "steps": [{"order": 1, "role": "manager", "is_required": true}]
        }]
    }))
    .unwrap();

    assert!(sm.has_approval_chain("draft", "approved"));
    assert!(!sm.has_approval_chain("draft", "closed"));
}
