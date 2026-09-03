//! Parity tests for the client-generic workflow engine.
//!
//! Uses an in-memory [`WorkflowTx`]/[`WorkflowClient`] pair (the `Memory` row
//! backend) to prove the transition state machine behaves identically to the
//! pre-refactor SeaORM engine without needing a live Postgres.

use std::sync::{Arc, Mutex};

use codegraph_workflow::{
    GenericWorkflowService, TransitionContext, TriggerSource, WfParam, WfRow, WfValue,
    WorkflowClient, WorkflowService, WorkflowTx,
};
use uuid::Uuid;

#[derive(Default)]
struct FakeState {
    definition: Option<(Uuid, String, Vec<String>, serde_json::Value)>,
    instance: Option<(Uuid, String, bool)>,
    executed: Vec<String>,
    committed: bool,
    rolled_back: bool,
}

struct FakeTx {
    state: Arc<Mutex<FakeState>>,
}

impl FakeTx {
    fn new(state: Arc<Mutex<FakeState>>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl WorkflowTx for FakeTx {
    async fn execute(
        &self,
        sql: &str,
        _params: &[WfParam],
    ) -> Result<u64, codegraph_workflow::WorkflowError> {
        self.state.lock().unwrap().executed.push(sql.to_string());
        Ok(1)
    }

    async fn query_one(
        &self,
        sql: &str,
        _params: &[WfParam],
    ) -> Result<Option<WfRow>, codegraph_workflow::WorkflowError> {
        let state = self.state.lock().unwrap();
        if sql.contains("FROM platform.workflow_definition") {
            if let Some((id, initial, terminal, sm)) = &state.definition {
                return Ok(Some(WfRow::Memory(vec![
                    ("id".to_string(), WfValue::Uuid(*id)),
                    (
                        "initial_state".to_string(),
                        WfValue::String(initial.clone()),
                    ),
                    (
                        "terminal_states".to_string(),
                        WfValue::StringVec(terminal.clone()),
                    ),
                    ("state_machine".to_string(), WfValue::Json(sm.clone())),
                ])));
            }
            return Ok(None);
        }
        if sql.contains("FROM platform.workflow_instance") && sql.contains("definition_id") {
            if let Some((id, current, terminal)) = &state.instance {
                return Ok(Some(WfRow::Memory(vec![
                    ("id".to_string(), WfValue::Uuid(*id)),
                    (
                        "current_state".to_string(),
                        WfValue::String(current.clone()),
                    ),
                    ("is_terminal".to_string(), WfValue::Bool(*terminal)),
                ])));
            }
            return Ok(None);
        }
        Ok(None)
    }

    async fn query_all(
        &self,
        _sql: &str,
        _params: &[WfParam],
    ) -> Result<Vec<WfRow>, codegraph_workflow::WorkflowError> {
        Ok(Vec::new())
    }

    async fn commit(&mut self) -> Result<(), codegraph_workflow::WorkflowError> {
        self.state.lock().unwrap().committed = true;
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), codegraph_workflow::WorkflowError> {
        self.state.lock().unwrap().rolled_back = true;
        Ok(())
    }
}

struct FakeClient {
    state: Arc<Mutex<FakeState>>,
}

#[async_trait::async_trait]
impl WorkflowClient for FakeClient {
    async fn begin(&self) -> Result<Box<dyn WorkflowTx>, codegraph_workflow::WorkflowError> {
        Ok(Box::new(FakeTx::new(self.state.clone())))
    }
}

fn simple_state_machine() -> serde_json::Value {
    serde_json::json!({
        "transitions": {"draft": ["active"], "active": ["closed"]}
    })
}

fn ctx(target_state: &str) -> TransitionContext {
    TransitionContext {
        tenant_id: Uuid::new_v4(),
        entity_id: Uuid::new_v4(),
        domain: "recruiting".to_string(),
        entity_table: "candidate".to_string(),
        target_state: target_state.to_string(),
        actor_id: None,
        correlation_id: Uuid::new_v4(),
        idempotency_key: None,
        comment: None,
        entity_data: serde_json::Value::Null,
        trigger_source: TriggerSource::User,
        session_user_id: None,
        session_api_key_id: None,
    }
}

#[test]
fn transition_commits_and_advances_state() {
    let def_id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();
    let state = Arc::new(Mutex::new(FakeState {
        definition: Some((
            def_id,
            "draft".to_string(),
            vec!["closed".to_string()],
            simple_state_machine(),
        )),
        instance: Some((instance_id, "draft".to_string(), false)),
        ..Default::default()
    }));

    let service = GenericWorkflowService::new(FakeClient {
        state: state.clone(),
    });

    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(service.transition(ctx("active")))
        .expect("transition should succeed");

    assert_eq!(result.current_state, "active");
    assert!(!result.is_terminal);
    assert_eq!(result.available_transitions, vec!["closed".to_string()]);

    let state = state.lock().unwrap();
    assert!(state.committed, "transition must commit on success");
    assert!(!state.rolled_back);
    assert!(
        state
            .executed
            .iter()
            .any(|s| s.contains("INSERT INTO platform.workflow_transition")),
        "transition row must be recorded"
    );
    assert!(
        state
            .executed
            .iter()
            .any(|s| s.contains("UPDATE platform.workflow_instance")),
        "instance must be updated"
    );
}

#[test]
fn invalid_transition_rolls_back() {
    let def_id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();
    let state = Arc::new(Mutex::new(FakeState {
        definition: Some((
            def_id,
            "draft".to_string(),
            vec!["closed".to_string()],
            simple_state_machine(),
        )),
        instance: Some((instance_id, "draft".to_string(), false)),
        ..Default::default()
    }));

    let service = GenericWorkflowService::new(FakeClient {
        state: state.clone(),
    });

    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(service.transition(ctx("closed")))
        .expect_err("draft -> closed must be invalid");

    assert!(
        matches!(
            result,
            codegraph_workflow::WorkflowError::InvalidTransition { .. }
        ),
        "expected InvalidTransition, got {result:?}"
    );

    let state = state.lock().unwrap();
    assert!(!state.committed, "invalid transition must not commit");
    assert!(state.rolled_back, "invalid transition must roll back");
}

#[test]
fn already_terminal_is_conflict() {
    let def_id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();
    let state = Arc::new(Mutex::new(FakeState {
        definition: Some((
            def_id,
            "closed".to_string(),
            vec!["closed".to_string()],
            simple_state_machine(),
        )),
        instance: Some((instance_id, "closed".to_string(), true)),
        ..Default::default()
    }));

    let service = GenericWorkflowService::new(FakeClient {
        state: state.clone(),
    });

    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(service.transition(ctx("active")))
        .expect_err("terminal instance must reject transition");

    assert!(matches!(
        result,
        codegraph_workflow::WorkflowError::AlreadyTerminal
    ));
    assert!(state.lock().unwrap().rolled_back);
}
