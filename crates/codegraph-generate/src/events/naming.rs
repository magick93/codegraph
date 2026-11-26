//! The events plane's ONE naming table (issues #455, #820): payload structs,
//! channel enums, and publish functions. The events generator and the
//! producer's typed-construction renderer both read it, so a channel cannot
//! be named one thing in `emit.rs` and another in the command that publishes
//! onto it.

use codegraph_naming::{to_pascal_case, to_snake_case};

/// The payload struct for an event (`OrderPlacedPayload`).
#[must_use]
pub fn payload_struct_name(event_name: &str) -> String {
    format!("{event_name}Payload")
}

/// The pinned-version const for an event (`ORDER_PLACED_VERSION`).
#[must_use]
pub fn version_const_name(event_name: &str) -> String {
    format!("{}_VERSION", to_snake_case(event_name).to_uppercase())
}

/// The per-channel event enum (`OrdersEvent`).
#[must_use]
pub fn channel_enum_name(channel: &str) -> String {
    format!("{}Event", to_pascal_case(channel))
}

/// The per-channel publish function (`publish_orders`).
#[must_use]
pub fn channel_publish_fn(channel: &str) -> String {
    format!("publish_{}", to_snake_case(channel))
}

/// The transaction-capable publish function (`publish_orders_in_tx`).
#[must_use]
pub fn channel_publish_in_tx_fn(channel: &str) -> String {
    format!("{}_in_tx", channel_publish_fn(channel))
}

/// A consumer's generated type stem (`BillingService`).
#[must_use]
pub fn consumer_type_name(consumer: &str) -> String {
    to_pascal_case(consumer)
}
