//! Resolve workflow config: external file or inline.

use std::path::Path;

use crate::config::{EntityConfig, WorkflowConfig};
use crate::error::DomainConfigError;

/// Top-level structure of an external workflow file.
#[derive(Debug, serde::Deserialize)]
struct WorkflowFile {
    workflow: WorkflowConfig,
    #[serde(default)]
    data_guards: Vec<crate::config::DataGuard>,
    #[serde(default)]
    timers: std::collections::HashMap<String, crate::config::TimerDef>,
    #[serde(default)]
    approval_chains: std::collections::HashMap<String, crate::config::ApprovalChainDef>,
}

/// Resolve the workflow config for an entity.
///
/// Priority: workflow_file > inline workflow block > None.
pub fn resolve_workflow_config(
    entity_cfg: &EntityConfig,
    base_dir: &Path,
) -> Result<Option<WorkflowConfig>, DomainConfigError> {
    if let Some(ref file_path) = entity_cfg.workflow_file {
        let full_path = base_dir.join(file_path);
        let content = std::fs::read_to_string(&full_path).map_err(|e| {
            DomainConfigError::Invalid(format!(
                "failed to read workflow file '{}': {e}",
                full_path.display()
            ))
        })?;
        let parsed: WorkflowFile = toml::from_str(&content).map_err(|e| {
            DomainConfigError::Invalid(format!(
                "failed to parse workflow file '{}': {e}",
                full_path.display()
            ))
        })?;
        let mut wf = parsed.workflow;
        wf.data_guards = parsed.data_guards;
        wf.timers = parsed.timers;
        wf.approval_chains = parsed.approval_chains;
        return Ok(Some(wf));
    }

    Ok(entity_cfg.workflow.clone())
}
