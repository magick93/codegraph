//! Background timer service for deadline/reminder/approval_timeout.
//!
//! [`process_pending_timers`] is client-generic: the Cloudflare Worker slice
//! calls it from a `#[event(scheduled)]` handler with a per-invocation
//! connection; the monolith runs the host-only [`TimerService`] tokio loop
//! around the same function.

use uuid::Uuid;

use crate::error::WorkflowError;
use crate::service::WorkflowService;
use crate::tx::{WfParam, WorkflowTx};
use crate::types::{ApprovalContext, ApprovalDecision, TransitionContext, TriggerSource};

/// Sweep pending timers, firing any whose deadline has passed.
///
/// `domain` optionally filters the sweep to `workflow_definition.domain = $1`
/// (the per-domain worker passes its own domain; the monolith passes `None`
/// to sweep everything). The sweep is tenant-agnostic: any
/// `platform.workflow_timer` whose definition belongs to the target domain is
/// fired, and the transition carries the timer's tenant via the instance row.
/// Because it is not scoped to one tenant it relies on a connection role that
/// bypasses the `platform.workflow_*` RLS policies.
pub async fn process_pending_timers(
    tx: &mut dyn WorkflowTx,
    domain: Option<&str>,
    workflow_service: &dyn WorkflowService,
) -> Result<(), WorkflowError> {
    let (sql, params): (String, Vec<WfParam>) = match domain {
        Some(d) => (
            r#"SELECT t.id, t.tenant_id, t.instance_id, t.timer_type, t.target_state,
                      wi.entity_id, wd.domain, wd.entity_table
               FROM platform.workflow_timer t
               JOIN platform.workflow_instance wi ON t.instance_id = wi.id
               JOIN platform.workflow_definition wd ON wi.definition_id = wd.id
               WHERE t.fires_at <= now() AND NOT t.is_fired AND wd.domain = $1
               ORDER BY t.fires_at ASC
               LIMIT 100"#
                .to_string(),
            vec![WfParam::Str(d.to_string())],
        ),
        None => (
            r#"SELECT t.id, t.tenant_id, t.instance_id, t.timer_type, t.target_state,
                      wi.entity_id, wd.domain, wd.entity_table
               FROM platform.workflow_timer t
               JOIN platform.workflow_instance wi ON t.instance_id = wi.id
               JOIN platform.workflow_definition wd ON wi.definition_id = wd.id
               WHERE t.fires_at <= now() AND NOT t.is_fired
               ORDER BY t.fires_at ASC
               LIMIT 100"#
                .to_string(),
            vec![],
        ),
    };

    let rows = tx.query_all(&sql, &params).await?;

    for row in rows {
        let timer_id = row.get_uuid("id")?;
        let timer_type = row.get_string("timer_type")?;
        let target_state = row.get_opt_string("target_state")?;
        let tenant_id = row.get_uuid("tenant_id")?;
        let entity_id = row.get_uuid("entity_id")?;
        let entity_domain = row.get_string("domain")?;
        let entity_table = row.get_string("entity_table")?;

        tx.execute(
            "UPDATE platform.workflow_timer SET is_fired = true WHERE id = $1",
            &[WfParam::Uuid(timer_id)],
        )
        .await?;

        match timer_type.as_str() {
            "deadline" => {
                if let Some(target) = target_state {
                    let ctx = TransitionContext {
                        tenant_id,
                        entity_id,
                        domain: entity_domain.clone(),
                        entity_table: entity_table.clone(),
                        target_state: target,
                        actor_id: None,
                        correlation_id: Uuid::new_v4(),
                        idempotency_key: None,
                        comment: Some("auto-transition by timer".to_string()),
                        entity_data: serde_json::Value::Null,
                        trigger_source: TriggerSource::Timer,
                        session_user_id: None,
                        session_api_key_id: None,
                    };
                    if let Err(e) = workflow_service.transition(ctx).await {
                        tracing::warn!("timer transition failed for entity {entity_id}: {e}");
                    }
                }
            }
            "reminder" => {
                tracing::info!(
                    "reminder fired for entity {entity_id} in {entity_domain}.{entity_table}"
                );
            }
            "approval_timeout" => {
                let state = workflow_service
                    .get_state(tenant_id, &entity_domain, &entity_table, entity_id)
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
                                domain: entity_domain.clone(),
                                entity_table: entity_table.clone(),
                                actor_id: Uuid::nil(),
                                decision,
                                correlation_id: Uuid::new_v4(),
                                idempotency_key: None,
                                comment: Some(comment.to_string()),
                                session_user_id: None,
                                session_api_key_id: None,
                            };
                            if let Err(e) = workflow_service.approval_action(ctx).await {
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

    tx.commit().await?;

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
mod host {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::tx::WorkflowClient;

    pub struct TimerService {
        client: Arc<dyn WorkflowClient>,
        workflow_service: Arc<dyn WorkflowService>,
        poll_interval: Duration,
    }

    impl TimerService {
        pub fn new(
            db: sea_orm::DatabaseConnection,
            workflow_service: Arc<dyn WorkflowService>,
            poll_interval: Duration,
        ) -> Self {
            Self {
                client: Arc::new(crate::engine::SeaOrmWorkflowClient::new(db)),
                workflow_service,
                poll_interval,
            }
        }

        pub async fn run(&self, shutdown: tokio_util::sync::CancellationToken) {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(self.poll_interval) => {
                        if let Err(e) = self.sweep_all_domains().await {
                            tracing::error!("timer service error: {e}");
                        }
                    }
                    _ = shutdown.cancelled() => {
                        tracing::info!("timer service shutting down");
                        return;
                    }
                }
            }
        }

        /// The monolith sweeps every domain it owns (no per-domain worker).
        async fn sweep_all_domains(&self) -> Result<(), WorkflowError> {
            let mut tx = self.client.begin().await?;
            let result =
                process_pending_timers(tx.as_mut(), None, self.workflow_service.as_ref()).await;
            if result.is_err() {
                let _ = tx.rollback().await;
            }
            result
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use host::TimerService;
