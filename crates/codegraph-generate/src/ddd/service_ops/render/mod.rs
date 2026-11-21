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
//! - `child_op` — the aggregate child `add`/`save`/`remove` plane.
//!
//! Only the BODY region moves — the doc comment, the `#[utoipa::path]`
//! header, the signature and the record-load/authorize prelude stay
//! template-owned for now. Every kind this module does not own renders as an
//! empty string, so an unwired arm keeps its template block.

mod basic;
mod child_op;
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
/// splice, the DTO patch, the command call and the reload envelope) and
/// `child_op` (the aggregate child `add`/`save`/`remove` plane). Every other
/// kind returns an empty string: its arm is still template-owned, so the
/// later wiring stays byte-identical for it.
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
        _ => String::new(),
    }
}

/// The `{Domain}Error` type the emitted `map_err` closures annotate.
pub(super) fn domain_error(facts: &ExecutionRenderFacts) -> String {
    format!("{}Error", codegraph_naming::to_pascal_case(&facts.domain))
}

/// The `include_deleted` argument for the auditable query arity.
pub(super) fn audit_arity(facts: &ExecutionRenderFacts) -> &'static str {
    if facts.auditable { ", false" } else { "" }
}
