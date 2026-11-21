//! Pre-rendered execution bodies for the service-op handler arms
//! (issue #778).
//!
//! The service-op execution arms live in `templates/api/handler.tera`'s
//! `{% for op in service_ops %}` loop. They move into Rust one arm at a
//! time, simplest first: [`render_execution_block`] ports the `seam`,
//! `read` and `delete` bodies VERBATIM (statements, comments, indentation
//! and blank lines included) so the template can later render
//! `{{ op.execution_block }}` in their place byte-for-byte.
//!
//! Only the BODY region moves — the doc comment, the `#[utoipa::path]`
//! header, the signature and the record-load/authorize prelude stay
//! template-owned for now. Every kind this module does not own renders as
//! an empty string, so an unwired arm keeps its template block.

use codegraph_naming::to_pascal_case;

use super::types::{
    BoundaryPayloadPlan, ChildAction, ChildParentLoad, ExecutionKind, ServiceOpRoute,
    ServiceOpState,
};

/// The seam arm's record-load preamble: the loaded record IS the route's
/// 404 contract, so it is acknowledged rather than dropped.
const SEAM_RECORD_LOAD_PREAMBLE: &str = concat!(
    "    // The record load is the route's 404 contract; the body has no\n",
    "    // executable lowering yet.\n",
    "    let _ = &response;\n",
    "\n",
);

/// The read arm: the loaded record is returned in the data envelope.
const READ_BLOCK: &str = concat!(
    "    Ok(axum::Json(serde_json::json!({\n",
    "        \"data\": response,\n",
    "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
    "    })).into_response())",
);

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

/// The handler-template execution body for one service op (issue #778).
///
/// Returns the verbatim Rust body the template emits for the op's
/// [`ExecutionKind`] — `seam` (the 501 naming the unlowered gap), `read`
/// (the loaded-record envelope), `delete` (the command call plus the ok
/// envelope), `update` (the machine-binding selection, the allowed-state
/// check, the design-invariant splice, the DTO patch, the command call and
/// the reload envelope) and `child_op` (the aggregate child `add`/`save`/
/// `remove` plane: the parent load-or-404, the boundary payload or request
/// body, the child's command call and its reload envelope). Every other kind
/// returns an empty string: its arm is still template-owned, so the later
/// wiring stays byte-identical for it.
///
/// The block is a pure function of the route as handed in: the update arm
/// reads `op.invariant_block` (the handler generator attaches it AFTER
/// `routes_for_entity`, so that plane re-renders the block once the block is
/// wired).
///
/// The block begins at the arm's first body line and carries NO trailing
/// newline — the wiring template supplies the line break (the
/// `invariant_block` convention), so `{{ op.execution_block }}` on its own
/// line reproduces the current arm byte-for-byte. The newlines the
/// template's `{% if %}` tags contribute around the arm stay the template's.
#[must_use]
pub fn render_execution_block(op: &ServiceOpRoute) -> String {
    match &op.execution {
        ExecutionKind::Delete => delete_block(op),
        ExecutionKind::Read => READ_BLOCK.to_string(),
        ExecutionKind::Seam { reason } => seam_block(op.record_load, reason),
        ExecutionKind::Update { state } => update_block(op, state.as_ref()),
        ExecutionKind::ChildOp {
            action,
            feature,
            parent_load,
            payload,
            ..
        } => child_op_block(op, *action, feature, parent_load.as_ref(), payload.as_ref()),
        _ => String::new(),
    }
}

/// The `child_op` arm (issue #661): a `children <Root> { … }` operation that
/// drives the CHILD's command plane under the parent's aggregate path.
///
/// `add` appends one element (proving the parent row first — the #692
/// load-or-404), `save` patches the parent-scoped element, `remove` deletes
/// it. `add` also carries the #767 modeled-boundary branch: when the op
/// declares an input boundary the handler CONSTRUCTS the child's request DTO
/// from it instead of decoding the generated `Create…Body`.
fn child_op_block(
    op: &ServiceOpRoute,
    action: ChildAction,
    feature: &str,
    parent_load: Option<&ChildParentLoad>,
    payload: Option<&BoundaryPayloadPlan>,
) -> String {
    let facts = &op.render_facts;
    let mut block = format!(
        concat!(
            "    // Aggregate child operation (issue #661): the root's\n",
            "    // `children {service}` block declared this `{action}`\n",
            "    // over its `{feature}` child collection. It runs against the\n",
            "    // CHILD's plane, under the parent's aggregate path, so the element can\n",
            "    // never land under another parent.\n",
        ),
        service = op.service,
        action = action.as_str(),
        feature = feature,
    );
    match action {
        ChildAction::Add => {
            block.push('\n');
            if let Some(load) = parent_load {
                block.push('\n');
                block.push_str(&parent_load_block(load, facts));
            }
            // The `{% endif %}` that closes the parent-load arm, then the
            // item arm's own opening tag newline (`{% if payload %}` /
            // `{% else %}`).
            block.push('\n');
            block.push('\n');
            match payload {
                Some(payload) => block.push_str(&add_boundary_item(payload, facts)),
                None => block.push_str(&add_body_match_item(facts)),
            }
            block.push('\n');
            block.push_str(CHILD_VALIDATE_BLOCK);
            block.push_str(&add_create_tail(facts));
        }
        ChildAction::Save => {
            block.push('\n');
            block.push_str(CHILD_SAVE_INTRO);
            block.push('\n');
            block.push_str(&match payload {
                Some(payload) => save_boundary_command(payload, facts),
                None => save_decoded_command(facts),
            });
            block.push('\n');
            block.push_str(&save_tail(facts));
        }
        ChildAction::Remove => {
            block.push('\n');
            block.push_str(&remove_block(facts));
        }
    }
    // Every sub-block self-terminates its last line; the block as a whole
    // carries NO trailing newline (the wiring template supplies it).
    debug_assert!(block.ends_with('\n'));
    block.pop();
    block
}

/// The #692 parent load-or-404: the create writes the parent FK from the
/// path, so it only runs for a parent the session can see (the RLS-scoped
/// query plane answers None for a bogus or foreign-org parent → 404, never
/// Postgres `23503` as a 500).
fn parent_load_block(load: &ChildParentLoad, facts: &super::types::ExecutionRenderFacts) -> String {
    let domain_error = domain_error(facts);
    let audit = if load.auditable { ", false" } else { "" };
    format!(
        concat!(
            "    // Issue #692: the parent load is the route's 404 contract. The create\n",
            "    // below writes the parent FK from the path, so it only runs for a\n",
            "    // parent the session can see — the RLS-scoped query plane answers None\n",
            "    // for a bogus or foreign-org parent, and the route answers 404 (never\n",
            "    // Postgres 23503 as a 500).\n",
            "    state.{domain}_{module}_queries\n",
            "        .find_by_id(parent_id{audit}, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
            "        .await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to find {entity}\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?\n",
            "        .ok_or_else(|| AppError::not_found(format!(\"{entity} {{parent_id}} not found\")).with_correlation_id(correlation_id))?;\n",
        ),
        domain = load.domain,
        module = load.module,
        audit = audit,
        domain_error = domain_error,
        entity = load.entity,
    )
}

/// The #767 add item: the modeled boundary IS the payload, mapped onto the
/// child's generated `Create…Request` at generation time. `extraction` and
/// `construct` are pre-rendered (they end with their own newline).
fn add_boundary_item(
    payload: &BoundaryPayloadPlan,
    facts: &super::types::ExecutionRenderFacts,
) -> String {
    let mut block = format!(
        concat!(
            "    // Issue #767: the modeled boundary IS the payload — its members were\n",
            "    // mapped onto the child's generated Create{entity}Request at\n",
            "    // generation time.\n",
        ),
        entity = facts.entity_name,
    );
    if !payload.defaulted.is_empty() {
        block.push_str(&format!(
            "    // Not carried by the boundary, defaulted to `None`: {}.\n",
            payload.defaulted.join(", ")
        ));
    }
    block.push_str(&payload.extraction);
    block.push_str(&format!(
        "    let item = Create{}Request {{\n",
        facts.entity_name
    ));
    block.push_str(&payload.construct);
    block.push_str("    };\n");
    block
}

/// The add item without a modeled boundary: decode the generated
/// `Create…Body` (rejecting a bulk array — a child action adds one element).
fn add_body_match_item(facts: &super::types::ExecutionRenderFacts) -> String {
    format!(
        concat!(
            "    let item = match body {{\n",
            "        Create{entity}Body::Single(item) => item,\n",
            "        Create{entity}Body::Bulk(_) => {{\n",
            "            return Err(AppError::bad_request(\n",
            "                \"A child action adds one element — send a single JSON object, not an array\",\n",
            "            )\n",
            "            .with_correlation_id(correlation_id));\n",
            "        }}\n",
            "    }};\n",
        ),
        entity = facts.entity_name,
    )
}

/// The `garde` validation both `add` item shapes run before the create.
const CHILD_VALIDATE_BLOCK: &str = concat!(
    "    if let Err(report) = garde::Validate::validate(&item) {\n",
    "        let details: Vec<crate::error::FieldError> = report.into_inner()\n",
    "            .into_iter()\n",
    "            .map(|(path, error)| crate::error::FieldError {\n",
    "                field: path.to_string(),\n",
    "                message: error.message().to_string(),\n",
    "            })\n",
    "            .collect();\n",
    "        return Err(AppError::validation(\"Request validation failed\", details)\n",
    "            .with_correlation_id(correlation_id));\n",
    "    }\n",
);

/// The add tail: create-and-link, then reload the created element (the
/// `include_deleted` arity when the CHILD is auditable) and return it.
fn add_create_tail(facts: &super::types::ExecutionRenderFacts) -> String {
    format!(
        concat!(
            "    let created_id = state.{domain}_{table}_commands.create(\n",
            "        item,\n",
            "        parent_id,\n",
            "        {types}::SourceContext::api(),\n",
            "        correlation_id,\n",
            "        api_key_info.api_key_id,\n",
            "        api_key_info.organization_id,\n",
            "        api_key_info.user_id,\n",
            "        api_key_info.role.clone(),\n",
            "    ).await.map_err(|e: {domain_error}| {{\n",
            "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to add the {entity} child\")\n",
            "            .with_correlation_id(correlation_id)\n",
            "    }})?;\n",
            "    let created = state.{domain}_{table}_queries\n",
            "        .find_by_id(created_id{audit}, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
            "        .await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload the {entity} child\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?\n",
            "        .ok_or_else(|| AppError::internal(\"Created child not found\").with_correlation_id(correlation_id))?;\n",
            "    Ok(axum::Json(serde_json::json!({{\n",
            "        \"data\": created,\n",
            "        \"meta\": {{ \"correlation_id\": correlation_id.to_string() }}\n",
            "    }})).into_response())\n",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        types = facts.domain_types_crate,
        domain_error = domain_error(facts),
        entity = facts.entity_name,
        audit = audit_arity(facts),
    )
}

/// The `save` arm's ownership note: the scoped load above IS the parent
/// contract, and the update re-reads the row through the command plane.
const CHILD_SAVE_INTRO: &str = concat!(
    "    // The scoped load above is the parent-ownership contract; the update\n",
    "    // itself re-reads the row through the command plane.\n",
    "    let _ = &response;\n",
);

/// The #767 save command: the modeled boundary IS the payload, mapped onto
/// the child's generated `Update…Request` at generation time.
fn save_boundary_command(
    payload: &BoundaryPayloadPlan,
    facts: &super::types::ExecutionRenderFacts,
) -> String {
    let mut block = format!(
        concat!(
            "    // Issue #767: the modeled boundary IS the payload — mapped onto the\n",
            "    // child's generated Update{entity}Request at generation time.\n",
        ),
        entity = facts.entity_name,
    );
    block.push_str(&payload.extraction);
    block.push_str(&format!(
        concat!(
            "    let cmd: crate::domain::{domain}::{table}::dto_update::Update{entity}Request =\n",
            "        Update{entity}Request {{\n",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        entity = facts.entity_name,
    ));
    block.push_str(&payload.construct);
    block.push_str("    };\n");
    block
}

/// The save command without a modeled boundary: decode the request body
/// into the child's generated `Update…Request`.
fn save_decoded_command(facts: &super::types::ExecutionRenderFacts) -> String {
    format!(
        concat!(
            "    let value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({{}}));\n",
            "    let cmd: crate::domain::{domain}::{table}::dto_update::Update{entity}Request =\n",
            "        serde_json::from_value(value).map_err(|e| {{\n",
            "            AppError::bad_request(format!(\"invalid {entity} update body: {{e}}\"))\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?;\n",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        entity = facts.entity_name,
    )
}

/// The save tail: the child's update command, the reload (the
/// `include_deleted` arity when the CHILD is auditable) and the envelope.
fn save_tail(facts: &super::types::ExecutionRenderFacts) -> String {
    format!(
        concat!(
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
            "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to save the {entity} child\")\n",
            "            .with_correlation_id(correlation_id)\n",
            "    }})?;\n",
            "    let saved = state.{domain}_{table}_queries\n",
            "        .find_by_id({param}{audit}, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
            "        .await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload the {entity} child\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?\n",
            "        .ok_or_else(|| AppError::not_found(\"{entity} not found\").with_correlation_id(correlation_id))?;\n",
            "    Ok(axum::Json(serde_json::json!({{\n",
            "        \"data\": saved,\n",
            "        \"meta\": {{ \"correlation_id\": correlation_id.to_string() }}\n",
            "    }})).into_response())\n",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        param = facts.id_param,
        types = facts.domain_types_crate,
        domain_error = domain_error(facts),
        entity = facts.entity_name,
        audit = audit_arity(facts),
    )
}

/// The `remove` arm: the parent-scoped record the route already loaded IS
/// the ownership proof, so the delete goes straight to the child's command
/// plane.
fn remove_block(facts: &super::types::ExecutionRenderFacts) -> String {
    format!(
        concat!(
            "    // `remove` deletes the element through the parent-scoped record the\n",
            "    // route already loaded — a child of another parent 404s above.\n",
            "    let _ = response;\n",
            "    state.{domain}_{table}_commands.delete({param}, {types}::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to remove the {entity} child\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?;\n",
            "    Ok(axum::Json(serde_json::json!({{ \"data\": {{ \"ok\": true }} }})).into_response())\n",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        param = facts.id_param,
        types = facts.domain_types_crate,
        domain_error = domain_error(facts),
        entity = facts.entity_name,
    )
}

/// The `{Domain}Error` type the emitted `map_err` closures annotate.
fn domain_error(facts: &super::types::ExecutionRenderFacts) -> String {
    format!("{}Error", to_pascal_case(&facts.domain))
}

/// The `include_deleted` argument for the auditable query arity.
fn audit_arity(facts: &super::types::ExecutionRenderFacts) -> &'static str {
    if facts.auditable { ", false" } else { "" }
}

/// The `update` arm: patch the loaded row through the update command. A
/// machine-bound op resolves its target state from the record's current state
/// (the named `match`, or the body-driven whitelist when the op selects no
/// transition); the design invariants (when the handler attached a block)
/// evaluate the candidate pre-write.
fn update_block(op: &ServiceOpRoute, state: Option<&ServiceOpState>) -> String {
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

/// The `delete` arm: the entity's command plane deletes the row and the
/// route answers the ok envelope. The AppState slot and the error prose
/// render with the very names the handler template's own context carries
/// ([`super::types::ExecutionRenderFacts`]).
fn delete_block(op: &ServiceOpRoute) -> String {
    let facts = &op.render_facts;
    let domain_error = format!("{}Error", to_pascal_case(&facts.domain));
    format!(
        concat!(
            "    state.{domain}_{table}_commands.delete({param}, {types}::SourceContext::api(), ",
            "correlation_id, api_key_info.api_key_id, api_key_info.organization_id, ",
            "api_key_info.user_id, api_key_info.role.clone()).await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), ",
            "\"Failed to delete {entity}\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?;\n",
            "    Ok(axum::Json(serde_json::json!({{ \"data\": {{ \"ok\": true }} }})).into_response())",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        param = facts.id_param,
        types = facts.domain_types_crate,
        entity = facts.entity_name,
        domain_error = domain_error,
    )
}

/// The 501 seam arm: names the gap, after acknowledging the record load
/// when the route performs one.
fn seam_block(record_load: bool, reason: &str) -> String {
    let mut block = String::new();
    if record_load {
        block.push_str(SEAM_RECORD_LOAD_PREAMBLE);
    }
    block.push_str("    Err(service_op_not_implemented(\n");
    block.push_str(&format!("        \"{reason}\",\n"));
    block.push_str("    ))");
    block
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddd::body_lower::BodyPlan;
    use crate::ddd::service_ops::types::{
        BoundaryPayloadPlan, ChildAction, ExecutionRenderFacts, ProjectionReadPlan, ServiceOpState,
        ServiceOpTransition, StateName, WireKey,
    };

    // ── T3.1 fixtures: the `payroll` route ────────────────────────────────

    /// The render facts a `payroll` route carries — the names the handler
    /// template's own context holds for the same entity.
    fn payroll_facts() -> ExecutionRenderFacts {
        ExecutionRenderFacts {
            domain: "payroll".to_string(),
            entity_name: "PayRunType".to_string(),
            entity_table: "pay_run".to_string(),
            id_param: "pay_run_id".to_string(),
            domain_types_crate: "domain_types".to_string(),
            nested_child: false,
            auditable: false,
        }
    }

    fn route(execution: ExecutionKind, record_load: bool) -> ServiceOpRoute {
        ServiceOpRoute {
            service: "PayRunService".to_string(),
            op_name: "submit".to_string(),
            snake: "submit".to_string(),
            fn_name: "service_op_submit".to_string(),
            capabilities: Vec::new(),
            authorize_fn: None,
            execution,
            record_load,
            authorize_async: false,
            injected_guard: None,
            invariant_block: None,
            throws: Vec::new(),
            raise_responses: Vec::new(),
            render_facts: payroll_facts(),
            execution_block: String::new(),
        }
    }

    /// The seam arm's record-load preamble acknowledges the load (it IS the
    /// route's 404 contract) and the 501 names the unlowered gap. The blank
    /// line between the acknowledgement and the `Err` is the template's own
    /// `{% endif %}` newline, preserved so the later wiring is
    /// byte-identical.
    #[test]
    fn seam_renders_the_501_arm_with_the_record_load_note() {
        let op = route(
            ExecutionKind::Seam {
                reason: "service op `submit` has no delegation target and no statement body \
                         — abstract service operations have no executable lowering yet \
                         (recorded follow-up)"
                    .to_string(),
            },
            true,
        );
        assert_eq!(
            render_execution_block(&op),
            concat!(
                "    // The record load is the route's 404 contract; the body has no\n",
                "    // executable lowering yet.\n",
                "    let _ = &response;\n",
                "\n",
                "    Err(service_op_not_implemented(\n",
                "        \"service op `submit` has no delegation target and no statement body — \
                 abstract service operations have no executable lowering yet (recorded \
                 follow-up)\",\n",
                "    ))",
            ),
            "the seam body is the template's else arm, verbatim"
        );
    }

    /// A seam route with NO record load (a capability bound to a non-root
    /// entity) skips the acknowledgement — the template's
    /// `{% if op.record_load %}` arm, verbatim.
    #[test]
    fn seam_without_a_record_load_skips_the_acknowledgement() {
        let op = route(
            ExecutionKind::Seam {
                reason: "no executable lowering".to_string(),
            },
            false,
        );
        assert_eq!(
            render_execution_block(&op),
            concat!(
                "    Err(service_op_not_implemented(\n",
                "        \"no executable lowering\",\n",
                "    ))",
            )
        );
    }

    /// The read arm returns the record the route loaded, in the data
    /// envelope (correlation id in `meta`).
    #[test]
    fn read_renders_the_loaded_record_envelope() {
        let op = route(ExecutionKind::Read, true);
        assert_eq!(
            render_execution_block(&op),
            concat!(
                "    Ok(axum::Json(serde_json::json!({\n",
                "        \"data\": response,\n",
                "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
                "    })).into_response())",
            )
        );
    }

    /// The delete arm addresses the entity's command slot with the render
    /// facts (`state.{domain}_{table}_commands.delete({id_param}, …)`) and
    /// answers the ok envelope through the domain error type.
    #[test]
    fn delete_renders_the_command_call_and_the_ok_status() {
        let op = route(ExecutionKind::Delete, true);
        let block = render_execution_block(&op);
        assert_eq!(
            block,
            concat!(
                "    state.payroll_pay_run_commands.delete(pay_run_id, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
                "        .map_err(|e: PayrollError| {\n",
                "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to delete PayRunType\")\n",
                "                .with_correlation_id(correlation_id)\n",
                "        })?;\n",
                "    Ok(axum::Json(serde_json::json!({ \"data\": { \"ok\": true } })).into_response())",
            )
        );
        assert!(
            block.contains("state.payroll_pay_run_commands.delete(pay_run_id,"),
            "the command slot renders from the route's render facts: {block}"
        );
    }

    /// Every kind this module does not own renders as an EMPTY string — the
    /// template keeps its own arm (so the future wiring is flag-off for
    /// them).
    #[test]
    fn every_unported_kind_renders_nothing() {
        let cases = vec![
            ExecutionKind::Create {
                plan: BoundaryPayloadPlan {
                    dto_prefix: "Create".to_string(),
                    extraction: String::new(),
                    construct: String::new(),
                    defaulted: Vec::new(),
                    request_body: None,
                    sample: serde_json::json!({}),
                    required_class_refs: Vec::new(),
                },
            },
            ExecutionKind::Body {
                plan: Box::new(BodyPlan {
                    block: "    // body\n".to_string(),
                    params: Vec::new(),
                    raises: Vec::new(),
                    state: None,
                    body_manages_state: None,
                    saves: false,
                    may_refuse: false,
                    responses: Vec::new(),
                }),
            },
            ExecutionKind::ProjectionRead {
                plan: ProjectionReadPlan {
                    module: "catalog".to_string(),
                    projection: "BookSummary".to_string(),
                    collection: false,
                    derived: Vec::new(),
                },
            },
        ];
        for execution in cases {
            let kind = format!("{execution:?}");
            let op = route(execution, true);
            assert_eq!(
                render_execution_block(&op),
                String::new(),
                "kind `{kind}` is still template-owned and must render nothing"
            );
        }
    }

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
        ExecutionRenderFacts {
            domain: "common".to_string(),
            entity_name: "Task".to_string(),
            entity_table: "task".to_string(),
            id_param: "task_id".to_string(),
            domain_types_crate: "domain_types".to_string(),
            nested_child: false,
            auditable: false,
        }
    }

    fn matrix_route(op_name: &str, execution: ExecutionKind) -> ServiceOpRoute {
        ServiceOpRoute {
            service: "TaskService".to_string(),
            op_name: op_name.to_string(),
            snake: codegraph_naming::to_snake_case(op_name),
            fn_name: format!("service_op_{}", codegraph_naming::to_snake_case(op_name)),
            capabilities: Vec::new(),
            authorize_fn: None,
            execution,
            record_load: true,
            authorize_async: false,
            injected_guard: None,
            invariant_block: None,
            throws: Vec::new(),
            raise_responses: Vec::new(),
            render_facts: matrix_facts(),
            execution_block: String::new(),
        }
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

    // ── T3.3 fixtures: the handler-matrix `service_ops_child_roles`
    //    permutation ─────────────────────────────────────────────────────
    //
    // The expected blocks below are the ARMS EXTRACTED VERBATIM from the
    // blessed snapshot (`<temp>/codegraph-handler-matrix/expected/
    // service_ops_child_roles.rs`), so these KATs pin the renderer against
    // real template output, not against a re-reading of the template source.

    /// The matrix fixture's child (`common`/`task_note`) facts. Every
    /// `child_op` route IS nested (the arm exists for the aggregate child
    /// plane), and the fixture CHILD is not auditable.
    fn child_facts() -> ExecutionRenderFacts {
        ExecutionRenderFacts {
            domain: "common".to_string(),
            entity_name: "TaskNote".to_string(),
            entity_table: "task_note".to_string(),
            id_param: "task_note_id".to_string(),
            domain_types_crate: "domain_types".to_string(),
            nested_child: true,
            auditable: false,
        }
    }

    fn child_route(op_name: &str, execution: ExecutionKind) -> ServiceOpRoute {
        ServiceOpRoute {
            service: "TaskService".to_string(),
            op_name: op_name.to_string(),
            snake: codegraph_naming::to_snake_case(op_name),
            fn_name: format!("service_op_{}", codegraph_naming::to_snake_case(op_name)),
            capabilities: Vec::new(),
            authorize_fn: None,
            execution,
            record_load: true,
            authorize_async: false,
            injected_guard: None,
            invariant_block: None,
            throws: Vec::new(),
            raise_responses: Vec::new(),
            render_facts: child_facts(),
            execution_block: String::new(),
        }
    }

    /// The matrix fixture's `Task` parent load (the aggregate root).
    fn task_parent(auditable: bool) -> ChildParentLoad {
        ChildParentLoad {
            entity: "Task".to_string(),
            module: "task".to_string(),
            domain: "common".to_string(),
            auditable,
        }
    }

    fn child_op(
        action: ChildAction,
        parent_load: Option<ChildParentLoad>,
        payload: Option<BoundaryPayloadPlan>,
    ) -> ExecutionKind {
        ExecutionKind::ChildOp {
            action,
            feature: "notes".to_string(),
            child_class: "common::TaskNote".to_string(),
            read_back: true,
            parent_load,
            payload,
        }
    }

    /// The matrix fixture's `NoteIntake` input boundary (`addNote`).
    fn add_boundary_payload() -> BoundaryPayloadPlan {
        BoundaryPayloadPlan {
            dto_prefix: "Create".to_string(),
            extraction: "    let payload: domain_types::dto::notes::NoteIntake = Json::<domain_types::dto::notes::NoteIntake>::from_value(serde_json::json!({}))?;\n".to_string(),
            construct: "        body: payload.body,\n        task_id: Some(parent_id),\n".to_string(),
            defaulted: vec!["task_id".to_string()],
            request_body: Some("domain_types::dto::notes::NoteIntake".to_string()),
            sample: serde_json::json!({ "body": "sample" }),
            required_class_refs: Vec::new(),
        }
    }

    /// The matrix fixture's `NotePatch` input boundary (`saveNote`).
    fn save_boundary_payload() -> BoundaryPayloadPlan {
        BoundaryPayloadPlan {
            dto_prefix: "Update".to_string(),
            extraction: "    let payload: domain_types::dto::notes::NotePatch = Json::<domain_types::dto::notes::NotePatch>::from_value(serde_json::json!({}))?;\n".to_string(),
            construct: "        body: payload.body,\n".to_string(),
            defaulted: Vec::new(),
            request_body: Some("domain_types::dto::notes::NotePatch".to_string()),
            sample: serde_json::json!({ "body": "patched" }),
            required_class_refs: Vec::new(),
        }
    }

    const MATRIX_CHILD_ADD_BOUNDARY_BLOCK: &str = concat!(
        "    // Aggregate child operation (issue #661): the root's\n",
        "    // `children TaskService` block declared this `add`\n",
        "    // over its `notes` child collection. It runs against the\n",
        "    // CHILD's plane, under the parent's aggregate path, so the element can\n",
        "    // never land under another parent.\n",
        "\n",
        "\n",
        "    // Issue #692: the parent load is the route's 404 contract. The create\n",
        "    // below writes the parent FK from the path, so it only runs for a\n",
        "    // parent the session can see — the RLS-scoped query plane answers None\n",
        "    // for a bogus or foreign-org parent, and the route answers 404 (never\n",
        "    // Postgres 23503 as a 500).\n",
        "    state.common_task_queries\n",
        "        .find_by_id(parent_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to find Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(format!(\"Task {parent_id} not found\")).with_correlation_id(correlation_id))?;\n",
        "\n",
        "\n",
        "    // Issue #767: the modeled boundary IS the payload — its members were\n",
        "    // mapped onto the child's generated CreateTaskNoteRequest at\n",
        "    // generation time.\n",
        "    // Not carried by the boundary, defaulted to `None`: task_id.\n",
        "    let payload: domain_types::dto::notes::NoteIntake = Json::<domain_types::dto::notes::NoteIntake>::from_value(serde_json::json!({}))?;\n",
        "    let item = CreateTaskNoteRequest {\n",
        "        body: payload.body,\n",
        "        task_id: Some(parent_id),\n",
        "    };\n",
        "\n",
        "    if let Err(report) = garde::Validate::validate(&item) {\n",
        "        let details: Vec<crate::error::FieldError> = report.into_inner()\n",
        "            .into_iter()\n",
        "            .map(|(path, error)| crate::error::FieldError {\n",
        "                field: path.to_string(),\n",
        "                message: error.message().to_string(),\n",
        "            })\n",
        "            .collect();\n",
        "        return Err(AppError::validation(\"Request validation failed\", details)\n",
        "            .with_correlation_id(correlation_id));\n",
        "    }\n",
        "    let created_id = state.common_task_note_commands.create(\n",
        "        item,\n",
        "        parent_id,\n",
        "        domain_types::SourceContext::api(),\n",
        "        correlation_id,\n",
        "        api_key_info.api_key_id,\n",
        "        api_key_info.organization_id,\n",
        "        api_key_info.user_id,\n",
        "        api_key_info.role.clone(),\n",
        "    ).await.map_err(|e: CommonError| {\n",
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to add the TaskNote child\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let created = state.common_task_note_queries\n",
        "        .find_by_id(created_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload the TaskNote child\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::internal(\"Created child not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": created,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    const MATRIX_CHILD_ADD_BODY_BLOCK: &str = concat!(
        "    // Aggregate child operation (issue #661): the root's\n",
        "    // `children TaskService` block declared this `add`\n",
        "    // over its `notes` child collection. It runs against the\n",
        "    // CHILD's plane, under the parent's aggregate path, so the element can\n",
        "    // never land under another parent.\n",
        "\n",
        "\n",
        "    // Issue #692: the parent load is the route's 404 contract. The create\n",
        "    // below writes the parent FK from the path, so it only runs for a\n",
        "    // parent the session can see — the RLS-scoped query plane answers None\n",
        "    // for a bogus or foreign-org parent, and the route answers 404 (never\n",
        "    // Postgres 23503 as a 500).\n",
        "    state.common_task_queries\n",
        "        .find_by_id(parent_id, false, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to find Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(format!(\"Task {parent_id} not found\")).with_correlation_id(correlation_id))?;\n",
        "\n",
        "\n",
        "    let item = match body {\n",
        "        CreateTaskNoteBody::Single(item) => item,\n",
        "        CreateTaskNoteBody::Bulk(_) => {\n",
        "            return Err(AppError::bad_request(\n",
        "                \"A child action adds one element — send a single JSON object, not an array\",\n",
        "            )\n",
        "            .with_correlation_id(correlation_id));\n",
        "        }\n",
        "    };\n",
        "\n",
        "    if let Err(report) = garde::Validate::validate(&item) {\n",
        "        let details: Vec<crate::error::FieldError> = report.into_inner()\n",
        "            .into_iter()\n",
        "            .map(|(path, error)| crate::error::FieldError {\n",
        "                field: path.to_string(),\n",
        "                message: error.message().to_string(),\n",
        "            })\n",
        "            .collect();\n",
        "        return Err(AppError::validation(\"Request validation failed\", details)\n",
        "            .with_correlation_id(correlation_id));\n",
        "    }\n",
        "    let created_id = state.common_task_note_commands.create(\n",
        "        item,\n",
        "        parent_id,\n",
        "        domain_types::SourceContext::api(),\n",
        "        correlation_id,\n",
        "        api_key_info.api_key_id,\n",
        "        api_key_info.organization_id,\n",
        "        api_key_info.user_id,\n",
        "        api_key_info.role.clone(),\n",
        "    ).await.map_err(|e: CommonError| {\n",
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to add the TaskNote child\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let created = state.common_task_note_queries\n",
        "        .find_by_id(created_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload the TaskNote child\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::internal(\"Created child not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": created,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    const MATRIX_CHILD_SAVE_BOUNDARY_BLOCK: &str = concat!(
        "    // Aggregate child operation (issue #661): the root's\n",
        "    // `children TaskService` block declared this `save`\n",
        "    // over its `notes` child collection. It runs against the\n",
        "    // CHILD's plane, under the parent's aggregate path, so the element can\n",
        "    // never land under another parent.\n",
        "\n",
        "    // The scoped load above is the parent-ownership contract; the update\n",
        "    // itself re-reads the row through the command plane.\n",
        "    let _ = &response;\n",
        "\n",
        "    // Issue #767: the modeled boundary IS the payload — mapped onto the\n",
        "    // child's generated UpdateTaskNoteRequest at generation time.\n",
        "    let payload: domain_types::dto::notes::NotePatch = Json::<domain_types::dto::notes::NotePatch>::from_value(serde_json::json!({}))?;\n",
        "    let cmd: crate::domain::common::task_note::dto_update::UpdateTaskNoteRequest =\n",
        "        UpdateTaskNoteRequest {\n",
        "        body: payload.body,\n",
        "    };\n",
        "\n",
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
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to save the TaskNote child\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let saved = state.common_task_note_queries\n",
        "        .find_by_id(task_note_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload the TaskNote child\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(\"TaskNote not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": saved,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    const MATRIX_CHILD_SAVE_BODY_BLOCK: &str = concat!(
        "    // Aggregate child operation (issue #661): the root's\n",
        "    // `children TaskService` block declared this `save`\n",
        "    // over its `notes` child collection. It runs against the\n",
        "    // CHILD's plane, under the parent's aggregate path, so the element can\n",
        "    // never land under another parent.\n",
        "\n",
        "    // The scoped load above is the parent-ownership contract; the update\n",
        "    // itself re-reads the row through the command plane.\n",
        "    let _ = &response;\n",
        "\n",
        "    let value = body.map(|Json(v)| v).unwrap_or_else(|| serde_json::json!({}));\n",
        "    let cmd: crate::domain::common::task_note::dto_update::UpdateTaskNoteRequest =\n",
        "        serde_json::from_value(value).map_err(|e| {\n",
        "            AppError::bad_request(format!(\"invalid TaskNote update body: {e}\"))\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "\n",
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
        "        AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to save the TaskNote child\")\n",
        "            .with_correlation_id(correlation_id)\n",
        "    })?;\n",
        "    let saved = state.common_task_note_queries\n",
        "        .find_by_id(task_note_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())\n",
        "        .await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to reload the TaskNote child\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::not_found(\"TaskNote not found\").with_correlation_id(correlation_id))?;\n",
        "    Ok(axum::Json(serde_json::json!({\n",
        "        \"data\": saved,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    })).into_response())",
    );

    const MATRIX_CHILD_REMOVE_BLOCK: &str = concat!(
        "    // Aggregate child operation (issue #661): the root's\n",
        "    // `children TaskService` block declared this `remove`\n",
        "    // over its `notes` child collection. It runs against the\n",
        "    // CHILD's plane, under the parent's aggregate path, so the element can\n",
        "    // never land under another parent.\n",
        "\n",
        "    // `remove` deletes the element through the parent-scoped record the\n",
        "    // route already loaded — a child of another parent 404s above.\n",
        "    let _ = response;\n",
        "    state.common_task_note_commands.delete(task_note_id, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to remove the TaskNote child\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    Ok(axum::Json(serde_json::json!({ \"data\": { \"ok\": true } })).into_response())",
    );

    /// `add` with a modeled boundary (issue #767) and the #692 parent
    /// load-or-404: the boundary IS the payload, mapped onto the child's
    /// `Create…Request`; the create-and-link only runs for a parent the
    /// session can see.
    #[test]
    fn child_add_with_a_parent_load_renders_the_guard_and_the_boundary_create() {
        let op = child_route(
            "addNote",
            child_op(
                ChildAction::Add,
                Some(task_parent(false)),
                Some(add_boundary_payload()),
            ),
        );
        assert_eq!(render_execution_block(&op), MATRIX_CHILD_ADD_BOUNDARY_BLOCK);
    }

    /// `add` without a modeled boundary: the generated `Create…Body`
    /// decoder (rejecting a bulk array), with an AUDITABLE parent — the
    /// parent load carries the `include_deleted` arity.
    #[test]
    fn child_add_without_a_boundary_decodes_the_generated_body() {
        let op = child_route(
            "appendNote",
            child_op(ChildAction::Add, Some(task_parent(true)), None),
        );
        assert_eq!(render_execution_block(&op), MATRIX_CHILD_ADD_BODY_BLOCK);
    }

    /// `add` with NO parent-load plan (a hand-built/fallback route): the
    /// template's `{% if op.execution.parent_load %}` FALSE arm — no 404
    /// guard at all, but the arm's tag newlines still apply.
    #[test]
    fn child_add_without_a_parent_load_omits_the_guard() {
        let op = child_route("appendNote", child_op(ChildAction::Add, None, None));
        let block = render_execution_block(&op);
        assert!(
            !block.contains("Issue #692"),
            "no parent load may be rendered: {block}"
        );
        assert!(
            block.contains("another parent.\n\n\n\n    let item = match body {"),
            "the header's tag newlines stay (parent if + item if/endif): {block}"
        );
        assert_eq!(
            block.matches("state.common_task_note_queries").count(),
            1,
            "only the create's reload touches the query plane: {block}"
        );
        assert!(
            !block.contains("state.common_task_queries"),
            "no parent-load query slot may be rendered: {block}"
        );
    }

    /// `save` with a modeled boundary: the boundary IS the payload, mapped
    /// onto the child's `Update…Request`.
    #[test]
    fn child_save_with_a_boundary_constructs_the_update_request() {
        let op = child_route(
            "saveNote",
            child_op(ChildAction::Save, None, Some(save_boundary_payload())),
        );
        assert_eq!(
            render_execution_block(&op),
            MATRIX_CHILD_SAVE_BOUNDARY_BLOCK
        );
    }

    /// `save` without a modeled boundary: decode the request body into the
    /// child's `Update…Request`.
    #[test]
    fn child_save_without_a_boundary_decodes_the_body() {
        let op = child_route("renameNote", child_op(ChildAction::Save, None, None));
        assert_eq!(render_execution_block(&op), MATRIX_CHILD_SAVE_BODY_BLOCK);
    }

    /// `remove`: the parent-scoped record the route already loaded IS the
    /// ownership proof, so the delete goes straight to the child's command
    /// plane.
    #[test]
    fn child_remove_deletes_through_the_command_plane() {
        let op = child_route("removeNote", child_op(ChildAction::Remove, None, None));
        assert_eq!(render_execution_block(&op), MATRIX_CHILD_REMOVE_BLOCK);
    }

    /// An AUDITABLE child's reloads take the `include_deleted` argument
    /// (`find_by_id(id, false, …)`) — the CHILD's arity, independent of the
    /// parent load's.
    #[test]
    fn child_reloads_carry_the_include_deleted_arity_when_the_child_is_auditable() {
        let mut op = child_route(
            "appendNote",
            child_op(ChildAction::Add, Some(task_parent(false)), None),
        );
        op.render_facts.auditable = true;
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                ".find_by_id(created_id, false, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())"
            ),
            "the add reload passes `, false`: {block}"
        );
        assert!(
            block.contains(".find_by_id(parent_id, api_key_info.api_key_id,"),
            "the PARENT load keeps its own (non-auditable) arity: {block}"
        );

        let mut op = child_route("renameNote", child_op(ChildAction::Save, None, None));
        op.render_facts.auditable = true;
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                ".find_by_id(task_note_id, false, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())"
            ),
            "the save reload passes `, false`: {block}"
        );
    }

    /// The child-op arm carries NO nesting-depth branch — the parent nesting
    /// lives in the route/signature OUTSIDE the arm, so the same execution
    /// renders the same bytes whether the child is a direct child or a
    /// grandchild (the `nested_child` fact is not consumed here).
    #[test]
    fn child_arm_renders_identically_regardless_of_nesting_depth() {
        let mut direct = child_route("removeNote", child_op(ChildAction::Remove, None, None));
        let nested = render_execution_block(&direct);
        direct.render_facts.nested_child = false;
        assert_eq!(render_execution_block(&direct), nested);
    }
}
