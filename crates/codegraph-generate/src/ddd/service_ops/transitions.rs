use codegraph_config::DomainConfig;
use codegraph_naming::to_snake_case;
use heck::ToLowerCamelCase;
use phero_expr::{BinOp, Expr, ExprKind, UnOp};

use crate::project_config::DtoKeyCasing;

use super::types::{FieldInfo, FieldKind, ServiceOpTransition, StateName, TriggerName, WireKey};

/// The machine's transitions as wire `(from, to)` pairs, declaration order.
pub(super) fn machine_transition_pairs(
    machine: &phero_ir::ddd::StateMachine,
) -> Vec<ServiceOpTransition> {
    machine
        .transitions
        .iter()
        .map(|t| ServiceOpTransition {
            from: StateName::new(t.from.clone()),
            to: StateName::new(t.to.clone()),
        })
        .collect()
}

/// The transition's trigger normalized for matching: the authored `via`, or
/// the bare-arrow derivation (the target state's lowerCamel, normalized to
/// snake_case here).
fn transition_trigger(t: &phero_ir::ddd::Transition) -> TriggerName {
    TriggerName::new(to_snake_case(t.trigger.as_deref().unwrap_or(&t.to)))
}

/// Name candidates an operation can match a trigger by: the exact op, the op
/// with the designing class's snake suffix stripped (`submit_pay_run` →
/// `submit`), and the op with the entity table suffix stripped.
fn trigger_candidates(op_name: &str, entity_class: &str, entity_table: &str) -> Vec<TriggerName> {
    let snake = to_snake_case(op_name);
    let mut candidates = vec![TriggerName::new(snake.clone())];
    for suffix in [
        to_snake_case(&short_class_name(entity_class)),
        to_snake_case(entity_table),
    ] {
        if !suffix.is_empty()
            && let Some(stripped) = snake.strip_suffix(&format!("_{suffix}"))
        {
            candidates.push(TriggerName::new(stripped));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

pub(super) fn short_class_name(class: &str) -> String {
    // phero-ir references are `pkg::Name` or `pkg.Name` (child scopes use the
    // dotted spelling) — compare the last segment under both qualifiers.
    let after_ns = class.rsplit("::").next().unwrap_or(class);
    after_ns.rsplit('.').next().unwrap_or(after_ns).to_string()
}

/// The state names an op-level `when` condition pins as its source: equality
/// comparisons between the machine field and a declared state (`runStatus ==
/// Draft`), OR-combined. Used when the op name does not match a trigger (the
/// `preparePayRun`/`Calculate` shape).
fn condition_source_states(
    source: &str,
    state_field_snake: &str,
    states: &[String],
) -> Vec<String> {
    let parsed = phero_expr::parse(source);
    let Some(ast) = parsed.ast.filter(|_| parsed.errors.is_empty()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_condition_states(&ast, state_field_snake, states, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_condition_states(
    expr: &Expr,
    state_field_snake: &str,
    states: &[String],
    out: &mut Vec<String>,
) {
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => match op {
            BinOp::Eq => {
                let field_state = |a: &Expr, b: &Expr| -> Option<String> {
                    let ExprKind::Name(field) = &a.kind else {
                        return None;
                    };
                    if to_snake_case(field) != state_field_snake {
                        return None;
                    }
                    let ExprKind::Name(state) = &b.kind else {
                        return None;
                    };
                    states.iter().find(|s| *s == state).cloned()
                };
                if let Some(state) = field_state(lhs, rhs).or_else(|| field_state(rhs, lhs)) {
                    out.push(state);
                }
            }
            // Both `&&` and `||` union the source states: the runtime
            // current-state match is the authoritative selector.
            BinOp::And | BinOp::Or => {
                collect_condition_states(lhs, state_field_snake, states, out);
                collect_condition_states(rhs, state_field_snake, states, out);
            }
            _ => {}
        },
        ExprKind::Unary {
            op: UnOp::Not,
            expr,
        } => collect_condition_states(expr, state_field_snake, states, out),
        _ => {}
    }
}

/// The transitions a service op applies.
///
/// **Primary selector (issue #660): the authored `transition` binding.** When
/// the operation declares one, the machine's transition is resolved from the
/// binding alone — the trigger must be one the machine declares (an explicit
/// `via` name or a bare arrow's derived lowerCamel name), the optional
/// qualifier must name the bound machine's enum type, and a module carrying
/// more than one machine-bearing design requires that qualifier. Any failure
/// is a HARD generation error naming the op, the binding, and the machines
/// involved: an authored binding NEVER falls back to an inference.
///
/// **No-binding fallback** — the pre-#660 heuristics, kept ONLY because
/// consumer models still declare ops without a binding (the upstream syntax
/// postdates them): op-name → trigger (exact, then entity-suffix-stripped),
/// op-level `when` state equality → the unique outgoing transition(s), and
/// the capability-sharing arm (a `protected` internal op carrying the real
/// trigger, `preparePayRun` → `calculate`). Every op that resolves a
/// transition this way reports a WARN naming the missing binding
/// ([`transition_binding_warnings`]).
pub(super) fn resolved_transitions(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    machine: &phero_ir::ddd::StateMachine,
    entity_title: &str,
    entity_class: &str,
    entity_table: &str,
) -> Result<Vec<ServiceOpTransition>, String> {
    match &op.transition {
        Some(binding) => resolve_transition_binding(op, binding, model, machine, entity_title),
        None => Ok(heuristic_transitions(
            op,
            model,
            machine,
            entity_class,
            entity_table,
        )),
    }
}

/// The authored spelling of a transition binding for messages:
/// `SignOff`, or `ReleaseStatus.SignOff` when qualified.
pub(super) fn binding_spelling(binding: &codegraph_core::types::DddTransitionRef) -> String {
    match &binding.machine {
        Some(qualifier) => format!("{qualifier}.{}", binding.trigger),
        None => binding.trigger.clone(),
    }
}

/// The machine's enum type name — the only machine qualifier an authored
/// binding can carry (Phero Lang machine declarations are field-named).
fn machine_enum_type_name(machine: &phero_ir::ddd::StateMachine) -> String {
    match &machine.enum_type {
        phero_ir::TypeRef::Enum { name, .. }
        | phero_ir::TypeRef::Class { name, .. }
        | phero_ir::TypeRef::Datatype { name, .. }
        | phero_ir::TypeRef::Interface { name, .. }
        | phero_ir::TypeRef::Vocabulary { name, .. } => name.clone(),
        phero_ir::TypeRef::Primitive(_) => String::new(),
    }
}

/// Every machine-bearing design of the entity's module, as
/// `(class, enum type)` — the ambiguity set an unqualified binding cannot
/// disambiguate (upstream E0352 requires the qualifier there).
fn module_machines(
    model: &codegraph_core::types::DddModelGraph,
    entity_title: &str,
) -> Vec<(String, String)> {
    let Some(design) = model
        .designs
        .iter()
        .find(|d| d.resolved_title.as_deref() == Some(entity_title))
    else {
        return Vec::new();
    };
    model
        .designs
        .iter()
        .filter(|d| {
            d.application == design.application
                && d.module == design.module
                && d.machine_json.is_some()
        })
        .filter_map(|d| {
            let machine: phero_ir::ddd::StateMachine =
                serde_json::from_value(d.machine_json.clone()?).ok()?;
            Some((d.class.clone(), machine_enum_type_name(&machine)))
        })
        .collect()
}

fn format_module_machines(machines: &[(String, String)]) -> String {
    machines
        .iter()
        .map(|(class, enum_type)| format!("`{class}`: `{enum_type}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The declared trigger vocabulary of a machine, in declaration order:
/// explicit `via` names and every bare arrow's derived lowerCamel name.
fn declared_triggers(machine: &phero_ir::ddd::StateMachine) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for transition in &machine.transitions {
        let name = match &transition.trigger {
            Some(trigger) => trigger.clone(),
            None => transition.to.to_lower_camel_case(),
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Resolve one authored binding against the entity's machine (issue #660).
///
/// The returned transitions are EXACTLY the machine transitions the binding
/// names — one per `(from, to)` pair carrying that trigger, so a trigger
/// declared from several states still selects at runtime by the record's
/// current state, but a trigger the machine does not declare (or a
/// qualifier that names another machine) can never silently become a
/// different transition.
pub(super) fn resolve_transition_binding(
    op: &codegraph_core::types::DddServiceOperation,
    binding: &codegraph_core::types::DddTransitionRef,
    model: &codegraph_core::types::DddModelGraph,
    machine: &phero_ir::ddd::StateMachine,
    entity_title: &str,
) -> Result<Vec<ServiceOpTransition>, String> {
    let machines = module_machines(model, entity_title);
    let spelling = binding_spelling(binding);
    let entity_machine = machine_enum_type_name(machine);
    match binding.machine.as_deref() {
        None if machines.len() > 1 => {
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, but the module carries \
                 more than one machine-bearing design ({}); qualify the binding as \
                 `<EnumType>.<Trigger>` — the machine bound to `{entity_title}` is \
                 `{}: {entity_machine}`",
                op.name,
                format_module_machines(&machines),
                machine.field,
            ));
        }
        Some(qualifier) if qualifier != entity_machine => {
            let others = machines
                .iter()
                .filter(|(class, _)| class.as_str() != entity_title)
                .map(|(class, _)| class.as_str())
                .collect::<Vec<_>>();
            let tail = if others.is_empty() {
                String::new()
            } else {
                format!(
                    " (the module's machines are {})",
                    format_module_machines(&machines)
                )
            };
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, but the machine bound to \
                 `{entity_title}` is `{}: {entity_machine}`, not `{qualifier}`{tail}",
                op.name, machine.field,
            ));
        }
        _ => {}
    }

    let mut explicit: Vec<&phero_ir::ddd::Transition> = Vec::new();
    let mut derived: Vec<&phero_ir::ddd::Transition> = Vec::new();
    for transition in &machine.transitions {
        match &transition.trigger {
            Some(trigger) if trigger == &binding.trigger => explicit.push(transition),
            None if transition.to.to_lower_camel_case() == binding.trigger => {
                derived.push(transition);
            }
            _ => {}
        }
    }
    let chosen: Vec<&phero_ir::ddd::Transition> = match (explicit.is_empty(), derived.is_empty()) {
        (false, true) => explicit,
        (true, false) => derived,
        (true, true) => {
            let declared = declared_triggers(machine);
            let vocabulary = if declared.is_empty() {
                "the machine declares no triggers".to_string()
            } else {
                format!("declared triggers: {}", declared.join(", "))
            };
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, but the machine bound to \
                 `{entity_title}` (`{}: {entity_machine}`) declares no such trigger ({vocabulary})",
                op.name, machine.field,
            ));
        }
        (false, false) => {
            return Err(format!(
                "service op `{}` declares `transition {spelling}`, which is ambiguous: the \
                 machine bound to `{entity_title}` (`{}: {entity_machine}`) declares it both \
                 as an explicit `via` trigger and as a bare arrow's derived name",
                op.name, machine.field,
            ));
        }
    };
    Ok(pairs_of(&chosen))
}

fn pairs_of(transitions: &[&phero_ir::ddd::Transition]) -> Vec<ServiceOpTransition> {
    let mut pairs: Vec<ServiceOpTransition> = transitions
        .iter()
        .map(|t| ServiceOpTransition {
            from: StateName::new(t.from.clone()),
            to: StateName::new(t.to.clone()),
        })
        .collect();
    pairs.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    pairs.dedup_by(|a, b| a.from == b.from && a.to == b.to);
    pairs
}

/// The pre-#660 inference (the no-binding fallback), kept ONLY for models
/// that declare no `transition` binding: op-name → trigger (exact, then
/// entity-suffix-stripped), then the op-level `when` source states, then
/// the capability-sharing `protected`-op arm. Every op that resolves a
/// transition here warns ([`transition_binding_warnings`]).
pub(super) fn heuristic_transitions(
    op: &codegraph_core::types::DddServiceOperation,
    model: &codegraph_core::types::DddModelGraph,
    machine: &phero_ir::ddd::StateMachine,
    entity_class: &str,
    entity_table: &str,
) -> Vec<ServiceOpTransition> {
    let states: Vec<String> = machine.states().into_iter().map(str::to_string).collect();
    let state_field_snake = to_snake_case(&machine.field);

    let by_trigger = |op_name: &str| -> Vec<ServiceOpTransition> {
        let op_candidates = trigger_candidates(op_name, entity_class, entity_table);
        machine
            .transitions
            .iter()
            .filter(|t| op_candidates.contains(&transition_trigger(t)))
            .map(|t| ServiceOpTransition {
                from: StateName::new(t.from.clone()),
                to: StateName::new(t.to.clone()),
            })
            .collect()
    };

    let mut chosen = by_trigger(&op.name);
    if chosen.is_empty()
        && let Some(condition) = op.condition.as_deref()
    {
        let source_states = condition_source_states(condition, &state_field_snake, &states);
        chosen = machine
            .transitions
            .iter()
            .filter(|t| source_states.iter().any(|s| s == &t.from))
            .map(|t| ServiceOpTransition {
                from: StateName::new(t.from.clone()),
                to: StateName::new(t.to.clone()),
            })
            .collect();
    }
    if chosen.is_empty() {
        for other in model.services.iter().flat_map(|s| &s.operations) {
            if !other.is_protected {
                continue;
            }
            let shares_capability = other
                .capabilities
                .iter()
                .any(|c| op.capabilities.iter().any(|own| own == c));
            if !shares_capability {
                continue;
            }
            let matched = by_trigger(&other.name);
            if !matched.is_empty() {
                chosen = matched;
                break;
            }
        }
    }
    // `dedup_by` only removes adjacent duplicates; sort then dedup keeps
    // the runtime match arms stable and unique.
    chosen.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    chosen.dedup_by(|a, b| a.from == b.from && a.to == b.to);
    chosen
}

/// The Rust expression producing the record's current machine state as a
/// `String` (the codelist/string comparison path).
pub(super) fn current_state_expr(field: &FieldInfo) -> String {
    match (field.kind, field.optional) {
        (FieldKind::Codelist, false) => format!("response.{}.to_string()", field.rust_name),
        (FieldKind::Codelist, true) => format!(
            "response.{}.as_ref().map(|v| v.to_string()).unwrap_or_default()",
            field.rust_name
        ),
        (_, false) => format!("response.{}.clone()", field.rust_name),
        (_, true) => format!("response.{}.clone().unwrap_or_default()", field.rust_name),
    }
}

pub(super) fn wire_key(rust_field: &str, casing: DtoKeyCasing) -> WireKey {
    WireKey::new(dto_wire_key(rust_field, casing))
}

/// The Update-DTO wire key of one entity field under the project's casing —
/// the key a `save`-payload op's patch writes (and the machine transition
/// arm patches).
pub(crate) fn dto_wire_key(rust_field: &str, casing: DtoKeyCasing) -> String {
    match casing {
        DtoKeyCasing::Snake => rust_field.to_string(),
        DtoKeyCasing::Camel => rust_field.to_lower_camel_case(),
    }
}

/// Whether the machine state field survives into the Update DTO. Workflow
/// status/approval fields and configured `immutable_fields` are excluded;
/// patching them through the update DTO would either 422 (`deny_unknown_
/// fields`) or be ignored, so those ops keep a seam until a dedicated
/// transition command lands.
pub(super) fn state_field_is_dto_updatable(
    config: &DomainConfig,
    domain: &str,
    entity_title: &str,
    state_field_rust: &str,
) -> bool {
    let Some(ec) = config
        .domains
        .get(domain)
        .and_then(|d| d.get_entity_config(entity_title))
    else {
        return true;
    };
    if ec
        .dto
        .immutable_fields
        .iter()
        .any(|f| f == state_field_rust)
    {
        return false;
    }
    if let Some(wf) = &ec.workflow
        && (wf.status_field == state_field_rust
            || wf.approval_status_field.as_deref() == Some(state_field_rust))
    {
        return false;
    }
    true
}
