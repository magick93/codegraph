//! Issue #635: the IFML action-leg planner under `api_key_mode = "agent"`.

use codegraph_core::mock::MockEngine;

/// Issue #635: under `api_key_mode = "agent"` the spec's API key is an
/// agent session — the route's key-arm verdict decides the legs before the
/// execution kind does (denial = ONE 403 leg asserting the handler's own
/// message; a `when`-only permit stays unbuilt; admitted/unaffected keep
/// the execution's legs).
#[tokio::test]
async fn agent_mode_key_verdict_decides_the_action_legs() {
    use super::spec_payload::ActionActKind;
    use crate::ddd::service_ops::ApiKeyGate;

    let db = MockEngine::builder().build();
    let mut route = crate::ddd::service_ops::ServiceOpRoute {
        service: "PayRunService".to_string(),
        op_name: "approve".to_string(),
        snake: "approve".to_string(),
        fn_name: "service_op_approve".to_string(),
        capabilities: vec!["ApprovePayRun".to_string()],
        authorize_fn: None,
        execution: crate::ddd::service_ops::ExecutionKind::Read,
        record_load: true,
        authorize_async: false,
        api_key: ApiKeyGate::Denied {
            reason: "capability `ApprovePayRun` is not granted to agent sessions".to_string(),
        },
        injected_guard: None,
        invariant_block: None,
        throws: Vec::new(),
        raise_responses: Vec::new(),
        create_guard_block: String::new(),
        create_invariant_block: String::new(),
        render_facts: Default::default(),
        execution_block: String::new(),
        publish_arg: None,
    };
    let kinds = |legs: Vec<(ActionActKind, Option<(String, String)>)>| {
        legs.into_iter().map(|(k, _)| k).collect::<Vec<_>>()
    };
    let legs = super::assembly::plan_action_legs(&db, "PayRunType", &route).await;
    assert_eq!(kinds(legs), vec![ActionActKind::AgentNotGranted]);
    assert_eq!(
        ActionActKind::AgentNotGranted.error_fragment(),
        Some("not granted to agent sessions")
    );

    route.api_key = ApiKeyGate::Denied {
        reason: "separation of duties: agent sessions hold `PreparePayRun`".to_string(),
    };
    let legs = super::assembly::plan_action_legs(&db, "PayRunType", &route).await;
    assert_eq!(kinds(legs), vec![ActionActKind::AgentSeparationOfDuty]);
    assert_eq!(ActionActKind::AgentSeparationOfDuty.status(), "403");

    route.api_key = ApiKeyGate::Conditional;
    assert!(
        super::assembly::plan_action_legs(&db, "PayRunType", &route)
            .await
            .is_empty()
    );

    route.api_key = ApiKeyGate::Admitted;
    let legs = super::assembly::plan_action_legs(&db, "PayRunType", &route).await;
    assert_eq!(kinds(legs), vec![ActionActKind::Ok]);
}
