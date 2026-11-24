//! Deserialized workflow definition from the `state_machine` JSONB.
//!
//! Since the machine kernel (#816), the seeded shape is an ENVELOPE: the
//! canonical xstate-ir `Machine` under `machine` (the ONE transition
//! decision-maker), its FNV-1a content hash under `machine_hash` (the
//! drift-detection discipline), and the engine-level layers beside it —
//! data guards, timers, dual-status guards, approval chains — whose
//! evaluators register as NAMED kernel guards while their mechanics
//! (parking, cron rows, idempotency probe) stay engine-level.
//!
//! Rows carrying only the pre-kernel `transitions` map (a deployment the
//! machine seed migration has not upgraded yet) fail closed in the engine
//! with a NAMED error pointing at the migration — never a silent walk of
//! the legacy map.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WorkflowDefinition {
    /// The canonical xstate-ir Machine — the kernel's definition. Absent
    /// for legacy (pre-kernel) rows; the engine refuses those by name.
    #[serde(default)]
    pub machine: Option<serde_json::Value>,
    /// FNV-1a 64 hex over the canonical machine JSON, recorded by the
    /// machine seed migration. Recomputed and compared on every load —
    /// a mismatch is a NAMED hard error, never a silent re-decision on a
    /// changed machine.
    #[serde(default)]
    pub machine_hash: Option<String>,
    #[serde(default)]
    pub data_guards: Vec<DataGuardDef>,
    #[serde(default)]
    pub timers: Vec<TimerDef>,
    #[serde(default)]
    pub dual_status_guards: HashMap<String, String>,
    #[serde(default)]
    pub approval_chains: Vec<ApprovalChainDef>,
    /// The pre-kernel transition map (legacy rows only). Parsed so the
    /// engine can NAME the legacy shape; never consulted for decisions.
    #[serde(default)]
    pub transitions: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalChainDef {
    pub transition_from: String,
    pub transition_to: String,
    pub steps: Vec<ApprovalStepDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalStepDef {
    pub order: i32,
    pub role: String,
    pub is_required: bool,
    #[serde(default)]
    pub timeout_hours: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataGuardDef {
    pub transition_to: String,
    pub rule: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimerDef {
    pub trigger_on_enter: String,
    #[serde(rename = "type")]
    pub timer_type: String,
    pub duration_hours: i64,
    pub target_state: Option<String>,
}

impl WorkflowDefinition {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    /// Whether the row carries the pre-kernel shape (a `transitions` map
    /// with no machine) — the engine's named-refusal class.
    pub fn is_legacy_shape(&self) -> bool {
        self.machine.is_none() && !self.transitions.is_empty()
    }

    /// The named kernel guards this definition contributes (data guards +
    /// dual-status guards, one composed entry per guarded target state).
    /// Guard names are deterministic so the lowered machine's transitions
    /// can reference them: `data_guard:{to}`, `dual_status:{to}`, and
    /// `guards:{to}` when a target carries both.
    pub fn guard_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut data_targets: Vec<&str> = self
            .data_guards
            .iter()
            .map(|g| g.transition_to.as_str())
            .collect();
        data_targets.sort_unstable();
        data_targets.dedup();
        let mut dual_targets: Vec<&String> = self.dual_status_guards.keys().collect();
        dual_targets.sort();
        for target in &data_targets {
            let name = if dual_targets.iter().any(|d| *d == target) {
                format!("guards:{target}")
            } else {
                format!("data_guard:{target}")
            };
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for target in &dual_targets {
            if !data_targets.contains(&target.as_str()) {
                names.push(format!("dual_status:{target}"));
            }
        }
        names.sort();
        names
    }

    pub fn data_guards_for(&self, target_state: &str) -> Vec<&DataGuardDef> {
        self.data_guards
            .iter()
            .filter(|g| g.transition_to == target_state)
            .collect()
    }

    pub fn timers_for_state(&self, state: &str) -> Vec<&TimerDef> {
        self.timers
            .iter()
            .filter(|t| t.trigger_on_enter == state)
            .collect()
    }

    pub fn required_approval_for(&self, status: &str) -> Option<&str> {
        self.dual_status_guards.get(status).map(|s| s.as_str())
    }

    pub fn has_approval_chain(&self, from: &str, to: &str) -> bool {
        self.approval_chains
            .iter()
            .any(|c| c.transition_from == from && c.transition_to == to)
    }

    pub fn approval_chain_for(&self, from: &str, to: &str) -> Option<&ApprovalChainDef> {
        self.approval_chains
            .iter()
            .find(|c| c.transition_from == from && c.transition_to == to)
    }
}
