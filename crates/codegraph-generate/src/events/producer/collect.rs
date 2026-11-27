//! The graph-backed `ProducerSource` (issue #820 slice a) plus the static
//! source the KATs drive.
//!
//! The source owns every graph read; the planner stays pure. The run path
//! and the doctor diagnostic mode both hand a [`GraphProducerSource`] to
//! [`super::plan_with`], so the two surfaces cannot derive different names
//! (`build_dto_context`/`resolve_dto_fields` are the ONE response-surface
//! rule). A future compiled-artifact source implements the same trait.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;

use crate::db::dialect::{DatabaseTarget, dialect_for_target};
use crate::ddd::design::DddDesignSurface;
use crate::ddd::dto::build_dto_context;
use crate::ddd::service_ops::{BodyRenderContext, ExecutionKind, bind_entity, routes_for_entity};
use crate::events::Transport;
use crate::events::model::PublicationSource;
use crate::events::naming::channel_enum_name;
use crate::project_config::DtoKeyCasing;

use super::planner::{
    EmissionArm, EmissionSite, ProducerChannel, ProducerEntitySurface, ProducerInputs,
    SurfaceChild, SurfaceField,
};

/// Where the planner's facts come from. The one seam a compiled-model /
/// artifact-backed source can implement later; the run path and doctor both
/// use [`GraphProducerSource`] today.
#[async_trait]
pub trait ProducerSource: Send + Sync {
    /// Assemble the planner inputs. An error means the analysis could not
    /// run (a graph/lowering failure); the caller surfaces it or stays
    /// silent — never a guessed diagnostic.
    async fn collect(&self) -> Result<ProducerInputs, String>;
}

/// A source over a live [`GraphQuerier`].
pub struct GraphProducerSource<'a> {
    pub db: &'a dyn GraphQuerier,
    pub config: &'a DomainConfig,
    pub workers_topology: bool,
    /// The generation target — decides whether a channel is queue-bound
    /// (slice b's R12) through the same dialect the events generator uses.
    pub database_target: DatabaseTarget,
}

impl<'a> GraphProducerSource<'a> {
    #[must_use]
    pub fn new(
        db: &'a dyn GraphQuerier,
        config: &'a DomainConfig,
        workers_topology: bool,
        database_target: DatabaseTarget,
    ) -> Self {
        Self {
            db,
            config,
            workers_topology,
            database_target,
        }
    }
}

/// A source over already-assembled inputs — the KATs, and the seam a future
/// compiled-model source slots into.
#[derive(Debug, Clone, Default)]
pub struct StaticProducerSource {
    pub inputs: ProducerInputs,
}

impl StaticProducerSource {
    #[must_use]
    pub fn new(inputs: ProducerInputs) -> Self {
        Self { inputs }
    }
}

#[async_trait]
impl ProducerSource for StaticProducerSource {
    async fn collect(&self) -> Result<ProducerInputs, String> {
        Ok(self.inputs.clone())
    }
}

/// One raw site before the response surfaces resolve the state facts.
struct RawSite {
    service: String,
    op: String,
    module_domain: String,
    emits: Vec<String>,
    is_protected: bool,
    child_bound: bool,
    entity: Option<String>,
    arm: EmissionArm,
    own_writes: BTreeSet<String>,
}

#[async_trait]
impl ProducerSource for GraphProducerSource<'_> {
    async fn collect(&self) -> Result<ProducerInputs, String> {
        let db = self.db;
        let config = self.config;
        let type_suffix = config.defaults.type_suffix.clone();
        let models = db.get_ddd_models().await.map_err(|e| e.to_string())?;
        if models.iter().all(|model| !model_has_emits(model)) {
            // No `emits` anywhere: zero diagnostics and no surface work.
            return Ok(ProducerInputs {
                type_suffix,
                ..Default::default()
            });
        }
        let events = crate::events::model::declared_events(db)
            .await
            .map_err(|e| e.to_string())?;
        let dialect = dialect_for_target(self.database_target);
        let channels = crate::events::model::declared_channels(db, dialect.as_ref())
            .await
            .map_err(|e| e.to_string())?;
        // Event → bound DSL channels (fanout targets). Trigger-only and
        // unbound channels never publish.
        let mut publish_targets: BTreeMap<String, Vec<ProducerChannel>> = BTreeMap::new();
        for channel in &channels {
            let Transport::Pgmq(pgmq) = &channel.transport else {
                continue;
            };
            for publication in &channel.publications {
                if !matches!(publication.source, PublicationSource::Application) {
                    continue;
                }
                let targets = publish_targets
                    .entry(publication.event.clone())
                    .or_default();
                if targets.iter().any(|target| target.name == channel.name) {
                    continue;
                }
                targets.push(ProducerChannel {
                    name: channel.name.clone(),
                    name_snake: codegraph_naming::to_snake_case(&channel.name),
                    enum_name: channel_enum_name(&channel.name),
                    queue: pgmq.queue.clone(),
                });
            }
        }
        let design_surface = DddDesignSurface::from_graph(db).await.ok();
        let render_context = BodyRenderContext {
            workers_topology: self.workers_topology,
            dto_casing: DtoKeyCasing::default(),
            ..Default::default()
        };

        let mut raw_sites: Vec<RawSite> = Vec::new();
        // (domain, title) → the route set (one lowering pass per entity).
        let mut routes_cache: HashMap<(String, String), Result<Vec<_>, String>> = HashMap::new();

        for model in &models {
            for service in &model.services {
                let domain = service.module.clone();
                for op in &service.operations {
                    if op.emits.is_empty() {
                        continue;
                    }
                    if op.is_protected {
                        raw_sites.push(RawSite {
                            service: service.name.clone(),
                            op: op.name.clone(),
                            module_domain: domain.clone(),
                            emits: op.emits.clone(),
                            is_protected: true,
                            child_bound: false,
                            entity: None,
                            arm: EmissionArm::Unknown,
                            own_writes: BTreeSet::new(),
                        });
                        continue;
                    }
                    let entity = bind_entity(model, op, db, &type_suffix).await;
                    let mut own_writes: BTreeSet<String> = op
                        .params
                        .iter()
                        .map(|param| codegraph_naming::to_snake_case(&param.name))
                        .collect();
                    let arm = match &entity {
                        Some(title) => {
                            let key = (domain.clone(), title.clone());
                            let routes = match routes_cache.get(&key) {
                                Some(routes) => routes.clone(),
                                None => {
                                    let routes = routes_for_entity(
                                        db,
                                        config,
                                        &domain,
                                        title,
                                        &render_context,
                                    )
                                    .await;
                                    routes_cache.insert(key.clone(), routes.clone());
                                    routes
                                }
                            };
                            match routes {
                                Err(_) => EmissionArm::Unknown,
                                Ok(routes) => match routes_ref(&routes, &op.name) {
                                    Some(route) => {
                                        if let ExecutionKind::Body { plan } = &route.execution {
                                            for assigned in &plan.assigned_fields {
                                                own_writes.insert(codegraph_naming::to_snake_case(
                                                    assigned,
                                                ));
                                            }
                                        }
                                        classify_arm(&route.execution)
                                    }
                                    None => EmissionArm::Unknown,
                                },
                            }
                        }
                        None => EmissionArm::Unknown,
                    };
                    raw_sites.push(RawSite {
                        service: service.name.clone(),
                        op: op.name.clone(),
                        module_domain: domain.clone(),
                        emits: op.emits.clone(),
                        is_protected: false,
                        child_bound: false,
                        entity,
                        arm,
                        own_writes,
                    });
                }
            }
            // Aggregate child operations (`children <Root> { … }`) are
            // declared on the ROOT design but act on the child — v1 has no
            // child-scoped publishing site (R4 `child-bound`).
            for design in &model.designs {
                for op in &design.child_operations {
                    if op.emits.is_empty() {
                        continue;
                    }
                    raw_sites.push(RawSite {
                        service: design.class.clone(),
                        op: op.name.clone(),
                        module_domain: design.module.clone(),
                        emits: op.emits.clone(),
                        is_protected: op.is_protected,
                        child_bound: true,
                        entity: None,
                        arm: EmissionArm::Unknown,
                        own_writes: BTreeSet::new(),
                    });
                }
            }
        }

        // Resolve the response surfaces (one per distinct bound entity).
        let mut entities: BTreeMap<(String, String), ProducerEntitySurface> = BTreeMap::new();
        let mut writable: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for site in &raw_sites {
            let Some(title) = site.entity.as_deref() else {
                continue;
            };
            let key = (site.module_domain.clone(), title.to_string());
            if entities.contains_key(&key) {
                continue;
            }
            let Ok(context) = build_dto_context(db, title, &site.module_domain, config).await
            else {
                continue;
            };
            let mut surface = surface_from_context(&context, title, &site.module_domain);
            // The natural-key / surrogate-identity alternatives (#821): the
            // graph's `id`-marked features (the DTO's synthetic `id` is the
            // storage key; W1 names the declared column).
            if let Ok(properties) = db.get_properties(title).await {
                surface.alt_identities = properties
                    .iter()
                    .filter(|prop| prop.is_id && prop.rust_field_name != "id")
                    .map(|prop| prop.rust_field_name.clone())
                    .collect();
            }
            let mut can_write: BTreeSet<String> = context
                .fields
                .iter()
                .filter(|field| !context.immutable_fields.contains(&field.name))
                .map(|field| field.name.clone())
                .collect();
            for child in &context.child_dtos {
                if !child.is_media {
                    can_write.insert(child.field_name.clone());
                }
            }
            writable.insert(key.clone(), can_write);
            entities.insert(key, surface);
        }

        // Machine facts (the state field + its DTO type) and the per-site
        // own-write/state joins.
        for ((_domain, title), surface) in entities.iter_mut() {
            if let Some(design) = design_surface.as_ref()
                && let Some(machine) = design.machine_for(title)
            {
                let field = codegraph_naming::to_snake_case(&machine.field);
                surface.state_type = surface
                    .fields
                    .iter()
                    .find(|f| f.name == field)
                    .map(|f| f.rust_type.clone());
                surface.state_field = Some(field);
            }
        }

        let mut sites = Vec::with_capacity(raw_sites.len());
        for raw in raw_sites {
            let key = raw
                .entity
                .as_ref()
                .map(|title| (raw.module_domain.clone(), title.clone()));
            let mut own_writes = raw.own_writes;
            let mut writable_fields = BTreeSet::new();
            if let Some(key) = &key {
                if let Some(surface) = entities.get(key)
                    && matches!(raw.arm, EmissionArm::Transition(_))
                    && let Some(state) = &surface.state_field
                {
                    own_writes.insert(state.clone());
                }
                if let Some(fields) = writable.get(key) {
                    writable_fields = fields.clone();
                }
            }
            sites.push(EmissionSite {
                service: raw.service,
                op: raw.op,
                module_domain: raw.module_domain,
                emits: raw.emits,
                is_protected: raw.is_protected,
                child_bound: raw.child_bound,
                entity: raw.entity,
                arm: raw.arm,
                own_writes,
                writable_fields,
            });
        }

        Ok(ProducerInputs {
            events,
            sites,
            entities,
            publish_targets,
            type_suffix,
        })
    }
}

fn model_has_emits(model: &codegraph_core::types::DddModelGraph) -> bool {
    model
        .services
        .iter()
        .any(|service| service.operations.iter().any(|op| !op.emits.is_empty()))
        || model.designs.iter().any(|design| {
            design
                .child_operations
                .iter()
                .any(|op| !op.emits.is_empty())
        })
}

fn routes_ref<'a>(
    routes: &'a [crate::ddd::service_ops::ServiceOpRoute],
    op: &str,
) -> Option<&'a crate::ddd::service_ops::ServiceOpRoute> {
    routes.iter().find(|route| route.op_name == op)
}

fn classify_arm(execution: &ExecutionKind) -> EmissionArm {
    match execution {
        ExecutionKind::Update { state: Some(state) } => EmissionArm::Transition(state.clone()),
        ExecutionKind::Update { state: None } => EmissionArm::Patch,
        // Slice (b) refuses statement bodies: their saves are multiple
        // `commands.update` calls, so one publish cannot be placed reliably
        // (the body seam is a recorded follow-up).
        ExecutionKind::Body { .. } => EmissionArm::NonPublishing("statement-body"),
        ExecutionKind::Create { .. } => EmissionArm::NonPublishing("create-shaped"),
        ExecutionKind::Delete => EmissionArm::NonPublishing("delete"),
        ExecutionKind::Read
        | ExecutionKind::ProjectionRead { .. }
        | ExecutionKind::ChildOp { .. } => EmissionArm::NonPublishing("read-only"),
        ExecutionKind::Seam { .. } => EmissionArm::Seam,
    }
}

/// Project the DTO plane's own context into the planner surface. Field names
/// and types are the generated `{Entity}Response` members — never a second
/// naming rule.
fn surface_from_context(
    context: &crate::ddd::dto::DtoContext,
    title: &str,
    domain: &str,
) -> ProducerEntitySurface {
    let mut fields: Vec<SurfaceField> = Vec::new();
    fields.push(SurfaceField {
        name: "id".to_string(),
        rust_type: "uuid::Uuid".to_string(),
        is_optional: false,
        is_array: false,
        is_entity_ref: false,
        is_codelist: false,
    });
    for field in &context.fields {
        if field.name == "id" {
            continue;
        }
        fields.push(SurfaceField {
            name: field.name.clone(),
            rust_type: field.rust_type.clone(),
            is_optional: !field.is_required,
            is_array: field.is_array,
            is_entity_ref: field.is_entity_ref,
            is_codelist: field.render_strategy == "codelist",
        });
    }
    for child in &context.child_dtos {
        fields.push(SurfaceField {
            name: child.field_name.clone(),
            rust_type: format!("{}Response", child.struct_name),
            is_optional: !child.is_array,
            is_array: child.is_array,
            is_entity_ref: false,
            is_codelist: false,
        });
    }
    if context.has_workflow {
        fields.push(SurfaceField {
            name: "workflow_state".to_string(),
            rust_type: format!("{}WorkflowState", context.entity_name),
            is_optional: true,
            is_array: false,
            is_entity_ref: false,
            is_codelist: false,
        });
    }
    for extra in &context.mox_derived_extras {
        if let Some(rust_type) = &extra.rust_type {
            fields.push(SurfaceField {
                name: extra.name.clone(),
                rust_type: rust_type.clone(),
                is_optional: false,
                is_array: false,
                is_entity_ref: false,
                is_codelist: false,
            });
        }
    }
    fields.push(SurfaceField {
        name: "created_at".to_string(),
        rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
        is_optional: false,
        is_array: false,
        is_entity_ref: false,
        is_codelist: false,
    });
    fields.push(SurfaceField {
        name: "updated_at".to_string(),
        rust_type: "chrono::DateTime<chrono::Utc>".to_string(),
        is_optional: false,
        is_array: false,
        is_entity_ref: false,
        is_codelist: false,
    });

    let children = context
        .all_child_dtos
        .iter()
        .map(|child| SurfaceChild {
            field_name: child.field_name.clone(),
            fields: child
                .fields
                .iter()
                .map(|field| SurfaceField {
                    name: field.name.clone(),
                    rust_type: field.rust_type.clone(),
                    is_optional: !field.is_required,
                    is_array: field.is_array,
                    is_entity_ref: field.is_entity_ref,
                    is_codelist: field.render_strategy == "codelist",
                })
                .collect(),
        })
        .collect();

    ProducerEntitySurface {
        domain: domain.to_string(),
        title: title.to_string(),
        rust_type_name: context.entity_name.clone(),
        fields,
        children,
        alt_identities: Vec::new(),
        state_field: None,
        state_type: None,
        has_read: context.operations.iter().any(|op| op == "read"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddd::body_lower::BodyPlan;

    #[test]
    fn statement_bodies_are_not_publishing_arms() {
        let plan = BodyPlan {
            block: String::new(),
            params: Vec::new(),
            raises: Vec::new(),
            state: None,
            body_manages_state: None,
            saves: true,
            assigned_fields: Vec::new(),
            may_refuse: false,
            responses: Vec::new(),
        };
        assert!(matches!(
            classify_arm(&ExecutionKind::Body {
                plan: Box::new(plan)
            }),
            EmissionArm::NonPublishing("statement-body")
        ));
    }
}
