use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::PropertyNode;
use codegraph_naming::to_snake_case;
use phero_expr::{BinOp, Expr, ExprKind, UnOp};

use crate::ddd::design::DddDesignSurface;
use crate::project_config::DtoKeyCasing;

use super::execution::{RouteShape, execution_for, seam};
use super::policy::{
    PolicySurface, render_authorize_fn_with_injected, render_authorize_fn_with_states,
};
use super::transitions::short_class_name;
use super::types::{
    ChildAction, ChildParentLoad, ExecutionKind, FreshFixtureGuard, InjectedGuard, ServiceOpRoute,
    fields_for,
};
use super::when_lower::{InjectedMachine, InjectedScope};

// ── Graph resolution ───────────────────────────────────────────────────

/// The project-side render context a body-carrying route needs (issue #662):
/// the domain-types crate name its command/query calls spell, and the
/// Update-DTO wire-key casing its patch keys.
#[derive(Debug, Clone)]
pub struct BodyRenderContext {
    /// The generated domain-types crate (`domain_types`).
    pub domain_types_crate: String,
    /// The Update-DTO wire-key casing.
    pub dto_casing: DtoKeyCasing,
    /// The deployment topology is `workers` (issue #663): an injected-scope
    /// `when` guard refuses. The #590 action handlers run in per-domain
    /// worker crates whose `AppState` carries only its OWN domain's query
    /// slots, so a cross-domain injected load (`state.{other_domain}_…`)
    /// renders a field no worker crate has — E0599 at consumer build time,
    /// silently. Same-domain loads would compile, but the plane refuses
    /// both until the worker media/async story exists (#338's "guards stay
    /// doc-only under workers" conservatism). Default `false`.
    pub workers_topology: bool,
}

impl Default for BodyRenderContext {
    /// The plane's default names — the fallback for the read-side callers
    /// (IFML e2e planning, router registration) that never emit the body
    /// block itself.
    fn default() -> Self {
        Self {
            domain_types_crate: "domain_types".to_string(),
            dto_casing: DtoKeyCasing::default(),
            workers_topology: false,
        }
    }
}

impl BodyRenderContext {
    /// The context of one project configuration.
    #[must_use]
    pub fn from_project(project: &crate::ProjectConfig) -> Self {
        Self {
            domain_types_crate: project.identity.domain_types_crate.clone(),
            dto_casing: project.codegen.dto_key_casing,
            workers_topology: project.is_workers_topology(),
        }
    }
}

/// The declared `{Domain}Error` variants a body raise may name (DD7).
async fn raise_table(
    db: &dyn GraphQuerier,
    domain: &str,
) -> Result<crate::ddd::body_lower::RaiseTable, String> {
    let declared = crate::ddd::errors::declared_error_defs(db, domain)
        .await
        .map_err(|e| e.to_string())?;
    let mut table = crate::ddd::body_lower::RaiseTable::default();
    for error in declared {
        table.declared.insert(
            error.variant_name.clone(),
            crate::ddd::body_lower::DeclaredError {
                variant: error.variant_name,
                code: error.code,
                status: error.http_status.max(0) as u16,
            },
        );
    }
    Ok(table)
}

/// Resolve a class name (possibly `pkg::Name` or `pkg.Name`) to a schema
/// title: exact first, then the configured type suffix (the
/// `schema_for_class` convention).
pub(super) async fn schema_title_for_class(
    db: &dyn GraphQuerier,
    class: &str,
    type_suffix: &str,
) -> Option<String> {
    let unqualified = short_class_name(class);
    if let Ok(Some(schema)) = db.get_schema(&unqualified).await {
        return Some(schema.title);
    }
    let suffixed = format!("{unqualified}{type_suffix}");
    if let Ok(Some(schema)) = db.get_schema(&suffixed).await {
        return Some(schema.title);
    }
    None
}

/// Resolve the service operations bound to one entity. Deterministic: models
/// in readback order, module-major, declaration order; ops binding the same
/// entity under one snake name dedup keeping the first.
pub async fn routes_for_entity(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    entity_title: &str,
    body: &BodyRenderContext,
) -> Result<Vec<ServiceOpRoute>, String> {
    let models = db.get_ddd_models().await.map_err(|e| e.to_string())?;
    // A model may carry aggregate child operations with no service at all
    // (issue #661) — the presence gate must see both planes.
    if models
        .iter()
        .all(|m| m.services.is_empty() && m.designs.iter().all(|d| d.child_operations.is_empty()))
    {
        return Ok(Vec::new());
    }
    let type_suffix = config.defaults.type_suffix.clone();
    let props = db
        .get_properties(entity_title)
        .await
        .map_err(|e| e.to_string())?;
    let fields = fields_for(&props);
    let surface = PolicySurface::from_graph(db)
        .await
        .map_err(|e| e.to_string())?;
    let Some(schema) = db
        .get_schema(entity_title)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(Vec::new());
    };
    let response_type = format!("{}Response", schema.rust_type_name);
    let module_name = schema.pg_table_name.clone();
    let entity_cfg = config
        .domains
        .get(domain)
        .and_then(|entry| entry.get_entity_config(entity_title));
    // The handler's own names, derived from the SAME inputs the handler
    // generator uses (`resolve_path_segment` + the path-segment→param-name
    // rule) so a body plan's command/query calls line up with the route.
    let id_param = crate::api::router::param_name_from_path_segment(
        &crate::api::api_model::resolve_path_segment(entity_cfg, &schema),
    );
    // The route's parent anchor mirrors the ROUTER's Source-0/Source-1
    // precedence (issue #666 slice 4b): an authored member nests under its
    // authored root (authored wins over a config parent); a class PROVEN
    // root by a member's authored declaration stays top-level even when
    // config declares it a child. Undecided boundaries keep the config
    // shape exactly.
    let authored_routing =
        crate::ddd::aggregate::authored_routing_parent(db, config, domain, entity_title).await;
    let authored_proven_root = authored_routing.is_none()
        && crate::ddd::aggregate::authored_root_proven(
            db,
            &config.defaults.type_suffix,
            entity_title,
        )
        .await;
    let scoped = if authored_proven_root {
        None
    } else if authored_routing.is_some() {
        Some("parent_id")
    } else {
        (entity_cfg.and_then(|cfg| cfg.role.as_deref()) == Some("child")).then_some("parent_id")
    };
    let auditable = crate::api::handler::resolve_is_auditable(db, entity_title, config, domain)
        .await
        .map_err(|e| e.to_string())?;

    // The design surface gates the execution legs: delegation targets must
    // survive the #449 op-set mapping (delete → commands.delete, findById
    // → queries.find_by_id) to lower. The same surface carries the machine
    // (P0.5): `save`/`update` delegations bind a transition when the op
    // matches one, and the op-level `when` lowering binds declared states.
    let design_surface = DddDesignSurface::from_graph(db).await.ok();
    let surface_ops = design_surface
        .as_ref()
        .and_then(|s| s.operations_for(entity_title))
        .unwrap_or_default();
    let machine = design_surface
        .as_ref()
        .and_then(|s| s.machine_for(entity_title));
    let machine_states: Vec<String> = machine
        .as_ref()
        .map(|m| m.states().into_iter().map(str::to_string).collect())
        .unwrap_or_default();
    // The declared raise vocabulary a statement body lowers against (issue
    // #662). Read only when a bodied op exists — a model without bodies pays
    // no extra graph read.
    let raises = if models.iter().any(|model| {
        model
            .services
            .iter()
            .any(|service| service.operations.iter().any(|op| !op.bodies.is_empty()))
    }) {
        raise_table(db, domain).await?
    } else {
        crate::ddd::body_lower::RaiseTable::default()
    };

    let mut routes: Vec<ServiceOpRoute> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for model in &models {
        for service in &model.services {
            for op in &service.operations {
                if op.is_protected {
                    continue;
                }
                let Some(bound) = bind_entity(model, op, db, &type_suffix).await else {
                    continue;
                };
                if bound != entity_title || !seen.insert(op.name.clone()) {
                    continue;
                }
                let snake = to_snake_case(&op.name);
                // The route's record load rides the #449 invariant: the
                // entity's query plane carries `find_by_id` iff its design
                // surface exposes `read`. Capability clauses can bind
                // non-root entities (e.g. a PaySlip/`Coverage` noun), which
                // expose no read — those routes render without the record
                // load and with a response-less authorize fn (crewbase#71;
                // #595 follow-up). `when`-conditioned ops on such entities
                // are a generation error (fail closed).
                let record_load = surface_ops.iter().any(|o| o == "read");
                // Issue #663: resolve the op's injected-scope `when` paths
                // into the per-scope plans the rendered gate loads (schema,
                // domain, query slot, FK) plus the class fields the
                // comparison lowers over. Unresolvable shapes are named
                // generation errors — never a silently narrower guard.
                let injected =
                    resolve_injected_scopes(db, config, entity_title, op, &props, &design_surface)
                        .await?;
                // Issue #663, workers topology: the #590 action handlers
                // run in per-domain worker crates whose AppState carries
                // only its own domain's query slots — a cross-domain
                // injected load renders a field no worker crate has
                // (E0599 at consumer build, silently). The plane refuses
                // both scopes until the worker async-AppState story
                // exists (the #338 "guards stay doc-only under workers"
                // conservatism).
                if body.workers_topology && !injected.is_empty() {
                    return Err(format!(
                        "service op `{}` carries an injected-scope `when` guard, but the \
                         deployment topology is `workers`: the action handlers run in \
                         per-domain worker crates whose AppState carries only its own \
                         domain's query slots, so the injected load cannot address the \
                         `{}` aggregate — use the monolith topology for injected guards \
                         (a recorded #663 deferral, not a silent break)",
                        op.name,
                        injected
                            .first()
                            .map(|scope| scope.root.as_str())
                            .unwrap_or("injected")
                    ));
                }
                let injected_guard = injected_guard_facts(op, &injected);
                let authorize_fn = render_authorize_fn_with_injected(
                    &snake,
                    &op.name,
                    &service.name,
                    &op.capabilities,
                    op.condition.as_deref(),
                    &format!(
                        "crate::domain::{domain}::{module_name}::dto_response::{response_type}"
                    ),
                    surface.as_ref(),
                    &fields,
                    entity_title,
                    record_load,
                    &machine_states,
                    &injected,
                )?;
                let params = if op.bodies.is_empty() {
                    Vec::new()
                } else {
                    crate::ddd::body_lower::body_params(&op.name, &op.params)
                        .map_err(|refusal| refusal.0)?
                };
                let throws: Vec<String> = op
                    .throws
                    .iter()
                    .filter_map(|json| json.get("name").and_then(serde_json::Value::as_str))
                    .map(str::to_string)
                    .collect();
                let shape = RouteShape {
                    domain,
                    entity_title,
                    entity_name: &schema.rust_type_name,
                    entity_table: &module_name,
                    id_param: &id_param,
                    scoped,
                    auditable,
                    domain_types_crate: &body.domain_types_crate,
                    dto_casing: body.dto_casing,
                    fields: &fields,
                    surface_ops: &surface_ops,
                    record_load,
                    machine: machine.as_ref(),
                    raises: &raises,
                    params: &params,
                    throws: &throws,
                };
                let execution = execution_for(op, model, config, &service.name, &shape)?;
                routes.push(ServiceOpRoute {
                    service: service.name.clone(),
                    op_name: op.name.clone(),
                    fn_name: format!("service_op_{snake}"),
                    snake,
                    capabilities: op.capabilities.clone(),
                    authorize_fn,
                    execution,
                    record_load,
                    authorize_async: !injected.is_empty(),
                    injected_guard,
                    invariant_block: None,
                });
            }
        }
    }

    // ── Aggregate child operations (issue #661) ─────────────────────────
    //
    // A `children <Root> { <op> => <feature>.<action> }` block is declared on
    // the ROOT's design but ACTS on the child: `add` appends to the root's
    // child collection, `remove` deletes one element, `save` updates one.
    // The route is therefore emitted into the CHILD entity's router — the
    // parent nests that router (`POST …/{parent}/{parent_id}/{child}/…`), so
    // the parent ownership load and the nested path are the SAME WS4 shape
    // the child CRUD plane already generates; nothing here invents a second
    // nesting. This pass collects the ops whose `childScope` resolves to the
    // entity being generated.
    for model in &models {
        for design in &model.designs {
            for op in &design.child_operations {
                let Some(scope) = op.child_scope.as_ref() else {
                    continue;
                };
                let Some(action) = ChildAction::from_scope(&scope.action) else {
                    crate::ddd::design::warn_once(
                        &format!("child-op-action:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}`: unknown child action `{}` (the \
                             closed set is add, remove, save); no route emitted",
                            op.name, design.class, scope.action
                        ),
                    );
                    continue;
                };
                // Resolution may be exact, `+type_suffix`, or a miss (the
                // class is not a schema in the graph).
                let Some(child_title) =
                    schema_title_for_class(db, &scope.class, &type_suffix).await
                else {
                    crate::ddd::design::warn_once(
                        &format!("child-op-class:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}`: child class `{}` resolves to no \
                             ingested schema; no route emitted",
                            op.name, design.class, scope.class
                        ),
                    );
                    continue;
                };
                if child_title != entity_title {
                    continue;
                }
                if !seen.insert(op.name.clone()) {
                    continue;
                }
                let Some(parent_title) = design.resolved_title.as_deref() else {
                    continue;
                };
                if op.is_protected {
                    crate::ddd::design::warn_once(
                        &format!("child-op-protected:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}` is `protected` — it emits no API \
                             surface (Sculptor visibility); IFML actions bound to it are \
                             dead triggers",
                            op.name, design.class
                        ),
                    );
                    continue;
                }
                if !schema.is_entity {
                    continue;
                }
                // The child's plane only nests under the parent when the
                // MODEL says so: config `role = "child"` + `parent` +
                // `parent_ref`, or — since issue #666 slice 4b — the AUTHORED
                // aggregate boundary (an authored membership naming this
                // root; authored wins over config). The parent ownership
                // load, the two-uuid path, and the scoped read all derive
                // from it. Without either there is no aggregated route to
                // emit — warn (never a top-level route that silently drops
                // the parent anchor).
                if !is_nested_child_of(
                    entity_cfg,
                    parent_title,
                    &design.class,
                    &type_suffix,
                    authored_routing.as_ref().map(|p| p.parent_title.as_str()),
                ) {
                    crate::ddd::design::warn_once(
                        &format!("child-op-nesting:{}::{}", design.class, op.name),
                        &format!(
                            "child operation `{}` of `{}`: child entity `{}` is not declared \
                             as a nested child of `{}` (domains.toml `role = \"child\"` + \
                             `parent = \"{}\"` + `parent_ref`); no route emitted",
                            op.name, design.class, entity_title, parent_title, parent_title
                        ),
                    );
                    continue;
                }
                let record_load = match action {
                    // `add` addresses the collection, not a child row.
                    ChildAction::Add => false,
                    ChildAction::Remove | ChildAction::Save => {
                        surface_ops.iter().any(|o| o == "read")
                    }
                };
                let execution = child_execution(op, action, scope, &surface_ops, record_load);
                // Issue #692: a lowered collection-level `add` resolves the
                // parent's by-id load at planning — the handler renders a
                // parent load-or-404 before the create, so the plan must
                // carry the parent's query slot, auditable arity and the
                // fail-closed proof that the method exists (a parent whose
                // query plane exposes no plain `find_by_id`, and a
                // cross-domain parent, are NAMED generation errors raised
                // by `child_parent_load` — never a handler referencing a
                // method that does not exist). Seamed adds render the 501
                // arm and never reach the create, so they pay no parent
                // resolution.
                let execution = match execution {
                    ExecutionKind::ChildOp {
                        action,
                        feature,
                        child_class,
                        read_back,
                        ..
                    } if action == ChildAction::Add => ExecutionKind::ChildOp {
                        action,
                        feature,
                        child_class,
                        read_back,
                        parent_load: Some(
                            child_parent_load(db, config, domain, parent_title, &op.name).await?,
                        ),
                    },
                    ExecutionKind::ChildOp {
                        action,
                        feature,
                        child_class,
                        read_back,
                        ..
                    } => ExecutionKind::ChildOp {
                        action,
                        feature,
                        child_class,
                        read_back,
                        parent_load: None,
                    },
                    other => other,
                };
                // Issue #663: injected-scope guards are a SERVICE-op plane
                // lowering — a child op's `when` types against the CHILD
                // class as `self`, so an injected path there has no id
                // source (the child's own path is the route anchor). A
                // named generation error, never a silently narrower guard.
                if op.condition_paths.iter().any(|path| {
                    matches!(
                        path.scope,
                        codegraph_core::types::DddConditionScope::Injected { .. }
                    )
                }) {
                    return Err(format!(
                        "child operation `{}` of `{}` carries an injected-scope \
                         `when` path — the child plane lowers own-record guards \
                         only (the child row IS the guard's record); bind the \
                         cross-entity guard on a root service operation instead",
                        op.name, design.class
                    ));
                }
                // The policy surface (capability membership + the op-level
                // `when`, which types against the CHILD class as `self`).
                // `add` carries no loaded record, so a guard there was seamed
                // out above; the response-less authorize fn still gates the
                // capability.
                let authorize_fn = if !op.capabilities.is_empty() || op.condition.is_some() {
                    render_authorize_fn_with_states(
                        &to_snake_case(&op.name),
                        &op.name,
                        parent_title,
                        &op.capabilities,
                        if record_load {
                            op.condition.as_deref()
                        } else {
                            None
                        },
                        &format!(
                            "crate::domain::{domain}::{module_name}::dto_response::{response_type}"
                        ),
                        surface.as_ref(),
                        &fields,
                        entity_title,
                        record_load,
                        &machine_states,
                    )?
                } else {
                    None
                };
                let snake = to_snake_case(&op.name);
                routes.push(ServiceOpRoute {
                    // The declaration site: a child op's `service` names the
                    // aggregate ROOT design whose `children` block declared
                    // it (there is no `service` in the artifact).
                    service: short_class_name(&design.class),
                    op_name: op.name.clone(),
                    fn_name: format!("service_op_{snake}"),
                    snake,
                    capabilities: op.capabilities.clone(),
                    authorize_fn,
                    execution,
                    record_load,
                    authorize_async: false,
                    injected_guard: None,
                    invariant_block: None,
                });
            }
        }
    }

    Ok(routes)
}

// ── Injected-scope `when` guards (issue #663) ──────────────────────────

/// Resolve an op's injected-scope `when` paths (issue #663) into the
/// per-scope plans the rendered gate loads: the injected class's schema
/// (exact → `+type_suffix`), its owning domain and query slot, the
/// auditable `find_by_id` arity, and the own-record FK carrying the id.
/// Every unresolvable shape is a named generation error — the closed
/// subset refuses, it never silently narrows. An op with no injected
/// paths resolves to an empty vec without touching the graph again.
#[allow(clippy::too_many_arguments)]
async fn resolve_injected_scopes(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity_title: &str,
    op: &codegraph_core::types::DddServiceOperation,
    props: &[PropertyNode],
    design_surface: &Option<DddDesignSurface>,
) -> Result<Vec<InjectedScope>, String> {
    let type_suffix = config.defaults.type_suffix.clone();
    // Distinct injected scopes, in the condition's declaration order.
    let mut requested: Vec<(String, String)> = Vec::new();
    for path in &op.condition_paths {
        if let codegraph_core::types::DddConditionScope::Injected { repository, class } =
            &path.scope
            && !requested.iter().any(|(r, c)| r == repository && c == class)
        {
            requested.push((repository.clone(), class.clone()));
        }
    }
    if requested.is_empty() {
        return Ok(Vec::new());
    }
    let mut scopes = Vec::with_capacity(requested.len());
    for (repository, class) in &requested {
        let Some(title) = schema_title_for_class(db, class, &type_suffix).await else {
            return Err(format!(
                "service op `{}`: the `when` guard's injected path `{repository}.…` names \
                 class `{class}`, which resolves to no schema in the graph — the injected \
                 repository's design must bind a modeled class",
                op.name
            ));
        };
        let Some(injected_schema) = db.get_schema(&title).await.map_err(|e| e.to_string())? else {
            return Err(format!(
                "service op `{}`: the injected class `{class}` resolved to `{title}`, \
                 which is not in the graph",
                op.name
            ));
        };
        let Some(injected_domain) = injected_schema.domain.clone() else {
            return Err(format!(
                "service op `{}`: the injected class `{class}` (`{title}`) carries no \
                 domain on its schema — the AppState query slot cannot be resolved",
                op.name
            ));
        };
        // A child-role injected entity exposes no plain `find_by_id` (its
        // plane is parent-scoped) — refuse with the modeling fix named.
        // The AUTHORED child (issue #666 slice 4b) is parent-scoped too:
        // its query plane carries the parent-scoped read through the same
        // authored nesting the router and handler consume.
        let injected_cfg = config
            .domains
            .get(&injected_domain)
            .and_then(|entry| entry.get_entity_config(&title));
        let injected_authored =
            crate::ddd::aggregate::authored_routing_parent(db, config, &injected_domain, &title)
                .await;
        if injected_cfg.and_then(|cfg| cfg.role.as_deref()) == Some("child")
            || injected_authored.is_some()
        {
            return Err(format!(
                "service op `{}`: the injected repository `{repository}` binds `{title}`, a \
                 child-role entity whose query plane is parent-scoped (`find_by_id_scoped`) — \
                 an injected `when` path cannot address it; bind the guard on the root \
                 aggregate's service operation instead",
                op.name
            ));
        }
        // The injected plane must expose `read` — the guard's load rides
        // the repository plane's own query (the #449 invariant).
        let surface_ops = design_surface
            .as_ref()
            .and_then(|s| s.operations_for(&title))
            .unwrap_or_default();
        if !surface_ops.iter().any(|o| o == "read") {
            return Err(format!(
                "service op `{}`: the injected repository `{repository}` binds `{title}`, \
                 whose design surface exposes no `read` — the guard's load has no query to \
                 ride; declare `findById` on the injected repository",
                op.name
            ));
        }
        // The id source: exactly ONE scalar entity-ref feature on the op's
        // entity targeting the injected class. Zero or ambiguous is a
        // named error — the guard has no honest record to load.
        let class_leaf = short_class_name(class);
        let fks: Vec<&PropertyNode> = props
            .iter()
            .filter(|p| {
                !p.is_array
                    && p.effective_kind()
                        == Some(codegraph_type_contracts::RefClassificationKind::EntityReference)
                    && p.ref_target.as_deref().is_some_and(|target| {
                        target == title
                            || target == class_leaf
                            || format!("{class_leaf}{type_suffix}") == target
                    })
            })
            .collect();
        let fk = match fks.as_slice() {
            [] => {
                return Err(format!(
                    "service op `{}`: the `when` guard reads `{repository}.…` (class \
                     `{class_leaf}`), but its bound entity `{entity_title}` carries no \
                     `{class_leaf}`-typed reference — the guard has no id source; add a \
                     `refers {class_leaf} timecard`-style feature to `{entity_title}`",
                    op.name
                ));
            }
            [only] => (only.rust_field_name.clone(), only.is_nullable),
            many => {
                let names: Vec<String> = many.iter().map(|p| p.rust_field_name.clone()).collect();
                return Err(format!(
                    "service op `{}`: `{entity_title}` carries more than one \
                     `{class_leaf}`-typed reference (`{}`) — the injected guard's id source is \
                     ambiguous; point the guard at exactly one reference (or a dedicated \
                     service parameter, a recorded follow-up)",
                    op.name,
                    names.join("`, `")
                ));
            }
        };
        let injected_fields = db.get_properties(&title).await.map_err(|e| e.to_string())?;
        let machine = design_surface.as_ref().and_then(|s| s.machine_for(&title));
        scopes.push(InjectedScope {
            root: repository.clone(),
            local: to_snake_case(repository),
            slot: format!("{injected_domain}_{}", injected_schema.pg_table_name),
            auditable: crate::api::handler::resolve_is_auditable(
                db,
                &title,
                config,
                &injected_domain,
            )
            .await
            .map_err(|e| e.to_string())?,
            fk_field: fk.0,
            fk_optional: fk.1,
            fields: fields_for(&injected_fields),
            machine: machine.map(|m| InjectedMachine {
                states: m.states().into_iter().map(str::to_string).collect(),
                field: m.field,
                initial: m.initial,
            }),
        });
    }
    Ok(scopes)
}

/// The compact guard facts the hurl contract's planner needs (issue #663):
/// the THREE-verdict model — [`FreshFixtureGuard::Satisfied`] (every
/// comparison machine-decided and passing on a fresh row), [`FreshFixtureGuard::Denied`]
/// (an optional FK — the fresh row's FK is NULL and the gate denies before
/// evaluating — or a machine-decided comparison failing on the fresh row),
/// or [`FreshFixtureGuard::Unverifiable`] (any comparison the collector
/// does not fully understand). `None` when the op carries no injected
/// scopes.
pub(super) fn injected_guard_facts(
    op: &codegraph_core::types::DddServiceOperation,
    scopes: &[InjectedScope],
) -> Option<InjectedGuard> {
    if scopes.is_empty() {
        return None;
    }
    let source = op.condition.as_deref()?;
    let verdict = |fresh: FreshFixtureGuard| {
        Some(InjectedGuard {
            condition: source.to_string(),
            fresh_fixture: fresh,
        })
    };
    // An optional FK denies BEFORE any comparison is evaluated: the create
    // body omits nullable props, so the fresh row's FK is NULL and the
    // rendered `let Some(..) else` fires. Provable 403 regardless of the
    // condition's content.
    if scopes.iter().any(|scope| scope.fk_optional) {
        return verdict(FreshFixtureGuard::Denied);
    }
    let parsed = phero_expr::parse(source);
    let Some(ast) = parsed.ast.filter(|_| parsed.errors.is_empty()) else {
        return verdict(FreshFixtureGuard::Unverifiable);
    };
    let mut comparisons = Vec::new();
    // Only comparisons under PLAIN AND from the root are collected (a
    // failing one there decides the whole guard); negation, `||`
    // composition and arithmetic set `fully_understood = false` and their
    // subtrees are not pushed — uncollected ⇒ Unverifiable (omit, never
    // guess).
    let mut fully_understood = true;
    collect_injected_comparisons(&ast, scopes, &mut comparisons, &mut fully_understood, true);
    // Every pushed comparison is machine-decidable (the collector filters);
    // a decided comparison FAILING on the fresh row means, under AND
    // composition (the only composition collected), the guard is false.
    let machine_decided: Vec<bool> = comparisons
        .iter()
        .map(|(scope_index, _, op, literal)| {
            let machine = scopes[*scope_index]
                .machine
                .as_ref()
                .expect("the collector only pushes machine-decidable comparisons");
            match op {
                BinOp::Eq => literal == &machine.initial,
                BinOp::Ne => literal != &machine.initial,
                _ => false,
            }
        })
        .collect();
    if machine_decided.iter().any(|passes| !passes) {
        return verdict(FreshFixtureGuard::Denied);
    }
    if !fully_understood || comparisons.is_empty() {
        return verdict(FreshFixtureGuard::Unverifiable);
    }
    verdict(FreshFixtureGuard::Satisfied)
}

/// The `(scope index, field, operator, literal)` triples of every injected
/// eq/ne comparison the planner can decide. ONLY comparisons under PLAIN
/// AND from the root are pushed, and ONLY machine-decidable ones (the
/// scope carries a machine and the field is its machine field): anything
/// else in a plain-AND context — ordering comparisons (`hours > 100`),
/// same-scope field-to-field, float/bool/date literals, non-machine
/// fields — is a conjunct that can fail on the fresh row while the
/// collected set reads Satisfied, so it sets `fully_understood = false`
/// and the planner's verdict becomes Unverifiable: the contract OMITS the
/// leg — a guess in either direction fails the suite just as hard.
/// Negation flips polarity and `||` may pass via the own-record side, so
/// their whole subtrees are undecided too. Pure own-record leaf
/// comparisons are skipped without marking: the planner's injected
/// verdict does not reason about them (the own-machine state is the
/// existing state-aware planner's input; a non-transition op's own-record
/// conjuncts are the documented pre-#663 surface).
#[allow(clippy::too_many_arguments)]
fn collect_injected_comparisons(
    expr: &Expr,
    scopes: &[InjectedScope],
    out: &mut Vec<(usize, String, BinOp, String)>,
    fully_understood: &mut bool,
    plain_and: bool,
) {
    // A decidable literal is a bare name (an enum member), a string, or an
    // integer — everything else (floats, bools, dates, arithmetic) leaves
    // the fresh value undecidable.
    let literal_side = |e: &Expr| -> Option<String> {
        match &e.kind {
            ExprKind::Name(name) => Some(name.clone()),
            ExprKind::String(text) => Some(text.clone()),
            ExprKind::Int(value) => Some(value.to_string()),
            _ => None,
        }
    };
    // Machine-decidable at COLLECT time: the scope carries a machine and
    // the field is its machine field. A non-machine Eq/Ne conjunct's fresh
    // value is fixture-seeded (required ints seed 42, strings seed
    // `contract-{table}`, codelists seed values[0]) — deciding it at the
    // fold would guess.
    let machine_decidable = |index: usize, field: &str| -> bool {
        scopes[index].machine.as_ref().is_some_and(|machine| {
            let snake = to_snake_case(&machine.field);
            let names = [machine.field.as_str(), snake.as_str()];
            names.contains(&field)
        })
    };
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => {
            let lhs_injected = injected_scope_of(lhs, scopes);
            let rhs_injected = injected_scope_of(rhs, scopes);
            let touches = lhs_injected.is_some() || rhs_injected.is_some();
            if plain_and && matches!(*op, BinOp::Eq | BinOp::Ne) && touches {
                // An injected-side Eq/Ne conjunct under plain AND: it can
                // decide the whole guard — understand it or mark undecided.
                // The operands of a comparison are values, not conjuncts —
                // nothing more to collect either way.
                if let (Some((index, field)), Some(literal)) = (lhs_injected, literal_side(rhs)) {
                    // NOTE: `literal` may be any bare Name — including an
                    // OWN-record field when a future relaxation lifts the
                    // cross-scope field-to-field refusal (the lowering
                    // refuses that pair today, so this push is unreachable
                    // for it). If that refusal ever lifts, re-derive the
                    // freshness reasoning here — a decided-but-meaningless
                    // comparison would silently mis-verdict (#749's
                    // follow-up ledger).
                    if machine_decidable(index, &field) {
                        out.push((index, field, *op, literal));
                    } else {
                        *fully_understood = false;
                    }
                    return;
                }
                if let (Some(literal), Some((index, field))) = (literal_side(lhs), rhs_injected) {
                    // Flip so the literal is compared against the field.
                    let flipped = match op {
                        BinOp::Eq => BinOp::Eq,
                        _ => BinOp::Ne,
                    };
                    if machine_decidable(index, &field) {
                        out.push((index, field, flipped, literal));
                    } else {
                        *fully_understood = false;
                    }
                    return;
                }
                // An injected comparison the collector cannot represent —
                // ordering, same-scope field-to-field, float/bool/date
                // literals, cross-scope pairs — is a dropped conjunct that
                // can fail on the fresh row while the collected set reads
                // Satisfied. Undecided, never dropped-silent.
                *fully_understood = false;
                return;
            }
            if plain_and && touches {
                // An ordering (or otherwise non-Eq/Ne) injected-side
                // conjunct under plain AND: the fresh value may be derivable
                // (required ints seed 42) but the planner does not model it
                // — undecided, never dropped-silent. The operands are
                // values, not conjuncts — nothing more to collect.
                *fully_understood = false;
                return;
            }
            if !touches && !matches!(*op, BinOp::And | BinOp::Or) {
                // A pure own-record LEAF comparison (its operands are
                // values — nothing injected anywhere in it): skip without
                // marking. AND/OR nodes always recurse: their operands ARE
                // the conjuncts.
                return;
            }
            match op {
                // `||`: the guard may pass via the own-record side — the
                // collected set no longer decides the whole condition, and
                // nothing under it is plain-AND from the root.
                BinOp::Or => *fully_understood = false,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => *fully_understood = false,
                _ => {}
            }
            let child_and = plain_and && matches!(*op, BinOp::And);
            collect_injected_comparisons(lhs, scopes, out, fully_understood, child_and);
            collect_injected_comparisons(rhs, scopes, out, fully_understood, child_and);
        }
        ExprKind::Unary { op, expr } => {
            // Negation flips polarity: an un-negated collect would decide
            // the comparison upside down, and nothing under it is
            // plain-AND from the root.
            if matches!(op, UnOp::Not) {
                *fully_understood = false;
            }
            collect_injected_comparisons(expr, scopes, out, fully_understood, false);
        }
        _ => {}
    }
}

/// The injected scope `expr`'s FEATURE-ACCESS root names, if it is a bare
/// `Root.field` chain (the collect shape).
fn injected_scope_of(expr: &Expr, scopes: &[InjectedScope]) -> Option<(usize, String)> {
    let ExprKind::FeatureAccess {
        receiver,
        name,
        optional_safe: false,
    } = &expr.kind
    else {
        return None;
    };
    let ExprKind::Name(root) = &receiver.kind else {
        return None;
    };
    let index = scopes.iter().position(|scope| &scope.root == root)?;
    Some((index, name.value.clone()))
}

/// Whether the entity nests under `parent_title`: config `role = "child"`
/// with `parent` and `parent_ref` (the WS4 contract), or — since issue
/// #666 slice 4b — the design's AUTHORED aggregate membership naming this
/// root. Authored WINS over config; an authored parent that names a
/// DIFFERENT root leaves the config check to decide, and the WARN walker
/// records the disagreement.
fn is_nested_child_of(
    entity_cfg: Option<&codegraph_config::EntityConfig>,
    parent_title: &str,
    parent_class: &str,
    type_suffix: &str,
    authored_parent_title: Option<&str>,
) -> bool {
    // Authored first: the authored root title is pre-resolved in-domain.
    if authored_parent_title == Some(parent_title) {
        return true;
    }
    let Some(cfg) = entity_cfg else {
        return false;
    };
    if cfg.role.as_deref() != Some("child") || cfg.parent_ref.is_none() {
        return false;
    }
    let Some(parent) = cfg.parent.as_deref() else {
        return false;
    };
    // The authored `parent` names the parent entity's title (the config
    // convention) but may use the bare class name — accept the class, its
    // `+type_suffix` spelling, and the resolved title.
    let parent_leaf = short_class_name(parent_class);
    parent == parent_title
        || parent == parent_leaf
        || format!("{parent}{type_suffix}") == parent_title
        || parent == format!("{parent_leaf}{type_suffix}")
}

/// The execution lowering for one aggregate child operation (issue #661).
///
/// A child action drives the CHILD entity's command plane:
///
/// - `add` → the child's create command (create-and-link into the parent's
///   child collection: the command writes the parent back-ref from the path);
/// - `save` → the child's update command over the parent-scoped record;
/// - `remove` → the child's delete command over the parent-scoped record.
///
/// Shapes that cannot be bound FAITHFULLY answer the 501 seam naming the
/// fix — never a silently broader write, never a patch of the parent row.
fn child_execution(
    op: &codegraph_core::types::DddServiceOperation,
    action: ChildAction,
    scope: &codegraph_core::types::DddChildRef,
    surface_ops: &[String],
    record_load: bool,
) -> ExecutionKind {
    let required = match action {
        ChildAction::Add => "create",
        ChildAction::Save => "update",
        ChildAction::Remove => "delete",
    };
    if !surface_ops.iter().any(|o| o == required) {
        return seam(format!(
            "child operation `{}` (action `{}`) needs the child's `{}` operation, but the \
             child's design exposes no `{}` — its repository must declare `{}` for the \
             action to lower",
            op.name,
            action.as_str(),
            required,
            required,
            match action {
                ChildAction::Add => "save",
                ChildAction::Save => "save",
                ChildAction::Remove => "delete",
            }
        ));
    }
    // A child action binds its input from the request BODY: zero parameters,
    // or exactly one object-shaped carrier (the child class or a DTO
    // boundary). A scalar/collection parameter has no honest binding — the
    // child id rides the route path, and everything else is a payload.
    if let Some(param) = op.params.first()
        && (op.params.len() > 1 || !is_body_carrier(param, &scope.class))
    {
        return seam(format!(
            "child operation `{}` declares parameter `{}` ({}), which has no executable \
             binding: a child action takes its payload from the request body — write \
             the parameter as the child class or a DTO boundary, and let the child id \
             ride the route path",
            op.name,
            param.name,
            body_carrier_hint(&param.type_json)
        ));
    }
    if op.condition.is_some() {
        if action == ChildAction::Add {
            return seam(format!(
                "child operation `{}` declares a `when` guard, but `add` runs before the \
                 child row exists — the guard types against a record that is not there \
                 (guard a `save`/`remove` action, or validate in the payload)",
                op.name
            ));
        }
        if !record_load {
            return seam(format!(
                "child operation `{}` declares a `when` guard, but the child entity exposes \
                 no `read` operation — the guard has no loaded record to evaluate",
                op.name
            ));
        }
    }
    ExecutionKind::ChildOp {
        action,
        feature: scope.feature.clone(),
        child_class: scope.class.clone(),
        read_back: surface_ops.iter().any(|o| o == "read"),
        // The parent load rides the child-op loop in `routes_for_entity`
        // (issue #692) — it resolves the parent's query slot with graph
        // access this sync lowering does not have, and only for a LOWERED
        // add (a seamed add renders the 501 arm, never the create).
        parent_load: None,
    }
}

/// The parent load-or-404 plan for one lowered collection-level child
/// `add` (issue #692). The `add` writes the child row with the parent's FK
/// taken from the path, so the handler renders a parent load-or-404 BEFORE
/// the create: the parent's own RLS-scoped query plane answers `None` for
/// a bogus or foreign-org parent, and the route turns that into 404 —
/// never Postgres `23503` as a 500 (the domain error classifier maps only
/// scope/role/RLS markers).
///
/// Fail closed, the #663 refusal posture: the plan resolves the parent's
/// query slot AT GENERATION TIME with the exact rule the query plane uses
/// to emit `find_by_id` (`has_create or (has_read and not parent_ref)` —
/// the `uses_find_by_id` computation in `templates/ddd/query.tera`). A
/// parent whose resolved op set exposes no plain by-id load, and a
/// cross-domain parent (child ops are same-domain by construction), are
/// NAMED generation errors — never a handler that references a method the
/// query plane does not carry.
async fn child_parent_load(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    domain: &str,
    parent_title: &str,
    op_name: &str,
) -> Result<super::types::ChildParentLoad, String> {
    let Some(parent_schema) = db
        .get_schema(parent_title)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Err(format!(
            "child operation `{op_name}`: the aggregate root `{parent_title}` resolves to no \
             schema in the graph — the collection add's parent load has no query slot to ride"
        ));
    };
    let parent_domain = parent_schema
        .domain
        .clone()
        .unwrap_or_else(|| domain.to_string());
    if parent_domain != domain {
        return Err(format!(
            "child operation `{op_name}`: the aggregate root `{parent_title}` lives in domain \
             `{parent_domain}`, but the child op routes in `{domain}` — child ops are \
             same-domain by construction, and the worker AppState carries only its own \
             domain's query slots (the #663 posture); declare the child under the root's \
             domain"
        ));
    }
    // The parent's resolved op set — the SAME resolver the query plane's
    // context builds from (`resolve_entity_operations`, which applies the
    // #449 design-surface override internally, title- and rust-name-keyed).
    let parent_ops = crate::api::api_model::resolve_entity_operations(
        db,
        config,
        &parent_domain,
        &parent_schema.title,
    )
    .await;
    let parent_cfg = config
        .domains
        .get(&parent_domain)
        .and_then(|entry| entry.get_entity_config(&parent_schema.title));
    let parent_candidates = db
        .get_parent_candidates()
        .await
        .map_err(|e| e.to_string())?;
    let parent_ref = crate::fk::resolve_parent_fk_column_same_domain(
        &parent_schema.title,
        &parent_candidates,
        parent_cfg,
        &parent_domain,
        config,
        db,
    )
    .await;
    let has_find_by_id = parent_ops.iter().any(|o| o == "create")
        || (parent_ops.iter().any(|o| o == "read") && parent_ref.is_none());
    if !has_find_by_id {
        return Err(format!(
            "child operation `{op_name}`: the collection add loads the parent aggregate \
             `{}` before its create (a bogus or foreign-org parent must answer 404, never \
             a 500), but the parent `{parent_title}`'s query plane exposes no plain \
             `find_by_id` — it emits only when the parent resolves `create`, or `read` \
             without a parent-scoped FK. Declare `findById` (or `save`) on the parent's \
             repository",
            parent_schema.rust_type_name
        ));
    }
    Ok(ChildParentLoad {
        entity: parent_schema.rust_type_name.clone(),
        module: parent_schema.pg_table_name.clone(),
        domain: parent_domain.clone(),
        auditable: crate::api::handler::resolve_is_auditable(
            db,
            &parent_schema.title,
            config,
            &parent_domain,
        )
        .await
        .map_err(|e| e.to_string())?,
    })
}

/// Whether a child operation's parameter is a request-body carrier: the
/// child class itself (a domain class reference) or a DTO boundary.
fn is_body_carrier(param: &codegraph_core::types::DddParam, child_class: &str) -> bool {
    if param.type_json.get("dto").is_some() {
        return true;
    }
    let Some(kind) = param.type_json.get("type").and_then(|t| t.as_str()) else {
        return false;
    };
    if kind != "class" {
        return false;
    }
    param
        .type_json
        .get("value")
        .and_then(|v| v.get("name"))
        .and_then(|n| n.as_str())
        .is_some_and(|name| name == short_class_name(child_class))
}

/// A human-readable spelling of a parameter's declared type for a refusal
/// (`a DTO boundary` / `the class X` / a primitive).
fn body_carrier_hint(type_json: &serde_json::Value) -> String {
    if let Some(dto) = type_json.get("dto").and_then(|d| d.as_str()) {
        return format!("the DTO boundary `{dto}`");
    }
    match type_json.get("type").and_then(|t| t.as_str()) {
        Some("class") => {
            let name = type_json
                .get("value")
                .and_then(|v| v.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("<unnamed>");
            format!("the class `{name}`")
        }
        Some("primitive") => "a primitive".to_string(),
        Some(other) => format!("a `{other}`"),
        None => "an unresolved type".to_string(),
    }
}

/// The entity title an operation binds to, if any (resolution order in the
/// module doc).
pub(super) async fn bind_entity(
    model: &codegraph_core::types::DddModelGraph,
    op: &codegraph_core::types::DddServiceOperation,
    db: &dyn GraphQuerier,
    type_suffix: &str,
) -> Option<String> {
    // 0. An authored transition binding names a machine on the DELEGATION
    //    target's designed class (issue #660), so that class is the entity
    //    the operation acts on — it wins over a capability binding, whose
    //    class only GATES the route. Without this an op that both binds a
    //    capability elsewhere and a transition would route to the wrong
    //    aggregate and be refused downstream.
    if op.transition.is_some()
        && let Some(target) = op.delegation_target.as_deref()
        && let Some(repo) = model.repositories.iter().find(|r| r.name == target)
        && let Some(title) = schema_title_for_class(db, &repo.design_class, type_suffix).await
    {
        return Some(title);
    }
    // 1. Capability class binding.
    if !op.capabilities.is_empty()
        && let Ok(capabilities) = db.get_capabilities().await
    {
        for capability in &op.capabilities {
            if let Some(node) = capabilities.iter().find(|c| &c.name == capability)
                && let Some(title) = schema_title_for_class(db, &node.class, type_suffix).await
            {
                return Some(title);
            }
        }
    }
    // 2. Delegation to a repository of this model.
    if let (Some(target), _) = (&op.delegation_target, &op.delegation_operation)
        && let Some(repo) = model.repositories.iter().find(|r| &r.name == target)
        && let Some(title) = schema_title_for_class(db, &repo.design_class, type_suffix).await
    {
        return Some(title);
    }
    // 3. The statement body's `=> <Repository>` receiver (issue #662): a
    //    bodied op declares no delegation, so the primary repository the
    //    body orchestrates is its entity binding.
    if !op.bodies.is_empty()
        && let Some(receiver) = op.body_receiver.as_deref()
        && let Some(repo) = model.repositories.iter().find(|r| r.name == receiver)
        && let Some(title) = schema_title_for_class(db, &repo.design_class, type_suffix).await
    {
        return Some(title);
    }
    // 3. Signature: first class-typed param, then the return type. The slot is
    //    a `DddTypeRef` (DD3) — the shared classifier reads the adjacently
    //    tagged domain `TypeRef` and reports a DTO boundary as "no domain
    //    class" rather than misreading it.
    let class_of = |json: Option<&serde_json::Value>| -> Option<String> {
        match crate::ddd::signature::signature_type_opt(json)?.as_domain()? {
            phero_ir::TypeRef::Class { package, name } if package.is_empty() => Some(name.clone()),
            phero_ir::TypeRef::Class { package, name } => Some(format!("{package}::{name}")),
            _ => None,
        }
    };
    for param in &op.params {
        if let Some(class) = class_of(Some(&param.type_json))
            && let Some(title) = schema_title_for_class(db, &class, type_suffix).await
        {
            return Some(title);
        }
    }
    if let Some(class) = class_of(op.return_type.as_ref())
        && let Some(title) = schema_title_for_class(db, &class, type_suffix).await
    {
        return Some(title);
    }
    None
}
