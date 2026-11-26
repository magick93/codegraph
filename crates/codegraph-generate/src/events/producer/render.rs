//! Typed publish-construction rendering (issue #820 slice b).
//!
//! Pure: turns [`ProducerPlan`]s into the generated Rust text the command
//! plane splices (the outgoing-event enum, the pre-write re-read, the publish
//! block) and the handler plane calls. Unit-KAT'd; the command generator
//! pre-renders these blocks so the template carries no binding logic (the
//! #338/#590 pre-rendered-block precedent).

use std::collections::BTreeSet;

use codegraph_naming::to_pascal_case;

use super::planner::{ContextFact, ProducerBinding, ProducerPlan};
use crate::events::naming::{channel_enum_name, payload_struct_name};

/// The variable names the generated command exposes to the expressions.
#[derive(Debug, Clone, Copy)]
pub struct PublishRender<'a> {
    /// The post-write `{Entity}Response` local.
    pub response: &'a str,
    /// The pre-write `{Entity}Response` local (only emitted when needed).
    pub previous: &'a str,
    pub correlation: &'a str,
    pub organization: &'a str,
}

impl Default for PublishRender<'_> {
    fn default() -> Self {
        Self {
            response: "after",
            previous: "publish_before",
            correlation: "correlation_id",
            organization: "organization_id",
        }
    }
}

/// The outgoing-event enum stem (`PayRunOutgoingEvent`).
#[must_use]
pub fn outgoing_enum_name(entity: &str) -> String {
    format!("{entity}OutgoingEvent")
}

/// The event variant name (`OrderPlaced`).
#[must_use]
pub fn variant_name(event: &str) -> String {
    to_pascal_case(event)
}

/// The typed construction expression for one binding.
#[must_use]
pub fn payload_expr(binding: &ProducerBinding, ctx: &PublishRender<'_>) -> String {
    match binding {
        ProducerBinding::Record {
            payload_type,
            rust_type,
            is_codelist,
            response_field,
            ..
        } => adapt(
            rust_type,
            payload_type,
            *is_codelist,
            &format!("{}.{response_field}", ctx.response),
        ),
        ProducerBinding::Context {
            fact,
            rust_type,
            payload_type,
            state_field,
            ..
        } => match fact {
            ContextFact::OccurredOn => "chrono::Utc::now()".to_string(),
            ContextFact::PreviousState | ContextFact::NextState => {
                let variable = if *fact == ContextFact::PreviousState {
                    ctx.previous
                } else {
                    ctx.response
                };
                let field = state_field.as_deref().unwrap_or("status");
                adapt(
                    rust_type,
                    payload_type,
                    rust_type != "String",
                    &format!("{variable}.{field}"),
                )
            }
            ContextFact::CorrelationId | ContextFact::OrganizationId => {
                let variable = if *fact == ContextFact::CorrelationId {
                    ctx.correlation
                } else {
                    ctx.organization
                };
                adapt("uuid::Uuid", payload_type, false, variable)
            }
        },
    }
}

/// Adapt a source expression to the declared payload type through the
/// contract's supported conversions (R7/R10).
fn adapt(source_type: &str, payload_type: &str, is_codelist: bool, expr: &str) -> String {
    if same_type(source_type, payload_type) {
        if is_copy(source_type) {
            expr.to_string()
        } else {
            format!("{expr}.clone()")
        }
    } else if is_uuid(source_type) && payload_type == "String" {
        format!("{expr}.to_string()")
    } else if source_type == "i32" && payload_type == "i64" {
        format!("i64::from({expr})")
    } else if is_codelist && payload_type == "String" {
        format!("{expr}.to_string()")
    } else {
        // Validation guarantees a supported conversion; cloning the named
        // value is the safe fallback for an enum-typed slot.
        format!("{expr}.clone()")
    }
}

fn is_uuid(ty: &str) -> bool {
    matches!(ty, "uuid::Uuid" | "Uuid")
}

fn same_type(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if is_uuid(a) && is_uuid(b) {
        return true;
    }
    let clock = |ty: &str| {
        matches!(
            ty,
            "chrono::DateTime<chrono::Utc>" | "chrono::DateTime<Utc>" | "chrono::NaiveDateTime"
        )
    };
    clock(a) && clock(b)
}

fn is_copy(ty: &str) -> bool {
    matches!(
        ty,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "f32"
            | "f64"
            | "bool"
            | "uuid::Uuid"
            | "Uuid"
            | "chrono::NaiveDate"
            | "chrono::NaiveDateTime"
            | "chrono::DateTime<chrono::Utc>"
            | "chrono::DateTime<Utc>"
            | "rust_decimal::Decimal"
    )
}

/// Dedupe an entity's plans by event name, preserving plan order. All plans
/// for one event must agree on the bindings (the same surface + event); the
/// first plan is the construction source.
fn unique_plans(plans: &[ProducerPlan]) -> Vec<&ProducerPlan> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for plan in plans {
        if seen.insert(plan.event.name.clone()) {
            unique.push(plan);
        }
    }
    unique
}

/// The outgoing-event enum definition (empty when there is nothing to
/// publish).
#[must_use]
pub fn render_outgoing_enum(plans: &[ProducerPlan], entity: &str) -> String {
    let unique = unique_plans(plans);
    if unique.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "/// Events an operation on `{entity}` publishes after its write (issue #820).\n\
         /// The handler passes the variants; the command constructs the payloads\n\
         /// from the post-write record.\n\
         #[derive(Debug, Clone, Copy)]\n\
         pub enum {} {{\n",
        outgoing_enum_name(entity)
    );
    for plan in unique {
        out.push_str(&format!("    {},\n", variant_name(&plan.event.name)));
    }
    out.push_str("}\n");
    out
}

/// The pre-write re-read (empty when no binding needs `previousState`).
/// Inserted before the repository write.
#[must_use]
pub fn render_pre_read(plans: &[ProducerPlan], domain_error: &str, auditable: bool) -> String {
    if !plans.iter().any(|plan| plan.needs_previous_state) {
        return String::new();
    }
    let include_deleted = if auditable { ", false" } else { "" };
    format!(
        "        let publish_before = if !outgoing.is_empty() {{\n\
         \x20           self.repo\n\
         \x20               .find_by_id(&tx, id{include_deleted})\n\
         \x20               .await\n\
         \x20               .map_err({domain_error}::from_repo_err)?\n\
         \x20       }} else {{\n\
         \x20           None\n\
         \x20       }};\n"
    )
}

/// The publish block, inserted after the `after_update_tx` hooks and before
/// `commit()`. Empty when there is nothing to publish.
#[must_use]
pub fn render_publish_block(
    plans: &[ProducerPlan],
    domain: &str,
    table: &str,
    entity: &str,
    domain_error: &str,
    auditable: bool,
    entity_table: &str,
) -> String {
    let unique = unique_plans(plans);
    if unique.is_empty() {
        return String::new();
    }
    let ctx = PublishRender::default();
    let include_deleted = if auditable { ", false" } else { "" };
    let enum_path = format!(
        "crate::domain::{domain}::{table}::command::{}",
        outgoing_enum_name(entity)
    );
    let mut out = String::from("        if !outgoing.is_empty() {\n");
    out.push_str(&format!(
        "            let after = self\n\
         \x20               .repo\n\
         \x20               .find_by_id(&tx, id{include_deleted})\n\
         \x20               .await\n\
         \x20               .map_err({domain_error}::from_repo_err)?\n\
         \x20               .ok_or({domain_error}::NotFound)?;\n"
    ));
    if plans.iter().any(|plan| plan.needs_previous_state) {
        out.push_str(&format!(
            "            let publish_before = publish_before\n\
             \x20               .as_ref()\n\
             \x20               .ok_or({domain_error}::NotFound)?;\n"
        ));
    }
    out.push_str(
        "            let publish_ctx = crate::events::emit::EventContext {\n\
         \x20               tenant_id: organization_id,\n\
         \x20               correlation_id,\n\
         \x20               traceparent: traceparent.clone(),\n",
    );
    out.push_str(&format!(
        "                entity_table: Some(\"{entity_table}\"),\n"
    ));
    out.push_str(
        "                entity_id: Some(id),\n\
         \x20           };\n\
         \x20           for event in outgoing {\n\
         \x20               match event {\n",
    );
    for plan in unique {
        let variant = variant_name(&plan.event.name);
        let payload_type = payload_struct_name(&plan.event.name);
        out.push_str(&format!(
            "                    {enum_path}::{variant} => {{\n"
        ));
        out.push_str(&format!(
            "                        let payload = crate::events::contracts::{payload_type} {{\n"
        ));
        for binding in &plan.bindings {
            out.push_str(&format!(
                "                            {}: {},\n",
                binding_payload_field(binding),
                payload_expr(binding, &ctx)
            ));
        }
        out.push_str("                        };\n");
        for (index, channel) in plan.publish.iter().enumerate() {
            let argument = if index + 1 == plan.publish.len() {
                "payload"
            } else {
                "payload.clone()"
            };
            out.push_str(&format!(
                "                        crate::events::emit::publish_{}_in_tx(\n\
                 \x20                           &tx,\n\
                 \x20                           crate::events::emit::{}::{variant}({argument}),\n\
                 \x20                           &publish_ctx,\n\
                 \x20                       )\n\
                 \x20                       .await\n\
                 \x20                       .map_err({domain_error}::from_repo_err)?;\n",
                channel.name_snake,
                channel_enum_name(&channel.name),
            ));
        }
        out.push_str("                    }\n");
    }
    out.push_str(
        "                    _ => {}\n\
         \x20               }\n\
         \x20           }\n\
         \x20       }\n",
    );
    out
}

fn binding_payload_field(binding: &ProducerBinding) -> &str {
    match binding {
        ProducerBinding::Record { payload_field, .. }
        | ProducerBinding::Context { payload_field, .. } => payload_field,
    }
}

/// The handler call argument: the outgoing variants for one op.
#[must_use]
pub fn render_handler_call(
    plans: &[ProducerPlan],
    domain: &str,
    table: &str,
    entity: &str,
) -> String {
    if plans.is_empty() {
        return "&[]".to_string();
    }
    let variants: Vec<String> = unique_plans(plans)
        .iter()
        .map(|plan| {
            format!(
                "crate::domain::{domain}::{table}::command::{}::{}",
                outgoing_enum_name(entity),
                variant_name(&plan.event.name)
            )
        })
        .collect();
    format!("&[{}]", variants.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddd::service_ops::{ServiceOpState, StateName, WireKey};
    use crate::events::model::{EventDefinition, EventFieldDef};
    use crate::events::producer::planner::{ProducerArm, ProducerChannel};

    fn record(payload: &str, payload_type: &str, response: &str, dto: &str) -> ProducerBinding {
        ProducerBinding::Record {
            payload_field: payload.to_string(),
            response_field: response.to_string(),
            payload_type: payload_type.to_string(),
            rust_type: dto.to_string(),
            is_optional: false,
            is_codelist: false,
        }
    }

    fn context(fact: ContextFact, payload_type: &str, rust_type: &str) -> ProducerBinding {
        ProducerBinding::Context {
            payload_field: format!("{}Field", fact.display()),
            fact,
            rust_type: rust_type.to_string(),
            payload_type: payload_type.to_string(),
            state_field: matches!(fact, ContextFact::PreviousState | ContextFact::NextState)
                .then(|| "status".to_string()),
        }
    }

    fn plan(bindings: Vec<ProducerBinding>) -> ProducerPlan {
        ProducerPlan {
            event: EventDefinition {
                name: "OrderPlaced".to_string(),
                version: None,
                fields: bindings
                    .iter()
                    .map(|binding| EventFieldDef {
                        name: match binding {
                            ProducerBinding::Record { payload_field, .. }
                            | ProducerBinding::Context { payload_field, .. } => {
                                payload_field.clone()
                            }
                        },
                        rust_type: match binding {
                            ProducerBinding::Record { payload_type, .. }
                            | ProducerBinding::Context { payload_type, .. } => payload_type.clone(),
                        },
                        resolved_title: None,
                        is_primitive: true,
                    })
                    .collect(),
                domain: Some("orders".to_string()),
            },
            service: "OrderService".to_string(),
            op: "place".to_string(),
            domain: "orders".to_string(),
            entity: "Order".to_string(),
            arm: ProducerArm::Transition(ServiceOpState {
                current_expr: "response.status.to_string()".to_string(),
                wire_key: WireKey::new("status"),
                initial: StateName::new("Draft"),
                machine_id: "order_status".to_string(),
                all: Vec::new(),
                op: Vec::new(),
            }),
            bindings,
            publish: vec![ProducerChannel {
                name: "orders".to_string(),
                name_snake: "orders".to_string(),
                enum_name: "OrdersEvent".to_string(),
                queue: "events_orders".to_string(),
            }],
            needs_previous_state: false,
            state_field: Some("status".to_string()),
            state_dto_type: Some("OrderStatus".to_string()),
        }
    }

    #[test]
    fn record_expressions_follow_the_supported_conversions() {
        let ctx = PublishRender::default();
        assert_eq!(
            payload_expr(&record("name", "String", "name", "String"), &ctx),
            "after.name.clone()"
        );
        assert_eq!(
            payload_expr(&record("total", "i64", "total", "i64"), &ctx),
            "after.total"
        );
        assert_eq!(
            payload_expr(&record("count", "i64", "count", "i32"), &ctx),
            "i64::from(after.count)"
        );
        assert_eq!(
            payload_expr(
                &record("customer", "String", "customer_ref", "uuid::Uuid"),
                &ctx
            ),
            "after.customer_ref.to_string()"
        );
        let mut codelist = record("status", "String", "status", "OrderStatus");
        if let ProducerBinding::Record { is_codelist, .. } = &mut codelist {
            *is_codelist = true;
        }
        assert_eq!(payload_expr(&codelist, &ctx), "after.status.to_string()");
    }

    #[test]
    fn context_expressions_use_the_render_variables() {
        let ctx = PublishRender::default();
        assert_eq!(
            payload_expr(
                &context(
                    ContextFact::OccurredOn,
                    "chrono::DateTime<chrono::Utc>",
                    "chrono::DateTime<chrono::Utc>"
                ),
                &ctx
            ),
            "chrono::Utc::now()"
        );
        assert_eq!(
            payload_expr(
                &context(ContextFact::PreviousState, "OrderStatus", "OrderStatus"),
                &ctx
            ),
            "publish_before.status.clone()"
        );
        assert_eq!(
            payload_expr(
                &context(ContextFact::NextState, "String", "OrderStatus"),
                &ctx
            ),
            "after.status.to_string()"
        );
        assert_eq!(
            payload_expr(
                &context(ContextFact::CorrelationId, "String", "uuid::Uuid"),
                &ctx
            ),
            "correlation_id.to_string()"
        );
        assert_eq!(
            payload_expr(
                &context(ContextFact::OrganizationId, "uuid::Uuid", "uuid::Uuid"),
                &ctx
            ),
            "organization_id"
        );
    }

    #[test]
    fn the_outgoing_enum_is_deduped_and_ordered() {
        let mut second = plan(vec![]);
        second.event.name = "OrderPlaced".to_string();
        let mut other = plan(vec![]);
        other.event.name = "OrderCancelled".to_string();
        let rendered = render_outgoing_enum(&[plan(vec![]), second, other], "Order");
        assert_eq!(
            rendered,
            "/// Events an operation on `Order` publishes after its write (issue #820).\n\
             /// The handler passes the variants; the command constructs the payloads\n\
             /// from the post-write record.\n\
             #[derive(Debug, Clone, Copy)]\n\
             pub enum OrderOutgoingEvent {\n\
             \x20   OrderPlaced,\n\
             \x20   OrderCancelled,\n\
             }\n"
        );
    }

    #[test]
    fn the_publish_block_constructs_and_fans_out() {
        let mut plan = plan(vec![
            record("name", "String", "name", "String"),
            context(ContextFact::NextState, "OrderStatus", "OrderStatus"),
        ]);
        plan.publish.push(ProducerChannel {
            name: "audit".to_string(),
            name_snake: "audit".to_string(),
            enum_name: "AuditEvent".to_string(),
            queue: "events_audit".to_string(),
        });
        let rendered = render_publish_block(
            &[plan],
            "orders",
            "order",
            "Order",
            "OrdersError",
            false,
            "order",
        );
        assert!(rendered.contains("let after = self"));
        assert!(rendered.contains("find_by_id(&tx, id)"));
        assert!(rendered.contains("let payload = crate::events::contracts::OrderPlacedPayload {"));
        assert!(rendered.contains("name: after.name.clone(),"));
        assert!(rendered.contains("nextStateField: after.status.clone(),"));
        assert!(rendered.contains(
            "crate::events::emit::publish_orders_in_tx(\n                            &tx,\n                            crate::events::emit::OrdersEvent::OrderPlaced(payload.clone()),"
        ));
        assert!(rendered.contains(
            "crate::events::emit::publish_audit_in_tx(\n                            &tx,\n                            crate::events::emit::AuditEvent::OrderPlaced(payload),"
        ));
        assert!(rendered.contains("entity_table: Some(\"order\")"));
        assert!(rendered.contains("map_err(OrdersError::from_repo_err)?"));
    }

    #[test]
    fn the_pre_read_and_handler_call_are_conditional() {
        let plain = plan(vec![]);
        assert_eq!(
            render_pre_read(std::slice::from_ref(&plain), "OrdersError", false),
            ""
        );
        let mut transition = plan(vec![context(
            ContextFact::PreviousState,
            "OrderStatus",
            "OrderStatus",
        )]);
        transition.needs_previous_state = true;
        let pre = render_pre_read(std::slice::from_ref(&transition), "OrdersError", true);
        assert!(pre.contains("let publish_before = if !outgoing.is_empty()"));
        assert!(pre.contains("find_by_id(&tx, id, false)"));
        assert_eq!(
            render_handler_call(
                std::slice::from_ref(&transition),
                "orders",
                "order",
                "Order"
            ),
            "&[crate::domain::orders::order::command::OrderOutgoingEvent::OrderPlaced]"
        );
        assert_eq!(render_handler_call(&[], "orders", "order", "Order"), "&[]");
    }
}
