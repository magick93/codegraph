use std::collections::{BTreeMap, HashMap};

use codegraph_core::error::GraphError;
use codegraph_core::traits::GraphQuerier;
use codegraph_naming::to_snake_case;

use super::types::FieldInfo;
use super::when_lower::{InjectedScope, lower_when_scoped, rust_string_literal};

// ── Policy surface ─────────────────────────────────────────────────────

/// The actor policy flattened for server-side enforcement: BaseJump role →
/// capability → grant `when` sources (None = unconditional), plus the
/// `never_both` groups. Role mapping mirrors the IFML hydration convention
/// (`route_generator/hydration.rs`): an actor whose snake_case name equals
/// the account role claims it; otherwise `owner` falls to the first actor
/// and `member` to the last.
#[derive(Debug, Clone, Default)]
pub(crate) struct PolicySurface {
    pub roles: BTreeMap<String, BTreeMap<String, Vec<Option<String>>>>,
    pub never_both: Vec<Vec<String>>,
}

impl PolicySurface {
    /// Build from the graph readback. `None` when the graph carries no
    /// actors (no policy ingested — capability/SoD gates are skipped with
    /// a warning, the IFML warn-and-continue posture).
    pub async fn from_graph(db: &dyn GraphQuerier) -> Result<Option<Self>, GraphError> {
        let actors = db.get_actors().await?;
        if actors.is_empty() {
            return Ok(None);
        }
        let grants = db.get_grants().await?;
        let actor_names: Vec<String> = actors.iter().map(|a| a.name.clone()).collect();
        let mut roles: BTreeMap<String, BTreeMap<String, Vec<Option<String>>>> = BTreeMap::new();
        for role in ["owner", "member", "manager", "employee"] {
            let picked = basejump_role_actors(role, &actor_names);
            if picked.is_empty() {
                continue;
            }
            let mut permits: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
            for actor in &picked {
                for permit in
                    codegraph_core::types::resolve_effective_permits(&actors, &grants, actor)
                {
                    let entry = permits.entry(permit.capability.clone()).or_default();
                    let when = permit.when.clone();
                    if !entry.contains(&when) {
                        entry.push(when);
                    }
                }
            }
            roles.insert(role.to_string(), permits);
        }
        let never_both = db
            .get_actor_policy()
            .await?
            .map(|policy| {
                policy
                    .never_both
                    .iter()
                    .map(|group| group.capabilities.clone())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(Self { roles, never_both }))
    }

    /// The capabilities that conflict with `capabilities` under
    /// `never_both` (deterministic order, deduped).
    pub fn sod_conflicts(&self, capabilities: &[String]) -> Vec<String> {
        let mut conflicts = Vec::new();
        for group in &self.never_both {
            if group.iter().any(|c| capabilities.contains(c)) {
                for member in group {
                    if !capabilities.contains(member) && !conflicts.contains(member) {
                        conflicts.push(member.clone());
                    }
                }
            }
        }
        conflicts
    }
}

/// The actors mapped to one BaseJump account role (the hydration
/// convention).
pub(super) fn basejump_role_actors(role: &str, actor_names: &[String]) -> Vec<String> {
    let snake_match = actor_names.iter().find(|a| to_snake_case(a) == role);
    match (role, snake_match, actor_names.first(), actor_names.last()) {
        (_, Some(a), _, _) => vec![a.clone()],
        ("owner", None, Some(first), _) => vec![first.clone()],
        ("member", None, _, Some(last)) => vec![last.clone()],
        _ => Vec::new(),
    }
}

// ── Authorization fn rendering ──────────────────────────────────────────

/// Render the per-op `authorize` fn encoding the policy for the route:
/// capability membership, `never_both` separation of duties, and per-grant
/// `when` guards evaluated over the loaded record.
///
/// `None` when no role holds any of the op's capabilities AND none even
/// conflicts — the route is emitted ungated-capability-free only when the
/// op declares no capabilities; a capability no grant mentions emits a
/// deny-all gate (the policy says nobody holds it), so this returns the
/// fn whenever the policy surface exists.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn(
    snake: &str,
    op_name: &str,
    service: &str,
    capabilities: &[String],
    condition: Option<&str>,
    response_type_path: &str,
    surface: Option<&PolicySurface>,
    fields: &HashMap<String, FieldInfo>,
    entity: &str,
    record_available: bool,
) -> Result<Option<String>, String> {
    render_authorize_fn_with_states(
        snake,
        op_name,
        service,
        capabilities,
        condition,
        response_type_path,
        surface,
        fields,
        entity,
        record_available,
        &[],
    )
}

/// [`render_authorize_fn`] with the entity's designed machine states bound
/// in `when` lowering (P0.5): `when (runStatus == AwaitingApproval)` treats
/// the bare state name as the enum value, not a record field.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn_with_states(
    snake: &str,
    op_name: &str,
    service: &str,
    capabilities: &[String],
    condition: Option<&str>,
    response_type_path: &str,
    surface: Option<&PolicySurface>,
    fields: &HashMap<String, FieldInfo>,
    entity: &str,
    record_available: bool,
    machine_states: &[String],
) -> Result<Option<String>, String> {
    render_authorize_fn_with_injected(
        snake,
        op_name,
        service,
        capabilities,
        condition,
        response_type_path,
        surface,
        fields,
        entity,
        record_available,
        machine_states,
        &[],
    )
}

/// [`render_authorize_fn_with_states`] with the op's injected-scope
/// `when` paths resolved (issue #663): the gate becomes `async`, takes the
/// `AppState` (the sibling query slot) and the session, loads every
/// injected scope through the repository plane's own query, then evaluates
/// the guard over the loaded records. `record_available` must hold — the
/// injected ids ride the loaded response — and is refused otherwise.
/// Empty `injected` renders byte-identically to
/// [`render_authorize_fn_with_states`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_authorize_fn_with_injected(
    snake: &str,
    op_name: &str,
    service: &str,
    capabilities: &[String],
    condition: Option<&str>,
    response_type_path: &str,
    surface: Option<&PolicySurface>,
    fields: &HashMap<String, FieldInfo>,
    entity: &str,
    record_available: bool,
    machine_states: &[String],
    injected: &[InjectedScope],
) -> Result<Option<String>, String> {
    // The enum-literal namespace is module-global upstream: the injected
    // machines' declared states bind as bare names exactly like the
    // op entity's own (`TimecardRepository.timecardStatus == Approved`
    // lowers `Approved` as the Timecard machine's literal).
    let mut all_states: Vec<String> = machine_states.to_vec();
    for scope in injected {
        if let Some(machine) = &scope.machine {
            for state in &machine.states {
                if !all_states.contains(state) {
                    all_states.push(state.clone());
                }
            }
        }
    }
    let lowered_condition = condition
        .map(|source| {
            let lowered =
                lower_when_scoped(source, op_name, entity, fields, &all_states, injected)?;
            Ok::<_, String>((lowered, source.to_string()))
        })
        .transpose()?;
    let condition_message = condition.map(|source| {
        rust_string_literal(&format!(
            "operation `{op_name}`: `when` condition not met ({source})"
        ))
    });
    if capabilities.is_empty() && lowered_condition.is_none() {
        return Ok(None);
    }
    let empty_roles = BTreeMap::new();
    let roles = match surface {
        Some(policy) => &policy.roles,
        None => &empty_roles,
    };
    // Does the rendered gate read the loaded record? The op-level `when`
    // always does; a per-grant `when` does whenever a held permit carries
    // one without an unconditional (None) override. (Over-approximates
    // only when a when-guarded permit also lands on a `never_both`
    // conflict — the conflict arm never reads the record — failing
    // closed there.)
    let needs_record = lowered_condition.is_some()
        || roles.values().any(|permits| {
            permits.iter().any(|(capability, whens)| {
                capabilities.contains(capability)
                    && !whens.is_empty()
                    && !whens.iter().any(Option::is_none)
                    && whens.iter().any(Option::is_some)
            })
        });
    if needs_record && !record_available {
        return Err(format!(
            "service op `{op_name}` carries `when` conditions, but its bound \
             entity `{entity}` has no record load (its design surface exposes \
             no `read`, so the query plane has no `find_by_id`) — generation \
             refuses to emit an unguarded route"
        ));
    }
    // An injected guard loads the related records off the response —
    // without a record load there is no honest id source (fail closed,
    // the same posture as the `when`-without-read refusal above).
    if !injected.is_empty() && !record_available {
        return Err(format!(
            "service op `{op_name}` carries an injected-scope `when` guard, \
             but its bound entity `{entity}` has no record load (its design \
             surface exposes no `read`) — the guard's id source is the \
             loaded record, so generation refuses"
        ));
    }
    let mut out = String::new();
    let mut body = String::new();
    if injected.is_empty() {
        // Without a record the gate is capability membership only: no
        // response parameter to evaluate `when`s over (there are none —
        // `needs_record` refused those above).
        let response_param = if record_available {
            format!("    response: &{response_type_path},\n")
        } else {
            String::new()
        };
        out.push_str(&format!(
            "/// Authorization gate for `{op_name}` (service `{service}`, issue #590).\n\
             ///\n\
             /// Encodes the actor policy at generation time: capability\n\
             /// membership{membership_note}\n\
             /// API-key sessions bypass the policy gates (RLS scopes still\n\
             /// apply) so contract suites and service callers keep working.\n\
             #[allow(clippy::result_large_err)]\n\
             fn service_op_{snake}_authorize(\n    \
                 role: &str,\n{response_param}\
             ) -> Result<(), String> {{\n",
            membership_note = if record_available {
                ", `never_both` separation-of-duties conflicts, and\n\
                 /// per-grant `when` conditions evaluated over the loaded record."
            } else {
                " and\n\
                 /// `never_both` separation-of-duties conflicts (no record load — \
                 the bound\n\
                 /// entity exposes no `read`, so `when` conditions cannot be \
                 evaluated; the\n\
                 /// op body is a 501 seam regardless)."
            },
            response_param = response_param,
        ));
    } else {
        out.push_str(&format!(
            "/// Authorization gate for `{op_name}` (service `{service}`, issue #590).\n\
             ///\n\
             /// Encodes the actor policy at generation time: capability\n\
             /// membership, `never_both` separation-of-duties conflicts, and\n\
             /// per-grant `when` conditions evaluated over the loaded record —\n\
             /// plus the injected-scope `when` guard (issue #663): every\n\
             /// injected scope loads through the repository plane's own query\n\
             /// (RLS-scoped by the session context) before the guard evaluates.\n\
             /// API-key sessions bypass the policy gates (RLS scopes still\n\
             /// apply) so contract suites and service callers keep working.\n\
             #[allow(clippy::result_large_err)]\n\
             async fn service_op_{snake}_authorize(\n    \
                 state: &AppState,\n    \
                 api_key_info: &ApiKeyInfo,\n    \
                 response: &{response_type_path},\n\
             ) -> Result<(), String> {{\n    \
                 let role = api_key_info.role.as_str();\n"
        ));
        for scope in injected {
            let message = condition_message.as_deref().unwrap_or("\"guard denied\"");
            // The own-record FK carries the injected id; an optional `refers`
            // with no related row cannot satisfy the guard — the denial IS
            // the guard's answer.
            if scope.fk_optional {
                body.push_str(&format!(
                    "    let Some({local}_id) = response.{fk} else {{\n        \
                         return Err({message}.to_string());\n    \
                     }};\n",
                    local = scope.local,
                    fk = scope.fk_field,
                    message = message,
                ));
            } else {
                body.push_str(&format!(
                    "    let {local}_id = response.{fk};\n",
                    local = scope.local,
                    fk = scope.fk_field,
                ));
            }
            let auditable_clause = if scope.auditable { " false," } else { "" };
            body.push_str(&format!(
                "    // The guard reads `{root}.*` — load the related record through\n    \
                     // the repository plane's own query (issue #663).\n    \
                     let {local} = state.{slot}_queries\n        \
                     .find_by_id({local}_id,{aud} api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n        \
                     .await\n        \
                     .map_err(|e| e.to_string())?;\n    \
                     let Some({local}) = {local} else {{\n        \
                         return Err({message}.to_string());\n    \
                     }};\n",
                root = scope.root,
                local = scope.local,
                slot = scope.slot,
                aud = auditable_clause,
                message = message,
            ));
        }
    }
    if let (Some((lowered, _)), Some(message)) = (&lowered_condition, &condition_message) {
        body.push_str(&format!(
            "    // op-level `when` transition guard — business semantics apply \
             to every session\n\
             if !({lowered}) {{\n                \
                 return Err({message}.to_string());\n            }}\n"
        ));
    }
    body.push_str(
        "    if role == \"api_key\" {\n                \
             return Ok(());\n            \
         }\n\
         match role {\n",
    );
    out.push_str(&body);
    let conflicts = surface
        .map(|policy| policy.sod_conflicts(capabilities))
        .unwrap_or_default();
    for (role, permits) in roles {
        let held: Vec<&String> = capabilities
            .iter()
            .filter(|c| permits.contains_key(*c))
            .collect();
        if held.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "        {role_quoted} => {{\n",
            role_quoted = rust_string_literal(role)
        ));
        let role_conflicts: Vec<&String> = conflicts
            .iter()
            .filter(|c| permits.contains_key(*c))
            .collect();
        if !role_conflicts.is_empty() {
            out.push_str(&format!(
                "            // separation of duties: `never_both` conflicts\n\
                 // with `{}`\n\
                 Err({}.to_string())\n        }}\n",
                role_conflicts
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join("`, `"),
                rust_string_literal(&format!(
                    "separation of duties: role `{role}` holds `{}`, which is \
                     `never_both` with `{}`",
                    role_conflicts
                        .iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join("`, `"),
                    capabilities.join("`, `")
                ))
            ));
            continue;
        }
        // Conditional permits over the held capabilities; an unconditional
        // permit clears the guard.
        let mut conditions: Vec<String> = Vec::new();
        let mut unconditional = false;
        for capability in &held {
            if let Some(whens) = permits.get(*capability) {
                if whens.iter().any(Option::is_none) {
                    unconditional = true;
                    break;
                }
                for when in whens.iter().flatten() {
                    let lowered = lower_when_scoped(
                        when,
                        capability,
                        entity,
                        fields,
                        machine_states,
                        injected,
                    )?;
                    conditions.push(lowered);
                }
            }
        }
        if !unconditional && !conditions.is_empty() {
            // Conditions are fully parenthesized by the lowering.
            let joined = conditions.join(" || ");
            let sources = held
                .iter()
                .filter_map(|c| permits.get(*c))
                .flat_map(|whens| whens.iter().flatten())
                .cloned()
                .collect::<Vec<_>>()
                .join("` / `");
            out.push_str(&format!(
                "            // `when` guard{plural} on {}\n\
                 if !({joined}) {{\n                \
                     return Err({}.to_string());\n            }}\n\
                 Ok(())\n        }}\n",
                sources,
                rust_string_literal(&format!(
                    "capability `{}`: `when` condition not met ({sources})",
                    held.iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join("`, `")
                )),
                plural = if conditions.len() == 1 { "" } else { "s" },
            ));
        } else {
            out.push_str("            Ok(())\n        }\n");
        }
    }
    out.push_str(&format!(
        "        _ => Err({}.to_string()),\n    }}\n}}\n",
        rust_string_literal(&format!(
            "capability `{}` required (no policy role holds it)",
            capabilities.join("`, `")
        ))
    ));
    Ok(Some(out))
}
