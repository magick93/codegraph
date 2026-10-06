use serde::{Deserialize, Serialize};

/// The graph-native mirror of one compiled rexlang `.evt` event-contract
/// artifact (`rex_ir::events::EventModel`, issue #454): the declared
/// events, the channels publishing them, and the subscriptions consuming
/// them. codegraph-core takes no rex-* deps, so payload field types cross
/// this boundary as pre-serialized `serde_json::Value` payloads (the
/// `DddParam::type_json` precedent — rex-ir `TypeRef` JSON, adjacent
/// tagging under the `"type"` key).
///
/// Events are file-scoped: channel `publishes` entries and subscription
/// `events` lists are intra-file event-name strings kept verbatim
/// (resolved by consumers against the same model — resolution is never a
/// graph-plane concern).
#[derive(Debug, Clone, PartialEq)]
pub struct EvtModelGraph {
    /// The `.evt` source file the model was parsed from. Groups the node
    /// families on read: one `EvtModelGraph` per source path.
    pub source_path: String,
    /// Declared events, in declaration order.
    pub events: Vec<EvtEventNode>,
    /// Declared channels, in declaration order.
    pub channels: Vec<EvtChannelNode>,
    /// Declared subscriptions, in declaration order.
    pub subscriptions: Vec<EvtSubscriptionNode>,
}

/// A declared event: a named contract with its typed payload fields
/// (rex-ir `EventDef` plus its source path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvtEventNode {
    /// The `.evt` source file the event was declared in.
    pub source_path: String,
    /// Event name, unique within its contract file.
    pub name: String,
    /// The pinned event version, when the declaration carried one; free-form
    /// and pinned verbatim.
    pub version: Option<String>,
    /// Payload fields, in declaration order.
    pub fields: Vec<EvtEventField>,
    /// Declaration order within the contract file.
    pub ordinal: usize,
}

/// One typed payload field of an [`EvtEventNode`] (rex-ir `EventField`).
/// Deliberately plain: a name and a resolved type, nothing else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvtEventField {
    /// Field name, unique within its event.
    pub name: String,
    /// The resolved field type as rex-ir `TypeRef` JSON (adjacent tagging).
    pub type_json: serde_json::Value,
    /// The schema title the field's type resolved to at ingest; `None` when
    /// the type is primitive or unresolved.
    pub resolved_title: Option<String>,
}

/// A declared channel with the events published on it (rex-ir `ChannelDef`
/// plus its source path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvtChannelNode {
    /// The `.evt` source file the channel was declared in.
    pub source_path: String,
    /// Channel name, unique within its contract file.
    pub name: String,
    /// The published event names, as authored, in declaration order.
    pub publishes: Vec<String>,
    /// Declaration order within the contract file.
    pub ordinal: usize,
}

/// A declared subscription: a named set of events delivered to one consumer
/// (rex-ir `SubscriptionDef` plus its source path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvtSubscriptionNode {
    /// The `.evt` source file the subscription was declared in.
    pub source_path: String,
    /// Subscription name, unique within its contract file.
    pub name: String,
    /// The subscribed event names, as authored, in declaration order.
    pub events: Vec<String>,
    /// The consumer receiving the events; an opaque name, verbatim.
    pub consumer: String,
    /// Declaration order within the contract file.
    pub ordinal: usize,
}
