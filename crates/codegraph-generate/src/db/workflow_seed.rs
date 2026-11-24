use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::GenerationEntry;
use crate::db::dialect::{DatabaseTarget, SqlDialect, db_template_for, dialect_for_target};
use crate::error::Result;
use crate::machines;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
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
    /// The machine kernel envelope (canonical xstate-ir Machine + its FNV
    /// drift hash + the engine-level guard/timer layers) — the 0007 seed's
    /// upsert payload (issue #816).
    pub machine_envelope_json: String,
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
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::WorkflowSeed
    }

    fn supported_targets(&self) -> Option<Vec<DatabaseTarget>> {
        Some(vec![DatabaseTarget::Postgres, DatabaseTarget::Sqlite])
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
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

                // The graph's table is authoritative: a mox `table "…"`
                // declaration (issue #668) overrides the title-derived name,
                // and seeding the derived table would write rows into a table
                // the DDL never creates. Absent a graph schema (config-only
                // entity) the derived name stays.
                let entity_table = match db.get_schema_in_domain(entity_name, domain_name).await {
                    Ok(Some(schema)) if !schema.pg_table_name.is_empty() => {
                        schema.pg_table_name.clone()
                    }
                    _ => {
                        codegraph_naming::to_snake_case(&config.defaults.strip_suffix(entity_name))
                    }
                };

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

                // ── Plane B seeding (issue #816, Decision 2) ────────────
                // One machine definition per entity: an artifact machine
                // bound to the entity embeds VERBATIM (cross-checked
                // against the config lowering — disagreement is a hard
                // error naming both sources); otherwise the config lowers
                // into the IR.
                let lowered = machines::workflow_config_to_machine(
                    workflow,
                    &format!("{domain_name}.{entity_table}"),
                );
                let lowered_value = serde_json::to_value(&lowered).map_err(|e| {
                    crate::error::Error::Config(format!(
                        "workflow machine for {domain_name}.{entity_table} does not serialize: {e}"
                    ))
                })?;
                let machine_value = match find_artifact_machine(db, entity_name).await {
                    Some((artifact, artifact_source)) => {
                        machines::compare_machine_surfaces(
                            &artifact,
                            &lowered_value,
                            &artifact_source,
                            &format!(
                                "domains.toml [domains.{domain_name}].workflow for `{entity_name}`"
                            ),
                        )
                        .map_err(crate::error::Error::Config)?;
                        artifact
                    }
                    None => lowered_value,
                };
                let machine_canonical = machines::canonical_ir_json(&machine_value);
                let machine_hash = fnv1a_64_hex(machine_canonical.as_bytes());
                let machine_envelope_json =
                    build_machine_envelope(&machine_value, &machine_hash, workflow);

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
                    machine_envelope_json,
                });
            }
        }

        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let ctx = WorkflowSeedContext { entries };
        let mut files = Vec::new();

        // 0006 stays BYTE-IDENTICAL (the legacy shape): deployments that
        // already recorded it must not see a changed tracked file (#756 —
        // a changed checksum is a hard boot error).
        let content = render_template_with_project(
            tera,
            &db_template_for(&*self.dialect, "workflow_seed"),
            &ctx,
            project,
        )?;
        files.push(GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir).join("0006_workflow_seed.sql"),
            content,
        });

        // 0007: the machine kernel seed — the envelope upsert + the
        // write-both reconciliation (the #700 backfill pattern:
        // idempotent, re-applies to a fixed point).
        let content =
            render_template_with_project(tera, "db/workflow_machine_seed.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: crate::db::migrations_root(&self.output_dir)
                .join("0007_workflow_machine_seed.sql"),
            content,
        });

        Ok(files)
    }
}

/// Finds the artifact machine bound to `entity_name`: a `.ddd` design whose
/// resolved title carries a machine IR. Returns `(IR value, the authored
/// source path for error messages)`.
async fn find_artifact_machine(
    db: &dyn GraphQuerier,
    entity_name: &str,
) -> Option<(serde_json::Value, String)> {
    let models = db.get_ddd_models().await.ok()?;
    for model in &models {
        for design in &model.designs {
            if design.resolved_title.as_deref() != Some(entity_name) {
                continue;
            }
            if let Some(ir) = &design.machine_ir_json {
                let source = design
                    .machine_provenance_json
                    .as_ref()
                    .and_then(|p| p.get("path"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("design `{}`", design.class));
                return Some((ir.clone(), source));
            }
        }
    }
    None
}

/// FNV-1a 64-bit lowercase hex over the ORIGINAL bytes — the same inline
/// digest the ops harness and the workflow engine use (no new dependency).
fn fnv1a_64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Builds the machine kernel envelope: the canonical machine, its drift
/// hash, and the engine-level layers (data guards, timers, dual-status
/// guards) whose evaluators register as NAMED kernel guards. Serialized
/// through `canonical_ir_json` so the seeded migration is byte-stable.
fn build_machine_envelope(
    machine_value: &serde_json::Value,
    machine_hash: &str,
    workflow: &codegraph_config::WorkflowConfig,
) -> String {
    let mut data_guards: Vec<serde_json::Value> = workflow
        .data_guards
        .iter()
        .map(|g| {
            serde_json::json!({
                "transition_to": g.transition_to,
                "rule": g.rule,
                "message": g.message,
            })
        })
        .collect();
    data_guards.sort_by(|a, b| {
        (
            a["transition_to"].as_str().unwrap_or_default(),
            a["rule"].as_str().unwrap_or_default(),
        )
            .cmp(&(
                b["transition_to"].as_str().unwrap_or_default(),
                b["rule"].as_str().unwrap_or_default(),
            ))
    });

    let mut timers: Vec<serde_json::Value> = workflow
        .timers
        .values()
        .map(|t| {
            serde_json::json!({
                "trigger_on_enter": t.trigger_on_enter,
                "type": t.timer_type,
                "duration_hours": t.duration_hours,
                "target_state": t.target_state,
            })
        })
        .collect();
    timers.sort_by(|a, b| {
        (
            a["trigger_on_enter"].as_str().unwrap_or_default(),
            a["type"].as_str().unwrap_or_default(),
        )
            .cmp(&(
                b["trigger_on_enter"].as_str().unwrap_or_default(),
                b["type"].as_str().unwrap_or_default(),
            ))
    });

    let dual_status_guards: std::collections::BTreeMap<String, String> = workflow
        .dual_status_guards
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let envelope = serde_json::json!({
        "machine": machine_value,
        "machine_hash": machine_hash,
        "data_guards": data_guards,
        "timers": timers,
        "dual_status_guards": dual_status_guards,
    });
    machines::canonical_ir_json(&envelope)
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

    let mut timers: Vec<TimerSer> = workflow
        .timers
        .values()
        .map(|t| TimerSer {
            trigger_on_enter: t.trigger_on_enter.clone(),
            timer_type: t.timer_type.clone(),
            duration_hours: t.duration_hours,
            target_state: t.target_state.clone(),
        })
        .collect();
    // HashMap iteration order is not stable across processes; sort so the
    // seeded JSONB state machine is byte-identical between runs.
    timers.sort_by(|a, b| {
        (&a.trigger_on_enter, &a.timer_type).cmp(&(&b.trigger_on_enter, &b.timer_type))
    });

    let sm = StateMachine {
        transitions,
        dual_status_guards,
        data_guards,
        timers,
    };

    serde_json::to_string(&sm).unwrap_or_else(|_| "{}".to_string())
}
