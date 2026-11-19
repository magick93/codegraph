use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_naming::to_snake_case;

use crate::ddd::design::DddDesignSurface;

use super::execution::{bind_entity_service_scoped, consults_the_machine};
use super::routes::{BodyRenderContext, bind_entity, routes_for_entity, schema_title_for_class};
use super::transitions::{binding_spelling, heuristic_transitions, resolve_transition_binding};

/// The IFML dead-trigger scan (Ask 1): every `.ifml` action whose name
/// binds (ASCII-case-insensitive, the pherolang `check_action_bindings`
/// convention) a service op that emits no API surface — `protected`, or
/// entity-unbound — is a dead button. Returns `(action, op, reason)`.
pub async fn ifml_dead_triggers(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<(String, String, String)>, GraphError> {
    let mut triggers = Vec::new();
    let Ok(actions) = db.get_ifml_actions().await else {
        return Ok(triggers);
    };
    if actions.is_empty() {
        return Ok(triggers);
    }
    let models = db.get_ddd_models().await?;
    if models.iter().all(|m| m.services.is_empty()) {
        return Ok(triggers);
    }
    let type_suffix = config.defaults.type_suffix.clone();
    for action in &actions {
        let Some((service, op)) = models.iter().find_map(|model| {
            for service in &model.services {
                for op in &service.operations {
                    if op.name.eq_ignore_ascii_case(&action.name) {
                        return Some((service.name.clone(), op.clone()));
                    }
                }
            }
            None
        }) else {
            // Binds no service op at all: the upstream binding check owns
            // that class of defect.
            continue;
        };
        if op.is_protected {
            triggers.push((
                action.name.clone(),
                op.name.clone(),
                "the op is `protected` (Sculptor visibility)".to_string(),
            ));
            continue;
        }
        let bound = bind_entity_service_scoped(&models, &service, &op, db, &type_suffix).await;
        if bound.is_none() {
            triggers.push((
                action.name.clone(),
                op.name.clone(),
                "the op binds no entity (no capability class, repository                  delegation, or class-typed signature)"
                    .to_string(),
            ));
        }
    }
    Ok(triggers)
}

/// Authored transition bindings that cannot be resolved (issue #660): the
/// HARD half of the binding diagnostics.
///
/// Upstream already validates a binding against the target repository's
/// designed machine, so on a freshly compiled artifact this list is empty —
/// it exists for the hand-built/stale-graph class (the same posture as
/// [`body_refusals`] and DD9's `query_refusals`), where a binding would
/// otherwise be resolved against the wrong machine, or silently dropped
/// after the tree was half-written. The run path turns each entry into a
/// `ValidationPass` ERROR before generation.
///
/// Each entry names the op, the binding as authored, and the machines
/// involved. Empty = every binding resolves.
pub async fn transition_binding_errors(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model
            .services
            .iter()
            .all(|service| service.operations.iter().all(|op| op.transition.is_none()))
    }) {
        return Ok(Vec::new());
    }
    let surface = match DddDesignSurface::from_graph(db).await {
        Ok(surface) => surface,
        Err(_) => return Ok(Vec::new()),
    };
    let type_suffix = config.defaults.type_suffix.clone();
    let mut errors = Vec::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                let Some(binding) = &op.transition else {
                    continue;
                };
                if op.is_protected || !consults_the_machine(op) {
                    continue;
                }
                let Some(title) = bind_entity(model, op, db, &type_suffix).await else {
                    // Unbound ops emit no route at all; the dead-surface
                    // warning owns that class.
                    continue;
                };
                let Some(machine) = surface.machine_for(&title) else {
                    errors.push(format!(
                        "service op `{}::{}` declares `transition {}`, but the bound entity \
                         `{title}` carries no lifecycle machine — the binding has nothing to fire",
                        service.name,
                        op.name,
                        binding_spelling(binding)
                    ));
                    continue;
                };
                if let Err(reason) =
                    resolve_transition_binding(op, binding, model, &machine, &title)
                {
                    // The resolver does not know the owning service; name
                    // it the same way the no-machine branch does.
                    errors.push(reason.replacen(
                        &format!("service op `{}`", op.name),
                        &format!("service op `{}::{}`", service.name, op.name),
                        1,
                    ));
                }
            }
        }
    }
    errors.sort();
    errors.dedup();
    Ok(errors)
}

/// One WARN per operation whose machine transition is INFERRED (issue
/// #660): the no-binding fallback stays for models that predate the
/// `transition` syntax, but every op it resolves must name the binding it
/// lacks, so the inference is visible in the run log instead of silently
/// re-selecting a transition after a rename.
///
/// Empty when every machine-bound op declares its binding (and when the
/// graph carries no machines).
pub async fn transition_binding_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    // No machine ⇒ nothing can be inferred, so the walk (which resolves the
    // entity binding per op) is skipped entirely: the common model pays no
    // graph reads here.
    if !models
        .iter()
        .any(|model| model.designs.iter().any(|d| d.machine_json.is_some()))
    {
        return Ok(Vec::new());
    }
    let surface = match DddDesignSurface::from_graph(db).await {
        Ok(surface) => surface,
        Err(_) => return Ok(Vec::new()),
    };
    let type_suffix = config.defaults.type_suffix.clone();
    let mut warnings = Vec::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.transition.is_some() || op.is_protected || !consults_the_machine(op) {
                    continue;
                }
                let Some(title) = bind_entity(model, op, db, &type_suffix).await else {
                    continue;
                };
                let Some(machine) = surface.machine_for(&title) else {
                    continue;
                };
                let entity_class = model
                    .designs
                    .iter()
                    .find(|d| d.resolved_title.as_deref() == Some(title.as_str()))
                    .map(|d| d.class.clone())
                    .unwrap_or_else(|| title.clone());
                let table = db
                    .get_schema(&title)
                    .await
                    .ok()
                    .flatten()
                    .map(|s| s.pg_table_name)
                    .unwrap_or_else(|| to_snake_case(&title));
                let inferred = heuristic_transitions(op, model, &machine, &entity_class, &table);
                if inferred.is_empty() {
                    continue;
                }
                let pairs = inferred
                    .iter()
                    .map(|t| format!("{} -> {}", t.from, t.to))
                    .collect::<Vec<_>>()
                    .join(", ");
                warnings.push(format!(
                    "service op `{}::{}` applies the machine transition {pairs} of `{title}` \
                     by inference: the operation declares no `transition` binding; add \
                     `transition <Trigger>` (or `transition <EnumType>.<Trigger>` in a \
                     multi-machine module) to pin it",
                    service.name, op.name,
                ));
            }
        }
    }
    warnings.sort();
    warnings.dedup();
    Ok(warnings)
}

/// Every statically-unlowerable statement body (issue #662): the DD9
/// `query_refusals` posture, one plane over. A body construct outside the
/// documented subset is a HARD generation error — the run path turns these
/// into `ValidationPass` errors BEFORE any tree is written, because the
/// domain-generator phase collects its own errors non-fatally (a refusal
/// surfacing there would half-generate).
///
/// Each entry names the service op and the construct. Empty = every bodied
/// op lowers.
pub async fn body_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model
            .services
            .iter()
            .all(|service| service.operations.iter().all(|op| op.bodies.is_empty()))
    }) {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.bodies.is_empty() || op.is_protected {
                    continue;
                }
                if let Some(title) = bind_entity(model, op, db, &type_suffix).await {
                    titles.insert(title);
                }
            }
        }
    }
    let mut refusals = Vec::new();
    for title in titles {
        let Ok(Some(schema)) = db.get_schema(&title).await else {
            continue;
        };
        let Some(domain) = schema.domain.clone() else {
            continue;
        };
        if !config.domains.contains_key(domain.as_str()) {
            continue;
        }
        if let Err(reason) =
            routes_for_entity(db, config, &domain, &title, &BodyRenderContext::default()).await
        {
            refusals.push(reason);
        }
    }
    refusals.sort();
    refusals.dedup();
    Ok(refusals)
}

/// Injected-scope `when` guards (issue #663): every guarded operation's
/// injected paths must resolve (schema, domain, query slot, exactly-one
/// FK id source) and the whole condition must lower. The same refusal
/// class the generation path raises — this pre-generation pass is what
/// makes it fail BEFORE any tree is written (the `body_refusals`
/// precedent).
pub async fn guard_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    workers_topology: bool,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model.services.iter().all(|service| {
            service.operations.iter().all(|op| {
                !op.condition_paths.iter().any(|path| {
                    matches!(
                        path.scope,
                        codegraph_core::types::DddConditionScope::Injected { .. }
                    )
                })
            })
        })
    }) {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                let guarded = op.condition_paths.iter().any(|path| {
                    matches!(
                        path.scope,
                        codegraph_core::types::DddConditionScope::Injected { .. }
                    )
                });
                if !guarded || op.is_protected {
                    continue;
                }
                if let Some(title) = bind_entity(model, op, db, &type_suffix).await {
                    titles.insert(title);
                }
            }
        }
    }
    let mut refusals = Vec::new();
    for title in titles {
        let Ok(Some(schema)) = db.get_schema(&title).await else {
            continue;
        };
        let Some(domain) = schema.domain.clone() else {
            continue;
        };
        if !config.domains.contains_key(domain.as_str()) {
            continue;
        }
        let body = BodyRenderContext {
            workers_topology,
            ..Default::default()
        };
        if let Err(reason) = routes_for_entity(db, config, &domain, &title, &body).await {
            refusals.push(reason);
        }
    }
    refusals.sort();
    refusals.dedup();
    Ok(refusals)
}

/// The Ask 1 diagnostic: service operations that emit no API surface —
/// `protected` ops and ops that bind no entity. Returned strings are
/// human-readable warnings (empty = clean).
pub async fn dead_surface_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    let mut warnings = Vec::new();
    let type_suffix = config.defaults.type_suffix.clone();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected {
                    warnings.push(format!(
                        "service op `{}::{}` is `protected` — it emits no API \
                         surface (Sculptor visibility); IFML actions bound to \
                         it are dead triggers",
                        service.name, op.name
                    ));
                    continue;
                }
                if bind_entity(model, op, db, &type_suffix).await.is_none() {
                    warnings.push(format!(
                        "service op `{}::{}` binds no entity (no capability \
                         class, repository delegation, or class-typed \
                         signature) — it emits no API surface",
                        service.name, op.name
                    ));
                }
            }
        }
        // Aggregate child operations (issue #661): a `children <Class>` op
        // with no mounted route is dead surface too — `protected` ops and
        // ops whose child class is not a schema in the graph (the nesting /
        // lowering misses warn at generation time, where the child's plane
        // is known).
        for design in &model.designs {
            for op in &design.child_operations {
                if op.is_protected {
                    warnings.push(format!(
                        "child operation `{}::{}` is `protected` — it emits no API \
                         surface (Sculptor visibility); IFML actions bound to it \
                         are dead triggers",
                        design.class, op.name
                    ));
                    continue;
                }
                if let Some(scope) = &op.child_scope
                    && schema_title_for_class(db, &scope.class, &type_suffix)
                        .await
                        .is_none()
                {
                    warnings.push(format!(
                        "child operation `{}::{}` names child class `{}`, which \
                         resolves to no ingested schema — it emits no API surface",
                        design.class, op.name, scope.class
                    ));
                }
            }
        }
    }
    Ok(warnings)
}
