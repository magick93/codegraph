//! The create-shaped service-op arm (issue #767): a collection-level
//! `POST …/{entity}/actions/{op}` whose body IS a modeled input boundary,
//! mapped onto the entity's generated `Create…Request` at generation time.
//!
//! The arm ends with the two post-pass blocks the handler generator attaches
//! (the #338 create-eligibility guards and the #666 slice-4d create
//! invariants) — spliced verbatim after the reload, exactly where the
//! template glues its own context keys.

use super::super::types::{BoundaryPayloadPlan, ExecutionRenderFacts, ServiceOpRoute};
use super::{REQUEST_VALIDATE_BLOCK, audit_arity, domain_error};

/// The `create` arm (issue #767): the boundary is the request, validated
/// through `garde`, created through the collection-level command, reloaded
/// and answered with 201.
pub(super) fn create_block(op: &ServiceOpRoute, plan: &BoundaryPayloadPlan) -> String {
    let facts = &op.render_facts;
    let mut block = format!(
        concat!(
            "    // Collection-level create body (issue #767): the modeled boundary IS the\n",
            "    // request — its members were mapped onto the generated\n",
            "    // Create{entity}Request at generation time (snake_case identity,\n",
            "    // primitive/multiplicity aligned with the DTO plane).\n",
        ),
        entity = facts.entity_name,
    );
    if !plan.defaulted.is_empty() {
        block.push_str(&format!(
            "    // Not carried by the boundary, defaulted to `None`: {}.\n",
            plan.defaulted.join(", ")
        ));
    }
    block.push_str(&plan.extraction);
    block.push_str(&format!(
        "    let item = Create{}Request {{\n",
        facts.entity_name
    ));
    block.push_str(&plan.construct);
    block.push_str("    };\n");
    block.push_str(REQUEST_VALIDATE_BLOCK);
    block.push_str(&create_call(facts));
    // The handler generator's post-pass blocks: each carries its own leading
    // newline, so they glue onto the reload's `?;` and onto each other.
    block.push_str(&op.create_guard_block);
    block.push_str(&op.create_invariant_block);
    block.push('\n');
    block.push_str(CREATE_ENVELOPE);
    block
}

/// The collection-level create command, its reload and the internal-error
/// guard (a just-created row that cannot be read back is a server fault,
/// never a 404).
fn create_call(facts: &ExecutionRenderFacts) -> String {
    format!(
        concat!(
            "    let id = state.{domain}_{table}_commands.create(item, {types}::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to create {entity}\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?;\n",
            "    let response = state.{domain}_{table}_queries.find_by_id(id{audit}, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
            "        .map_err(|e: {domain_error}| {{\n",
            "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to find {entity}\")\n",
            "                .with_correlation_id(correlation_id)\n",
            "        }})?\n",
            "        .ok_or_else(|| AppError::internal(\"Created entity not found\")\n",
            "            .with_correlation_id(correlation_id))?;",
        ),
        domain = facts.domain,
        table = facts.entity_table,
        types = facts.domain_types_crate,
        domain_error = domain_error(facts),
        entity = facts.entity_name,
        audit = audit_arity(facts),
    )
}

/// The 201 envelope: the reloaded row, `StatusCode::CREATED`.
const CREATE_ENVELOPE: &str = concat!(
    "    Ok((StatusCode::CREATED, Json(serde_json::json!({\n",
    "        \"data\": response,\n",
    "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
    "    }))).into_response())",
);

#[cfg(test)]
mod tests {
    use crate::ddd::service_ops::render::{render_execution_block, test_support};
    use crate::ddd::service_ops::types::{BoundaryPayloadPlan, ExecutionKind, ServiceOpRoute};

    /// The matrix fixture's `Intake` input boundary, verbatim.
    fn intake_boundary() -> BoundaryPayloadPlan {
        BoundaryPayloadPlan {
            dto_prefix: "Create".to_string(),
            extraction: "    let payload: domain_types::dto::intake::Intake = Json::<domain_types::dto::intake::Intake>::from_value(serde_json::json!({}))?;\n".to_string(),
            construct: "        title: payload.title,\n        status: None,\n".to_string(),
            defaulted: vec!["status".to_string(), "assignee_id".to_string()],
            request_body: Some("domain_types::dto::intake::Intake".to_string()),
            sample: serde_json::json!({ "title": "sample" }),
            required_class_refs: Vec::new(),
        }
    }

    fn create_route(record_load: bool) -> ServiceOpRoute {
        test_support::route(
            "TaskService",
            "openTask",
            test_support::matrix_facts(),
            ExecutionKind::Create {
                plan: intake_boundary(),
            },
            record_load,
        )
    }

    /// The matrix fixture's `create_invariant_block`, verbatim (the handler's
    /// post-pass). Shorter than the generator's own text because the fixture
    /// authors it — the arm splices whatever it is handed.
    const MATRIX_CREATE_INVARIANT_BLOCK: &str = concat!(
        "\n    // Design invariants (DD5, issue #666 slice 4d): server-generated\n",
        "    // fields exist only post-insert, so CREATE evaluates the CREATED\n",
        "    // record. A violation COMPENSATES before the 409.\n",
        "    if !crate::domain::common::rules::task_invariant_0(&response) {\n",
        "        tracing::warn!(entity_id = %id, \"design invariant violated on create — compensating\");\n",
        "        if let Err(comp_err) = state.common_task_commands.delete(id, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await {\n",
        "            tracing::error!(entity_id = %id, \"invariant compensation delete failed: {comp_err}\");\n",
        "        }\n",
        "        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: \"INVARIANT_VIOLATION\", message: format!(\"Invariant {} of `Task` violated: total > 0\", 0usize), details: Some(vec![crate::error::FieldError { field: \"Task\".to_string(), message: \"invariantIndex: 0\".to_string() }]), correlation_id: Some(correlation_id) });\n",
        "    }",
    );

    /// The `openTask` arm EXTRACTED VERBATIM from the blessed
    /// `root_service_ops` snapshot.
    const MATRIX_CREATE_BLOCK: &str = concat!(
        "    // Collection-level create body (issue #767): the modeled boundary IS the\n",
        "    // request — its members were mapped onto the generated\n",
        "    // CreateTaskRequest at generation time (snake_case identity,\n",
        "    // primitive/multiplicity aligned with the DTO plane).\n",
        "    // Not carried by the boundary, defaulted to `None`: status, assignee_id.\n",
        "    let payload: domain_types::dto::intake::Intake = Json::<domain_types::dto::intake::Intake>::from_value(serde_json::json!({}))?;\n",
        "    let item = CreateTaskRequest {\n",
        "        title: payload.title,\n",
        "        status: None,\n",
        "    };\n",
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
        "    let id = state.common_task_commands.create(item, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to create Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?;\n",
        "    let response = state.common_task_queries.find_by_id(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
        "        .map_err(|e: CommonError| {\n",
        "            AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to find Task\")\n",
        "                .with_correlation_id(correlation_id)\n",
        "        })?\n",
        "        .ok_or_else(|| AppError::internal(\"Created entity not found\")\n",
        "            .with_correlation_id(correlation_id))?;",
        "\n    // Design invariants (DD5, issue #666 slice 4d): server-generated\n",
        "    // fields exist only post-insert, so CREATE evaluates the CREATED\n",
        "    // record. A violation COMPENSATES before the 409.\n",
        "    if !crate::domain::common::rules::task_invariant_0(&response) {\n",
        "        tracing::warn!(entity_id = %id, \"design invariant violated on create — compensating\");\n",
        "        if let Err(comp_err) = state.common_task_commands.delete(id, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await {\n",
        "            tracing::error!(entity_id = %id, \"invariant compensation delete failed: {comp_err}\");\n",
        "        }\n",
        "        return Err(AppError { status: axum::http::StatusCode::CONFLICT, code: \"INVARIANT_VIOLATION\", message: format!(\"Invariant {} of `Task` violated: total > 0\", 0usize), details: Some(vec![crate::error::FieldError { field: \"Task\".to_string(), message: \"invariantIndex: 0\".to_string() }]), correlation_id: Some(correlation_id) });\n",
        "    }\n",
        "    Ok((StatusCode::CREATED, Json(serde_json::json!({\n",
        "        \"data\": response,\n",
        "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
        "    }))).into_response())",
    );

    /// The matrix fixture's #338 create-eligibility block, in the shape
    /// `build_guard_blocks` emits (leading newline, compensation, then 403).
    /// The matrix permutation carries no create-eligible rule, so this text
    /// is pinned here rather than snapshot-compared.
    const CREATE_GUARD_BLOCK: &str = concat!(
        "\n    // Eligibility rule 'PositiveTotal' (issue #338): deny with 403 when it fails.\n",
        "    if !crate::domain::common::rules::task_positive_total(&response) {\n",
        "        tracing::warn!(entity_id = %id, \"eligibility rule denied create — compensating\");\n",
        "        if let Err(comp_err) = state.common_task_commands.delete(id, domain_types::SourceContext::api(), correlation_id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await {\n",
        "            tracing::error!(entity_id = %id, \"eligibility-rule compensation delete failed: {comp_err}\");\n",
        "        }\n",
        "        return Err(AppError::forbidden(format!(\"Eligibility rule 'PositiveTotal' denied this Task operation\"))\n",
        "            .with_correlation_id(correlation_id));\n",
        "    }",
    );

    /// The create arm renders the boundary request, the `garde` validation,
    /// the collection-level create, the reload and the 201 envelope — with
    /// the post-pass invariant block spliced verbatim after the reload.
    #[test]
    fn create_renders_the_boundary_request_and_the_201_envelope() {
        let mut op = create_route(false);
        op.create_invariant_block = MATRIX_CREATE_INVARIANT_BLOCK.to_string();
        assert_eq!(render_execution_block(&op), MATRIX_CREATE_BLOCK);
    }

    /// Without either post-pass block the arm closes correctly: the reload's
    /// `?;` runs straight into the 201 envelope (no stray blank line) — the
    /// pre-#338/#666 shape.
    #[test]
    fn create_without_post_pass_blocks_goes_straight_to_the_201() {
        let op = create_route(false);
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                "            .with_correlation_id(correlation_id))?;\n    Ok((StatusCode::CREATED,"
            ),
            "the envelope follows the reload with no inserted newline: {block}"
        );
        assert!(!block.contains("Design invariants"));
        assert!(!block.contains("Eligibility rule"));
    }

    /// The #338 guard block splices FIRST, then the invariant block — the
    /// template's `{{ create_guard_block }}{{ create_invariant_block }}`
    /// order, both carrying their own leading newline.
    #[test]
    fn create_splices_the_guard_block_before_the_invariant_block() {
        let mut op = create_route(false);
        op.create_guard_block = CREATE_GUARD_BLOCK.to_string();
        op.create_invariant_block = MATRIX_CREATE_INVARIANT_BLOCK.to_string();
        let block = render_execution_block(&op);
        let guard = block
            .find("Eligibility rule 'PositiveTotal'")
            .expect("guard");
        let invariant = block.find("Design invariants (DD5").expect("invariant");
        assert!(guard < invariant, "guard precedes invariant: {block}");
        assert!(
            block.contains(
                "            .with_correlation_id(correlation_id))?;\n    // Eligibility rule"
            ),
            "the guard starts its own line after the reload: {block}"
        );
        assert!(
            block.contains(
                "            .with_correlation_id(correlation_id));\n    }\n    // Design invariants"
            ),
            "the guard's closing brace separates the two blocks: {block}"
        );
    }

    /// An AUDITABLE entity's reload takes the `include_deleted` argument.
    #[test]
    fn create_auditable_reload_passes_the_include_deleted_arity() {
        let mut op = create_route(false);
        op.render_facts.auditable = true;
        let block = render_execution_block(&op);
        assert!(
            block.contains(
                ".find_by_id(id, false, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone())"
            ),
            "an auditable create reload passes `, false`: {block}"
        );
    }

    /// The create lowering is COLLECTION-level: it never loads a record, so
    /// the arm's text is the same either way — the record-load prelude lives
    /// outside the arm.
    #[test]
    fn create_arm_is_independent_of_the_record_load_flag() {
        let loaded = create_route(true);
        let unloaded = create_route(false);
        assert_eq!(
            render_execution_block(&loaded),
            render_execution_block(&unloaded)
        );
    }
}
