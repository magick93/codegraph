//! Write-both transitions (issue #816 — Decision 3): every transition
//! writes the entity's status column and the workflow record in ONE
//! transaction.
//!
//! Two bands:
//!
//! 1. **Node-free pins** over the recording in-memory [`WorkflowTx`]: the
//!    entity-column UPDATE rides the same transaction as the instance
//!    update + transition insert (statement shape, parameter shape, and
//!    ordering), and a failing entity write rolls the whole transition
//!    back (nothing lands).
//! 2. **The live atomicity pin** (ignored, real Postgres): a transition
//!    moves BOTH rows or neither.

use std::sync::{Arc, Mutex};

use codegraph_workflow::{
    GenericWorkflowService, TransitionContext, TriggerSource, WfParam, WfRow, WfValue,
    WorkflowClient, WorkflowError, WorkflowService, WorkflowTx,
};
use serde_json::{Value, json};
use uuid::Uuid;

fn definition_envelope() -> Value {
    let machine = json!({
        "context": {"fields": []},
        "id": "payroll.pay_run",
        "initial": "Draft",
        "setup": null,
        "states": [
            {
                "children": [], "description": null, "entry": [], "exit": [],
                "initial": null, "invoke": null, "name": "Draft",
                "on_done": [], "output": null, "tags": [],
                "transitions": [{
                    "actions": [], "description": null, "event": "Calculating",
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
    });
    let canonical = codegraph_workflow::machine_runtime::canonical_json(&machine);
    json!({
        "machine": machine,
        "machine_hash": codegraph_workflow::machine_runtime::fnv1a_64_hex(canonical.as_bytes()),
        "data_guards": [],
        "timers": [],
        "dual_status_guards": {},
    })
}

#[derive(Default)]
struct FakeState {
    definition: Option<(Uuid, String, Vec<String>, Value, String)>,
    instance: Option<(Uuid, String, bool)>,
    executed: Vec<(String, Vec<WfParam>)>,
    committed: bool,
    rolled_back: bool,
    /// When set, the write-both entity UPDATE returns 0 rows (the row
    /// vanished mid-transition) — the failure the atomicity pin drives.
    fail_entity_write: bool,
}

struct FakeTx {
    state: Arc<Mutex<FakeState>>,
}

impl FakeTx {
    fn new(state: Arc<Mutex<FakeState>>) -> Self {
        Self { state }
    }
}

fn entity_write_index(state: &FakeState) -> Option<usize> {
    state
        .executed
        .iter()
        .position(|(sql, _)| sql.contains("SET \"status\" ="))
}

#[async_trait::async_trait]
impl WorkflowTx for FakeTx {
    async fn execute(&self, sql: &str, params: &[WfParam]) -> Result<u64, WorkflowError> {
        let mut state = self.state.lock().unwrap();
        if sql.contains("SET \"status\" =") && state.fail_entity_write {
            // Record it, then report the row vanished.
            state.executed.push((sql.to_string(), params.to_vec()));
            return Ok(0);
        }
        state.executed.push((sql.to_string(), params.to_vec()));
        Ok(1)
    }

    async fn query_one(
        &self,
        sql: &str,
        _params: &[WfParam],
    ) -> Result<Option<WfRow>, WorkflowError> {
        let state = self.state.lock().unwrap();
        if sql.contains("FROM platform.workflow_definition") {
            if let Some((id, initial, terminal, sm, status_field)) = &state.definition {
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
                    (
                        "status_field".to_string(),
                        WfValue::String(status_field.clone()),
                    ),
                ])));
            }
            return Ok(None);
        }
        if sql.contains("FROM platform.workflow_instance WHERE") && sql.contains("definition_id") {
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
    ) -> Result<Vec<WfRow>, WorkflowError> {
        Ok(Vec::new())
    }

    async fn commit(&mut self) -> Result<(), WorkflowError> {
        self.state.lock().unwrap().committed = true;
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), WorkflowError> {
        self.state.lock().unwrap().rolled_back = true;
        Ok(())
    }
}

fn fixture_state() -> Arc<Mutex<FakeState>> {
    let state = FakeState {
        definition: Some((
            Uuid::new_v4(),
            "Draft".to_string(),
            vec!["Calculating".to_string()],
            definition_envelope(),
            "status".to_string(),
        )),
        instance: Some((Uuid::new_v4(), "Draft".to_string(), false)),
        ..Default::default()
    };
    Arc::new(Mutex::new(state))
}

fn ctx(entity_id: Uuid, target: &str) -> TransitionContext {
    TransitionContext {
        tenant_id: Uuid::new_v4(),
        entity_id,
        domain: "payroll".to_string(),
        entity_table: "pay_run".to_string(),
        target_state: target.to_string(),
        actor_id: None,
        correlation_id: Uuid::new_v4(),
        idempotency_key: None,
        comment: None,
        entity_data: json!({"total": 42}),
        trigger_source: TriggerSource::User,
        session_user_id: None,
        session_api_key_id: None,
    }
}

#[tokio::test]
async fn the_transition_writes_the_entity_status_column_in_the_same_transaction() {
    let state = fixture_state();
    let client = FakeClient(state.clone());
    let service = GenericWorkflowService::new(client);
    let entity_id = Uuid::new_v4();
    let ws = service
        .transition(ctx(entity_id, "Calculating"))
        .await
        .expect("the kernel advances the transition");
    assert_eq!(ws.current_state, "Calculating");

    let guard = state.lock().unwrap();
    // The write-both statement targets the entity's domain-qualified table
    // and the definition's status column, bound to the NEW state + entity id.
    let (sql, params) = guard
        .executed
        .iter()
        .find(|(sql, _)| sql.contains("SET \"status\" ="))
        .expect("the transition must write the entity status column");
    assert!(
        sql.contains("UPDATE \"payroll\".\"pay_run\""),
        "the write is domain-schema qualified: {sql}"
    );
    assert_eq!(
        sql,
        "UPDATE \"payroll\".\"pay_run\" SET \"status\" = $1 WHERE id = $2"
    );
    match (&params[0], &params[1]) {
        (WfParam::Str(s), WfParam::Uuid(id)) => {
            assert_eq!(s, "Calculating");
            assert_eq!(*id, entity_id);
        }
        other => panic!("unexpected write-both params: {other:?}"),
    }

    // Same transaction, committed: the entity write lands between the
    // workflow-record writes and the commit.
    let instance_update = guard
        .executed
        .iter()
        .position(|(sql, _)| {
            sql.contains("UPDATE platform.workflow_instance") && sql.contains("is_terminal")
        })
        .expect("instance update recorded");
    let entity_write = entity_write_index(&guard).expect("entity write recorded");
    let commit = guard.executed.len(); // commit is not an execute; ordering asserted via the flag
    let _ = commit;
    assert!(
        instance_update < entity_write,
        "the workflow record moves before the entity column"
    );
    assert!(guard.committed, "the transaction committed");
    assert!(!guard.rolled_back);
}

#[tokio::test]
async fn a_failed_entity_write_rolls_back_the_whole_transition() {
    let state = fixture_state();
    state.lock().unwrap().fail_entity_write = true;
    let client = FakeClient(state.clone());
    let service = GenericWorkflowService::new(client);
    let err = service
        .transition(ctx(Uuid::new_v4(), "Calculating"))
        .await
        .expect_err("the vanished entity row must fail the transition");
    assert!(
        err.to_string().contains("write-both"),
        "the failure names the write-both contract: {err}"
    );
    let guard = state.lock().unwrap();
    assert!(guard.rolled_back, "the whole transaction rolled back");
    assert!(!guard.committed, "nothing landed");
}

#[tokio::test]
async fn an_invalid_transition_never_touches_the_entity_column() {
    let state = fixture_state();
    let client = FakeClient(state.clone());
    let service = GenericWorkflowService::new(client);
    let err = service
        .transition(ctx(Uuid::new_v4(), "Voided"))
        .await
        .expect_err("no edge targets Voided");
    assert!(err.to_string().contains("invalid transition"), "{err}");
    let guard = state.lock().unwrap();
    assert!(
        entity_write_index(&guard).is_none(),
        "the kernel refused before any write: {:?}",
        guard.executed
    );
    assert!(guard.rolled_back);
}

#[tokio::test]
async fn a_drifted_machine_hash_is_a_named_hard_error() {
    let mut envelope = definition_envelope();
    envelope["machine_hash"] = Value::String("deadbeefdeadbeef".to_string());
    let state = Arc::new(Mutex::new(FakeState {
        definition: Some((
            Uuid::new_v4(),
            "Draft".to_string(),
            vec!["Calculating".to_string()],
            envelope,
            "status".to_string(),
        )),
        instance: Some((Uuid::new_v4(), "Draft".to_string(), false)),
        ..Default::default()
    }));
    let client = FakeClient(state.clone());
    let service = GenericWorkflowService::new(client);
    let err = service
        .transition(ctx(Uuid::new_v4(), "Calculating"))
        .await
        .expect_err("a drifted machine must never silently re-decide");
    let text = err.to_string();
    assert!(text.contains("drift"), "the error names the drift: {text}");
    assert!(
        text.contains("deadbeefdeadbeef"),
        "the recorded hash is named: {text}"
    );
    assert!(
        text.contains("payroll.pay_run"),
        "the machine id is named: {text}"
    );
    let guard = state.lock().unwrap();
    assert!(guard.rolled_back);
    assert!(entity_write_index(&guard).is_none());
}

#[tokio::test]
async fn a_legacy_shape_definition_is_a_named_error_pointing_at_the_band() {
    let legacy = json!({
        "transitions": {"Draft": ["Calculating"]},
        "data_guards": [],
        "timers": [],
        "dual_status_guards": {},
    });
    let state = Arc::new(Mutex::new(FakeState {
        definition: Some((
            Uuid::new_v4(),
            "Draft".to_string(),
            vec!["Calculating".to_string()],
            legacy,
            "status".to_string(),
        )),
        instance: Some((Uuid::new_v4(), "Draft".to_string(), false)),
        ..Default::default()
    }));
    let client = FakeClient(state.clone());
    let service = GenericWorkflowService::new(client);
    let err = service
        .transition(ctx(Uuid::new_v4(), "Calculating"))
        .await
        .expect_err("a pre-kernel definition must fail closed, not walk the map");
    let text = err.to_string();
    assert!(
        text.contains("machine kernel"),
        "the error names the machine kernel: {text}"
    );
    assert!(
        text.contains("0007_workflow_machine_seed.sql"),
        "the error names the upgrade migration: {text}"
    );
}

struct FakeClient(Arc<Mutex<FakeState>>);

#[async_trait::async_trait]
impl WorkflowClient for FakeClient {
    async fn begin(&self) -> Result<Box<dyn WorkflowTx>, WorkflowError> {
        Ok(Box::new(FakeTx::new(self.0.clone())))
    }
}

// ---------------------------------------------------------------------------
// Band 2: the live atomicity pin over real Postgres.
// ---------------------------------------------------------------------------

/// Creates a scratch database, applies the minimal workflow + entity DDL,
/// and drives one transition through the REAL SeaORM transaction path:
/// the entity column and the workflow record must move together, and a
/// refused transition must move neither.
#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
#[ignore = "needs Postgres (DATABASE_URL) — the write-both live atomicity pin"]
async fn write_both_is_atomic_over_real_postgres() {
    use sea_orm::ConnectionTrait;

    let base = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/postgres".to_string());
    // Replace the trailing database segment with the scratch db's name.
    let (root, _) = base
        .rsplit_once('/')
        .expect("DATABASE_URL must carry a database segment");
    let admin = sea_orm::Database::connect(&base)
        .await
        .expect("connect to Postgres");
    let db_name = format!("wf_write_both_{}", Uuid::new_v4().simple());
    admin
        .execute_unprepared(&format!("CREATE DATABASE {db_name}"))
        .await
        .expect("create scratch db");

    let result = run_live_pin(&format!("{root}/{db_name}")).await;

    let _ = admin
        .execute_unprepared(&format!("DROP DATABASE {db_name} WITH (FORCE)"))
        .await;
    result.expect("the live write-both pin");
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_live_pin(url: &str) -> Result<(), String> {
    use codegraph_workflow::engine::SeaOrmWorkflowService;
    use sea_orm::{ConnectionTrait, Statement};

    let db = sea_orm::Database::connect(url)
        .await
        .map_err(|e| e.to_string())?;

    let ddl = "\
        CREATE SCHEMA payroll; \
        CREATE SCHEMA platform; \
        CREATE TABLE platform.workflow_definition (\
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
            tenant_id UUID NOT NULL, \
            name TEXT NOT NULL, \
            domain TEXT NOT NULL, \
            entity_table TEXT NOT NULL, \
            status_field TEXT NOT NULL, \
            approval_status_field TEXT, \
            initial_state TEXT NOT NULL, \
            terminal_states TEXT[] NOT NULL DEFAULT '{}', \
            state_machine JSONB NOT NULL DEFAULT '{}', \
            version INT NOT NULL DEFAULT 1, \
            is_active BOOLEAN NOT NULL DEFAULT true, \
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
            UNIQUE (tenant_id, domain, entity_table, version)); \
        CREATE TABLE platform.workflow_instance (\
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
            tenant_id UUID NOT NULL, \
            definition_id UUID NOT NULL REFERENCES platform.workflow_definition(id), \
            entity_id UUID NOT NULL, \
            current_state TEXT NOT NULL, \
            approval_state TEXT, \
            is_terminal BOOLEAN NOT NULL DEFAULT false, \
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
            UNIQUE (tenant_id, definition_id, entity_id)); \
        CREATE TABLE platform.workflow_transition (\
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
            tenant_id UUID NOT NULL, \
            instance_id UUID NOT NULL REFERENCES platform.workflow_instance(id), \
            from_state TEXT NOT NULL, \
            to_state TEXT NOT NULL, \
            correlation_id UUID, \
            actor_id UUID, \
            comment TEXT, \
            idempotency_key UUID, \
            occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()); \
        CREATE TABLE platform.workflow_timer (\
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
            tenant_id UUID NOT NULL, \
            instance_id UUID NOT NULL REFERENCES platform.workflow_instance(id), \
            timer_type TEXT NOT NULL, \
            fires_at TIMESTAMPTZ NOT NULL, \
            target_state TEXT, \
            is_fired BOOLEAN NOT NULL DEFAULT false); \
        CREATE TABLE payroll.pay_run (\
            id UUID PRIMARY KEY, \
            status TEXT NOT NULL, \
            total NUMERIC NOT NULL DEFAULT 0);";
    db.execute_unprepared(ddl)
        .await
        .map_err(|e| e.to_string())?;

    // Seed the definition with the machine envelope (the 0007 shape).
    let envelope = definition_envelope().to_string();
    db.execute_unprepared(&format!(
        "INSERT INTO platform.workflow_definition \
         (tenant_id, name, domain, entity_table, status_field, initial_state, terminal_states, state_machine) \
         VALUES ('00000000-0000-0000-0000-000000000000', 'payroll.pay_run', 'payroll', 'pay_run', \
                 'status', 'Draft', ARRAY['Calculating']::TEXT[], '{envelope}'::jsonb)"
    ))
    .await
    .map_err(|e| e.to_string())?;

    let entity_id = Uuid::new_v4();
    db.execute_unprepared(&format!(
        "INSERT INTO payroll.pay_run (id, status, total) VALUES ('{entity_id}', 'Draft', 42)"
    ))
    .await
    .map_err(|e| e.to_string())?;

    let service = SeaOrmWorkflowService::new(db.clone());
    let tenant = Uuid::nil();

    // 1. A valid transition moves BOTH rows in one commit.
    let ws = service
        .transition(TransitionContext {
            tenant_id: tenant,
            entity_id,
            domain: "payroll".into(),
            entity_table: "pay_run".into(),
            target_state: "Calculating".into(),
            actor_id: None,
            correlation_id: Uuid::new_v4(),
            idempotency_key: None,
            comment: None,
            entity_data: json!({}),
            trigger_source: TriggerSource::User,
            session_user_id: None,
            session_api_key_id: None,
        })
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(ws.current_state, "Calculating");

    let row = db
        .query_one_raw(Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            format!("SELECT status FROM payroll.pay_run WHERE id = '{entity_id}'"),
        ))
        .await
        .map_err(|e| e.to_string())?
        .expect("entity row");
    let status: String = row
        .try_get::<String>("", "status")
        .map_err(|e| e.to_string())?;
    assert_eq!(
        status, "Calculating",
        "the entity column moved with the record"
    );

    let transitions = db
        .query_all_raw(Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            "SELECT to_state FROM platform.workflow_transition".to_string(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(transitions.len(), 1, "one audit row");

    // 2. A refused transition (final state) moves NEITHER row.
    let refused = service.transition(TransitionContext {
        tenant_id: tenant,
        entity_id,
        domain: "payroll".into(),
        entity_table: "pay_run".into(),
        target_state: "Draft".into(),
        actor_id: None,
        correlation_id: Uuid::new_v4(),
        idempotency_key: None,
        comment: None,
        entity_data: json!({}),
        trigger_source: TriggerSource::User,
        session_user_id: None,
        session_api_key_id: None,
    });
    assert!(
        refused.await.is_err(),
        "a final state refuses further transitions"
    );
    let row = db
        .query_one_raw(Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            format!("SELECT status FROM payroll.pay_run WHERE id = '{entity_id}'"),
        ))
        .await
        .map_err(|e| e.to_string())?
        .expect("entity row");
    let status: String = row.try_get::<String>("", "status").expect("status column");
    assert_eq!(status, "Calculating", "the refusal moved nothing");

    Ok(())
}
