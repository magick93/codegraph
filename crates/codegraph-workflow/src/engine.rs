//! Client-generic workflow engine.
//!
//! The state machine (transition / approval / state / history) is expressed as
//! free functions over [`WorkflowTx`]; the [`GenericWorkflowService`] in
//! [`crate::tx`] opens a transaction and dispatches. The native SeaORM
//! backend (and its [`SeaOrmWorkflowService`] alias) lives here too, so the
//! monolith keeps its existing constructor while the worker slice supplies its
//! own [`WorkflowClient`] from generated code.

use uuid::Uuid;

use crate::definition::WorkflowDefinition;
use crate::error::WorkflowError;
use crate::guard::GuardEvaluator;
use crate::machine_runtime::{GuardDecision, MachineError, MachineGuards, MachineRuntime};
use crate::tx::{WfParam, WorkflowTx};
use crate::types::*;

/// Quote an identifier for interpolation into the write-both statement
/// (the double-quote doubling — identifiers come from the definition row,
/// and the quoting is defense-in-depth).
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

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

/// A loaded workflow definition: the kernel runtime (built + drift-verified
/// from the row's machine JSON), the engine-level layers, and the
/// write-both facts.
pub struct LoadedDefinition {
    pub def_id: Uuid,
    pub initial_state: String,
    pub terminal_states: Vec<String>,
    /// The kernel: the ONE transition decision-maker (issue #816).
    pub runtime: MachineRuntime,
    /// The engine-level layers (data guards, timers, dual-status guards,
    /// approval chains) — their evaluators register as named kernel guards;
    /// their mechanics stay engine-level.
    pub definition: WorkflowDefinition,
    /// The workflow-managed column (config `status_field`) — the write-both
    /// target on the entity's own table.
    pub status_field: String,
}

/// Load workflow definition by (tenant, domain, entity_table), building the
/// kernel runtime and verifying the drift hash (issue #816): a row whose
/// recorded `machine_hash` does not match its machine JSON's current hash is
/// a NAMED hard error — never a silent re-decision on a changed machine.
/// Rows carrying the pre-kernel shape fail closed pointing at the upgrade
/// migration.
async fn load_definition(
    tx: &dyn WorkflowTx,
    tenant_id: Uuid,
    domain: &str,
    entity_table: &str,
) -> Result<LoadedDefinition, WorkflowError> {
    let row = tx
        .query_one(
            r#"SELECT id, initial_state, terminal_states, state_machine, status_field
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
    let status_field = row.get_string("status_field")?;
    let sm_json = row.get_json("state_machine")?;
    let envelope = WorkflowDefinition::from_json(&sm_json)
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

    let Some(machine_json) = &envelope.machine else {
        return Err(WorkflowError::DefinitionNotMachineShaped {
            domain: domain.to_string(),
            entity_table: entity_table.to_string(),
        });
    };
    let runtime = MachineRuntime::from_json(machine_json).map_err(map_machine_error)?;

    // Drift discipline: recompute FNV-1a over the canonical machine JSON
    // and compare to the recorded hash. Absent hash (a hand-authored row
    // predating the discipline) cannot be verified — allowed, recorded.
    if let Some(stored) = &envelope.machine_hash {
        let canonical = crate::machine_runtime::canonical_json(machine_json);
        let computed = crate::machine_runtime::fnv1a_64_hex(canonical.as_bytes());
        if computed != *stored {
            return Err(WorkflowError::MachineDrift {
                machine_id: runtime.machine_id().to_string(),
                stored: stored.clone(),
                computed,
            });
        }
    }

    Ok(LoadedDefinition {
        def_id,
        initial_state,
        terminal_states,
        runtime,
        definition: envelope,
        status_field,
    })
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

/// The engine's named-guard source: data guards and dual-status guards
/// register as NAMED kernel guards whose bodies are the existing
/// evaluators (issue #816). Evaluation order is preserved inside the
/// composed body: data guards first, then the dual-status check — and the
/// KERNEL decision (validity + guards) runs before the engine-level
/// approval chain. Timer-triggered transitions skip the data-guard half
/// (no entity data available), exactly as before.
struct EngineGuards<'a> {
    definition: &'a WorkflowDefinition,
    entity_data: &'a serde_json::Value,
    /// The instance's pre-loaded approval state (loaded only when the
    /// target carries a dual-status entry, preserving today's query count).
    approval_state: Option<&'a str>,
    timer_source: bool,
    /// The app's guard seam (artifact-machine guards implemented in the
    /// generated machines module). `None` ⇒ every foreign guard name is
    /// NotImplemented (the fail-closed 501).
    app_guards: Option<&'a dyn MachineGuards>,
}

impl EngineGuards<'_> {
    /// The data-guard half: every data guard targeting `to`, in
    /// declaration order. Skipped for timer sources.
    fn evaluate_data_guards(&self, to: &str) -> GuardDecision {
        if self.timer_source {
            return GuardDecision::Allow;
        }
        for guard in self.definition.data_guards_for(to) {
            match GuardEvaluator::evaluate(&guard.rule, self.entity_data) {
                Ok(true) => {}
                Ok(false) => return GuardDecision::Deny(guard.message.clone()),
                Err(e) => return GuardDecision::Failed(e.to_string()),
            }
        }
        GuardDecision::Allow
    }

    /// The dual-status half: the target status requires the instance's
    /// pre-loaded approval state to match.
    fn evaluate_dual_status(&self, status: &str) -> GuardDecision {
        match self.definition.required_approval_for(status) {
            Some(required) => {
                if self.approval_state == Some(required) {
                    GuardDecision::Allow
                } else {
                    GuardDecision::Deny(format!(
                        "status '{status}' requires approval state '{required}'"
                    ))
                }
            }
            None => GuardDecision::NotImplemented,
        }
    }
}

impl MachineGuards for EngineGuards<'_> {
    fn evaluate(
        &self,
        name: &str,
        context: &serde_json::Value,
        event: &serde_json::Value,
        params: &serde_json::Value,
    ) -> GuardDecision {
        // Engine-level composed guard (data + dual on one target): data
        // first, then dual — the historical order.
        if let Some(to) = name.strip_prefix("guards:") {
            let data = self.evaluate_data_guards(to);
            if let GuardDecision::Allow = data {
                return self.evaluate_dual_status(to);
            }
            return data;
        }
        if let Some(to) = name.strip_prefix("data_guard:") {
            return self.evaluate_data_guards(to);
        }
        if let Some(status) = name.strip_prefix("dual_status:") {
            return self.evaluate_dual_status(status);
        }
        // Not an engine guard: the app's seam answers (fail-closed when
        // absent).
        match self.app_guards {
            Some(app) => app.evaluate(name, context, event, params),
            None => GuardDecision::NotImplemented,
        }
    }
}

/// Map a kernel denial back onto the engine's wire error variants — the
/// 409/422 semantics the generated handlers already map are preserved.
fn guard_denial_to_workflow_error(
    definition: &WorkflowDefinition,
    denial: MachineError,
) -> WorkflowError {
    if let MachineError::GuardDenied { guard, message, .. } = &denial {
        if let Some(status) = guard
            .strip_prefix("guards:")
            .or_else(|| guard.strip_prefix("dual_status:"))
        {
            return WorkflowError::DualStatusGuardFailed {
                status: status.to_string(),
                required_approval: definition
                    .required_approval_for(status)
                    .unwrap_or_default()
                    .to_string(),
            };
        }
        if let Some(to) = guard.strip_prefix("data_guard:") {
            let rule = definition
                .data_guards_for(to)
                .first()
                .map(|g| g.rule.clone())
                .unwrap_or_else(|| guard.clone());
            return WorkflowError::GuardFailed {
                rule,
                message: message.clone(),
            };
        }
        return WorkflowError::GuardFailed {
            rule: guard.clone(),
            message: message.clone(),
        };
    }
    map_machine_error(denial)
}

/// Map every other kernel error onto the engine error surface.
fn map_machine_error(err: MachineError) -> WorkflowError {
    match err {
        MachineError::InvalidTransition { current, target } => {
            WorkflowError::InvalidTransition { current, target }
        }
        MachineError::UnimplementedGuard(name) => WorkflowError::UnimplementedGuard(name),
        other => WorkflowError::Internal(Box::new(other)),
    }
}

/// Write-both (issue #816 Decision 3): move the entity's workflow-managed
/// status column inside the transition's transaction. The statement
/// targets the entity's domain-qualified table and the definition row's
/// status column — the same identifiers the seed wrote — and zero rows
/// updated is a NAMED error (the row vanished mid-transition; the whole
/// transaction rolls back).
async fn write_entity_status(
    tx: &dyn WorkflowTx,
    domain: &str,
    entity_table: &str,
    status_field: &str,
    entity_id: Uuid,
    new_state: &str,
) -> Result<(), WorkflowError> {
    let sql = format!(
        "UPDATE {}.{} SET {} = $1 WHERE id = $2",
        quote_ident(domain),
        quote_ident(entity_table),
        quote_ident(status_field),
    );
    let updated = tx
        .execute(
            &sql,
            &[
                WfParam::Str(new_state.to_string()),
                WfParam::Uuid(entity_id),
            ],
        )
        .await?;
    if updated == 0 {
        return Err(WorkflowError::WriteBothFailed(format!(
            "the status column of {domain}.{entity_table} did not update (the entity row \
             vanished mid-transition); the whole transition rolled back"
        )));
    }
    Ok(())
}

/// Execute a state transition. The caller (a `WorkflowService` impl) owns the
/// transaction; this function commits only where the transition is durable
/// (including the approval-required branch, which persists the pending state).
pub async fn transition(
    tx: &mut dyn WorkflowTx,
    ctx: &TransitionContext,
) -> Result<WorkflowState, WorkflowError> {
    transition_with_guards(tx, ctx, None).await
}

/// [`transition`] with an app-level guard source (issue #816): machine
/// transitions that name a foreign guard consult it; unregistered names
/// still fail closed (the named 501).
pub async fn transition_with_guards(
    tx: &mut dyn WorkflowTx,
    ctx: &TransitionContext,
    app_guards: Option<&dyn MachineGuards>,
) -> Result<WorkflowState, WorkflowError> {
    set_rls_org(
        tx,
        ctx.tenant_id,
        ctx.session_user_id,
        ctx.session_api_key_id,
    )
    .await?;

    let def = load_definition(tx, ctx.tenant_id, &ctx.domain, &ctx.entity_table).await?;

    let (instance_id, current_state, is_terminal) = load_or_create_instance(
        tx,
        ctx.tenant_id,
        def.def_id,
        ctx.entity_id,
        &def.initial_state,
    )
    .await?;

    if is_terminal {
        return Err(WorkflowError::AlreadyTerminal);
    }

    // Pre-load the approval state when the TARGET carries a dual-status
    // entry — the same query the engine has always run, hoisted before the
    // kernel decision so the dual-status guard body is synchronous.
    let approval_state = if def
        .definition
        .required_approval_for(&ctx.target_state)
        .is_some()
    {
        tx.query_one(
            "SELECT approval_state FROM platform.workflow_instance WHERE id = $1",
            &[WfParam::Uuid(instance_id)],
        )
        .await?
        .and_then(|r| r.get_opt_string("approval_state").ok().flatten())
    } else {
        None
    };

    // THE kernel decision (issue #816): the restingstate interpreter
    // validates the edge AND evaluates the named guards (data guards →
    // dual-status, composed in the guard body). No engine-side transition
    // map exists anymore.
    let guards = EngineGuards {
        definition: &def.definition,
        entity_data: &ctx.entity_data,
        approval_state: approval_state.as_deref(),
        timer_source: ctx.trigger_source == TriggerSource::Timer,
        app_guards,
    };
    let outcome = def
        .runtime
        .decide(&current_state, &ctx.target_state, &ctx.entity_data, &guards)
        .map_err(|e| guard_denial_to_workflow_error(&def.definition, e))?;

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

    if def
        .definition
        .has_approval_chain(&current_state, &ctx.target_state)
    {
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
            def.def_id,
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

    let new_is_terminal = def.terminal_states.contains(&ctx.target_state);
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

    // WRITE-BOTH (issue #816 Decision 3): the entity's status column moves
    // in THIS transaction — failure rolls the workflow record back with it.
    write_entity_status(
        tx,
        &ctx.domain,
        &ctx.entity_table,
        &def.status_field,
        ctx.entity_id,
        &ctx.target_state,
    )
    .await?;

    tx.execute(
        "UPDATE platform.workflow_timer SET is_fired = true \
         WHERE instance_id = $1 AND NOT is_fired",
        &[WfParam::Uuid(instance_id)],
    )
    .await?;

    for timer in def.definition.timers_for_state(&ctx.target_state) {
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

    let available = if new_is_terminal {
        vec![]
    } else {
        def.runtime.available_transitions(&ctx.target_state)
    };
    let _ = outcome;
    Ok(WorkflowState {
        entity_id: ctx.entity_id,
        current_state: ctx.target_state.clone(),
        approval_state: None,
        is_terminal: new_is_terminal,
        available_transitions: available,
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

    let def = load_definition(tx, ctx.tenant_id, &ctx.domain, &ctx.entity_table).await?;

    let row = tx
        .query_one(
            r#"SELECT id, current_state
               FROM platform.workflow_instance
               WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
            &[
                WfParam::Uuid(ctx.tenant_id),
                WfParam::Uuid(def.def_id),
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

    let pending = crate::approval::get_pending_step(tx, def.def_id, instance_id, &from, &to)
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

        // WRITE-BOTH: the rejection moves the instance back to `from` —
        // the entity column follows in the same transaction (issue #816).
        write_entity_status(
            tx,
            &ctx.domain,
            &ctx.entity_table,
            &def.status_field,
            ctx.entity_id,
            &from,
        )
        .await?;

        tx.commit().await?;

        let available = def.runtime.available_transitions(&from);
        return Ok(WorkflowState {
            entity_id: ctx.entity_id,
            current_state: from,
            approval_state: None,
            is_terminal: false,
            available_transitions: available,
            pending_approvals: vec![],
        });
    }

    if crate::approval::is_chain_complete(tx, def.def_id, instance_id, &from, &to).await? {
        let new_is_terminal = def.terminal_states.contains(&to);
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

        // WRITE-BOTH: the approval-completed transition moves the entity
        // column in the same transaction (issue #816).
        write_entity_status(
            tx,
            &ctx.domain,
            &ctx.entity_table,
            &def.status_field,
            ctx.entity_id,
            &to,
        )
        .await?;

        tx.commit().await?;

        let available = if new_is_terminal {
            vec![]
        } else {
            def.runtime.available_transitions(&to)
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
            crate::approval::get_pending_step(tx, def.def_id, instance_id, &from, &to).await?;
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

    let def = load_definition(tx, tenant_id, domain, entity_table).await?;

    let row = tx
        .query_one(
            r#"SELECT current_state, approval_state, is_terminal
               FROM platform.workflow_instance
               WHERE tenant_id = $1 AND definition_id = $2 AND entity_id = $3"#,
            &[
                WfParam::Uuid(tenant_id),
                WfParam::Uuid(def.def_id),
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
        None => (def.initial_state.clone(), None, false),
    };

    let available = if is_terminal {
        vec![]
    } else {
        def.runtime.available_transitions(&current_state)
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
                .execute_raw(sea_orm::Statement::from_sql_and_values(
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
                .query_one_raw(sea_orm::Statement::from_sql_and_values(
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
                .query_all_raw(sea_orm::Statement::from_sql_and_values(
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

    /// Registers the app's machine-guard implementations (issue #816) —
    /// the generated machines module's `DefaultMachineGuards`.
    pub fn with_guards(
        mut self,
        guards: std::sync::Arc<dyn crate::machine_runtime::MachineGuards>,
    ) -> Self {
        self.0 = self.0.with_guards(guards);
        self
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
