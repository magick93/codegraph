//! Handler-side producer wiring (issue #820 slice b, split from `mod.rs` for
//! the #751 file budget): attach the outgoing-event argument to publishing
//! Update routes before the execution blocks are re-rendered.

use std::sync::Arc;

use crate::ddd::service_ops::{ExecutionKind, ServiceOpRoute};
use crate::events::producer::ProducerPlanIndex;
use crate::events::producer::render::render_handler_call;

/// Attach `route.publish_arg` for every Update route whose operation owns a
/// planned event. A no-op without a plan index (the flag is off, or no
/// entity publishes), so the route set stays byte-identical.
pub(super) fn attach_publish_args(
    service_ops: &mut [ServiceOpRoute],
    plans: Option<&Arc<ProducerPlanIndex>>,
    domain: &str,
    schema_title: &str,
    module_name: &str,
    entity_name: &str,
) {
    let Some(index) = plans else {
        return;
    };
    let plans = index.for_entity(domain, schema_title);
    for route in service_ops {
        if !matches!(route.execution, ExecutionKind::Update { .. }) {
            continue;
        }
        let op_plans: Vec<_> = plans
            .iter()
            .filter(|plan| plan.service == route.service && plan.op == route.op_name)
            .cloned()
            .collect();
        if !op_plans.is_empty() {
            route.publish_arg = Some(render_handler_call(
                &op_plans,
                domain,
                module_name,
                entity_name,
            ));
        }
    }
}
