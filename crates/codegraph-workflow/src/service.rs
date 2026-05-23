//! WorkflowService trait definition.

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::WorkflowError;
use crate::types::{
    ApprovalContext, DelegationContext, ProcessHistoryEntry, TransitionContext, WorkflowState,
};

#[async_trait]
pub trait WorkflowService: Send + Sync {
    async fn transition(&self, ctx: TransitionContext) -> Result<WorkflowState, WorkflowError>;

    async fn approval_action(&self, ctx: ApprovalContext) -> Result<WorkflowState, WorkflowError>;

    async fn get_state(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<WorkflowState, WorkflowError>;

    async fn delegate(&self, ctx: DelegationContext) -> Result<(), WorkflowError>;

    async fn get_history(
        &self,
        tenant_id: Uuid,
        domain: &str,
        entity_table: &str,
        entity_id: Uuid,
    ) -> Result<Vec<ProcessHistoryEntry>, WorkflowError>;
}
