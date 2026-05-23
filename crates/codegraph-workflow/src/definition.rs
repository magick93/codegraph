//! Deserialized state machine definition from JSONB.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StateMachineDefinition {
    #[serde(default)]
    transitions: HashMap<String, Vec<String>>,
    #[serde(default)]
    data_guards: Vec<DataGuardDef>,
    #[serde(default)]
    timers: Vec<TimerDef>,
    #[serde(default)]
    dual_status_guards: HashMap<String, String>,
    #[serde(default)]
    approval_chains: Vec<ApprovalChainDef>,
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

impl StateMachineDefinition {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }

    pub fn transitions_from(&self, state: &str) -> Vec<String> {
        self.transitions.get(state).cloned().unwrap_or_default()
    }

    pub fn is_valid_transition(&self, from: &str, to: &str) -> bool {
        self.transitions
            .get(from)
            .is_some_and(|targets| targets.contains(&to.to_string()))
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
