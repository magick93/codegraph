//! The `save`/`update` service-op arm: patch the loaded row through the
//! update command, with the machine-binding selection (the named transition
//! `match` or the body-driven whitelist) and the design-invariant splice.

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
/// machine-bound op resolves its target state from the record's current state
/// (the named `match`, or the body-driven whitelist when the op selects no
/// transition); the design invariants (when the handler attached a block)
/// evaluate the candidate pre-write.
pub(super) fn update_block(op: &ServiceOpRoute, state: Option<&ServiceOpState>) -> String {
    let mut block = String::from(UPDATE_HEAD);

    match state {
        Some(state) => {
            block.push_str("\n    let current_state = ");
            block.push_str(&state.current_expr);
            block.push_str(";\n");
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

/// The named-transition arm: a `match` over the record's current state
/// selecting the op's declared target, refusing with 409 when the state
/// allows no transition.
fn push_named_transition_arm(block: &mut String, op: &ServiceOpRoute, state: &ServiceOpState) {
    block.push_str(
        "\n    let target_state: Option<&'static str> = match current_state.as_str() {\n",
    );
    for transition in &state.op {
        block.push_str(&format!(
            "        \"{}\" => Some(\"{}\"),\n",
            transition.from, transition.to
        ));
    }
    block.push_str("        _ => None,\n");
    block.push_str("    };\n");
    block.push_str("    let Some(target_state) = target_state else {\n");
    block.push_str("        return Err(AppError::conflict(format!(\n");
    block.push_str(&format!(
        "            \"operation `{}` has no transition from the current state `{{}}`\",\n",
        op.op_name
    ));
    block.push_str("            current_state\n");
    block.push_str("        )).with_correlation_id(correlation_id));\n");
    block.push_str("    };\n");
    block.push_str(&format!(
        "    value[\"{}\"] = serde_json::Value::String(target_state.to_string());\n",
        state.wire_key.as_str()
    ));
}

/// The body-driven arm: the op selects no transition of its own, so a request
/// that moves the machine field may only follow one of the machine's declared
/// transitions (or stay put). The chain is the machine's `all` table.
fn push_whitelist_arm(block: &mut String, op: &ServiceOpRoute, state: &ServiceOpState) {
    block.push_str("\n    if let Some(target) = value.get(\"");
    block.push_str(state.wire_key.as_str());
    block.push_str("\").and_then(|v| v.as_str()) {\n");
    block.push_str("        let allowed = target == current_state");
    for transition in &state.all {
        block.push_str(&format!(
            "\n            || (current_state == \"{}\" && target == \"{}\")",
            transition.from, transition.to
        ));
    }
    block.push_str(";\n");
    block.push_str("        if !allowed {\n");
    block.push_str("            return Err(AppError::conflict(format!(\n");
    block.push_str(&format!(
        "                \"operation `{}` cannot move `{}` from `{{}}` to `{{}}`\",\n",
        op.op_name,
        state.wire_key.as_str()
    ));
    block.push_str("                current_state, target\n");
    block.push_str("            )).with_correlation_id(correlation_id));\n");
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

    // ── T3.2 fixtures: the handler-matrix `root_service_ops` /
    //    `service_ops_child_roles` permutations ────────────────────────────
    //
    // The expected blocks below are the ARMS EXTRACTED VERBATIM from the
    // blessed snapshots (`<temp>/codegraph-handler-matrix/expected/
    // {root_service_ops,service_ops_child_roles}.rs`), so these KATs pin the
    // renderer against real template output, not against a re-reading of the
    // template source.

    /// The matrix fixture's `common`/`task` facts (root, non-auditable).
    fn matrix_facts() -> ExecutionRenderFacts {
        test_support::matrix_facts()
    }

    fn matrix_route(op_name: &str, execution: ExecutionKind) -> ServiceOpRoute {
        test_support::route("TaskService", op_name, matrix_facts(), execution, true)
    }

    /// The fixture machine: `Draft -> Submitted -> Approved`.
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
            all: machine_transitions(),
            op,
        }
    }

    const MATRIX_NAMED_BLOCK: &str = concat!(
        "    // Executable `save`/`update` delegation (P0): patch the loaded row\n",
        "    // through the command plane. A machine-bound op resolves its target\n",
        "    // state from the record's current state; an unbound op may change the\n",
        "    // machine field only along a declared transition.\n",
        "    let mut value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
        "\n",
        "    let current_state = response.status.clone().unwrap_or_default();\n",
        "\n",
        "    let target_state: Option<&'static str> = match current_state.as_str() {\n",
        "        \"Draft\" => Some(\"Submitted\"),\n",
        "        _ => None,\n",
        "    };\n",
        "    let Some(target_state) = target_state else {\n",
        "        return Err(AppError::conflict(format!(\n",
        "            \"operation `submit` has no transition from the current state `{}`\",\n",
        "            current_state\n",
        "        )).with_correlation_id(correlation_id));\n",
        "    };\n",
        "    value[\"status\"] = serde_json::Value::String(target_state.to_string());\n",
        "\n",
        "\n",
        "    // A save with no fields to change (no body, no transition) is an\n",
        "    // idempotent read — do not issue a no-op UPDATE.\n",
        "    if value.as_object().is_some_and(|o| o.is_empty()) {\n",
        "        return Ok(axum::Json(serde_json::json!({\n",
        "            \"data\": response,\n",
        "            \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "        })).into_response());\n",
        "    }\n",
        "    // Design invariants (DD5, issue #666 slice 4d): the candidate is\n",
        "    // the loaded record with the action patch — the machine state move\n",
        "    // included — applied; evaluated BEFORE the write.\n",
        "    let candidate: domain_types::common::task::dto_response::TaskResponse = serde_json::from_value({\n",
        "        let mut candidate_json = serde_json::to_value(&response).unwrap_or_default();\n",
        "        if let (Some(candidate_obj), Some(patch)) = (candidate_json.as_object_mut(), value.as_object()) {\n",
        "            for (key, item) in patch {\n",
        "                candidate_obj.insert(key.clone(), item.clone());\n",
        "            }\n",
        "        }\n",
        "        candidate_json\n",
        "    }).map_err(|e| AppError::internal(format!(\"Failed to build the invariant candidate: {e}\")))?;\n",
        "    if !crate::domain::common::rules::task_invariant_0(&candidate) {\n",
        "        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: \"INVARIANT_VIOLATION\", message: format!(\"Invariant {} of `Task` violated: total > 0\", 0usize), details: Some(vec![crate::error::FieldError { field: \"Task\".to_string(), message: \"invariantIndex: 0\".to_string() }]), correlation_id: Some(correlation_id) });\n",
        "    }\n",
        "    let cmd: crate::domain::common::task::dto_update::UpdateTaskRequest =\n",
        "        serde_json::from_value(value).map_err(|e| {\n",
        "            AppError::bad_request(format!(\"invalid Task update body: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    state.common_task_commands.update(\n",
        "        task_id,\n",
        "        cmd,\n",
        "        domain_types::SourceContext::api(),\n",
        "        correlation_id,\n",
        "        api_key_info.api_key_id,\n",
        "        api_key_info.organization_id,\n",
        "        api_key_info.user_id,\n",
        "        api_key_info.role.clone(),\n",
        "    ).await.map_err(|e: CommonError| {\n",
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to update Task\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let updated = state.common_task_queries\n",
        "        .find_by_id(task_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(\"Task not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": updated,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    const MATRIX_WHITELIST_BLOCK: &str = concat!(
        "    // Executable `save`/`update` delegation (P0): patch the loaded row\n",
        "    // through the command plane. A machine-bound op resolves its target\n",
        "    // state from the record's current state; an unbound op may change the\n",
        "    // machine field only along a declared transition.\n",
        "    let mut value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
        "\n",
        "    let current_state = response.status.clone().unwrap_or_default();\n",
        "\n",
        "    if let Some(target) = value.get(\"status\").and_then(|v| v.as_str()) {\n",
        "        let allowed = target == current_state\n",
        "            || (current_state == \"Draft\" && target == \"Submitted\")\n",
        "            || (current_state == \"Submitted\" && target == \"Approved\");\n",
        "        if !allowed {\n",
        "            return Err(AppError::conflict(format!(\n",
        "                \"operation `changeStatus` cannot move `status` from `{}` to `{}`\",\n",
        "                current_state, target\n",
        "            )).with_correlation_id(correlation_id));\n",
        "        }\n",
        "    }\n",
        "\n",
        "\n",
        "    // A save with no fields to change (no body, no transition) is an\n",
        "    // idempotent read — do not issue a no-op UPDATE.\n",
        "    if value.as_object().is_some_and(|o| o.is_empty()) {\n",
        "        return Ok(axum::Json(serde_json::json!({\n",
        "            \"data\": response,\n",
        "            \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "        })).into_response());\n",
        "    }\n",
        "    let cmd: crate::domain::common::task::dto_update::UpdateTaskRequest =\n",
        "        serde_json::from_value(value).map_err(|e| {\n",
        "            AppError::bad_request(format!(\"invalid Task update body: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    state.common_task_commands.update(\n",
        "        task_id,\n",
        "        cmd,\n",
        "        domain_types::SourceContext::api(),\n",
        "        correlation_id,\n",
        "        api_key_info.api_key_id,\n",
        "        api_key_info.organization_id,\n",
        "        api_key_info.user_id,\n",
        "        api_key_info.role.clone(),\n",
        "    ).await.map_err(|e: CommonError| {\n",
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to update Task\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let updated = state.common_task_queries\n",
        "        .find_by_id(task_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(\"Task not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": updated,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    const MATRIX_NO_MACHINE_BLOCK: &str = concat!(
        "    // Executable `save`/`update` delegation (P0): patch the loaded row\n",
        "    // through the command plane. A machine-bound op resolves its target\n",
        "    // state from the record's current state; an unbound op may change the\n",
        "    // machine field only along a declared transition.\n",
        "    let mut value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
        "\n",
        "    // A save with no fields to change (no body, no transition) is an\n",
        "    // idempotent read — do not issue a no-op UPDATE.\n",
        "    if value.as_object().is_some_and(|o| o.is_empty()) {\n",
        "        return Ok(axum::Json(serde_json::json!({\n",
        "            \"data\": response,\n",
        "            \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "        })).into_response());\n",
        "    }\n",
        "    // Design invariants (DD5, issue #666 slice 4d): the candidate is\n",
        "    // the loaded record with the action patch — the machine state move\n",
        "    // included — applied; evaluated BEFORE the write.\n",
        "    let candidate: domain_types::common::task::dto_response::TaskResponse = serde_json::from_value({\n",
        "        let mut candidate_json = serde_json::to_value(&response).unwrap_or_default();\n",
        "        if let (Some(candidate_obj), Some(patch)) = (candidate_json.as_object_mut(), value.as_object()) {\n",
        "            for (key, item) in patch {\n",
        "                candidate_obj.insert(key.clone(), item.clone());\n",
        "            }\n",
        "        }\n",
        "        candidate_json\n",
        "    }).map_err(|e| AppError::internal(format!(\"Failed to build the invariant candidate: {e}\")))?;\n",
        "    if !crate::domain::common::rules::task_invariant_0(&candidate) {\n",
        "        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: \"INVARIANT_VIOLATION\", message: format!(\"Invariant {} of `Task` violated: total > 0\", 0usize), details: Some(vec![crate::error::FieldError { field: \"Task\".to_string(), message: \"invariantIndex: 0\".to_string() }]), correlation_id: Some(correlation_id) });\n",
        "    }\n",
        "    let cmd: crate::domain::common::task::dto_update::UpdateTaskRequest =\n",
        "        serde_json::from_value(value).map_err(|e| {\n",
        "            AppError::bad_request(format!(\"invalid Task update body: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    state.common_task_commands.update(\n",
        "        task_id,\n",
        "        cmd,\n",
        "        domain_types::SourceContext::api(),\n",
        "        correlation_id,\n",
        "        api_key_info.api_key_id,\n",
        "        api_key_info.organization_id,\n",
        "        api_key_info.user_id,\n",
        "        api_key_info.role.clone(),\n",
        "    ).await.map_err(|e: CommonError| {\n",
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to update Task\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let updated = state.common_task_queries\n",
        "        .find_by_id(task_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(\"Task not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": updated,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    const MATRIX_CHILD_BLOCK: &str = concat!(
        "    // Executable `save`/`update` delegation (P0): patch the loaded row\n",
        "    // through the command plane. A machine-bound op resolves its target\n",
        "    // state from the record's current state; an unbound op may change the\n",
        "    // machine field only along a declared transition.\n",
        "    let mut value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
        "\n",
        "    let current_state = response.status.clone().unwrap_or_default();\n",
        "\n",
        "    let target_state: Option<&'static str> = match current_state.as_str() {\n",
        "        \"Draft\" => Some(\"Approved\"),\n",
        "        _ => None,\n",
        "    };\n",
        "    let Some(target_state) = target_state else {\n",
        "        return Err(AppError::conflict(format!(\n",
        "            \"operation `approveNote` has no transition from the current state `{}`\",\n",
        "            current_state\n",
        "        )).with_correlation_id(correlation_id));\n",
        "    };\n",
        "    value[\"status\"] = serde_json::Value::String(target_state.to_string());\n",
        "\n",
        "\n",
        "    // A save with no fields to change (no body, no transition) is an\n",
        "    // idempotent read — do not issue a no-op UPDATE.\n",
        "    if value.as_object().is_some_and(|o| o.is_empty()) {\n",
        "        return Ok(axum::Json(serde_json::json!({\n",
        "            \"data\": response,\n",
        "            \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "        })).into_response());\n",
        "    }\n",
        "    let cmd: crate::domain::common::task_note::dto_update::UpdateTaskNoteRequest =\n",
        "        serde_json::from_value(value).map_err(|e| {\n",
        "            AppError::bad_request(format!(\"invalid TaskNote update body: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    state.common_task_note_commands.update(\n",
        "        task_note_id,\n",
        "        cmd,\n",
        "        domain_types::SourceContext::api(),\n",
        "        correlation_id,\n",
        "        api_key_info.api_key_id,\n",
        "        api_key_info.organization_id,\n",
        "        api_key_info.user_id,\n",
        "        api_key_info.role.clone(),\n",
        "    ).await.map_err(|e: CommonError| {\n",
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to update TaskNote\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let updated = state.common_task_note_queries\n",
        "        .find_by_id_scoped(task_note_id, parent_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload TaskNote\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(\"TaskNote not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": updated,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

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

    /// The NAMED-transition arm (`state.op` non-empty): the runtime `match`
    /// selects the op's declared target, refusing 409 when the current state
    /// allows none — then the design-invariant block, the DTO patch, the
    /// command and the reload envelope.
    #[test]
    fn update_named_transition_renders_the_match_and_the_patch() {
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
        assert_eq!(render_execution_block(&op), MATRIX_NAMED_BLOCK);
    }

    /// The BODY-DRIVEN arm (`state.op` empty): the request's own value must
    /// follow one of the machine's declared transitions (or stay put) — the
    /// `state.all` whitelist chain, no `match`.
    #[test]
    fn update_body_driven_whitelist_renders_the_allowed_chain() {
        let op = matrix_route(
            "changeStatus",
            ExecutionKind::Update {
                state: Some(status_state(Vec::new())),
            },
        );
        assert_eq!(render_execution_block(&op), MATRIX_WHITELIST_BLOCK);
    }

    /// An update with NO machine binding: the plain patch arm (one blank line
    /// where the template's `{% endif %}` sits), no state selection at all.
    #[test]
    fn update_without_a_machine_binding_patches_the_body() {
        let mut op = matrix_route("touch", ExecutionKind::Update { state: None });
        op.invariant_block = Some(MATRIX_INVARIANT_BLOCK.to_string());
        assert_eq!(render_execution_block(&op), MATRIX_NO_MACHINE_BLOCK);
    }

    /// A nested CHILD route reloads through the parent-scoped read
    /// (`find_by_id_scoped({id}, parent_id, …)`), not the plain one.
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
        };
        assert_eq!(render_execution_block(&op), MATRIX_CHILD_BLOCK);
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

    /// A whitelist machine with NO declared transitions still renders a
    /// valid check (`let allowed = target == current_state;`) — the chain is
    /// empty, the arms are closed, never a syntax error.
    #[test]
    fn update_whitelist_without_declared_transitions_stays_syntactically_closed() {
        let mut state = status_state(Vec::new());
        state.all = Vec::new();
        let op = matrix_route("changeStatus", ExecutionKind::Update { state: Some(state) });
        let block = render_execution_block(&op);
        assert!(block.contains("        let allowed = target == current_state;\n"));
        assert!(!block.contains("            || (current_state =="));
    }
}
