use codegraph_workflow::definition::StateMachineDefinition;
use serde_json::json;

#[test]
fn parse_transitions() {
    let json = json!({
        "transitions": {"new": ["active", "cancelled"], "active": ["closed"]}
    });
    let def = StateMachineDefinition::from_json(&json).unwrap();
    assert_eq!(def.transitions_from("new"), vec!["active", "cancelled"]);
    assert_eq!(def.transitions_from("active"), vec!["closed"]);
    assert!(def.transitions_from("closed").is_empty());
}

#[test]
fn is_valid_transition() {
    let json = json!({"transitions": {"new": ["active"]}});
    let def = StateMachineDefinition::from_json(&json).unwrap();
    assert!(def.is_valid_transition("new", "active"));
    assert!(!def.is_valid_transition("new", "closed"));
    assert!(!def.is_valid_transition("active", "new"));
}

#[test]
fn data_guards_for_state() {
    let json = json!({
        "transitions": {},
        "data_guards": [
            {"transition_to": "offer", "rule": "salary > 0", "message": "Need salary"},
            {"transition_to": "screening", "rule": "quals IS NOT EMPTY", "message": "Need quals"}
        ]
    });
    let def = StateMachineDefinition::from_json(&json).unwrap();
    let guards = def.data_guards_for("offer");
    assert_eq!(guards.len(), 1);
    assert_eq!(guards[0].rule, "salary > 0");
    assert!(def.data_guards_for("active").is_empty());
}

#[test]
fn timers_for_state() {
    let json = json!({
        "transitions": {},
        "timers": [
            {"trigger_on_enter": "screening", "type": "deadline", "duration_hours": 48, "target_state": "escalated"},
            {"trigger_on_enter": "screening", "type": "reminder", "duration_hours": 24}
        ]
    });
    let def = StateMachineDefinition::from_json(&json).unwrap();
    let timers = def.timers_for_state("screening");
    assert_eq!(timers.len(), 2);
    assert!(def.timers_for_state("new").is_empty());
}

#[test]
fn dual_status_guards() {
    let json = json!({
        "transitions": {},
        "dual_status_guards": {"active": "Approved"}
    });
    let def = StateMachineDefinition::from_json(&json).unwrap();
    assert_eq!(def.required_approval_for("active"), Some("Approved"));
    assert_eq!(def.required_approval_for("draft"), None);
}

#[test]
fn empty_json_works() {
    let json = json!({});
    let def = StateMachineDefinition::from_json(&json).unwrap();
    assert!(def.transitions_from("any").is_empty());
    assert!(def.data_guards_for("any").is_empty());
}
