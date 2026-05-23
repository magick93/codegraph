//! Approval chain orchestration.

use sea_orm::*;
use uuid::Uuid;

use crate::error::WorkflowError;
use crate::types::PendingApproval;

/// Get the next pending approval step for a workflow instance.
pub async fn get_pending_step(
    conn: &impl ConnectionTrait,
    definition_id: Uuid,
    instance_id: Uuid,
    from: &str,
    to: &str,
) -> Result<Option<PendingApproval>, WorkflowError> {
    // Find the first step that doesn't have an 'approved' decision
    let row = conn
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"SELECT s.id, s.step_order, s.role, s.is_required, s.timeout_hours
               FROM platform.approval_step s
               WHERE s.definition_id = $1
                 AND s.transition_from = $2
                 AND s.transition_to = $3
                 AND NOT EXISTS (
                     SELECT 1 FROM platform.approval_decision d
                     WHERE d.step_id = s.id
                       AND d.instance_id = $4
                       AND d.decision = 'approved'
                 )
               ORDER BY s.step_order ASC
               LIMIT 1"#,
            [
                definition_id.into(),
                from.into(),
                to.into(),
                instance_id.into(),
            ],
        ))
        .await
        .map_err(|e| WorkflowError::Internal(Box::new(e)))?;

    match row {
        Some(r) => {
            let step_id: Uuid = r
                .try_get("", "id")
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            let step_order: i32 = r
                .try_get("", "step_order")
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            let role: String = r
                .try_get("", "role")
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            let is_required: bool = r
                .try_get("", "is_required")
                .map_err(|e| WorkflowError::Internal(Box::new(e)))?;
            let timeout_hours: Option<i32> = r.try_get("", "timeout_hours").ok().flatten();
            Ok(Some(PendingApproval {
                step_id,
                step_order,
                role,
                is_required,
                timeout_hours,
                deadline: None,
            }))
        }
        None => Ok(None), // All steps approved
    }
}

/// Check if all required approval steps are complete.
pub async fn is_chain_complete(
    conn: &impl ConnectionTrait,
    definition_id: Uuid,
    instance_id: Uuid,
    from: &str,
    to: &str,
) -> Result<bool, WorkflowError> {
    let pending = get_pending_step(conn, definition_id, instance_id, from, to).await?;
    match pending {
        None => Ok(true),
        Some(step) => Ok(!step.is_required), // If next pending step is optional, chain is functionally complete
    }
}
