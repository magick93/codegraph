use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::ParentCandidate;
use serde::Serialize;

use crate::api::api_model::resolve_entity_operations;
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{EntityGenerator, EntityGeneratorKind, GeneratedFile};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct CommandContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub operations: Vec<String>,
    pub has_create: bool,
    pub has_update: bool,
    pub has_delete: bool,
    pub parent_ref: Option<String>,
    /// A push-direction extension point covers this entity (issue #503):
    /// the command layer carries dispatcher.notify call sites.
    pub has_extension_points: bool,
    /// Whether THIS entity's crate carries the dispatcher module at all
    /// (issue #553): gates the dispatcher field/param TYPE, which
    /// references `crate::integration_dispatch`. Monolith: the project
    /// flag (one crate, module always present when the flag is on).
    /// Workers: the domain's push coverage — an uncovered worker emits no
    /// module, so its handlers must drop the param entirely.
    pub worker_has_dispatch: bool,
    /// The full `domain.table` ref the dispatcher addresses and the
    /// installations SQL matches against `ep.allowed_entities`.
    pub entity_ref: String,
    /// Events-producer publishing facts (issue #820 slice b): the outgoing
    /// enum, the pre-write re-read, and the publish block. `None` when the
    /// flag is off or the entity owns no publishable events — byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<ProducerCommandContext>,
}

/// The pre-rendered publishing snippet set for one entity's command module.
#[derive(Debug, Clone, Serialize)]
pub struct ProducerCommandContext {
    /// The outgoing-event enum stem (`PayRunOutgoingEvent`).
    pub enum_name: String,
    /// The full enum definition, emitted above `impl`.
    pub enum_definition: String,
    /// The pre-write re-read (empty when no binding needs `previousState`).
    pub pre_read: String,
    /// The publish block (empty only when no plans exist, which never
    /// reaches here).
    pub publish_block: String,
}

pub struct CommandGenerator {
    output_dir: PathBuf,
    parent_candidates: Vec<ParentCandidate>,
    /// Push-covered entity refs ("domain.table") from the parsed
    /// extension-points config (issue #503).
    extension_covered: std::collections::HashSet<String>,
    /// Events-producer plans (issue #820 slice b), when publishing is on.
    producer_plans: Option<std::sync::Arc<crate::events::producer::ProducerPlanIndex>>,
}

impl CommandGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            parent_candidates: Vec::new(),
            extension_covered: std::collections::HashSet::new(),
            producer_plans: None,
        }
    }

    #[must_use]
    pub fn with_producer_plans(
        mut self,
        plans: Option<std::sync::Arc<crate::events::producer::ProducerPlanIndex>>,
    ) -> Self {
        self.producer_plans = plans;
        self
    }

    pub fn with_parent_candidates(mut self, candidates: Vec<ParentCandidate>) -> Self {
        self.parent_candidates = candidates;
        self
    }

    pub fn with_extension_points(mut self, covered: std::collections::HashSet<String>) -> Self {
        self.extension_covered = covered;
        self
    }
}

#[async_trait]
impl EntityGenerator for CommandGenerator {
    fn kind(&self) -> EntityGeneratorKind {
        EntityGeneratorKind::Command
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = db
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        // DDD design surface (issue #449): a design covering this title
        // replaces the operations with its repository built-in mapping.
        let design_surface = crate::ddd::design::DddDesignSurface::from_graph(db).await?;
        let mut operations = resolve_entity_operations(db, config, &domain, &entity_name).await;
        if let Some(design_ops) = design_surface.operations_for(schema_title) {
            operations = design_ops;
        }

        // Resolve parent_ref for child entities
        let parent_ref = crate::resolve_parent_fk_column_same_domain(
            schema_title,
            &self.parent_candidates,
            entity_cfg,
            &domain,
            config,
            db,
        )
        .await;

        // Hook-only entities (no create/update/delete) get no command handler —
        // emit nothing rather than a dead skeleton (unused imports, dead struct).
        let has_command_ops = operations
            .iter()
            .any(|op| op == "create" || op == "update" || op == "delete");
        if !has_command_ops {
            return Ok(Vec::new());
        }

        let plans = self
            .producer_plans
            .as_ref()
            .map(|index| index.for_entity(&domain, schema_title).to_vec())
            .unwrap_or_default();
        let producer = if plans.is_empty() {
            None
        } else {
            let auditable =
                crate::api::handler::resolve_is_auditable(db, schema_title, config, &domain)
                    .await
                    .unwrap_or(false);
            let domain_error = format!("{}Error", codegraph_naming::to_pascal_case(&domain));
            Some(ProducerCommandContext {
                enum_name: crate::events::producer::render::outgoing_enum_name(&entity_name),
                enum_definition: crate::events::producer::render::render_outgoing_enum(
                    &plans,
                    &entity_name,
                ),
                pre_read: crate::events::producer::render::render_pre_read(
                    &plans,
                    &domain_error,
                    auditable,
                ),
                publish_block: crate::events::producer::render::render_publish_block(
                    &plans,
                    &domain,
                    &module_name,
                    &entity_name,
                    &domain_error,
                    auditable,
                    &module_name,
                ),
            })
        };

        let ctx = CommandContext {
            has_create: operations.contains(&"create".to_string()),
            has_update: operations.contains(&"update".to_string()),
            has_delete: operations.contains(&"delete".to_string()),
            parent_ref,
            entity_name,
            has_extension_points: self
                .extension_covered
                .contains(&format!("{domain}.{}", module_name)),
            worker_has_dispatch: worker_has_dispatch(project, &domain, &self.extension_covered),
            entity_ref: format!("{domain}.{}", module_name),
            module_name: module_name.clone(),
            domain: domain.clone(),
            operations,
            producer,
        };

        let content = render_template_with_project(tera, "ddd/command.tera", &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("domain")
                .join(&domain)
                .join(&module_name)
                .join("command.rs"),
            content,
        }])
    }
}

/// Whether the crate carrying this entity's command module also carries the
/// `integration_dispatch` module (issue #553). The monolith is ONE crate:
/// the dispatcher param rides the project flag and signatures stay uniform
/// across entities. Under the workers topology each domain worker is its
/// own crate and the module exists only for push-covered domains — the
/// dispatcher field/param must be dropped there, or the handlers
/// type-reference a module the worker never received (E0433).
fn worker_has_dispatch(
    project: &ProjectConfig,
    domain: &str,
    extension_covered: &std::collections::HashSet<String>,
) -> bool {
    if project.deployment.deployment_topology == crate::profile::DeploymentTopology::Workers {
        extension_covered
            .iter()
            .any(|entity| entity.split('.').next() == Some(domain))
    } else {
        project.integration.has_extension_points
    }
}
