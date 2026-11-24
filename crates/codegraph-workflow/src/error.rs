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

    /// The machine names a guard no registered source implements — the
    /// wire 501 NOT_IMPLEMENTED (issue #816). Fail-closed: never silently
    /// allowed.
    #[error("guard '{0}' is not implemented")]
    UnimplementedGuard(String),

    /// The definition row's recorded machine hash does not match the
    /// machine JSON's current hash (issue #816 drift discipline) — the row
    /// was hand-edited or seeded by a stale generator. A NAMED hard error;
    /// never a silent re-decision on a changed machine.
    #[error(
        "machine '{machine_id}' drifted: the definition row records hash {stored} but the \
         machine JSON's current hash is {computed} — the row was hand-edited or seeded by a \
         stale generator; re-apply the machine seed migration"
    )]
    MachineDrift {
        machine_id: String,
        stored: String,
        computed: String,
    },

    /// The definition row carries the pre-kernel shape (a `transitions`
    /// map, no machine) — the engine refuses it by name instead of walking
    /// the legacy map (issue #816).
    #[error(
        "workflow definition for {domain}.{entity_table} carries no machine kernel JSON \
         (a pre-kernel transitions map) — apply the generated migration band \
         (0007_workflow_machine_seed.sql) to upgrade it, then retry"
    )]
    DefinitionNotMachineShaped {
        domain: String,
        entity_table: String,
    },

    /// The write-both contract failed: the entity's status column did not
    /// move in the transition's transaction (issue #816 Decision 3).
    #[error("workflow write-both failed: the entity row did not update — {0}")]
    WriteBothFailed(String),

    #[error("internal error: {0}")]
    Internal(#[from] Box<dyn std::error::Error + Send + Sync>),
}
