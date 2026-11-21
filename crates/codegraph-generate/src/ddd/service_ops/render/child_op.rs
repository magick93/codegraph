//! The aggregate child service-op arm (issue #661): the `add`/`save`/`remove`
//! plane that drives the CHILD's command under the parent's aggregate path,
//! with the #692 parent load-or-404 and the #767 modeled-boundary payload.

use super::super::types::{
    BoundaryPayloadPlan, ChildAction, ChildParentLoad, ExecutionRenderFacts, ServiceOpRoute,
};
use super::{REQUEST_VALIDATE_BLOCK, audit_arity, domain_error};

/// The `child_op` arm (issue #661): a `children <Root> { … }` operation that
/// drives the CHILD's command plane under the parent's aggregate path.
///
/// `add` appends one element (proving the parent row first — the #692
/// load-or-404), `save` patches the parent-scoped element, `remove` deletes
/// it. `add` also carries the #767 modeled-boundary branch: when the op
/// declares an input boundary the handler CONSTRUCTS the child's request DTO
/// from it instead of decoding the generated `Create…Body`.
pub(super) fn child_op_block(
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
            block.push_str(REQUEST_VALIDATE_BLOCK);
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
fn parent_load_block(load: &ChildParentLoad, facts: &ExecutionRenderFacts) -> String {
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
fn add_boundary_item(payload: &BoundaryPayloadPlan, facts: &ExecutionRenderFacts) -> String {
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
fn add_body_match_item(facts: &ExecutionRenderFacts) -> String {
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

/// The add tail: create-and-link, then reload the created element (the
/// `include_deleted` arity when the CHILD is auditable) and return it.
fn add_create_tail(facts: &ExecutionRenderFacts) -> String {
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
fn save_boundary_command(payload: &BoundaryPayloadPlan, facts: &ExecutionRenderFacts) -> String {
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
fn save_decoded_command(facts: &ExecutionRenderFacts) -> String {
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
fn save_tail(facts: &ExecutionRenderFacts) -> String {
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
fn remove_block(facts: &ExecutionRenderFacts) -> String {
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
#[cfg(test)]
mod tests {
    use crate::ddd::service_ops::render::{render_execution_block, test_support};
    use crate::ddd::service_ops::types::{
        BoundaryPayloadPlan, ChildAction, ChildParentLoad, ExecutionKind, ExecutionRenderFacts,
        ServiceOpRoute,
    };

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
        test_support::child_facts()
    }

    fn child_route(op_name: &str, execution: ExecutionKind) -> ServiceOpRoute {
        test_support::route("TaskService", op_name, child_facts(), execution, true)
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
