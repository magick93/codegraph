//! Delegation operations.

use sea_orm::*;
use uuid::Uuid;

use crate::error::WorkflowError;
use crate::types::DelegationContext;

/// Execute a delegation: record a 'delegated' decision from one actor to another.
pub async fn execute_delegation(
    db: &DatabaseConnection,
    ctx: &DelegationContext,
) -> Result<(), WorkflowError> {
    // Find the pending approval step for this entity
    let step = db
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
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
            [
                ctx.entity_id.into(),
                ctx.domain.clone().into(),
                ctx.entity_table.clone().into(),
                ctx.tenant_id.into(),
            ],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?
        .ok_or(WorkflowError::NoPendingApproval)?;

    let step_id: Uuid = step
        .try_get("", "step_id")
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
    let instance_id: Uuid = step
        .try_get("", "instance_id")
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

    // Record delegation decision
    db.execute(Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"INSERT INTO platform.approval_decision
           (tenant_id, instance_id, step_id, actor_id, delegated_from, decision, correlation_id, comment)
           VALUES ($1, $2, $3, $4, $5, 'delegated', $6, $7)"#,
        [
            ctx.tenant_id.into(),
            instance_id.into(),
            step_id.into(),
            ctx.to_actor_id.into(),
            ctx.from_actor_id.into(),
            ctx.correlation_id.into(),
            ctx.reason
                .clone()
                .map(sea_orm::Value::from)
                .unwrap_or(sea_orm::Value::from(None::<String>)),
        ],
    ))
    .await
    .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

    Ok(())
}
