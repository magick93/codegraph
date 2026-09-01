//! Client-generic workflow engine.
//!
//! The state machine (transition / approval / state / history) is expressed as
//! free functions over [`WorkflowTx`]; the [`GenericWorkflowService`] in
//! [`crate::tx`] opens a transaction and dispatches. The native SeaORM
//! backend (and its [`SeaOrmWorkflowService`] alias) lives here too, so the
//! monolith keeps its existing constructor while the worker slice supplies its
//! own [`WorkflowClient`] from generated code.

use uuid::Uuid;

use crate::definition::StateMachineDefinition;
use crate::error::WorkflowError;
use crate::guard::GuardEvaluator;
use crate::tx::{WfParam, WorkflowTx};
use crate::types::*;

/// Set `app.organization_id`, `app.user_id`, and `app.current_api_key` session
/// variables for RLS enforcement. Must run inside the current transaction
/// (each backend opens one) because `is_local = true` scopes the config to it.
pub async fn set_rls_org(
    tx: &dyn WorkflowTx,
    tenant_id: Uuid,
    user_id: Option<Uuid>,
    api_key_id: Option<Uuid>,
) -> Result<(), WorkflowError> {
    let uid = user_id.unwrap_or(Uuid::nil()).to_string();
    let aid = api_key_id.unwrap_or(Uuid::nil()).to_string();
    tx.query_one(
        "SELECT set_config('app.organization_id', $1, true), \
                set_config('app.user_id', $2, true), \
                set_config('app.current_api_key', $3, true)",
        &[
            WfParam::Str(tenant_id.to_string()),
            WfParam::Str(uid),
            WfParam::Str(aid),
        ],
    )
    .await?;
    Ok(())
}

/// Load workflow definition by (tenant, domain, entity_table).
async fn load_definition(
    tx: &dyn WorkflowTx,
    tenant_id: Uuid,
    domain: &str,
    entity_table: &str,
) -> Result<(Uuid, String, Vec<String>, StateMachineDefinition), WorkflowError> {
    let row = tx
        .query_one(
            r#"SELECT id, initial_state, terminal_states, state_machine
               FROM platform.workflow_definition
               WHERE tenant_id IN ($1, '00000000-0000-0000-0000-000000000000'::uuid)
                 AND domain = $2 AND entity_table = $3 AND is_active = true
               ORDER BY CASE WHEN tenant_id = $1 THEN 0 ELSE 1 END, version DESC
               LIMIT 1"#,
            &[
                WfParam::Uuid(tenant_id),
                WfParam::Str(domain.to_string()),
                WfParam::Str(entity_table.to_string()),
            ],
        )
        .await?
        .ok_or(WorkflowError::NotFound)?;

    let def_id = row.get_uuid("id")?;
    let initial_state = row.get_string("initial_state")?;
    let terminal_states = row.get_string_vec("terminal_states")?;
    let sm_json = row.get_json("state_machine")?;
    let sm = StateMachineDefinition::from_json(&sm_json)
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

    Ok((def_id, initial_state, terminal_states, sm))
}

/// Load or lazily create the workflow instance.
async fn load_or_create_instance(
    tx: &dyn WorkflowTx,
    tenant_id: Uuid,
    def_id: Uuid,
    entity_id: Uuid,
    initial_state: &str,
) -> Result<(Uuid, String, bool), WorkflowError> {
    let existing = tx
        .query_one(
            r#"SELECT id, current_state, is_terminal
               FROM platform.workflow_instance
               WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
            &[
                WfParam::Uuid(tenant_id),
                WfParam::Uuid(def_id),
                WfParam::Uuid(entity_id),
            ],
        )
        .await?;

    if let Some(row) = existing {
        let id = row.get_uuid("id")?;
        let state = row.get_string("current_state")?;
        let terminal = row.get_bool("is_terminal")?;
        return Ok((id, state, terminal));
    }

    let instance_id = Uuid::new_v4();
    tx.execute(
        r#"INSERT INTO platform.workflow_instance
           (id, tenant_id, definition_id, entity_id, current_state, is_terminal)
           VALUES ($1, $2, $3, $4, $5, false)"#,
        &[
            WfParam::Uuid(instance_id),
            WfParam::Uuid(tenant_id),
            WfParam::Uuid(def_id),
            WfParam::Uuid(entity_id),
            WfParam::Str(initial_state.to_string()),
        ],
    )
    .await?;

    Ok((instance_id, initial_state.to_string(), false))
}

/// Execute a state transition. The caller (a `WorkflowService` impl) owns the
/// transaction; this function commits only where the transition is durable
/// (including the approval-required branch, which persists the pending state).
pub async fn transition(
    tx: &mut dyn WorkflowTx,
    ctx: &TransitionContext,
) -> Result<WorkflowState, WorkflowError> {
    set_rls_org(
        tx,
        ctx.tenant_id,
        ctx.session_user_id,
        ctx.session_api_key_id,
    )
    .await?;

    let (def_id, initial_state, terminal_states, sm) =
        load_definition(tx, ctx.tenant_id, &ctx.domain, &ctx.entity_table).await?;

    let (instance_id, current_state, is_terminal) =
        load_or_create_instance(tx, ctx.tenant_id, def_id, ctx.entity_id, &initial_state).await?;

    if is_terminal {
        return Err(WorkflowError::AlreadyTerminal);
    }

    if !sm.is_valid_transition(&current_state, &ctx.target_state) {
        return Err(WorkflowError::InvalidTransition {
            current: current_state,
            target: ctx.target_state.clone(),
        });
    }

    if ctx.trigger_source != TriggerSource::Timer {
        for guard in sm.data_guards_for(&ctx.target_state) {
            if !GuardEvaluator::evaluate(&guard.rule, &ctx.entity_data)
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?
            {
                return Err(WorkflowError::GuardFailed {
                    rule: guard.rule.clone(),
                    message: guard.message.clone(),
                });
            }
        }
    }

    if let Some(required_approval) = sm.required_approval_for(&ctx.target_state) {
        let approval_state = tx
            .query_one(
                "SELECT approval_state FROM platform.workflow_instance WHERE id = $1",
                &[WfParam::Uuid(instance_id)],
            )
            .await?
            .and_then(|r| r.get_opt_string("approval_state").ok().flatten());

        if approval_state.as_deref() != Some(required_approval) {
            return Err(WorkflowError::DualStatusGuardFailed {
                status: ctx.target_state.clone(),
                required_approval: required_approval.to_string(),
            });
        }
    }

    if let Some(key) = ctx.idempotency_key {
        let exists = tx
            .query_one(
                "SELECT id FROM platform.workflow_transition WHERE idempotency_key = $1",
                &[WfParam::Uuid(key)],
            )
            .await?;
        if exists.is_some() {
            return Err(WorkflowError::IdempotencyConflict { key });
        }
    }

    if sm.has_approval_chain(&current_state, &ctx.target_state) {
        let pending_state = format!("pending_approval:{}->{}", current_state, ctx.target_state);
        tx.execute(
            r#"UPDATE platform.workflow_instance
               SET current_state = $1, updated_at = now()
               WHERE id = $2 AND current_state = $3"#,
            &[
                WfParam::Str(pending_state),
                WfParam::Uuid(instance_id),
                WfParam::Str(current_state.clone()),
            ],
        )
        .await?;

        let pending = crate::approval::get_pending_step(
            tx,
            def_id,
            instance_id,
            &current_state,
            &ctx.target_state,
        )
        .await?
        .ok_or_else(|| WorkflowError::Internal("no approval steps found".into()))?;

        tx.commit().await?;
        return Err(WorkflowError::ApprovalRequired {
            pending_step: pending,
        });
    }

    tx.execute(
        "SELECT set_config('app.correlation_id', $1, true)",
        &[WfParam::Str(ctx.correlation_id.to_string())],
    )
    .await?;

    let new_is_terminal = terminal_states.contains(&ctx.target_state);
    let updated = tx
        .execute(
            r#"UPDATE platform.workflow_instance
               SET current_state = $1, is_terminal = $2, updated_at = now()
               WHERE id = $3 AND current_state = $4"#,
            &[
                WfParam::Str(ctx.target_state.clone()),
                WfParam::Bool(new_is_terminal),
                WfParam::Uuid(instance_id),
                WfParam::Str(current_state.clone()),
            ],
        )
        .await?;

    if updated == 0 {
        return Err(WorkflowError::ConcurrentModification);
    }

    tx.execute(
        r#"INSERT INTO platform.workflow_transition
           (tenant_id, instance_id, from_state, to_state, correlation_id, actor_id, comment, idempotency_key)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        &[
            WfParam::Uuid(ctx.tenant_id),
            WfParam::Uuid(instance_id),
            WfParam::Str(current_state.clone()),
            WfParam::Str(ctx.target_state.clone()),
            WfParam::Uuid(ctx.correlation_id),
            ctx.actor_id
                .map(WfParam::Uuid)
                .unwrap_or(WfParam::Null("uuid")),
            ctx.comment
                .clone()
                .map(WfParam::Str)
                .unwrap_or(WfParam::Null("text")),
            ctx.idempotency_key
                .map(WfParam::Uuid)
                .unwrap_or(WfParam::Null("uuid")),
        ],
    )
    .await?;

    tx.execute(
        "UPDATE platform.workflow_timer SET is_fired = true \
         WHERE instance_id = $1 AND NOT is_fired",
        &[WfParam::Uuid(instance_id)],
    )
    .await?;

    for timer in sm.timers_for_state(&ctx.target_state) {
        let fires_at = chrono::Utc::now() + chrono::Duration::hours(timer.duration_hours);
        tx.execute(
            r#"INSERT INTO platform.workflow_timer
               (tenant_id, instance_id, timer_type, fires_at, target_state)
               VALUES ($1, $2, $3, $4, $5)"#,
            &[
                WfParam::Uuid(ctx.tenant_id),
                WfParam::Uuid(instance_id),
                WfParam::Str(timer.timer_type.clone()),
                WfParam::DateTime(fires_at),
                timer
                    .target_state
                    .clone()
                    .map(WfParam::Str)
                    .unwrap_or(WfParam::Null("text")),
            ],
        )
        .await?;
    }

    tx.commit().await?;

    let available = sm.transitions_from(&ctx.target_state);
    Ok(WorkflowState {
        entity_id: ctx.entity_id,
        current_state: ctx.target_state.clone(),
        approval_state: None,
        is_terminal: new_is_terminal,
        available_transitions: if new_is_terminal { vec![] } else { available },
        pending_approvals: vec![],
    })
}

/// Act on a pending approval step (approve / reject).
pub async fn approval_action(
    tx: &mut dyn WorkflowTx,
    ctx: &ApprovalContext,
) -> Result<WorkflowState, WorkflowError> {
    set_rls_org(
        tx,
        ctx.tenant_id,
        ctx.session_user_id,
        ctx.session_api_key_id,
    )
    .await?;

    let (def_id, _initial, terminal_states, sm) =
        load_definition(tx, ctx.tenant_id, &ctx.domain, &ctx.entity_table).await?;

    let row = tx
        .query_one(
            r#"SELECT id, current_state
               FROM platform.workflow_instance
               WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
            &[
                WfParam::Uuid(ctx.tenant_id),
                WfParam::Uuid(def_id),
                WfParam::Uuid(ctx.entity_id),
            ],
        )
        .await?
        .ok_or(WorkflowError::NotFound)?;

    let instance_id = row.get_uuid("id")?;
    let current_state = row.get_string("current_state")?;

    let (from, to) = current_state
        .strip_prefix("pending_approval:")
        .and_then(|s| s.split_once("->"))
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .ok_or(WorkflowError::NoPendingApproval)?;

    let pending = crate::approval::get_pending_step(tx, def_id, instance_id, &from, &to)
        .await?
        .ok_or(WorkflowError::NoPendingApproval)?;

    let decision_str = match ctx.decision {
        ApprovalDecision::Approved => "approved",
        ApprovalDecision::Rejected => "rejected",
    };

    tx.execute(
        r#"INSERT INTO platform.approval_decision
           (tenant_id, step_id, instance_id, actor_id, decision, correlation_id, comment)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        &[
            WfParam::Uuid(ctx.tenant_id),
            WfParam::Uuid(pending.step_id),
            WfParam::Uuid(instance_id),
            WfParam::Uuid(ctx.actor_id),
            WfParam::Str(decision_str.to_string()),
            WfParam::Uuid(ctx.correlation_id),
            ctx.comment
                .clone()
                .map(WfParam::Str)
                .unwrap_or(WfParam::Null("text")),
        ],
    )
    .await?;

    if ctx.decision == ApprovalDecision::Rejected {
        tx.execute(
            r#"UPDATE platform.workflow_instance
               SET current_state = $1, updated_at = now()
               WHERE id = $2"#,
            &[WfParam::Str(from.clone()), WfParam::Uuid(instance_id)],
        )
        .await?;

        tx.commit().await?;

        let available = sm.transitions_from(&from);
        return Ok(WorkflowState {
            entity_id: ctx.entity_id,
            current_state: from,
            approval_state: None,
            is_terminal: false,
            available_transitions: available,
            pending_approvals: vec![],
        });
    }

    if crate::approval::is_chain_complete(tx, def_id, instance_id, &from, &to).await? {
        let new_is_terminal = terminal_states.contains(&to);
        tx.execute(
            r#"UPDATE platform.workflow_instance
               SET current_state = $1, is_terminal = $2, updated_at = now()
               WHERE id = $3"#,
            &[
                WfParam::Str(to.clone()),
                WfParam::Bool(new_is_terminal),
                WfParam::Uuid(instance_id),
            ],
        )
        .await?;

        tx.execute(
            r#"INSERT INTO platform.workflow_transition
               (tenant_id, instance_id, from_state, to_state, correlation_id, actor_id, comment, idempotency_key)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            &[
                WfParam::Uuid(ctx.tenant_id),
                WfParam::Uuid(instance_id),
                WfParam::Str(from.clone()),
                WfParam::Str(to.clone()),
                WfParam::Uuid(ctx.correlation_id),
                WfParam::Uuid(ctx.actor_id),
                ctx.comment
                    .clone()
                    .map(WfParam::Str)
                    .unwrap_or(WfParam::Null("text")),
                ctx.idempotency_key
                    .map(WfParam::Uuid)
                    .unwrap_or(WfParam::Null("uuid")),
            ],
        )
        .await?;

        tx.commit().await?;

        let available = if new_is_terminal {
            vec![]
        } else {
            sm.transitions_from(&to)
        };
        Ok(WorkflowState {
            entity_id: ctx.entity_id,
            current_state: to,
            approval_state: None,
            is_terminal: new_is_terminal,
            available_transitions: available,
            pending_approvals: vec![],
        })
    } else {
        let next_pending =
            crate::approval::get_pending_step(tx, def_id, instance_id, &from, &to).await?;
        tx.commit().await?;

        Ok(WorkflowState {
            entity_id: ctx.entity_id,
            current_state: current_state.clone(),
            approval_state: Some(format!(
                "awaiting_step_{}",
                next_pending.as_ref().map_or(0, |s| s.step_order)
            )),
            is_terminal: false,
            available_transitions: vec![],
            pending_approvals: next_pending.into_iter().collect(),
        })
    }
}

/// Read the current workflow state for an entity.
pub async fn get_state(
    tx: &mut dyn WorkflowTx,
    tenant_id: Uuid,
    domain: &str,
    entity_table: &str,
    entity_id: Uuid,
) -> Result<WorkflowState, WorkflowError> {
    set_rls_org(tx, tenant_id, None, None).await?;

    let (def_id, initial_state, _terminal_states, sm) =
        load_definition(tx, tenant_id, domain, entity_table).await?;

    let row = tx
        .query_one(
            r#"SELECT current_state, approval_state, is_terminal
               FROM platform.workflow_instance
               WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
            &[
                WfParam::Uuid(tenant_id),
                WfParam::Uuid(def_id),
                WfParam::Uuid(entity_id),
            ],
        )
        .await?;

    let (current_state, approval_state, is_terminal) = match row {
        Some(r) => {
            let cs = r.get_string("current_state")?;
            let aps = r.get_opt_string("approval_state")?;
            let it = r.get_bool("is_terminal")?;
            (cs, aps, it)
        }
        None => (initial_state, None, false),
    };

    let available = if is_terminal {
        vec![]
    } else {
        sm.transitions_from(&current_state)
    };

    tx.commit().await?;

    Ok(WorkflowState {
        entity_id,
        current_state,
        approval_state,
        is_terminal,
        available_transitions: available,
        pending_approvals: vec![],
    })
}

/// Read the process history for an entity.
pub async fn get_history(
    tx: &mut dyn WorkflowTx,
    tenant_id: Uuid,
    domain: &str,
    entity_table: &str,
    entity_id: Uuid,
) -> Result<Vec<ProcessHistoryEntry>, WorkflowError> {
    set_rls_org(tx, tenant_id, None, None).await?;

    let rows = tx
        .query_all(
            r#"SELECT wt.id AS id, wt.occurred_at AS occurred_at, wt.to_state AS to_state,
                      wt.from_state AS from_state, wt.correlation_id AS correlation_id,
                      wt.actor_id AS actor_id, wt.comment AS comment
               FROM platform.workflow_transition wt
               JOIN platform.workflow_instance wi ON wt.instance_id = wi.id
               JOIN platform.workflow_definition wd ON wi.definition_id = wd.id
               WHERE wi.entity_id = $1 AND wi.tenant_id = $2
                 AND wd.domain = $3 AND wd.entity_table = $4
               ORDER BY wt.occurred_at ASC"#,
            &[
                WfParam::Uuid(entity_id),
                WfParam::Uuid(tenant_id),
                WfParam::Str(domain.to_string()),
                WfParam::Str(entity_table.to_string()),
            ],
        )
        .await?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(ProcessHistoryEntry {
            id: row.get_uuid("id")?,
            action_date: row.get_datetime("occurred_at")?,
            status: row.get_string("to_state")?,
            previous_status: row.get_opt_string("from_state")?,
            correlation_id: row.get_opt_uuid("correlation_id")?,
            actor_id: row.get_opt_uuid("actor_id")?,
            comment: row.get_opt_string("comment")?,
        });
    }

    tx.commit().await?;

    Ok(entries)
}

#[cfg(not(target_arch = "wasm32"))]
mod sea_orm_impl {
    use super::*;

    use sea_orm::ConnectionTrait;
    use sea_orm::TransactionTrait;

    use crate::tx::{WfRow, WorkflowClient};

    /// Convert a [`WfParam`] into a SeaORM `Value`.
    fn sea_value(p: &WfParam) -> sea_orm::Value {
        match p {
            WfParam::Uuid(v) => sea_orm::Value::from(*v),
            WfParam::Str(s) => sea_orm::Value::from(s.clone()),
            WfParam::Bool(b) => sea_orm::Value::from(*b),
            WfParam::I32(i) => sea_orm::Value::from(*i),
            WfParam::I64(i) => sea_orm::Value::from(*i),
            WfParam::DateTime(d) => sea_orm::Value::from(*d),
            WfParam::Null("uuid") => sea_orm::Value::from(None::<Uuid>),
            WfParam::Null("text") => sea_orm::Value::from(None::<String>),
            WfParam::Null("int4") => sea_orm::Value::from(None::<i32>),
            WfParam::Null(_) => sea_orm::Value::from(None::<String>),
        }
    }

    /// A [`WorkflowTx`] over a SeaORM `DatabaseTransaction`.
    pub struct SeaOrmWorkflowTx {
        tx: Option<sea_orm::DatabaseTransaction>,
    }

    impl SeaOrmWorkflowTx {
        fn inner(&self) -> Result<&sea_orm::DatabaseTransaction, WorkflowError> {
            self.tx
                .as_ref()
                .ok_or_else(|| WorkflowError::Internal("transaction already finished".into()))
        }
    }

    #[async_trait::async_trait]
    impl WorkflowTx for SeaOrmWorkflowTx {
        async fn execute(&self, sql: &str, params: &[WfParam]) -> Result<u64, WorkflowError> {
            let tx = self.inner()?;
            let values: Vec<sea_orm::Value> = params.iter().map(sea_value).collect();
            let res = tx
                .execute(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Postgres,
                    sql,
                    values,
                ))
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            Ok(res.rows_affected())
        }

        async fn query_one(
            &self,
            sql: &str,
            params: &[WfParam],
        ) -> Result<Option<WfRow>, WorkflowError> {
            let tx = self.inner()?;
            let values: Vec<sea_orm::Value> = params.iter().map(sea_value).collect();
            let res = tx
                .query_one(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Postgres,
                    sql,
                    values,
                ))
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            Ok(res.map(WfRow::SeaOrm))
        }

        async fn query_all(
            &self,
            sql: &str,
            params: &[WfParam],
        ) -> Result<Vec<WfRow>, WorkflowError> {
            let tx = self.inner()?;
            let values: Vec<sea_orm::Value> = params.iter().map(sea_value).collect();
            let res = tx
                .query_all(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Postgres,
                    sql,
                    values,
                ))
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            Ok(res.into_iter().map(WfRow::SeaOrm).collect())
        }

        async fn commit(&mut self) -> Result<(), WorkflowError> {
            if let Some(tx) = self.tx.take() {
                tx.commit()
                    .await
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            }
            Ok(())
        }

        async fn rollback(&mut self) -> Result<(), WorkflowError> {
            if let Some(tx) = self.tx.take() {
                tx.rollback()
                    .await
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            }
            Ok(())
        }
    }

    /// A [`WorkflowClient`] backed by a SeaORM `DatabaseConnection`.
    pub struct SeaOrmWorkflowClient {
        db: sea_orm::DatabaseConnection,
    }

    impl SeaOrmWorkflowClient {
        pub fn new(db: sea_orm::DatabaseConnection) -> Self {
            Self { db }
        }
    }

    #[async_trait::async_trait]
    impl WorkflowClient for SeaOrmWorkflowClient {
        async fn begin(&self) -> Result<Box<dyn WorkflowTx>, WorkflowError> {
            let tx = self
                .db
                .begin()
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            Ok(Box::new(SeaOrmWorkflowTx { tx: Some(tx) }))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use sea_orm_impl::{SeaOrmWorkflowClient, SeaOrmWorkflowTx};

/// Native SeaORM-backed workflow service. A thin wrapper over the
/// client-generic [`GenericWorkflowService`] so the monolith keeps its
/// existing `SeaOrmWorkflowService::new(db)` constructor.
#[cfg(not(target_arch = "wasm32"))]
pub struct SeaOrmWorkflowService(
    crate::tx::GenericWorkflowService<sea_orm_impl::SeaOrmWorkflowClient>,
);

#[cfg(not(target_arch = "wasm32"))]
impl SeaOrmWorkflowService {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self(crate::tx::GenericWorkflowService::new(
            sea_orm_impl::SeaOrmWorkflowClient::new(db),
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl crate::service::WorkflowService for SeaOrmWorkflowService {
    async fn transition(&self, ctx: TransitionContext) -> Result<WorkflowState, WorkflowError> {
        self.0.transition(ctx).await
    }

    async fn approval_action(&self, ctx: ApprovalContext) -> Result<WorkflowState, WorkflowError> {
        self.0.approval_action(ctx).await
    }

    async fn get_state(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<WorkflowState, WorkflowError> {
        self.0
            .get_state(tenant_id, domain, entity_table, entity_id)
            .await
    }

    async fn delegate(&self, ctx: DelegationContext) -> Result<(), WorkflowError> {
        self.0.delegate(ctx).await
    }

    async fn get_history(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<Vec<ProcessHistoryEntry>, WorkflowError> {
        self.0
            .get_history(tenant_id, domain, entity_table, entity_id)
            .await
    }
}
