//! Delegation operations (client-generic).

use crate::error::WorkflowError;
use crate::tx::{WfParam, WorkflowTx};
use crate::types::DelegationContext;

/// Execute a delegation: record a 'delegated' decision from one actor to
/// another. Runs inside the caller's transaction, with RLS session variables
/// set so the per-request worker connection can reach `platform.*` rows.
pub async fn execute_delegation(
    tx: &mut dyn WorkflowTx,
    ctx: &DelegationContext,
) -> Result<(), WorkflowError> {
    crate::engine::set_rls_org(
        tx,
        ctx.tenant_id,
        ctx.session_user_id,
        ctx.session_api_key_id,
    )
    .await?;

    let step = tx
        .query_one(
            r#"SELECT s.id as step_id, wi.id as instance_id
               FROM platform.approval_step s
               JOIN platform.workflow_definition wd ON s.definition_id = wd.id
               JOIN platform.workflow_instance wi ON wi.definition_id = wd.id AND wi.entity_id = $1
               WHERE wd.domain = $2 AND wd.entity_table = $3 AND wi.tenant_id = $4
                 AND wi.current_state LIKE 'pending_approval:%'
                 AND NOT EXISTS (
                     SELECT 1 FROM platform.approval_decision d
                     WHERE d.step_id = s.id AND d.instance_id = wi.id
                       AND d.decision IN ('approved', 'rejected')
                 )
               ORDER BY s.step_order ASC
               LIMIT 1"#,
            &[
                WfParam::Uuid(ctx.entity_id),
                WfParam::Str(ctx.domain.clone()),
                WfParam::Str(ctx.entity_table.clone()),
                WfParam::Uuid(ctx.tenant_id),
            ],
        )
        .await?
        .ok_or(WorkflowError::NoPendingApproval)?;

    let step_id = step.get_uuid("step_id")?;
    let instance_id = step.get_uuid("instance_id")?;

    tx.execute(
        r#"INSERT INTO platform.approval_decision
           (tenant_id, instance_id, step_id, actor_id, delegated_from, decision, correlation_id, comment)
           VALUES ($1, $2, $3, $4, $5, 'delegated', $6, $7)"#,
        &[
            WfParam::Uuid(ctx.tenant_id),
            WfParam::Uuid(instance_id),
            WfParam::Uuid(step_id),
            WfParam::Uuid(ctx.to_actor_id),
            WfParam::Uuid(ctx.from_actor_id),
            WfParam::Uuid(ctx.correlation_id),
            ctx.reason
                .clone()
                .map(WfParam::Str)
                .unwrap_or(WfParam::Null("text")),
        ],
    )
    .await?;

    tx.commit().await?;

    Ok(())
}
