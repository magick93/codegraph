use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::db::dialect::{db_template_for, dialect_for_target, DatabaseTarget, SqlDialect};
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

/// A workflow definition to seed into `platform.workflow_definition`.
#[derive(Debug, Serialize)]
pub struct WorkflowSeedEntry {
    pub name: String,
    pub domain: String,
    pub entity_table: String,
    pub status_field: String,
    pub approval_status_field: Option<String>,
    pub initial_state: String,
    pub terminal_states: Vec<String>,
    /// JSON-encoded state machine (transition map + guards).
    pub state_machine_json: String,
    /// Approval chains for this entity's workflow.
    pub approval_chains: Vec<ApprovalChainSeed>,
}

/// An approval chain to seed into `platform.approval_step`.
#[derive(Debug, Serialize)]
pub struct ApprovalChainSeed {
    pub from: String,
    pub to: String,
    pub steps: Vec<ApprovalStepSeed>,
}

/// A single step in an approval chain seed.
#[derive(Debug, Serialize)]
pub struct ApprovalStepSeed {
    pub role: String,
    pub required: bool,
    pub timeout_hours: Option<i32>,
}

/// Context for the workflow seed migration template.
#[derive(Debug, Serialize)]
pub struct WorkflowSeedContext {
    pub entries: Vec<WorkflowSeedEntry>,
}

pub struct WorkflowSeedGenerator {
    output_dir: PathBuf,
    dialect: Box<dyn SqlDialect>,
}

impl WorkflowSeedGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            dialect: dialect_for_target(DatabaseTarget::Postgres),
        }
    }

    pub fn with_dialect(mut self, dialect: Box<dyn SqlDialect>) -> Self {
        self.dialect = dialect;
        self
    }
}

#[async_trait]
impl GlobalGenerator for WorkflowSeedGenerator {
    fn name(&self) -> &str {
        "workflow_seed"
    }

    fn supported_targets(&self) -> Option<Vec<DatabaseTarget>> {
        Some(vec![DatabaseTarget::Postgres, DatabaseTarget::Sqlite])
    }

    async fn generate(
        &self,
        _db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Workflow seed data inserts into platform.* tables — PG-only (schema-qualified)
        if !self.dialect.has_schemas() {
            return Ok(vec![]);
        }
        let mut entries = Vec::new();

        let mut domain_names: Vec<&String> = config.domains.keys().collect();
        domain_names.sort();

        for domain_name in domain_names {
            let domain_entry = &config.domains[domain_name];
            let mut entity_names: Vec<&String> = domain_entry.entity_config.keys().collect();
            entity_names.sort();

            for entity_name in entity_names {
                let entity_cfg = &domain_entry.entity_config[entity_name];
                let workflow = match &entity_cfg.workflow {
                    Some(wf) => wf,
                    None => continue,
                };

                // Build state machine JSON from transitions + dual_status_guards
                let state_machine = build_state_machine_json(workflow);

                let entity_table =
                    codegraph_naming::to_snake_case(&config.defaults.strip_suffix(entity_name));

                // Build approval chain seeds
                let approval_chains: Vec<ApprovalChainSeed> = workflow
                    .approval_chains
                    .values()
                    .map(|chain| ApprovalChainSeed {
                        from: chain.from.clone(),
                        to: chain.to.clone(),
                        steps: chain
                            .steps
                            .iter()
                            .map(|step| ApprovalStepSeed {
                                role: step.role.clone(),
                                required: step.required,
                                timeout_hours: step.timeout_hours,
                            })
                            .collect(),
                    })
                    .collect();

                entries.push(WorkflowSeedEntry {
                    name: format!("{}.{}", domain_name, entity_table),
                    domain: domain_name.clone(),
                    entity_table,
                    status_field: workflow.status_field.clone(),
                    approval_status_field: workflow.approval_status_field.clone(),
                    initial_state: workflow.initial_state.clone(),
                    terminal_states: workflow.terminal_states.clone(),
                    state_machine_json: state_machine,
                    approval_chains,
                });
            }
        }

        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let ctx = WorkflowSeedContext { entries };
        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "workflow_seed"),
            &ctx,
            project,
        )?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0006_workflow_seed.sql"),
            content,
        }])
    }
}

/// Build a JSON state machine object from WorkflowConfig transitions + guards + timers.
fn build_state_machine_json(workflow: &codegraph_config::WorkflowConfig) -> String {
    use std::collections::BTreeMap;

    #[derive(serde::Serialize)]
    struct StateMachine {
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        transitions: BTreeMap<String, Vec<String>>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        dual_status_guards: BTreeMap<String, String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        data_guards: Vec<DataGuardSer>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        timers: Vec<TimerSer>,
    }

    #[derive(serde::Serialize)]
    struct DataGuardSer {
        transition_to: String,
        rule: String,
        message: String,
    }

    #[derive(serde::Serialize)]
    struct TimerSer {
        trigger_on_enter: String,
        #[serde(rename = "type")]
        timer_type: String,
        duration_hours: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_state: Option<String>,
    }

    let transitions: BTreeMap<String, Vec<String>> = workflow
        .transitions
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let dual_status_guards: BTreeMap<String, String> = workflow
        .dual_status_guards
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let data_guards: Vec<DataGuardSer> = workflow
        .data_guards
        .iter()
        .map(|g| DataGuardSer {
            transition_to: g.transition_to.clone(),
            rule: g.rule.clone(),
            message: g.message.clone(),
        })
        .collect();

    let timers: Vec<TimerSer> = workflow
        .timers
        .values()
        .map(|t| TimerSer {
            trigger_on_enter: t.trigger_on_enter.clone(),
            timer_type: t.timer_type.clone(),
            duration_hours: t.duration_hours,
            target_state: t.target_state.clone(),
        })
        .collect();

    let sm = StateMachine {
        transitions,
        dual_status_guards,
        data_guards,
        timers,
    };

    serde_json::to_string(&sm).unwrap_or_else(|_| "{}".to_string())
}
