//! Approval chain orchestration (client-generic).

use uuid::Uuid;

use crate::error::WorkflowError;
use crate::tx::{WfParam, WorkflowTx};
use crate::types::PendingApproval;

/// Get the next pending approval step for a workflow instance.
pub async fn get_pending_step(
    conn: &dyn WorkflowTx,
    definition_id: Uuid,
    instance_id: Uuid,
    from: &str,
    to: &str,
) -> Result<Option<PendingApproval>, WorkflowError> {
    let row = conn
        .query_one(
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
            &[
                WfParam::Uuid(definition_id),
                WfParam::Str(from.to_string()),
                WfParam::Str(to.to_string()),
                WfParam::Uuid(instance_id),
            ],
        )
        .await?;

    match row {
        Some(r) => {
            let step_id = r.get_uuid("id")?;
            let step_order = r.get_i32("step_order")?;
            let role = r.get_string("role")?;
            let is_required = r.get_bool("is_required")?;
            let timeout_hours = r.get_opt_i32("timeout_hours")?;
            Ok(Some(PendingApproval {
                step_id,
                step_order,
                role,
                is_required,
                timeout_hours,
                deadline: None,
            }))
        }
        None => Ok(None),
    }
}

/// Check if all required approval steps are complete.
pub async fn is_chain_complete(
    conn: &dyn WorkflowTx,
    definition_id: Uuid,
    instance_id: Uuid,
    from: &str,
    to: &str,
) -> Result<bool, WorkflowError> {
    let pending = get_pending_step(conn, definition_id, instance_id, from, to).await?;
    match pending {
        None => Ok(true),
        Some(step) => Ok(!step.is_required),
    }
}
