//! The low-logic service-op arms: `seam` (the 501), `read` (the loaded
//! record envelope), `delete` (the command call) and `body` (the checked
//! `.ddd` statement block) — the arms with no machine binding and no child
//! plane.

use codegraph_naming::to_pascal_case;

use super::super::types::ServiceOpRoute;

/// The seam arm's record-load preamble: the loaded record IS the route's
/// 404 contract, so it is acknowledged rather than dropped.
pub(super) const SEAM_RECORD_LOAD_PREAMBLE: &str = concat!(
    "    // The record load is the route's 404 contract; the body has no\n",
    "    // executable lowering yet.\n",
    "    let _ = &response;\n",
    "\n",
);

/// The read arm: the loaded record is returned in the data envelope.
pub(super) const READ_BLOCK: &str = concat!(
    "    Ok(axum::Json(serde_json::json!({\n",
    "        \"data\": response,\n",
    "        \"meta\": { \"correlation_id\": correlation_id.to_string() }\n",
    "    })).into_response())",
);

/// The `delete` arm: the entity's command plane deletes the row and the
/// route answers the ok envelope. The AppState slot and the error prose
/// render with the very names the handler template's own context carries
/// (`ExecutionRenderFacts`).
pub(super) fn delete_block(op: &ServiceOpRoute) -> String {
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
pub(super) fn seam_block(record_load: bool, reason: &str) -> String {
    let mut block = String::new();
    if record_load {
        block.push_str(SEAM_RECORD_LOAD_PREAMBLE);
    }
    block.push_str("    Err(service_op_not_implemented(\n");
    block.push_str(&format!("        \"{reason}\",\n"));
    block.push_str("    ))");
    block
}

/// The `body` arm (issue #662): the checked `.ddd` statement body, lowered
/// at generation time over the loaded record.
///
/// `BodyPlan.block` is a SELF-TERMINATING statement region: the lowerer ends
/// it with its own newline (it closes on the `Ok(…into_response())` line),
/// while the template splices it on a line of its own. The emitted arm is
/// therefore the three comment lines plus the block VERBATIM — this arm is
/// the one place where the returned text may legitimately end with `\n`,
/// because that newline is the block's own content rather than a template
/// tag's. Adding another (or trimming this one) would move the blank line
/// the template leaves before the handler's closing brace, so neither
/// happens.
pub(super) fn body_block(plan: &crate::ddd::body_lower::BodyPlan) -> String {
    format!(
        concat!(
            "    // Checked `.ddd` statement body (issue #662): the statements below were\n",
            "    // lowered at generation time over the loaded record. A construct outside\n",
            "    // the documented subset is a generation error, never a silent seam.\n",
            "{}",
        ),
        plan.block,
    )
}

#[cfg(test)]
mod tests {
    use crate::ddd::body_lower::{BodyParam, BodyPlan};
    use crate::ddd::service_ops::render::{render_execution_block, test_support};
    use crate::ddd::service_ops::types::{ExecutionKind, ExecutionRenderFacts, ServiceOpRoute};

    // ── T3.1 fixtures: the `payroll` route ────────────────────────────────

    /// The render facts a `payroll` route carries — the names the handler
    /// template's own context holds for the same entity.
    fn payroll_facts() -> ExecutionRenderFacts {
        test_support::payroll_facts()
    }

    fn route(execution: ExecutionKind, record_load: bool) -> ServiceOpRoute {
        test_support::route(
            "PayRunService",
            "submit",
            payroll_facts(),
            execution,
            record_load,
        )
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

    // ── T3.4: the `body` arm (`root_service_ops`'s two body variants) ────

    /// The matrix fixture's `BodyPlan.block`, verbatim: the lowerer's own
    /// text, carrying NO trailing newline in the fixture.
    const MATRIX_BODY_STATEMENTS: &str = concat!(
        "    let current = self.findById(\"current\").await.map_err(|e| e)?;\n",
        "    guard current.status != \"Archived\" else error Conflict(\"archived tasks cannot be submitted\");\n",
        "    local.status = \"Submitted\";\n",
        "    let latest = self.save(local).await?;\n",
        "    return Ok(serde_json::json!({ \"data\": latest }));",
    );

    /// The `submitAndSave` arm EXTRACTED VERBATIM from the blessed
    /// `root_service_ops` snapshot: three comment lines + the plan block.
    const MATRIX_BODY_ARM: &str = concat!(
        "    // Checked `.ddd` statement body (issue #662): the statements below were\n",
        "    // lowered at generation time over the loaded record. A construct outside\n",
        "    // the documented subset is a generation error, never a silent seam.\n",
        "    let current = self.findById(\"current\").await.map_err(|e| e)?;\n",
        "    guard current.status != \"Archived\" else error Conflict(\"archived tasks cannot be submitted\");\n",
        "    local.status = \"Submitted\";\n",
        "    let latest = self.save(local).await?;\n",
        "    return Ok(serde_json::json!({ \"data\": latest }));",
    );

    fn body_plan(block: &str, params: Vec<BodyParam>) -> BodyPlan {
        BodyPlan {
            block: block.to_string(),
            params,
            raises: Vec::new(),
            state: None,
            body_manages_state: None,
            saves: true,
            assigned_fields: Vec::new(),
            may_refuse: true,
            responses: Vec::new(),
        }
    }

    /// A checked `.ddd` statement body renders the arm's three comment lines
    /// plus `BodyPlan.block` verbatim — the snapshot's `submitAndSave` arm.
    #[test]
    fn body_renders_the_statement_block_verbatim() {
        let op = route(
            ExecutionKind::Body {
                plan: Box::new(body_plan(MATRIX_BODY_STATEMENTS, Vec::new())),
            },
            true,
        );
        assert_eq!(render_execution_block(&op), MATRIX_BODY_ARM);
    }

    /// The `direct_body` boundary variant (`submitBoundary`) declares its
    /// lone modeled-DTO parameter on the SIGNATURE; the execution arm is
    /// byte-identical — the snapshot proves it.
    #[test]
    fn body_direct_boundary_renders_the_same_arm() {
        let direct = route(
            ExecutionKind::Body {
                plan: Box::new(body_plan(
                    MATRIX_BODY_STATEMENTS,
                    vec![BodyParam {
                        name: "payload".to_string(),
                        rust_type: "domain_types::dto::submission::Submission".to_string(),
                        optional: false,
                        direct_body: true,
                        sample: serde_json::json!({ "note": "sample" }),
                        request_body: Some("domain_types::dto::submission::Submission".to_string()),
                    }],
                )),
            },
            true,
        );
        assert_eq!(render_execution_block(&direct), MATRIX_BODY_ARM);

        let plain = route(
            ExecutionKind::Body {
                plan: Box::new(body_plan(MATRIX_BODY_STATEMENTS, Vec::new())),
            },
            true,
        );
        assert_eq!(
            render_execution_block(&direct),
            render_execution_block(&plain),
            "the boundary variant differs on the signature only, never in the arm"
        );
    }

    /// A REAL `BodyPlan.block` self-terminates with its own newline (the
    /// lowerer closes on the `Ok(…into_response())` line). That newline is
    /// the block's content, so it is neither trimmed nor doubled: the
    /// returned text ends with exactly one `\n`, which is what leaves the
    /// single blank line the template emits before the handler's closing
    /// brace.
    #[test]
    fn body_preserves_the_plan_blocks_own_trailing_newline_without_doubling_it() {
        let op = route(
            ExecutionKind::Body {
                plan: Box::new(body_plan(
                    "    let data = latest.unwrap_or(response);\n    Ok(axum::Json(serde_json::json!({ \"data\": data })).into_response())\n",
                    Vec::new(),
                )),
            },
            true,
        );
        let block = render_execution_block(&op);
        assert!(
            block.ends_with(")).into_response())\n"),
            "the plan's own trailing newline survives verbatim: {block:?}"
        );
        assert!(
            !block.ends_with("\n\n"),
            "the arm never adds a second newline: {block:?}"
        );
        assert!(
            block.starts_with("    // Checked `.ddd` statement body (issue #662)"),
            "the arm's comment header leads: {block:?}"
        );
    }
}
