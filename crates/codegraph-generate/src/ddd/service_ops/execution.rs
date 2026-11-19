use std::collections::HashMap;

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_naming::to_snake_case;

use crate::project_config::DtoKeyCasing;

use super::routes::bind_entity;
use super::transitions::{
    binding_spelling, current_state_expr, machine_transition_pairs, resolved_transitions,
    short_class_name, state_field_is_dto_updatable, wire_key,
};
use super::types::{ExecutionKind, FieldInfo, ServiceOpState, StateName};

/// The render context one route needs from the project configuration and
/// the graph (issue #662: the body lowering renders command/query calls, so
/// it needs the same names the handler template has).
pub struct RouteShape<'a> {
    /// The route's domain.
    pub domain: &'a str,
    /// The bound entity's schema title.
    pub entity_title: &'a str,
    /// The bound entity's Rust type name (`PayRunType`).
    pub entity_name: &'a str,
    /// The bound entity's module name (pg table name).
    pub entity_table: &'a str,
    /// The handler's path-parameter binding (`pay_run_id`).
    pub id_param: &'a str,
    /// `Some("parent_id")` for a nested child route (the scoped record load).
    pub scoped: Option<&'a str>,
    /// The auditable flag the query/command calls carry.
    pub auditable: bool,
    /// The generated domain-types crate name (`domain_types`).
    pub domain_types_crate: &'a str,
    /// The Update-DTO wire-key casing.
    pub dto_casing: DtoKeyCasing,
    /// The entity's properties.
    pub fields: &'a HashMap<String, FieldInfo>,
    /// The design-mapped operation set (create/read/update/delete/list).
    pub surface_ops: &'a [String],
    /// Whether the route performs the record load.
    pub record_load: bool,
    /// The entity's designed machine, when any.
    pub machine: Option<&'a phero_ir::ddd::StateMachine>,
    /// The declared error vocabulary a body may raise (DD7).
    pub raises: &'a crate::ddd::body_lower::RaiseTable,
    /// The declared action-body parameters.
    pub params: &'a [crate::ddd::body_lower::BodyParam],
    /// The operation's declared `throws` set (authored error names).
    pub throws: &'a [String],
}

/// The execution lowering. A checked statement body takes precedence
/// (issue #662); otherwise repository `delete`/`findById` builtins wire to
/// the command/query planes, `save`/`update` wire to the update command with
/// an optional machine transition applied, and everything else seams.
pub(super) fn seam(reason: impl Into<String>) -> ExecutionKind {
    ExecutionKind::Seam {
        reason: reason.into(),
    }
}

pub(super) fn execution_for(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    service: &str,
    shape: &RouteShape<'_>,
) -> Result<ExecutionKind, String> {
    // A checked body WINS over the delegation lowering (issue #662). The
    // upstream compiler attaches a body only to an operation that declares
    // one, so `bodies` non-empty is the author's explicit intent.
    if !op.bodies.is_empty() {
        return body_execution(op, model, config, service, shape);
    }
    let Some(target) = &op.delegation_target else {
        return Ok(seam(format!(
            "service op `{}` has no delegation target and no statement body — \
             abstract service operations have no executable lowering yet \
             (recorded follow-up)",
            op.name
        )));
    };
    let Some(target_repo) = model.repositories.iter().find(|r| &r.name == target) else {
        return Ok(seam(format!(
            "service op `{}` delegates to `{target}`, which is not a \
             repository — service-to-service and search delegation have \
             no executable lowering yet (recorded follow-up)",
            op.name
        )));
    };

    // Fail closed on cross-entity (child/aggregate) delegation. A public op
    // routing through a repository for a DIFFERENT design class would
    // otherwise patch the parent row and silently drop the child intent
    // (`addTimecardItem`, `punch`, `ingestHours`, assignment/deployment ops).
    // The diagnostic names the op, the target repository, and the target
    // class so the consumer can model a dedicated command.
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(shape.entity_title))
        .map(|d| d.class.as_str());
    if let Some(entity_class) = entity_class
        && target_repo.design_class != entity_class
    {
        // A binding names a transition on the DELEGATION target's machine;
        // when the route binds another aggregate that machine is not the one
        // the route can move — refuse rather than seam silently (issue
        // #660: an authored binding never degrades).
        if let Some(binding) = &op.transition {
            return Err(format!(
                "service op `{}` declares `transition {}` on repository `{target}` (class \
                 `{}`), but the operation binds entity `{}` (`{entity_class}`) — the \
                 binding's machine belongs to another aggregate",
                op.name,
                binding_spelling(binding),
                target_repo.design_class,
                shape.entity_title
            ));
        }
        return Ok(seam(format!(
            "service op `{}` delegates to repository `{}` for class `{}`, not the route \
             entity `{}` (`{entity_class}`) — a cross-aggregate delegation has no executable \
             lowering. Model the child mutation as a `children {header} {{ … => {feature}.<add|remove|save> }}` \
             block (pherolang child operations, whose ops carry `childScope`): those lower to \
             the nested child action route. Refusing to patch the parent row silently",
            op.name,
            target,
            target_repo.design_class,
            shape.entity_title,
            header = short_class_name(entity_class),
            feature = "<childFeature>",
        )));
    }

    let operation = op
        .delegation_operation
        .as_deref()
        .map(|o| o.to_ascii_lowercase())
        .unwrap_or_default();
    Ok(match operation.as_str() {
        "delete" if shape.surface_ops.iter().any(|o| o == "delete") => ExecutionKind::Delete,
        "findbyid" if shape.surface_ops.iter().any(|o| o == "read") => ExecutionKind::Read,
        "save" | "update" if shape.surface_ops.iter().any(|o| o == "update") => {
            if !shape.record_load {
                return Ok(seam(format!(
                    "service op `{}` delegates to repository `update`, which \
                     needs the loaded row, but `{}` exposes no read operation \
                     (no record load)",
                    op.name, shape.entity_title
                )));
            }
            let state = match shape.machine {
                Some(machine) => {
                    let field_snake = to_snake_case(&machine.field);
                    let entity_class = entity_class
                        .map(str::to_string)
                        .unwrap_or_else(|| shape.entity_title.to_string());
                    let Some(field) = shape
                        .fields
                        .get(&machine.field)
                        .or_else(|| shape.fields.get(&field_snake))
                    else {
                        // The machine binds a field the graph exposes no
                        // property for — a transition cannot be rendered.
                        let transitions = resolved_transitions(
                            op,
                            model,
                            machine,
                            shape.entity_title,
                            &entity_class,
                            shape.entity_table,
                        )?;
                        if transitions.is_empty() {
                            return Ok(ExecutionKind::Update { state: None });
                        }
                        return Ok(seam(format!(
                            "service op `{}` applies a machine transition, but the \
                             machine field `{}` is not a property of `{}`",
                            op.name, machine.field, shape.entity_title
                        )));
                    };
                    let transitions = resolved_transitions(
                        op,
                        model,
                        machine,
                        shape.entity_title,
                        &entity_class,
                        shape.entity_table,
                    )?;
                    if !state_field_is_dto_updatable(
                        config,
                        shape.domain,
                        shape.entity_title,
                        &field.rust_name,
                    ) {
                        // A state-only transition cannot ride the update DTO
                        // (workflow-managed/immutable field). Fail closed
                        // rather than silently no-op the action.
                        if !transitions.is_empty() {
                            return Ok(seam(format!(
                                "service op `{}` applies a machine transition on \
                                 `{}`, but the field is workflow-managed/immutable \
                                 and excluded from the update DTO (dedicated \
                                 transition lowering pending)",
                                op.name, field.rust_name
                            )));
                        }
                        None
                    } else {
                        Some(ServiceOpState {
                            current_expr: current_state_expr(field),
                            wire_key: wire_key(&field.rust_name, shape.dto_casing),
                            initial: StateName::new(machine.initial.clone()),
                            all: machine_transition_pairs(machine),
                            op: transitions,
                        })
                    }
                }
                None => {
                    // An authored binding on an entity with no machine has
                    // nothing to fire — never degrade it to a plain update
                    // (issue #660).
                    if let Some(binding) = &op.transition {
                        return Err(format!(
                            "service op `{}` declares `transition {}`, but the bound entity \
                             `{}` carries no lifecycle machine — the binding has nothing to fire",
                            op.name,
                            binding_spelling(binding),
                            shape.entity_title
                        ));
                    }
                    None
                }
            };
            ExecutionKind::Update { state }
        }
        other => seam(format!(
            "service op `{}` delegates to repository `{other}`, which has \
             no executable lowering yet (recorded follow-up)",
            op.name
        )),
    })
}

/// Lower a checked `.ddd` statement body (issue #662).
///
/// Every refusal is a HARD generation error naming the op and the construct
/// (the DD9 `query_refusals` posture): the body plane never emits a partially
/// interpreted handler, and never silently answers 501 for a body it cannot
/// honour.
fn body_execution(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    service: &str,
    shape: &RouteShape<'_>,
) -> Result<ExecutionKind, String> {
    let Some(json) = op.bodies.get("stmt") else {
        let targets: Vec<&str> = op.bodies.keys().map(String::as_str).collect();
        return Err(format!(
            "service op `{}`: the operation carries body target(s) `{}`, but the \
             action plane lowers the `stmt` target only",
            op.name,
            targets.join("`, `")
        ));
    };
    let body = crate::ddd::body_lower::parse_body(&op.name, json).map_err(|refusal| refusal.0)?;

    // The body orchestrates ONE aggregate: the route's own. A receiver for
    // another design class is the cross-aggregate load the upstream checker
    // refuses, re-checked here against the graph (a hand-built or stale
    // artifact cannot smuggle one past the plane).
    let Some(receiver) = op.body_receiver.as_deref() else {
        return Err(format!(
            "service op `{}`: the operation carries a statement body but no \
             `body_receiver` (the `=> <Repository>` the body binds as `self`); \
             regenerate the graph",
            op.name
        ));
    };
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(shape.entity_title))
        .map(|d| d.class.as_str());
    if let Some(entity_class) = entity_class {
        match model.repositories.iter().find(|r| r.name == receiver) {
            Some(repository) if repository.design_class == entity_class => {}
            Some(repository) => {
                return Err(format!(
                    "service op `{}`: the body orchestrates repository `{receiver}` \
                     for class `{}`, not the route entity `{}` (`{entity_class}`) — \
                     a body loads one aggregate, the one the action addresses",
                    op.name, repository.design_class, shape.entity_title
                ));
            }
            None => {
                return Err(format!(
                    "service op `{}`: the body receiver `{receiver}` is not a \
                     repository of the model — the body has no aggregate to \
                     orchestrate",
                    op.name
                ));
            }
        }
    }

    // The inferred machine transition survives only when the body leaves the
    // state field alone (the lowering drops it otherwise — the TODO(#662)
    // boundary). No transitions selected ⇒ no patch at all: unlike the
    // deployment `save` leg, a body op never gets the body-driven target
    // whitelist (an action body must not move the machine arbitrarily).
    let transition = match shape.machine {
        Some(machine) => body_transition_state(op, model, config, machine, shape)?,
        None => {
            if let Some(binding) = &op.transition {
                return Err(format!(
                    "service op `{}` declares `transition {}`, but the bound entity `{}` \
                     carries no lifecycle machine — the binding has nothing to fire",
                    op.name,
                    binding_spelling(binding),
                    shape.entity_title
                ));
            }
            None
        }
    };
    // The declared `throws` vocabulary (authored names from the phero-ir
    // `ErrorRef` JSON) — a raise outside it is a refusal, even though the
    // upstream compiler already proves it.
    let throws: Vec<String> = op
        .throws
        .iter()
        .filter_map(|json| json.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect();
    let scope = crate::ddd::body_lower::BodyScope {
        op_name: &op.name,
        service,
        domain: shape.domain,
        entity_name: shape.entity_name,
        module_name: shape.entity_table,
        id_param: shape.id_param,
        scoped: shape.scoped,
        auditable: shape.auditable,
        domain_types_crate: shape.domain_types_crate,
        dto_casing: shape.dto_casing,
        fields: shape.fields,
        state_field: shape
            .machine
            .and_then(|machine| shape.fields.get(&machine.field))
            .map(|field| field.rust_name.as_str()),
        transition: transition.as_ref(),
        raises: shape.raises,
        params: shape.params,
        throws: &throws,
        receiver: Some(receiver),
        has_update: shape.surface_ops.iter().any(|o| o == "update"),
        record_load: shape.record_load,
    };
    let plan = crate::ddd::body_lower::lower_body(&scope, &body).map_err(|refusal| refusal.0)?;
    Ok(ExecutionKind::Body {
        plan: Box::new(plan),
    })
}

/// The machine transition a bodied op applies when its body leaves the state
/// field alone. `None` when the machine field is not a property, the field is
/// workflow-managed/immutable, or the op matches no declared transition (a
/// bodied op gets NO body-driven target whitelist). An authored binding that
/// cannot be resolved is a HARD error, never `None` (issue #660: a binding
/// never degrades silently).
fn body_transition_state(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    config: &DomainConfig,
    machine: &phero_ir::ddd::StateMachine,
    shape: &RouteShape<'_>,
) -> Result<Option<ServiceOpState>, String> {
    let field_snake = to_snake_case(&machine.field);
    let Some(field) = shape
        .fields
        .get(&machine.field)
        .or_else(|| shape.fields.get(&field_snake))
    else {
        if let Some(binding) = &op.transition {
            return Err(format!(
                "service op `{}` declares `transition {}`, but the machine field `{}` is \
                 not a property of `{}` — the binding cannot be rendered",
                op.name,
                binding_spelling(binding),
                machine.field,
                shape.entity_title
            ));
        }
        return Ok(None);
    };
    if !state_field_is_dto_updatable(config, shape.domain, shape.entity_title, &field.rust_name) {
        if let Some(binding) = &op.transition {
            return Err(format!(
                "service op `{}` declares `transition {}`, but the machine field `{}` is \
                 workflow-managed/immutable and excluded from the update DTO — the binding \
                 cannot be rendered",
                op.name,
                binding_spelling(binding),
                field.rust_name
            ));
        }
        return Ok(None);
    }
    let entity_class = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(shape.entity_title))
        .map(|d| d.class.clone())
        .unwrap_or_else(|| shape.entity_title.to_string());
    let transitions = resolved_transitions(
        op,
        model,
        machine,
        shape.entity_title,
        &entity_class,
        shape.entity_table,
    )?;
    if transitions.is_empty() {
        return Ok(None);
    }
    Ok(Some(ServiceOpState {
        current_expr: current_state_expr(field),
        wire_key: wire_key(&field.rust_name, shape.dto_casing),
        initial: StateName::new(machine.initial.clone()),
        all: machine_transition_pairs(machine),
        op: transitions,
    }))
}

/// [`bind_entity`] scoped to one already-found service.
pub(super) async fn bind_entity_service_scoped(
    models: &[codegraph_core::types::DddModelGraph],
    service_name: &str,
    op: &codegraph_core::types::DddServiceOperation,
    db: &dyn GraphQuerier,
    type_suffix: &str,
) -> Option<String> {
    let model = models
        .iter()
        .find(|m| m.services.iter().any(|s| s.name == service_name))?;
    bind_entity(model, op, db, type_suffix).await
}

/// Whether an operation consults the machine at all: a `save`/`update`
/// delegation (the shapes that patch the loaded row) or a statement body
/// (whose inferred transition rides the same lowering). Everything else —
/// delete/read delegations, abstract ops, seams — applies no transition, so
/// a missing binding on those is not a diagnostic.
pub(super) fn consults_the_machine(op: &codegraph_core::types::DddServiceOperation) -> bool {
    if !op.bodies.is_empty() {
        return true;
    }
    matches!(
        op.delegation_operation
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("save") | Some("update")
    )
}
