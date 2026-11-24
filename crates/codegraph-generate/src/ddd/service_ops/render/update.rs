//! The `save`/`update` service-op arm: patch the loaded row through the
//! update command, with the machine-binding kernel probes (issue #818 —
//! the hand-rendered `match current_state` transition arms are retired)
//! and the design-invariant splice.

use codegraph_naming::to_pascal_case;

use super::super::types::{ServiceOpRoute, ServiceOpState};

/// The update arm's head: the patch accumulator the machine arm (when any)
/// writes into and the DTO decode consumes.
const UPDATE_HEAD: &str = concat!(
    "    // Executable `save`/`update` delegation (P0): patch the loaded row\n",
    "    // through the command plane. A machine-bound op resolves its target\n",
    "    // state from the record's current state; an unbound op may change the\n",
    "    // machine field only along a declared transition.\n",
    "    let mut value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
);

/// The update arm's idempotent-read early return. Ends at its closing `}` with
/// no newline: the design-invariant block (when present) glues straight onto
/// it, exactly as the template does.
const UPDATE_IDEMPOTENT_READ: &str = concat!(
    "    // A save with no fields to change (no body, no transition) is an\n",
    "    // idempotent read — do not issue a no-op UPDATE.\n",
    "    if value.as_object().is_some_and(|o| o.is_empty()) {\n",
    "        return Ok(axum::Json(serde_json::json!({\n",
    "            \"data\": response,\n",
    "            \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
    "        })).into_response());\n",
    "    }",
);

/// The `update` arm: patch the loaded row through the update command. A
/// machine-bound op probes the kernel ([`codegraph_workflow::MachineRuntime`]
/// over the entity's registry machine) — the machine decides, the renderer
/// bakes the candidate targets (issue #818); the design invariants (when the
/// handler attached a block) evaluate the candidate pre-write.
pub(super) fn update_block(op: &ServiceOpRoute, state: Option<&ServiceOpState>) -> String {
    let mut block = String::from(UPDATE_HEAD);

    match state {
        Some(state) => {
            block.push_str("\n    let current_state = ");
            block.push_str(&state.current_expr);
            block.push_str(";\n");
            block.push_str(&runtime_construction(op, state));
            if state.op.is_empty() {
                push_whitelist_arm(&mut block, op, state);
            } else {
                push_named_transition_arm(&mut block, op, state);
            }
            block.push_str("\n\n");
        }
        // No machine binding: the plain patch arm (one blank line before the
        // idempotent read, the `{% endif %}` the template leaves behind).
        None => block.push('\n'),
    }

    block.push_str(UPDATE_IDEMPOTENT_READ);
    if let Some(invariant) = &op.invariant_block {
        block.push_str(invariant);
    }
    block.push('\n');
    block.push_str(&update_tail(op));
    block
}

/// The runtime construction the machine arms share: load the registry
/// embed, parse it, and build the fail-closed
/// [`codegraph_workflow::MachineRuntime`]. Every error arm is a defect
/// seam (generation validated the embed) — an internal 500, never a
/// silently skipped transition.
fn runtime_construction(op: &ServiceOpRoute, state: &ServiceOpState) -> String {
    crate::ddd::kernel_probe::runtime_construction(
        &op.render_facts.domain_types_crate,
        &state.machine_id,
        "    ",
    )
}

/// The candidate target literals baked from a pair list: deduped, in
/// declaration order — the kernel probes them from the current state.
fn candidates_of(pairs: &[super::super::types::ServiceOpTransition]) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    for pair in pairs {
        let target = pair.to.as_str().to_string();
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// The named-transition arm: the op's declared targets (#660 binding
/// resolved) are probed through the kernel. Exactly one legal edge from
/// the current state fires; none is the 409 with the contract message; an
/// ambiguous fork (two edges from one state in the op's set — the shape
/// the old first-match `match` arm silently collapsed) names its targets.
fn push_named_transition_arm(block: &mut String, op: &ServiceOpRoute, state: &ServiceOpState) {
    let candidates = candidates_of(&state.op);
    block.push('\n');
    block.push_str(&crate::ddd::kernel_probe::probe_legal_block(
        &candidates,
        "    ",
    ));
    block.push_str(&crate::ddd::kernel_probe::named_verdict_block(
        &op.op_name,
        "    ",
    ));
    block.push_str(&format!(
        "    value[\"{}\"] = serde_json::Value::String(target_state.to_string());\n",
        state.wire_key.as_str()
    ));
}

/// The body-driven arm: the op selects no transition of its own, so a
/// request that moves the machine field must land on a transition the
/// machine declares from the current state. The candidate universe (the
/// machine's declared targets) is probed per pair through the kernel
/// (issue #818, Option A) — exactly one legal target that matches the
/// request synthesizes; none or ambiguous is the 409 naming the target.
/// BEHAVIOR HARDENING (recorded on issue #818): the old hand-rendered
/// whitelist allowed any value the machine could EVER reach; a state
/// value with no legal connecting edge from the current state now 409s
/// instead of persisting. Stay-put is not a transition and stays allowed.
fn push_whitelist_arm(block: &mut String, op: &ServiceOpRoute, state: &ServiceOpState) {
    let candidates = candidates_of(&state.all);
    block.push_str("\n    if let Some(target) = value.get(\"");
    block.push_str(state.wire_key.as_str());
    block.push_str("\").and_then(|v| v.as_str()) {\n");
    block.push_str("        if target != current_state {\n");
    block.push_str(&crate::ddd::kernel_probe::probe_legal_block(
        &candidates,
        "            ",
    ));
    block.push_str("            match legal.as_slice() {\n");
    block.push_str("                [only] if *only == target => {\n");
    block.push_str(&format!(
        "                    value[\"{}\"] = serde_json::Value::String(target.to_string());\n",
        state.wire_key.as_str()
    ));
    block.push_str("                }\n");
    block.push_str("                [] => {\n");
    block.push_str("                    return Err(AppError::conflict(format!(\n");
    block.push_str(&format!(
        "                        \"operation `{}` cannot move `{}` from `{{}}` to `{{}}`: no transition from the current state\",\n",
        op.op_name,
        state.wire_key.as_str()
    ));
    block.push_str("                        current_state, target\n");
    block.push_str("                    )).with_correlation_id(correlation_id));\n");
    block.push_str("                }\n");
    block.push_str("                _ => {\n");
    block.push_str("                    return Err(AppError::conflict(format!(\n");
    block.push_str(&format!(
        "                        \"operation `{}` cannot move `{}` from `{{}}` to `{{}}`: no unambiguous transition from the current state (legal targets: {{}})\",\n",
        op.op_name,
        state.wire_key.as_str()
    ));
    block.push_str("                        current_state,\n");
    block.push_str("                        target,\n");
    block.push_str("                        legal.join(\", \")\n");
    block.push_str("                    )).with_correlation_id(correlation_id));\n");
    block.push_str("                }\n");
    block.push_str("            }\n");
    block.push_str("        }\n");
    block.push_str("    }\n");
}

/// The update arm's tail: the DTO decode, the update command, and the reload
/// envelope. The reload is the parent-scoped read on a nested child route and
/// carries the `include_deleted` arity when the entity is auditable.
fn update_tail(op: &ServiceOpRoute) -> String {
    let facts = &op.render_facts;
    let domain_error = format!("{}Error", to_pascal_case(&facts.domain));
    let mut reload_call = if facts.nested_child {
        format!("find_by_id_scoped({}, parent_id", facts.id_param)
    } else {
        format!("find_by_id({}", facts.id_param)
    };
    if facts.auditable {
        reload_call.push_str(", false");
    }
    format!(
        concat!(
            "    let cmd: crate::domain::{domain}::{table}::dto_update::Update{entity}Request =\n",
            "        serde_json::from_value(value).map_err(|e| {{\n",
            "            AppError::bad_request(format!(\"invalid {entity} update body: {{e}}\"))\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?;\n",
            "    state.{domain}_{table}_commands.update(\n",
            "        {param},\n",
            "        cmd,\n",
            "        {types}::SourceContext::api(),\n",
            "        correlation_id,\n",
            "        api_key_info.api_key_id,\n",
            "        api_key_info.organization_id,\n",
            "        api_key_info.user_id,\n",
            "        api_key_info.role.clone(),\n",
            "    ).await.map_err(|e: {domain_error}| {{\n",
            "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to update {entity}\")\n",
            "            .with_correlation_id(correlation_id)\n",
            "    }})?;\n",
            "    let updated = state.{domain}_{table}_queries\n",
            "        .{reload}, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
            "        .await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload {entity}\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?\n",
            "        .ok_or_else(|| AppError::not_found(\"{entity} not found\").with_correlation_id(correlation_id))?;\n",
            "    Ok(axum::Json(serde_json::json!({{\n",
            "        \"data\": updated,\n",
            "        \"meta\": {{ \"correlation_id\": correlation_id.to_string() }}\n",
            "    }})).into_response())",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        entity = facts.entity_name,
        param = facts.id_param,
        types = facts.domain_types_crate,
        domain_error = domain_error,
        reload = reload_call,
    )
}
#[cfg(test)]
mod tests {
    use crate::ddd::service_ops::render::{render_execution_block, test_support};
    use crate::ddd::service_ops::types::{
        ExecutionKind, ExecutionRenderFacts, ServiceOpRoute, ServiceOpState, ServiceOpTransition,
        StateName, WireKey,
    };

    // The fixture machine: `Draft -> Submitted -> Approved`.
    fn machine_transitions() -> Vec<ServiceOpTransition> {
        vec![
            ServiceOpTransition {
                from: StateName::new("Draft"),
                to: StateName::new("Submitted"),
            },
            ServiceOpTransition {
                from: StateName::new("Submitted"),
                to: StateName::new("Approved"),
            },
        ]
    }

    fn status_state(op: Vec<ServiceOpTransition>) -> ServiceOpState {
        ServiceOpState {
            current_expr: "response.status.clone().unwrap_or_default()".to_string(),
            wire_key: WireKey::new("status"),
            initial: StateName::new("Draft"),
            machine_id: "common_status".to_string(),
            all: machine_transitions(),
            op,
        }
    }

    fn matrix_route(op_name: &str, execution: ExecutionKind) -> ServiceOpRoute {
        test_support::route(
            "TaskService",
            op_name,
            test_support::matrix_facts(),
            execution,
            true,
        )
    }

    /// The matrix fixture's `OP_INVARIANT_BLOCK`, verbatim (the handler
    /// generator attaches exactly this text to an `Update` route).
    const MATRIX_INVARIANT_BLOCK: &str = r#"
    // Design invariants (DD5, issue #666 slice 4d): the candidate is
    // the loaded record with the action patch — the machine state move
    // included — applied; evaluated BEFORE the write.
    let candidate: domain_types::common::task::dto_response::TaskResponse = serde_json::from_value({
        let mut candidate_json = serde_json::to_value(&response).unwrap_or_default();
        if let (Some(candidate_obj), Some(patch)) = (candidate_json.as_object_mut(), value.as_object()) {
            for (key, item) in patch {
                candidate_obj.insert(key.clone(), item.clone());
            }
        }
        candidate_json
    }).map_err(|e| AppError::internal(format!("Failed to build the invariant candidate: {e}")))?;
    if !crate::domain::common::rules::task_invariant_0(&candidate) {
        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: "INVARIANT_VIOLATION", message: format!("Invariant {} of `Task` violated: total > 0", 0usize), details: Some(vec![crate::error::FieldError { field: "Task".to_string(), message: "invariantIndex: 0".to_string() }]), correlation_id: Some(correlation_id) });
    }"#;

    // ── the kernel-probe arm regions (issue #818) ────────────────────────
    //
    // Byte-exact for the region the flip changed: the head, the runtime
    // construction, the kernel probes, and the verdict — everything before
    // the idempotent-read early return. The unchanged tail (early return,
    // invariant splice, DTO decode, command, reload) is pinned by the
    // `contains` assertions below and by `handler_matrix_tests` (the byte
    // oracle over the whole handler).

    /// The NAMED-transition arm region (`state.op` non-empty): the op's
    /// declared targets are probed through the kernel; exactly one legal
    /// edge fires, none is the 409, an ambiguous fork names its targets.
    const NAMED_ARM_REGION: &str = concat!(
        "    // Executable `save`/`update` delegation (P0): patch the loaded row\n",
        "    // through the command plane. A machine-bound op resolves its target\n",
        "    // state from the record's current state; an unbound op may change the\n",
        "    // machine field only along a declared transition.\n",
        "    let mut value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
        "\n",
        "    let current_state = response.status.clone().unwrap_or_default();\n",
        "    // Kernel transition (issue #818, Option A): the machine decides —\n",
        "    // the hand-rendered match arms are retired. The candidate targets\n",
        "    // are probed through the runtime; exactly one legal edge from the\n",
        "    // current state fires; none or ambiguous is the 409.\n",
        "    let machine_embedded = domain_types::machines::machine_json(\"common_status\")\n",
        "        .ok_or_else(|| {\n",
        "            AppError::internal(\"machine `common_status` is not in the machines registry\").with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    let machine_ir: serde_json::Value = serde_json::from_str(machine_embedded)\n",
        "        .map_err(|e| {\n",
        "            AppError::internal(format!(\"machine `common_status` does not parse: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    let runtime = codegraph_workflow::MachineRuntime::from_json(&machine_ir)\n",
        "        .map_err(|e| {\n",
        "            AppError::internal(format!(\"machine `common_status` is invalid: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "\n",
        "    let legal: Vec<&str> = [\"Submitted\"]\n",
        "        .iter()\n",
        "        .copied()\n",
        "        .filter(|candidate| runtime.transition_for(&current_state, candidate).is_some())\n",
        "        .collect();\n",
        "    let target_state: &str = match legal.as_slice() {\n",
        "        [only] => *only,\n",
        "        [] => {\n",
        "            return Err(AppError::conflict(format!(\n",
        "                \"operation `submit` has no transition from the current state `{}`\",\n",
        "                current_state\n",
        "            )).with_correlation_id(correlation_id));\n",
        "        }\n",
        "        many => {\n",
        "            return Err(AppError::conflict(format!(\n",
        "                \"operation `submit` is ambiguous from the current state `{}`: the machine declares transitions to {}\",\n",
        "                current_state,\n",
        "                many.join(\", \")\n",
        "            )).with_correlation_id(correlation_id));\n",
        "        }\n",
        "    };\n",
        "    value[\"status\"] = serde_json::Value::String(target_state.to_string());\n",
    );

    /// The BODY-DRIVEN arm region (`state.op` empty): the candidate
    /// universe (the machine's declared targets) is probed per pair; a
    /// request naming the unique legal target synthesizes, none or
    /// ambiguous is the 409 naming the target.
    const WHITELIST_ARM_REGION: &str = concat!(
        "    // Executable `save`/`update` delegation (P0): patch the loaded row\n",
        "    // through the command plane. A machine-bound op resolves its target\n",
        "    // state from the record's current state; an unbound op may change the\n",
        "    // machine field only along a declared transition.\n",
        "    let mut value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
        "\n",
        "    let current_state = response.status.clone().unwrap_or_default();\n",
        "    // Kernel transition (issue #818, Option A): the machine decides —\n",
        "    // the hand-rendered match arms are retired. The candidate targets\n",
        "    // are probed through the runtime; exactly one legal edge from the\n",
        "    // current state fires; none or ambiguous is the 409.\n",
        "    let machine_embedded = domain_types::machines::machine_json(\"common_status\")\n",
        "        .ok_or_else(|| {\n",
        "            AppError::internal(\"machine `common_status` is not in the machines registry\").with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    let machine_ir: serde_json::Value = serde_json::from_str(machine_embedded)\n",
        "        .map_err(|e| {\n",
        "            AppError::internal(format!(\"machine `common_status` does not parse: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    let runtime = codegraph_workflow::MachineRuntime::from_json(&machine_ir)\n",
        "        .map_err(|e| {\n",
        "            AppError::internal(format!(\"machine `common_status` is invalid: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "\n",
        "    if let Some(target) = value.get(\"status\").and_then(|v| v.as_str()) {\n",
        "        if target != current_state {\n",
        "            let legal: Vec<&str> = [\"Submitted\", \"Approved\"]\n",
        "                .iter()\n",
        "                .copied()\n",
        "                .filter(|candidate| runtime.transition_for(&current_state, candidate).is_some())\n",
        "                .collect();\n",
        "            match legal.as_slice() {\n",
        "                [only] if *only == target => {\n",
        "                    value[\"status\"] = serde_json::Value::String(target.to_string());\n",
        "                }\n",
        "                [] => {\n",
        "                    return Err(AppError::conflict(format!(\n",
        "                        \"operation `changeStatus` cannot move `status` from `{}` to `{}`: no transition from the current state\",\n",
        "                        current_state, target\n",
        "                    )).with_correlation_id(correlation_id));\n",
        "                }\n",
        "                _ => {\n",
        "                    return Err(AppError::conflict(format!(\n",
        "                        \"operation `changeStatus` cannot move `status` from `{}` to `{}`: no unambiguous transition from the current state (legal targets: {})\",\n",
        "                        current_state,\n",
        "                        target,\n",
        "                        legal.join(\", \")\n",
        "                    )).with_correlation_id(correlation_id));\n",
        "                }\n",
        "            }\n",
        "        }\n",
        "    }\n",
    );

    /// The NAMED arm renders the kernel probes byte-exactly: the registry
    /// load, the fail-closed runtime, the probe, and the verdict — then
    /// the unchanged invariant splice and tail.
    #[test]
    fn update_named_transition_renders_the_kernel_probes() {
        let mut op = matrix_route(
            "submit",
            ExecutionKind::Update {
                state: Some(status_state(vec![ServiceOpTransition {
                    from: StateName::new("Draft"),
                    to: StateName::new("Submitted"),
                }])),
            },
        );
        op.invariant_block = Some(MATRIX_INVARIANT_BLOCK.to_string());
        let block = render_execution_block(&op);
        assert!(
            block.starts_with(NAMED_ARM_REGION),
            "the named arm must render the kernel probes exactly:\n{block}"
        );
        // The unchanged tail follows: the invariant splice glues onto the
        // early return, the DTO decode, the command, and the reload.
        assert!(block.contains("    }\n    // Design invariants (DD5, issue #666 slice 4d)"));
        assert!(
            block.contains("let cmd: crate::domain::common::task::dto_update::UpdateTaskRequest")
        );
        assert!(block.contains(r#""data": updated"#));
    }

    /// The BODY-DRIVEN arm renders the whitelist kernel probes byte-
    /// exactly. The candidate universe is the machine's declared targets,
    /// deduped in declaration order.
    #[test]
    fn update_body_driven_whitelist_renders_the_kernel_probes() {
        let op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        let block = render_execution_block(&op);
        assert!(
            block.starts_with(WHITELIST_ARM_REGION),
            "the whitelist arm must render the kernel probes exactly:\n{block}"
        );
    }

    /// An update with NO machine binding: the plain patch arm (one blank
    /// line where the template's `{% endif %}` sits), no state selection
    /// and no kernel load at all.
    #[test]
    fn update_without_a_machine_binding_patches_the_body() {
        let mut op = matrix_route("touch", ExecutionKind::Update { state: None });
        op.invariant_block = Some(MATRIX_INVARIANT_BLOCK.to_string());
        let block = render_execution_block(&op);
        assert!(!block.contains("machines::machine_json"));
        assert!(!block.contains("MachineRuntime"));
        assert!(block.contains("    }\n    // Design invariants (DD5, issue #666 slice 4d)"));
        assert!(
            block.contains("let cmd: crate::domain::common::task::dto_update::UpdateTaskRequest")
        );
    }

    // ── Option A semantics (issue #818): the three whitelist outcomes ────

    /// UNIQUE legal edge → SYNTHESIZES: the rendered arm's success arm is
    /// `[only] if *only == target` — the kernel legalized exactly the
    /// requested move and the handler writes it.
    #[test]
    fn whitelist_unique_legal_edge_synthesizes() {
        let op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        let block = render_execution_block(&op);
        assert!(
            block.contains("                [only] if *only == target => {"),
            "the unique-legal synthesis arm must render:\n{block}"
        );
        assert!(
            block.contains(
                "                    value[\"status\"] = serde_json::Value::String(target.to_string());"
            ),
            "{block}"
        );
    }

    /// AMBIGUOUS (a fork: two edges legal from one current state) → 409
    /// naming the target and the legal set.
    #[test]
    fn whitelist_ambiguous_fork_is_the_409_naming_the_target() {
        let op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        let block = render_execution_block(&op);
        assert!(
            block.contains("no unambiguous transition from the current state (legal targets: {})"),
            "{block}"
        );
        assert!(block.contains("                        legal.join(\", \")\n"));
    }

    /// NO legal edge → 409 naming the target with the contract message
    /// shape ("no transition from the current state").
    #[test]
    fn whitelist_no_legal_edge_is_the_409_with_the_contract_message() {
        let op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                "cannot move `status` from `{}` to `{}`: no transition from the current state"
            ),
            "{block}"
        );
    }

    /// Stay-put is NOT a transition: the arm skips the probes entirely
    /// when the request names the current state (`if target !=
    /// current_state`), so an unchanged status keeps today's allowed
    /// path.
    #[test]
    fn whitelist_stay_put_skips_the_kernel_probes() {
        let op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        let block = render_execution_block(&op);
        assert!(
            block.contains("        if target != current_state {\n"),
            "{block}"
        );
    }

    /// A whitelist machine with NO declared transitions renders an EMPTY
    /// candidate list (`[]`) — the probe loop stays closed, every move
    /// answers the none-409, and stay-put stays allowed.
    #[test]
    fn whitelist_without_declared_transitions_renders_an_empty_candidate_list() {
        let mut state = status_state(Vec::new());
        state.all = Vec::new();
        let op = matrix_route("changeStatus", ExecutionKind::Update { state: Some(state) });
        let block = render_execution_block(&op);
        assert!(
            block.contains("            let legal: Vec<&str> = []\n"),
            "{block}"
        );
    }

    /// A nested CHILD route reloads through the parent-scoped read
    /// (`find_by_id_scoped({id}, parent_id, …)`) — the machine arm is the
    /// kernel shape, the reload is the child shape.
    #[test]
    fn update_nested_child_reloads_through_the_scoped_read() {
        let mut op = matrix_route(
            "approveNote",
            ExecutionKind::Update {
                state: Some(status_state(vec![ServiceOpTransition {
                    from: StateName::new("Draft"),
                    to: StateName::new("Approved"),
                }])),
            },
        );
        op.render_facts = ExecutionRenderFacts {
            domain: "common".to_string(),
            entity_name: "TaskNote".to_string(),
            entity_table: "task_note".to_string(),
            id_param: "task_note_id".to_string(),
            domain_types_crate: "domain_types".to_string(),
            nested_child: true,
            auditable: false,
            ux_sort_fields: false,
        };
        let block = render_execution_block(&op);
        // The kernel arm shape (probes + verdict) with the child op's own
        // target, then the scoped reload.
        assert!(
            block.contains("let legal: Vec<&str> = [\"Approved\"]"),
            "{block}"
        );
        assert!(block.contains("MachineRuntime::from_json(&machine_ir)"));
        assert!(
            block.contains(
                "        .find_by_id_scoped(task_note_id, parent_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())"
            ),
            "a nested child reloads through the scoped read: {block}"
        );
    }

    /// The design-invariant block (attached by the handler generator AFTER
    /// the routes are built) glues straight onto the idempotent-read early
    /// return's `}` — the block's own leading newline starts its first
    /// comment line, and no blank line separates them.
    #[test]
    fn update_splices_the_invariant_block_after_the_idempotent_read() {
        let mut op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        assert!(
            !render_execution_block(&op).contains("Design invariants"),
            "the route carries no invariant block: nothing may be spliced"
        );
        op.invariant_block = Some(MATRIX_INVARIANT_BLOCK.to_string());
        let block = render_execution_block(&op);
        assert!(
            block.contains("    }\n    // Design invariants (DD5, issue #666 slice 4d)"),
            "the block must start on the `}}` line itself (its leading newline): {block}"
        );
        assert!(
            !block.contains("    }\n\n    // Design invariants"),
            "no blank line may separate the early return from the invariant block"
        );
        assert!(
            block.contains("    }\n    let cmd: crate::domain::common::task::dto_update"),
            "the DTO decode follows the invariant block's last line: {block}"
        );
    }

    /// Without an invariant block the tail follows the early return directly
    /// — the `{% endif %}` newline, byte-identical to the pre-`invariant`
    /// shape.
    #[test]
    fn update_without_an_invariant_block_has_no_splice() {
        let op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        let block = render_execution_block(&op);
        assert!(!block.contains("Design invariants"));
        assert!(
            block.contains("    }\n    let cmd: crate::domain::common::task::dto_update"),
            "the DTO decode follows the early return's `}}` with no blank line: {block}"
        );
    }

    /// An AUDITABLE entity's query plane takes the `include_deleted`
    /// argument — the reload carries the extra `, false` on both shapes.
    #[test]
    fn update_auditable_reload_passes_the_include_deleted_arity() {
        let mut op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        op.render_facts.auditable = true;
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                "        .find_by_id(task_id, false, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())"
            ),
            "an auditable root passes `, false`: {block}"
        );

        op.render_facts.nested_child = true;
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                "        .find_by_id_scoped(task_id, parent_id, false, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())"
            ),
            "an auditable child passes `, false` after the parent id: {block}"
        );
    }

    // ── the END-STATE pin (issue #818): the match arms are RETIRED ───────

    /// NO rendered update arm may carry the retired match-arm vocabulary —
    /// neither `match current_state` nor the `from => Some(to)` arm
    /// spelling nor the old whitelist chain — and the machine-bound arms
    /// must carry the kernel probe positively.
    #[test]
    fn the_match_arm_vocabulary_is_retired() {
        let shapes = vec![
            // named arm
            ExecutionKind::Update {
                state: Some(status_state(vec![ServiceOpTransition {
                    from: StateName::new("Draft"),
                    to: StateName::new("Submitted"),
                }])),
            },
            // whitelist arm
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
            // no machine
            ExecutionKind::Update { state: None },
        ];
        for execution in shapes {
            let op = matrix_route("op", execution);
            let block = render_execution_block(&op);
            assert!(
                !block.contains("match current_state"),
                "the retired match-arm vocabulary must not render:\n{block}"
            );
            assert!(
                !block.contains("=> Some(\""),
                "the retired arm spelling must not render:\n{block}"
            );
            assert!(
                !block.contains("let allowed = target == current_state"),
                "the retired whitelist chain must not render:\n{block}"
            );
        }
        // The machine-bound arms carry the kernel, positively.
        let named = matrix_route(
            "submit",
            ExecutionKind::Update {
                state: Some(status_state(vec![ServiceOpTransition {
                    from: StateName::new("Draft"),
                    to: StateName::new("Submitted"),
                }])),
            },
        );
        let block = render_execution_block(&named);
        assert!(block.contains("codegraph_workflow::MachineRuntime::from_json(&machine_ir)"));
        assert!(block.contains("runtime.transition_for(&current_state, candidate).is_some()"));
    }
}
