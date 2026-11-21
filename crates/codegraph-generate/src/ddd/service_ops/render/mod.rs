//! Pre-rendered execution bodies for the service-op handler arms
//! (issue #778).
//!
//! The service-op execution arms live in `templates/api/handler.tera`'s
//! `{% for op in service_ops %}` loop. They move into Rust one arm at a
//! time: [`render_execution_block`] dispatches to the per-arm module, each of
//! which ports its template arm VERBATIM (statements, comments, indentation
//! and blank lines included) so the template can later render
//! `{{ op.execution_block }}` in their place byte-for-byte.
//!
//! Layout (the `ddd/service_ops/` reference shape):
//!
//! - `basic` — the low-logic arms: `seam`, `read`, `delete`, `body`;
//! - `update` — the machine-binding `save`/`update` arm;
//! - `child_op` — the aggregate child `add`/`save`/`remove` plane;
//! - `create` — the collection-level modeled-boundary create (issue #767);
//! - `projection` — the item/collection projection reads (issue #768).
//!
//! Every [`ExecutionKind`] arm is ported: nothing is left template-owned.
//!
//! Only the BODY region moves — the doc comment, the `#[utoipa::path]`
//! header, the signature and the record-load/authorize prelude stay
//! template-owned for now. Every kind this module does not own renders as an
//! empty string, so an unwired arm keeps its template block.

mod basic;
mod child_op;
mod create;
mod projection;
mod update;

use super::types::{ExecutionKind, ExecutionRenderFacts, ServiceOpRoute};

#[cfg(test)]
mod test_support;

/// The handler-template execution body for one service op (issue #778).
///
/// Returns the verbatim Rust body the template emits for the op's
/// [`ExecutionKind`] — `seam` (the 501 naming the unlowered gap), `read`
/// (the loaded-record envelope), `delete` (the command call plus the ok
/// envelope), `body` (the checked `.ddd` statement block), `update` (the
/// machine-binding selection, the allowed-state check, the design-invariant
/// splice, the DTO patch, the command call and the reload envelope),
/// `child_op` (the aggregate child `add`/`save`/`remove` plane), `create`
/// (the collection-level modeled-boundary create) and `projection_read` (the
/// item and collection read models). Every variant of the closed
/// [`ExecutionKind`] set is ported — there is no template-owned arm left, so
/// an unwired kind renders nothing only if a future variant is added.
///
/// The block is a pure function of the route as handed in: the update arm
/// reads `op.invariant_block` (the handler generator attaches it AFTER
/// `routes_for_entity`, so that plane re-renders the block once the block is
/// wired).
///
/// The block begins at the arm's first body line and carries NO trailing
/// newline of its own — the wiring template supplies the line break (the
/// `invariant_block` convention), so `{{ op.execution_block }}` on its own
/// line reproduces the current arm byte-for-byte. The one exception is the
/// `body` arm, whose `BodyPlan.block` is a self-terminating statement
/// region: it is spliced VERBATIM, trailing newline included (see
/// `basic::body_block`). The newlines the template's `{% if %}` tags
/// contribute around an arm stay the template's.
#[must_use]
pub fn render_execution_block(op: &ServiceOpRoute) -> String {
    match &op.execution {
        ExecutionKind::Delete => basic::delete_block(op),
        ExecutionKind::Read => basic::READ_BLOCK.to_string(),
        ExecutionKind::Seam { reason } => basic::seam_block(op.record_load, reason),
        ExecutionKind::Body { plan } => basic::body_block(plan),
        ExecutionKind::Update { state } => update::update_block(op, state.as_ref()),
        ExecutionKind::ChildOp {
            action,
            feature,
            parent_load,
            payload,
            ..
        } => child_op::child_op_block(op, *action, feature, parent_load.as_ref(), payload.as_ref()),
        ExecutionKind::Create { plan } => create::create_block(op, plan),
        ExecutionKind::ProjectionRead { plan } => projection::projection_block(op, plan),
    }
}

/// The `garde` validation every create-shaped arm runs before the write:
/// the child `add` item (both item shapes) and the collection-level create
/// request.
pub(super) const REQUEST_VALIDATE_BLOCK: &str = concat!(
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

/// The `{Domain}Error` type the emitted `map_err` closures annotate.
pub(super) fn domain_error(facts: &ExecutionRenderFacts) -> String {
    format!("{}Error", codegraph_naming::to_pascal_case(&facts.domain))
}

/// The `include_deleted` argument for the auditable query arity.
pub(super) fn audit_arity(facts: &ExecutionRenderFacts) -> &'static str {
    if facts.auditable { ", false" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddd::body_lower::BodyPlan;
    use crate::ddd::service_ops::types::{
        BoundaryPayloadPlan, ChildAction, ProjectionReadPlan, ServiceOpState, StateName, WireKey,
    };

    /// Every variant of the closed [`ExecutionKind`] set now has a real
    /// renderer: nothing is left template-owned. A future variant added to
    /// the enum fails this KAT until its arm is ported (the `_` arm in
    /// [`render_execution_block`] would silently render nothing).
    #[test]
    fn every_kind_is_ported() {
        let machine = ServiceOpState {
            current_expr: "response.status.clone().unwrap_or_default()".to_string(),
            wire_key: WireKey::new("status"),
            initial: StateName::new("Draft"),
            all: Vec::new(),
            op: Vec::new(),
        };
        let boundary = BoundaryPayloadPlan {
            dto_prefix: "Create".to_string(),
            extraction: String::new(),
            construct: String::new(),
            defaulted: Vec::new(),
            request_body: None,
            sample: serde_json::json!({}),
            required_class_refs: Vec::new(),
        };
        let cases: Vec<(&str, ExecutionKind)> = vec![
            ("delete", ExecutionKind::Delete),
            ("read", ExecutionKind::Read),
            (
                "seam",
                ExecutionKind::Seam {
                    reason: "no executable lowering".to_string(),
                },
            ),
            (
                "body",
                ExecutionKind::Body {
                    plan: Box::new(BodyPlan {
                        block: "    let data = response;\n".to_string(),
                        params: Vec::new(),
                        raises: Vec::new(),
                        state: None,
                        body_manages_state: None,
                        saves: false,
                        may_refuse: false,
                        responses: Vec::new(),
                    }),
                },
            ),
            (
                "update",
                ExecutionKind::Update {
                    state: Some(machine),
                },
            ),
            (
                "child_op",
                ExecutionKind::ChildOp {
                    action: ChildAction::Remove,
                    feature: "lines".to_string(),
                    child_class: "payroll.PayRunLineType".to_string(),
                    read_back: false,
                    parent_load: None,
                    payload: None,
                },
            ),
            (
                "create",
                ExecutionKind::Create {
                    plan: boundary.clone(),
                },
            ),
            (
                "projection_read",
                ExecutionKind::ProjectionRead {
                    plan: ProjectionReadPlan {
                        module: "summary".to_string(),
                        projection: "TaskSummary".to_string(),
                        collection: false,
                        derived: Vec::new(),
                    },
                },
            ),
        ];
        for (name, execution) in cases {
            let op = test_support::route(
                "TaskService",
                "op",
                test_support::matrix_facts(),
                execution,
                true,
            );
            let block = render_execution_block(&op);
            assert!(
                !block.is_empty(),
                "kind `{name}` renders nothing — its arm is (again) template-owned"
            );
            assert!(
                block.starts_with("    "),
                "kind `{name}` must render an indented fn body: {block:?}"
            );
        }
    }
}
