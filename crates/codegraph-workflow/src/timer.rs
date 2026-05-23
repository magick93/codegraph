//! Background timer service for deadline/reminder/approval_timeout.

use std::sync::Arc;
use std::time::Duration;

use sea_orm::*;
use uuid::Uuid;

use crate::service::WorkflowService;
use crate::types::{ApprovalContext, ApprovalDecision, TransitionContext, TriggerSource};

pub struct TimerService {
    db: DatabaseConnection,
    workflow_service: Arc<dyn WorkflowService>,
    poll_interval: Duration,
}

impl TimerService {
    pub fn new(
        db: DatabaseConnection,
        workflow_service: Arc<dyn WorkflowService>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            db,
            workflow_service,
            poll_interval,
        }
    }

    pub async fn run(&self, mut shutdown: tokio::sync::watch::Receiver<()>) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.poll_interval) => {
                    if let Err(e) = self.process_pending_timers().await {
                        tracing::error!("timer service error: {e}");
                    }
                }
                _ = shutdown.changed() => {
                    tracing::info!("timer service shutting down");
                    return;
                }
            }
        }
    }

    async fn process_pending_timers(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"SELECT t.id, t.tenant_id, t.instance_id, t.timer_type, t.target_state,
                          wi.entity_id, wd.domain, wd.entity_table
                   FROM platform.workflow_timer t
                   JOIN platform.workflow_instance wi ON t.instance_id = wi.id
                   JOIN platform.workflow_definition wd ON wi.definition_id = wd.id
                   WHERE t.fires_at <= now() AND NOT t.is_fired
                   ORDER BY t.fires_at ASC
                   LIMIT 100"#,
                [],
            ))
            .await?;

        for row in rows {
            let timer_id: Uuid = row.try_get("", "id")?;
            let timer_type: String = row.try_get("", "timer_type")?;
            let target_state: Option<String> = row.try_get("", "target_state").ok().flatten();
            let tenant_id: Uuid = row.try_get("", "tenant_id")?;
            let entity_id: Uuid = row.try_get("", "entity_id")?;
            let domain: String = row.try_get("", "domain")?;
            let entity_table: String = row.try_get("", "entity_table")?;

            // Mark as fired
            self.db
                .execute(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "UPDATE platform.workflow_timer SET is_fired = true WHERE id = $1",
                    [timer_id.into()],
                ))
                .await?;

            match timer_type.as_str() {
                "deadline" => {
                    if let Some(target) = target_state {
                        let ctx = TransitionContext {
                            tenant_id,
                            entity_id,
                            domain: domain.clone(),
                            entity_table: entity_table.clone(),
                            target_state: target,
                            actor_id: None,
                            correlation_id: Uuid::new_v4(),
                            idempotency_key: None,
                            comment: Some("auto-transition by timer".to_string()),
                            entity_data: serde_json::Value::Null,
                            trigger_source: TriggerSource::Timer,
                        };
                        if let Err(e) = self.workflow_service.transition(ctx).await {
                            tracing::warn!("timer transition failed for entity {entity_id}: {e}");
                        }
                    }
                }
                "reminder" => {
                    tracing::info!(
                        "reminder fired for entity {entity_id} in {domain}.{entity_table}"
                    );
                }
                "approval_timeout" => {
                    let state = self
                        .workflow_service
                        .get_state(tenant_id, &domain, &entity_table, entity_id)
                        .await;
                    match state {
                        Ok(ws) if ws.current_state.starts_with("pending_approval:") => {
                            for pending in &ws.pending_approvals {
                                let decision = if pending.is_required {
                                    ApprovalDecision::Rejected
                                } else {
                                    ApprovalDecision::Approved
                                };
                                let comment = if pending.is_required {
                                    "auto-rejected: approval timeout"
                                } else {
                                    "auto-approved: optional step timeout"
                                };
                                let ctx = ApprovalContext {
                                    tenant_id,
                                    entity_id,
                                    domain: domain.clone(),
                                    entity_table: entity_table.clone(),
                                    actor_id: Uuid::nil(),
                                    decision,
                                    correlation_id: Uuid::new_v4(),
                                    idempotency_key: None,
                                    comment: Some(comment.to_string()),
                                };
                                if let Err(e) = self.workflow_service.approval_action(ctx).await {
                                    tracing::warn!(
                                        "approval timeout action failed for {entity_id}: {e}"
                                    );
                                }
                            }
                        }
                        _ => {
                            tracing::warn!(
                                "approval timeout fired but entity {entity_id} not in pending_approval state"
                            );
                        }
                    }
                }
                other => {
                    tracing::warn!("unknown timer type: {other}");
                }
            }
        }

        Ok(())
    }
}
