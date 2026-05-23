//! SeaORM-based implementation of WorkflowService.

use async_trait::async_trait;
use sea_orm::*;
use uuid::Uuid;

use crate::definition::StateMachineDefinition;
use crate::error::WorkflowError;
use crate::guard::GuardEvaluator;
use crate::service::WorkflowService;
use crate::types::*;

pub struct SeaOrmWorkflowService {
    db: DatabaseConnection,
}

impl SeaOrmWorkflowService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Set `app.organization_id` session variable for RLS enforcement.
    /// Must be called at the start of every transaction that touches platform tables.
    async fn set_rls_org(
        conn: &impl ConnectionTrait,
        tenant_id: Uuid,
    ) -> Result<(), WorkflowError> {
        // Use query_one (not execute) so the SELECT actually runs on the same connection.
        conn.query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT set_config('app.organization_id', $1, true)",
            [tenant_id.to_string().into()],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
        Ok(())
    }

    /// Load workflow definition by (tenant, domain, entity_table).
    async fn load_definition(
        &self,
        conn: &impl ConnectionTrait,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
    ) -> Result<(Uuid, String, Vec<String>, StateMachineDefinition), WorkflowError> {
        let row = conn
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"SELECT id, initial_state, terminal_states, state_machine
                   FROM platform.workflow_definition
                   WHERE tenant_id IN ($1, '00000000-0000-0000-0000-000000000000'::uuid)
                     AND domain = $2 AND entity_table = $3 AND is_active = true
                   ORDER BY CASE WHEN tenant_id = $1 THEN 0 ELSE 1 END, version DESC
                   LIMIT 1"#,
                [tenant_id.into(), domain.into(), entity_table.into()],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?
            .ok_or(WorkflowError::NotFound)?;

        let def_id: Uuid = row
            .try_get("", "id")
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
        let initial_state: String = row
            .try_get("", "initial_state")
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
        let terminal_states: Vec<String> = row
            .try_get("", "terminal_states")
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
        let sm_json: serde_json::Value = row
            .try_get("", "state_machine")
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
        let sm = StateMachineDefinition::from_json(&sm_json)
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        Ok((def_id, initial_state, terminal_states, sm))
    }

    /// Load or lazily create workflow instance.
    async fn load_or_create_instance(
        &self,
        tx: &DatabaseTransaction,
        tenant_id: Uuid,
        def_id: Uuid,
        entity_id: Uuid,
        initial_state: &str,
    ) -> Result<(Uuid, String, bool), WorkflowError> {
        let existing = tx
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"SELECT id, current_state, is_terminal
                   FROM platform.workflow_instance
                   WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
                [tenant_id.into(), def_id.into(), entity_id.into()],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        if let Some(row) = existing {
            let id: Uuid = row
                .try_get("", "id")
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            let state: String = row
                .try_get("", "current_state")
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            let terminal: bool = row
                .try_get("", "is_terminal")
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            return Ok((id, state, terminal));
        }

        // Lazy create
        let instance_id = Uuid::new_v4();
        tx.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"INSERT INTO platform.workflow_instance
               (id, tenant_id, definition_id, entity_id, current_state, is_terminal)
               VALUES ($1, $2, $3, $4, $5, false)"#,
            [
                instance_id.into(),
                tenant_id.into(),
                def_id.into(),
                entity_id.into(),
                initial_state.into(),
            ],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        Ok((instance_id, initial_state.to_string(), false))
    }
}

#[async_trait]
impl WorkflowService for SeaOrmWorkflowService {
    async fn transition(&self, ctx: TransitionContext) -> Result<WorkflowState, WorkflowError> {
        let tx = self
            .db
            .begin()
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        // Set RLS session variable so tenant isolation policies allow access.
        Self::set_rls_org(&tx, ctx.tenant_id).await?;

        // 1. Load definition
        let (def_id, initial_state, terminal_states, sm) = self
            .load_definition(&tx, ctx.tenant_id, &ctx.domain, &ctx.entity_table)
            .await?;

        // Load or create instance
        let (instance_id, current_state, is_terminal) = self
            .load_or_create_instance(&tx, ctx.tenant_id, def_id, ctx.entity_id, &initial_state)
            .await?;

        // 2. Check terminal
        if is_terminal {
            return Err(WorkflowError::AlreadyTerminal);
        }

        // 3. Validate transition
        if !sm.is_valid_transition(&current_state, &ctx.target_state) {
            return Err(WorkflowError::InvalidTransition {
                current: current_state,
                target: ctx.target_state,
            });
        }

        // 4. Evaluate data guards (skipped for timer-triggered transitions)
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

        // 5. Dual-status guards
        if let Some(required_approval) = sm.required_approval_for(&ctx.target_state) {
            let approval_state: Option<String> = tx
                .query_one(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT approval_state FROM platform.workflow_instance WHERE id = $1",
                    [instance_id.into()],
                ))
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?
                .and_then(|r| {
                    r.try_get::<Option<String>>("", "approval_state")
                        .ok()
                        .flatten()
                });

            if approval_state.as_deref() != Some(required_approval) {
                return Err(WorkflowError::DualStatusGuardFailed {
                    status: ctx.target_state.clone(),
                    required_approval: required_approval.to_string(),
                });
            }
        }

        // 6. Idempotency check
        if let Some(key) = ctx.idempotency_key {
            let exists = tx
                .query_one(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT id FROM platform.workflow_transition WHERE idempotency_key = $1",
                    [key.into()],
                ))
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            if exists.is_some() {
                return Err(WorkflowError::IdempotencyConflict { key });
            }
        }

        // 7. Check approval chains
        if sm.has_approval_chain(&current_state, &ctx.target_state) {
            let pending_state = format!("pending_approval:{}->{}", current_state, ctx.target_state);
            tx.execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"UPDATE platform.workflow_instance
                   SET current_state = $1, updated_at = now()
                   WHERE id = $2 AND current_state = $3"#,
                [
                    pending_state.into(),
                    instance_id.into(),
                    current_state.clone().into(),
                ],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

            let pending = crate::approval::get_pending_step(
                &tx,
                def_id,
                instance_id,
                &current_state,
                &ctx.target_state,
            )
            .await?
            .ok_or_else(|| WorkflowError::Internal("no approval steps found".into()))?;

            tx.commit()
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            return Err(WorkflowError::ApprovalRequired {
                pending_step: pending,
            });
        }

        // 8. Set correlation_id via set_config (supports parameterized values)
        tx.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT set_config('app.correlation_id', $1, true)",
            [ctx.correlation_id.to_string().into()],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        // 9. Update instance (optimistic lock)
        let new_is_terminal = terminal_states.contains(&ctx.target_state);
        let updated = tx
            .execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"UPDATE platform.workflow_instance
                   SET current_state = $1, is_terminal = $2, updated_at = now()
                   WHERE id = $3 AND current_state = $4"#,
                [
                    ctx.target_state.clone().into(),
                    new_is_terminal.into(),
                    instance_id.into(),
                    current_state.clone().into(),
                ],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        if updated.rows_affected() == 0 {
            return Err(WorkflowError::ConcurrentModification);
        }

        // 10. Record transition
        tx.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"INSERT INTO platform.workflow_transition
               (tenant_id, instance_id, from_state, to_state, correlation_id, actor_id, comment, idempotency_key)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            [
                ctx.tenant_id.into(),
                instance_id.into(),
                current_state.clone().into(),
                ctx.target_state.clone().into(),
                ctx.correlation_id.into(),
                ctx.actor_id
                    .map(sea_orm::Value::from)
                    .unwrap_or(sea_orm::Value::from(None::<Uuid>)),
                ctx.comment
                    .clone()
                    .map(sea_orm::Value::from)
                    .unwrap_or(sea_orm::Value::from(None::<String>)),
                ctx.idempotency_key
                    .map(sea_orm::Value::from)
                    .unwrap_or(sea_orm::Value::from(None::<Uuid>)),
            ],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        // 11. Cancel old timers
        tx.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"UPDATE platform.workflow_timer SET is_fired = true
               WHERE instance_id = $1 AND NOT is_fired"#,
            [instance_id.into()],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        // 12. Schedule new timers
        for timer in sm.timers_for_state(&ctx.target_state) {
            let fires_at = chrono::Utc::now() + chrono::Duration::hours(timer.duration_hours);
            tx.execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"INSERT INTO platform.workflow_timer
                   (tenant_id, instance_id, timer_type, fires_at, target_state)
                   VALUES ($1, $2, $3, $4, $5)"#,
                [
                    ctx.tenant_id.into(),
                    instance_id.into(),
                    timer.timer_type.clone().into(),
                    fires_at.into(),
                    timer
                        .target_state
                        .clone()
                        .map(sea_orm::Value::from)
                        .unwrap_or(sea_orm::Value::from(None::<String>)),
                ],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
        }

        tx.commit()
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        // 13. Return state
        let available = sm.transitions_from(&ctx.target_state);
        Ok(WorkflowState {
            entity_id: ctx.entity_id,
            current_state: ctx.target_state,
            approval_state: None,
            is_terminal: new_is_terminal,
            available_transitions: if new_is_terminal { vec![] } else { available },
            pending_approvals: vec![],
        })
    }

    async fn approval_action(&self, ctx: ApprovalContext) -> Result<WorkflowState, WorkflowError> {
        let tx = self
            .db
            .begin()
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        // Set RLS session variable so tenant isolation policies allow access.
        Self::set_rls_org(&tx, ctx.tenant_id).await?;

        let (def_id, _initial, terminal_states, sm) = self
            .load_definition(&tx, ctx.tenant_id, &ctx.domain, &ctx.entity_table)
            .await?;

        // Load instance
        let row = tx
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"SELECT id, current_state
                   FROM platform.workflow_instance
                   WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
                [ctx.tenant_id.into(), def_id.into(), ctx.entity_id.into()],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?
            .ok_or(WorkflowError::NotFound)?;

        let instance_id: Uuid = row
            .try_get("", "id")
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
        let current_state: String = row
            .try_get("", "current_state")
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        // Parse pending_approval:{from}->{to} state
        let (from, to) = current_state
            .strip_prefix("pending_approval:")
            .and_then(|s| s.split_once("->"))
            .map(|(f, t)| (f.to_string(), t.to_string()))
            .ok_or(WorkflowError::NoPendingApproval)?;

        // Find the pending step
        let pending = crate::approval::get_pending_step(&tx, def_id, instance_id, &from, &to)
            .await?
            .ok_or(WorkflowError::NoPendingApproval)?;

        // Record the decision
        let decision_str = match ctx.decision {
            ApprovalDecision::Approved => "approved",
            ApprovalDecision::Rejected => "rejected",
        };

        tx.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"INSERT INTO platform.approval_decision
               (tenant_id, step_id, instance_id, actor_id, decision, correlation_id, comment)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            [
                ctx.tenant_id.into(),
                pending.step_id.into(),
                instance_id.into(),
                ctx.actor_id.into(),
                decision_str.into(),
                ctx.correlation_id.into(),
                ctx.comment
                    .clone()
                    .map(sea_orm::Value::from)
                    .unwrap_or(sea_orm::Value::from(None::<String>)),
            ],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        if ctx.decision == ApprovalDecision::Rejected {
            // Roll back to the original {from} state
            tx.execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"UPDATE platform.workflow_instance
                   SET current_state = $1, updated_at = now()
                   WHERE id = $2"#,
                [from.clone().into(), instance_id.into()],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

            tx.commit()
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

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

        // Approved — check if chain is complete
        if crate::approval::is_chain_complete(&tx, def_id, instance_id, &from, &to).await? {
            // Chain complete — execute the original transition
            let new_is_terminal = terminal_states.contains(&to);
            tx.execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"UPDATE platform.workflow_instance
                   SET current_state = $1, is_terminal = $2, updated_at = now()
                   WHERE id = $3"#,
                [
                    to.clone().into(),
                    new_is_terminal.into(),
                    instance_id.into(),
                ],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

            // Record transition
            tx.execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"INSERT INTO platform.workflow_transition
                   (tenant_id, instance_id, from_state, to_state, correlation_id, actor_id, comment, idempotency_key)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                [
                    ctx.tenant_id.into(),
                    instance_id.into(),
                    from.clone().into(),
                    to.clone().into(),
                    ctx.correlation_id.into(),
                    ctx.actor_id.into(),
                    ctx.comment
                        .map(sea_orm::Value::from)
                        .unwrap_or(sea_orm::Value::from(None::<String>)),
                    ctx.idempotency_key
                        .map(sea_orm::Value::from)
                        .unwrap_or(sea_orm::Value::from(None::<Uuid>)),
                ],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

            tx.commit()
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

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
            // More steps needed — stay in pending_approval state
            let next_pending =
                crate::approval::get_pending_step(&tx, def_id, instance_id, &from, &to).await?;
            tx.commit()
                .await
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

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

    async fn get_state(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<WorkflowState, WorkflowError> {
        // A transaction is required even for read-only operations because
        // `set_config('app.organization_id', $1, true)` uses `true` (is_local=true),
        // which scopes the config to the current transaction.  Without an explicit
        // transaction the session variable would not be visible to subsequent queries
        // on the same connection, breaking RLS enforcement.
        let tx = self
            .db
            .begin()
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        Self::set_rls_org(&tx, tenant_id).await?;

        let (def_id, initial_state, _terminal_states, sm) = self
            .load_definition(&tx, tenant_id, domain, entity_table)
            .await?;

        let row = tx
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"SELECT current_state, approval_state, is_terminal
                   FROM platform.workflow_instance
                   WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
                [tenant_id.into(), def_id.into(), entity_id.into()],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        let (current_state, approval_state, is_terminal) = match row {
            Some(r) => {
                let cs: String = r
                    .try_get("", "current_state")
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
                let aps: Option<String> = r.try_get("", "approval_state").ok().flatten();
                let it: bool = r
                    .try_get("", "is_terminal")
                    .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
                (cs, aps, it)
            }
            None => (initial_state, None, false),
        };

        let available = if is_terminal {
            vec![]
        } else {
            sm.transitions_from(&current_state)
        };

        tx.commit()
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        Ok(WorkflowState {
            entity_id,
            current_state,
            approval_state,
            is_terminal,
            available_transitions: available,
            pending_approvals: vec![], // TODO: load from approval_decision
        })
    }

    async fn delegate(&self, ctx: DelegationContext) -> Result<(), WorkflowError> {
        crate::delegation::execute_delegation(&self.db, &ctx).await
    }

    async fn get_history(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<Vec<ProcessHistoryEntry>, WorkflowError> {
        // Transaction required: `set_config(..., true)` (is_local=true) only
        // takes effect within an explicit transaction — see get_state for details.
        let tx = self
            .db
            .begin()
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        Self::set_rls_org(&tx, tenant_id).await?;

        let rows = tx
            .query_all(Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Postgres,
                r#"SELECT wt.id, wt.occurred_at, wt.to_state, wt.from_state,
                          wt.correlation_id, wt.actor_id, wt.comment
                   FROM platform.workflow_transition wt
                   JOIN platform.workflow_instance wi ON wt.instance_id = wi.id
                   JOIN platform.workflow_definition wd ON wi.definition_id = wd.id
                   WHERE wi.entity_id = $1 AND wi.tenant_id = $2
                     AND wd.domain = $3 AND wd.entity_table = $4
                   ORDER BY wt.occurred_at ASC"#,
                [
                    entity_id.into(),
                    tenant_id.into(),
                    domain.into(),
                    entity_table.into(),
                ],
            ))
            .await
            .map_err(|e| WorkflowError::Internal(e.to_string().into()))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(ProcessHistoryEntry {
                id: row
                    .try_get_by_index(0)
                    .map_err(|e| WorkflowError::Internal(e.to_string().into()))?,
                action_date: row
                    .try_get_by_index(1)
                    .map_err(|e| WorkflowError::Internal(e.to_string().into()))?,
                status: row
                    .try_get_by_index(2)
                    .map_err(|e| WorkflowError::Internal(e.to_string().into()))?,
                previous_status: row
                    .try_get_by_index::<Option<String>>(3)
                    .map_err(|e| WorkflowError::Internal(e.to_string().into()))?,
                correlation_id: row.try_get_by_index::<Option<Uuid>>(4).ok().flatten(),
                actor_id: row.try_get_by_index::<Option<Uuid>>(5).ok().flatten(),
                comment: row
                    .try_get_by_index::<Option<String>>(6)
                    .map_err(|e| WorkflowError::Internal(e.to_string().into()))?,
            });
        }

        tx.commit()
            .await
            .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

        Ok(entries)
    }
}
