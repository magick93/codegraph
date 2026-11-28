use codegraph_config::DomainConfig;
use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_naming::to_snake_case;

use crate::ddd::design::DddDesignSurface;

use super::execution::{bind_entity_service_scoped, consults_the_machine};
use super::routes::{BodyRenderContext, bind_entity, routes_for_entity, schema_title_for_class};
use super::transitions::{binding_spelling, heuristic_transitions, resolve_transition_binding};

/// The Ask 1 binding rule: an `.ifml` action binds an operation by
/// ASCII-case-insensitive name equality — the pherolang
/// `check_action_bindings` convention (IFML actions are user-facing
/// PascalCase labels, operations camelCase) and the one the IFML action
/// planner resolves routes by.
pub fn op_binds_action(op_name: &str, action_name: &str) -> bool {
    op_name.eq_ignore_ascii_case(action_name)
}

/// One operation as the protected-op join sees it, whatever plane it was
/// read from (graph readbacks on the run path, compiled models in doctor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpRef<'a> {
    /// The owner: a service name, or the aggregate root's class for a
    /// child operation. The qualified label the warning carries is
    /// `{owner}::{name}`.
    pub owner: &'a str,
    /// The operation name, as authored.
    pub name: &'a str,
    /// The `protected` modifier.
    pub protected: bool,
}

impl<'a> OpRef<'a> {
    /// The qualified spelling the dead-trigger line names the op by.
    pub fn qualified(&self) -> String {
        format!("{}::{}", self.owner, self.name)
    }
}

/// The protected-op half of the Ask 1 join (issue #619) — ONE function for
/// the run path (graph readbacks, [`ifml_dead_triggers`]) and `codegraph
/// doctor` (compiled models), so the rule, the precedence, the labels and
/// the reasons cannot drift. For `action`, the protected operation it
/// binds, as `(qualified op, reason)`; `None` when the action binds a
/// PUBLIC service op, or no protected operation at all.
///
/// Service operations outrank child operations of the same name (the
/// action plane resolves service ops first), and a public child op is
/// never a trigger — it emits a route or the generation-time warns cover
/// the unmounted shapes.
pub fn protected_binding<'a>(
    action: &str,
    service_ops: impl Iterator<Item = OpRef<'a>>,
    child_ops: impl Iterator<Item = OpRef<'a>>,
) -> Option<(String, &'static str)> {
    let bind = |op: OpRef<'_>, reason: &'static str| op.protected.then(|| (op.qualified(), reason));
    for op in service_ops {
        if op_binds_action(op.name, action) {
            return bind(op, PROTECTED_OP_REASON);
        }
    }
    for op in child_ops {
        if op_binds_action(op.name, action) {
            return bind(op, PROTECTED_CHILD_OP_REASON);
        }
    }
    None
}

/// The dead-trigger reason for a `protected` service op (Sculptor
/// visibility: the op stays off the public interface).
pub const PROTECTED_OP_REASON: &str = "the op is `protected` (Sculptor visibility)";

/// The dead-trigger reason for a `protected` aggregate child operation
/// (issue #661): the same visibility, one plane over.
pub const PROTECTED_CHILD_OP_REASON: &str =
    "the child operation is `protected` (Sculptor visibility)";

/// The dead-trigger reason for a public op the run cannot route.
const UNBOUND_OP_REASON: &str =
    "the op binds no entity (no capability class, repository delegation, or class-typed signature)";

/// The Ask 1 dead-trigger line — ONE shape shared by the run-path
/// validation pass (`validate.rs::check_service_op_surface`) and
/// `codegraph doctor`, so the two surfaces cannot drift (issue #619).
pub fn dead_trigger_message(action: &str, op: &str, reason: &str) -> String {
    format!(
        "action '{action}' targets op '{op}' which emits no API surface ({reason}); \
         the trigger is dead"
    )
}

/// The service operation `action` binds by name, with its owning service.
fn service_op_named<'a>(
    models: &'a [codegraph_core::types::DddModelGraph],
    action: &str,
) -> Option<(&'a str, &'a codegraph_core::types::DddServiceOperation)> {
    models.iter().find_map(|model| {
        model.services.iter().find_map(|service| {
            service
                .operations
                .iter()
                .find(|op| op_binds_action(&op.name, action))
                .map(|op| (service.name.as_str(), op))
        })
    })
}

/// The IFML dead-trigger scan (Ask 1): every `.ifml` action whose name
/// binds an op that emits no API surface is a dead button. Returns
/// `(action, op, reason)` per trigger.
///
/// Since issue #619 this scan is the ONLY place a `protected` op is
/// warned about: Sculptor `protected` is a valid, intentional visibility,
/// so an op nobody bound is by design and [`dead_surface_warnings`]
/// stays silent about it. The two classes reported here:
///
/// - **`protected`** — a service op, or an aggregate child operation no
///   service op outranks (the IFML action planner resolves actions against
///   `routes_for_entity`, which carries child-op routes, so an action
///   naming a protected child op is the same dead button). `op` carries the
///   qualified `Service::op` / `RootClass::op` spelling, because this line
///   is the whole report for the op;
/// - **entity-unbound** — a public op with no capability class, repository
///   delegation, or class-typed signature (the unchanged #590 shape).
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
    if models
        .iter()
        .all(|m| m.services.is_empty() && m.designs.iter().all(|d| d.child_operations.is_empty()))
    {
        return Ok(triggers);
    }
    let service_ops = || {
        models.iter().flat_map(|model| {
            model.services.iter().flat_map(|service| {
                service.operations.iter().map(move |op| OpRef {
                    owner: service.name.as_str(),
                    name: op.name.as_str(),
                    protected: op.is_protected,
                })
            })
        })
    };
    let child_ops = || {
        models.iter().flat_map(|model| {
            model.designs.iter().flat_map(|design| {
                design.child_operations.iter().map(move |op| OpRef {
                    owner: design.class.as_str(),
                    name: op.name.as_str(),
                    protected: op.is_protected,
                })
            })
        })
    };
    let type_suffix = config.defaults.type_suffix.clone();
    for action in &actions {
        if let Some((op, reason)) = protected_binding(&action.name, service_ops(), child_ops()) {
            triggers.push((action.name.clone(), op, reason.to_string()));
            continue;
        }
        // Either no service op is named (an action binding no operation at
        // all is the upstream binding check's class of defect), or a PUBLIC
        // service op is: that is the entity-binding question.
        let Some((service, op)) = service_op_named(&models, &action.name) else {
            continue;
        };
        let bound = bind_entity_service_scoped(&models, service, op, db, &type_suffix).await;
        if bound.is_none() {
            triggers.push((
                action.name.clone(),
                op.name.clone(),
                UNBOUND_OP_REASON.to_string(),
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
    // Issue #767: a create-shaped op declares NO statement body, so the
    // body-only gate would skip the very plane whose refusals must fail the
    // run BEFORE any tree is written. Any op carrying a modeled-boundary
    // parameter is relevant too (body params, the create body, the child
    // planes), and a child op's payload maps through the same code.
    let relevant = |op: &codegraph_core::types::DddServiceOperation| {
        !op.bodies.is_empty()
            || op.params.iter().any(|param| {
                crate::ddd::signature::signature_type(&param.type_json)
                    .dto_name()
                    .is_some()
            })
    };
    let child_ops_exist = models.iter().any(|model| {
        model
            .designs
            .iter()
            .any(|design| !design.child_operations.is_empty())
    });
    if !child_ops_exist
        && models.iter().all(|model| {
            model
                .services
                .iter()
                .all(|service| service.operations.iter().all(|op| !relevant(op)))
        })
    {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected || !relevant(op) {
                    continue;
                }
                if let Some(title) = bind_entity(model, op, db, &type_suffix).await {
                    titles.insert(title);
                }
            }
        }
        // A child op is declared on the ROOT's design but lowers into the
        // CHILD's routes; the child title is what `routes_for_entity` must
        // walk to surface a payload refusal (an `add`-only child is covered
        // by `check_child_add_parent_load`, a `save`-only child would not be).
        for design in &model.designs {
            for op in &design.child_operations {
                let Some(scope) = op.child_scope.as_ref() else {
                    continue;
                };
                if let Some(title) = schema_title_for_class(db, &scope.class, &type_suffix).await {
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

/// Declared `throws` vocabularies (DD7, issue #665): every operation's
/// `throws` entry must resolve against the shared error derivation — a
/// prelude name or a declared variant the generated `{Domain}Error` plane
/// carries. An entry naming nothing is a NAMED refusal (the
/// `body_refusals` posture): re-running `routes_for_entity` per
/// throws-carrying entity surfaces the exact op and entry before any tree
/// is written.
pub async fn throws_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model
            .services
            .iter()
            .all(|service| service.operations.iter().all(|op| op.throws.is_empty()))
            && model.designs.iter().all(|design| {
                design
                    .child_operations
                    .iter()
                    .all(|op| op.throws.is_empty())
            })
    }) {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        // DD7 (issue #665, review N4): a throws-carrying CHILD op refuses
        // through the child's own entity — the child pass of
        // `routes_for_entity` collects and resolves its vocabulary. The
        // service-op loop below cannot see that title, so the child scope
        // resolves it here.
        for design in &model.designs {
            for op in &design.child_operations {
                if op.throws.is_empty() || op.is_protected {
                    continue;
                }
                let Some(scope) = op.child_scope.as_ref() else {
                    continue;
                };
                if let Some(title) =
                    super::routes::schema_title_for_class(db, &scope.class, &type_suffix).await
                {
                    titles.insert(title);
                }
            }
        }
        for service in &model.services {
            for op in &service.operations {
                if op.throws.is_empty() || op.is_protected {
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

/// The collection-level child `add`'s parent load (issue #692): every
/// lowered `add` must resolve the parent's plain by-id query slot at
/// planning (the handler renders a parent load-or-404 before the create,
/// so the method it calls must provably exist). The same refusal class the
/// generation path raises — this pre-generation pass is what makes it fail
/// BEFORE any tree is written (the `body_refusals` / `guard_refusals`
/// posture).
pub async fn child_parent_load_refusals(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<String>, GraphError> {
    let models = db.get_ddd_models().await?;
    if models.iter().all(|model| {
        model.designs.iter().all(|design| {
            design
                .child_operations
                .iter()
                .all(|op| op.child_scope.as_ref().map(|scope| scope.action.as_str()) != Some("add"))
        })
    }) {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        for design in &model.designs {
            for op in &design.child_operations {
                let Some(scope) = &op.child_scope else {
                    continue;
                };
                if op.is_protected || scope.action != "add" {
                    continue;
                }
                if let Some(title) = schema_title_for_class(db, &scope.class, &type_suffix).await {
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

/// The Ask 1 diagnostic for the class that is a DEFECT: service operations
/// that bind no entity, and child operations whose child class resolves to
/// no schema — public surface the run cannot route. Returned strings are
/// human-readable warnings (empty = clean).
///
/// `protected` ops are NOT reported here (issue #619): Sculptor
/// `protected` is a valid, intentional visibility, so an op nobody bound is
/// by design. A protected op an `.ifml` action binds IS a defect (the model
/// contradicts itself) and is reported — exactly once, naming the action —
/// by [`ifml_dead_triggers`].
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
                    // Off the public interface by design; it is not an
                    // entity-binding question (`ifml_dead_triggers` owns
                    // the bound case).
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
        // whose child class is not a schema in the graph is dead surface
        // too (the nesting / lowering misses warn at generation time, where
        // the child's plane is known). A `protected` child op is by design
        // — `ifml_dead_triggers` owns the bound case.
        for design in &model.designs {
            for op in &design.child_operations {
                if op.is_protected {
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

/// API-key authorization diagnostics (issue #635), Warning severity on the
/// run path — never fatal. One aggregate line per finding class:
///
/// - `api_key_mode = "bypass"`: the service ops carrying a grant `when`
///   or a `never_both` conflict on some role arm — the gates API-key
///   sessions skip under the documented v1 semantics.
/// - `api_key_mode = "agent"`: the capabilities no agent-admitting grant
///   holds — every op gated by one answers 403 to every API key.
///
/// Silent when the graph carries no actor policy / no capability-gated
/// service op (the bypass is then moot).
pub async fn api_key_authz_warnings(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    authz: crate::profile::ApiKeyAuthz,
) -> Result<Vec<String>, GraphError> {
    let Some(surface) = super::policy::PolicySurface::from_graph(db, authz).await? else {
        return Ok(Vec::new());
    };
    let models = db.get_ddd_models().await?;
    let type_suffix = config.defaults.type_suffix.clone();
    let mut titles: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.capabilities.is_empty() || op.is_protected {
                    continue;
                }
                if let Some(title) = bind_entity(model, op, db, &type_suffix).await {
                    titles.insert(title);
                }
            }
        }
    }
    let body = BodyRenderContext {
        api_key_authz: authz,
        ..Default::default()
    };
    let mut skipped: Vec<String> = Vec::new();
    let mut unreachable: std::collections::BTreeSet<String> = Default::default();
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
        // A lowering refusal is reported by its own check.
        let Ok(routes) = routes_for_entity(db, config, &domain, &title, &body).await else {
            continue;
        };
        for route in routes.iter().filter(|r| !r.capabilities.is_empty()) {
            match authz.mode {
                crate::profile::ApiKeyMode::Bypass => {
                    // Mirrors the renderer: a `never_both` conflict binds
                    // a role arm only when that arm holds BOTH sides.
                    let conflicts = surface.sod_conflicts(&route.capabilities);
                    let sod = surface.roles.values().any(|permits| {
                        route.capabilities.iter().any(|c| permits.contains_key(c))
                            && conflicts.iter().any(|c| permits.contains_key(c))
                    });
                    let when_gated = surface.roles.values().any(|permits| {
                        route.capabilities.iter().any(|c| {
                            permits
                                .get(c)
                                .is_some_and(|whens| !whens.iter().any(Option::is_none))
                        })
                    });
                    if sod || when_gated {
                        skipped.push(format!("{}::{}", route.service, route.op_name));
                    }
                }
                crate::profile::ApiKeyMode::Agent => {
                    if let super::ApiKeyGate::Denied { reason } = &route.api_key
                        && !reason.starts_with("separation of duties")
                    {
                        unreachable.extend(route.capabilities.iter().cloned());
                    }
                }
            }
        }
    }
    skipped.sort();
    skipped.dedup();
    let mut warnings = Vec::new();
    if !skipped.is_empty() {
        warnings.push(format!(
            "api_key_mode = \"bypass\": API-key sessions skip the grant `when` / \
             `never_both` gates of {} service op(s) ({}) — the documented v1 \
             semantics; set api_key_mode = \"agent\" in profiles.toml [features] to \
             enforce them on keys (issue #635)",
            skipped.len(),
            skipped.join(", ")
        ));
    }
    if !unreachable.is_empty() {
        warnings.push(format!(
            "api_key_mode = \"agent\": no agent-admitting grant holds {} — every \
             service op gated by {} answers 403 to API-key sessions (grant it \
             without a `human` modifier, or `permit agent …`; issue #635)",
            unreachable
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", "),
            if unreachable.len() == 1 { "it" } else { "them" }
        ));
    }
    Ok(warnings)
}
