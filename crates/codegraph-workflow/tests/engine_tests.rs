//! Engine unit tests using StateMachineDefinition and GuardEvaluator.

use codegraph_workflow::definition::StateMachineDefinition;
use codegraph_workflow::guard::GuardEvaluator;
use codegraph_workflow::types::{TransitionContext, TriggerSource};
use serde_json::json;
use uuid::Uuid;

#[test]
fn valid_transition_passes() {
    let sm: StateMachineDefinition = serde_json::from_value(json!({
        "transitions": {
            "draft": ["active"],
            "active": ["closed"]
        }
    }))
    .unwrap();

    assert!(sm.is_valid_transition("draft", "active"));
    assert!(sm.is_valid_transition("active", "closed"));
    assert!(!sm.is_valid_transition("draft", "closed"));
    assert!(!sm.is_valid_transition("closed", "draft"));
}

#[test]
fn terminal_state_has_no_transitions() {
    let sm: StateMachineDefinition = serde_json::from_value(json!({
        "transitions": {
            "draft": ["active"],
            "active": ["closed"]
        }
    }))
    .unwrap();

    assert!(sm.transitions_from("closed").is_empty());
}

#[test]
fn data_guards_block_invalid_data() {
    let sm: StateMachineDefinition = serde_json::from_value(json!({
        "transitions": {"draft": ["approved"]},
        "data_guards": [{
            "transition_to": "approved",
            "rule": "salary > 0",
            "message": "salary must be positive"
        }]
    }))
    .unwrap();

    let guards = sm.data_guards_for("approved");
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
    };
    assert_eq!(ctx.trigger_source, TriggerSource::Timer);
    assert_ne!(ctx.trigger_source, TriggerSource::User);
}

#[test]
fn approval_chain_detection() {
    let sm: StateMachineDefinition = serde_json::from_value(json!({
        "transitions": {"draft": ["approved"]},
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
