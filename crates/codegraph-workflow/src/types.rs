//! Core types for the workflow engine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How a transition was triggered — affects guard evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerSource {
    /// User-initiated transition — all guards evaluated.
    User,
    /// Timer-initiated transition — data guards are skipped (no entity data available).
    Timer,
}

/// Context for requesting a state transition.
#[derive(Debug, Clone)]
pub struct TransitionContext {
    pub tenant_id: Uuid,
    pub entity_id: Uuid,
    pub domain: String,
    pub entity_table: String,
    pub target_state: String,
    pub actor_id: Option<Uuid>,
    pub correlation_id: Uuid,
    pub idempotency_key: Option<Uuid>,
    pub comment: Option<String>,
    pub entity_data: serde_json::Value,
    /// Source of the transition. Defaults to `User`.
    pub trigger_source: TriggerSource,
}

/// Current workflow state for an entity.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowState {
    pub entity_id: Uuid,
    pub current_state: String,
    pub approval_state: Option<String>,
    pub is_terminal: bool,
    pub available_transitions: Vec<String>,
    pub pending_approvals: Vec<PendingApproval>,
}

/// A pending approval step waiting for action.
#[derive(Debug, Clone, Serialize)]
pub struct PendingApproval {
    pub step_id: Uuid,
    pub step_order: i32,
    pub role: String,
    pub is_required: bool,
    pub timeout_hours: Option<i32>,
    pub deadline: Option<DateTime<Utc>>,
}

/// Decision on an approval step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
}

/// Context for an approval action.
#[derive(Debug, Clone)]
pub struct ApprovalContext {
    pub tenant_id: Uuid,
    pub entity_id: Uuid,
    pub domain: String,
    pub entity_table: String,
    pub actor_id: Uuid,
    pub decision: ApprovalDecision,
    pub correlation_id: Uuid,
    pub idempotency_key: Option<Uuid>,
    pub comment: Option<String>,
}

/// A single entry in the process history (workflow transition log).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessHistoryEntry {
    pub id: Uuid,
    pub action_date: DateTime<Utc>,
    pub status: String,
    pub previous_status: Option<String>,
    pub correlation_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub comment: Option<String>,
}

/// Context for delegating an approval step.
#[derive(Debug, Clone)]
pub struct DelegationContext {
    pub tenant_id: Uuid,
    pub entity_id: Uuid,
    pub domain: String,
    pub entity_table: String,
    pub from_actor_id: Uuid,
    pub to_actor_id: Uuid,
    pub correlation_id: Uuid,
    pub reason: Option<String>,
}
