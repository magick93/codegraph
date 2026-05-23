//! Workflow error types.

use uuid::Uuid;

use crate::types::PendingApproval;

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("workflow instance not found")]
    NotFound,

    #[error("entity is in terminal state; no transitions allowed")]
    AlreadyTerminal,

    #[error("invalid transition from '{current}' to '{target}'")]
    InvalidTransition { current: String, target: String },

    #[error("guard failed: {message}")]
    GuardFailed { rule: String, message: String },

    #[error("status '{status}' requires approval state '{required_approval}'")]
    DualStatusGuardFailed {
        status: String,
        required_approval: String,
    },

    #[error("idempotency conflict for key {key}")]
    IdempotencyConflict { key: Uuid },

    #[error("transition requires approval")]
    ApprovalRequired { pending_step: PendingApproval },

    #[error("no pending approval step for this actor")]
    NoPendingApproval,

    #[error("concurrent modification detected")]
    ConcurrentModification,

    #[error("internal error: {0}")]
    Internal(#[from] Box<dyn std::error::Error + Send + Sync>),
}
