//! Cloudflare Worker (wasm32) workflow service stub.
//!
//! The SeaORM-backed workflow engine (`engine::SeaOrmWorkflowService` and its
//! approval/delegation/timer modules) requires `sea-orm`/`sqlx`, which do not
//! compile to `wasm32-unknown-unknown`. The worker slice therefore carries
//! this stub: it satisfies `WorkflowService` so generated workflow action
//! handlers compile unchanged, and returns a clear error at runtime.
//!
//! TODO(worker-workflow): runtime parity — route workflow state transitions
//! through the cornucopia query layer (multi-worker epic, later step).

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::WorkflowError;
use crate::service::WorkflowService;
use crate::types::{
    ApprovalContext, DelegationContext, ProcessHistoryEntry, TransitionContext, WorkflowState,
};

fn unsupported() -> WorkflowError {
    WorkflowError::Internal(Box::new(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "workflow engine is not available in the Cloudflare Worker slice \
         (SeaORM-backed engine does not compile to wasm32)",
    )))
}

/// Workflow service for the wasm32 slice — always returns an error.
#[derive(Debug, Default, Clone)]
pub struct UnavailableWorkflowService;

impl UnavailableWorkflowService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WorkflowService for UnavailableWorkflowService {
    async fn transition(&self, _ctx: TransitionContext) -> Result<WorkflowState, WorkflowError> {
        Err(unsupported())
    }

    async fn approval_action(&self, _ctx: ApprovalContext) -> Result<WorkflowState, WorkflowError> {
        Err(unsupported())
    }

    async fn get_state(
        &self,
        _tenant_id: Uuid,
        _domain: &str,
        _entity_table: &str,
        _entity_id: Uuid,
    ) -> Result<WorkflowState, WorkflowError> {
        Err(unsupported())
    }

    async fn delegate(&self, _ctx: DelegationContext) -> Result<(), WorkflowError> {
        Err(unsupported())
    }

    async fn get_history(
        &self,
        _tenant_id: Uuid,
        _domain: &str,
        _entity_table: &str,
        _entity_id: Uuid,
    ) -> Result<Vec<ProcessHistoryEntry>, WorkflowError> {
        Err(unsupported())
    }
}
