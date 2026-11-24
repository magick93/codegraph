use codegraph_workflow::definition::WorkflowDefinition;
use serde_json::json;

#[test]
fn parses_the_machine_envelope() {
    let json = json!({
        "machine": {"id": "payrun", "initial": "draft", "context": {"fields": []}, "states": []},
        "machine_hash": "af63dc4c8601ec8c",
        "data_guards": [
            {"transition_to": "offer", "rule": "salary > 0", "message": "Need salary"}
        ],
        "dual_status_guards": {"active": "Approved"}
    });
    let def = WorkflowDefinition::from_json(&json).unwrap();
    assert!(def.machine.is_some());
    assert_eq!(def.machine_hash.as_deref(), Some("af63dc4c8601ec8c"));
    assert!(!def.is_legacy_shape());
    assert_eq!(def.required_approval_for("active"), Some("Approved"));
}

#[test]
fn legacy_rows_are_detected_by_shape() {
    let legacy = json!({"transitions": {"new": ["active", "cancelled"], "active": ["closed"]}});
    let def = WorkflowDefinition::from_json(&legacy).unwrap();
    assert!(
        def.is_legacy_shape(),
        "a transitions map with no machine is legacy"
    );
    assert!(def.machine.is_none());
}

#[test]
fn data_guards_for_state() {
    let json = json!({
        "machine": null,
        "data_guards": [
            {"transition_to": "offer", "rule": "salary > 0", "message": "Need salary"},
            {"transition_to": "screening", "rule": "quals IS NOT EMPTY", "message": "Need quals"}
        ]
    });
    let def = WorkflowDefinition::from_json(&json).unwrap();
    let guards = def.data_guards_for("offer");
    assert_eq!(guards.len(), 1);
    assert_eq!(guards[0].rule, "salary > 0");
    assert!(def.data_guards_for("active").is_empty());
}

#[test]
fn timers_for_state() {
    let json = json!({
        "timers": [
            {"trigger_on_enter": "screening", "type": "deadline", "duration_hours": 48, "target_state": "escalated"},
            {"trigger_on_enter": "screening", "type": "reminder", "duration_hours": 24}
        ]
    });
    let def = WorkflowDefinition::from_json(&json).unwrap();
    let timers = def.timers_for_state("screening");
    assert_eq!(timers.len(), 2);
    assert!(def.timers_for_state("new").is_empty());
}

#[test]
fn empty_json_works() {
    let json = json!({});
    let def = WorkflowDefinition::from_json(&json).unwrap();
    assert!(def.data_guards_for("any").is_empty());
    assert!(
        !def.is_legacy_shape(),
        "an empty row is not legacy — it is just empty"
    );
}

#[test]
fn guard_names_are_deterministic_and_composed() {
    let json = json!({
        "data_guards": [
            {"transition_to": "offer", "rule": "salary > 0", "message": "Need salary"},
            {"transition_to": "active", "rule": "quals IS NOT EMPTY", "message": "Need quals"}
        ],
        "dual_status_guards": {"active": "Approved", "paused": "Reviewed"}
    });
    let def = WorkflowDefinition::from_json(&json).unwrap();
    // `offer` is data-only, `active` carries both (composed), `paused` is
    // dual-only — sorted for determinism.
    assert_eq!(
        def.guard_names(),
        vec![
            "data_guard:offer".to_string(),
            "dual_status:paused".to_string(),
            "guards:active".to_string(),
        ]
    );
    // Determinism: two parses, same names.
    let again = WorkflowDefinition::from_json(&json).unwrap();
    assert_eq!(def.guard_names(), again.guard_names());
}

#[test]
fn approval_chain_detection() {
    let json = json!({
        "approval_chains": [{
            "transition_from": "draft",
            "transition_to": "approved",
            "steps": [{"order": 1, "role": "manager", "is_required": true}]
        }]
    });
    let def = WorkflowDefinition::from_json(&json).unwrap();
    assert!(def.has_approval_chain("draft", "approved"));
    assert!(!def.has_approval_chain("draft", "closed"));
    assert!(def.approval_chain_for("draft", "approved").is_some());
}
