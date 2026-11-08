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
    /// The CloudEvents attribute passthrough (`type`/`source`/`subject`),
    /// when the declaration carried one (rexlang #31's delivery model
    /// slice): stored verbatim and AUTHORITATIVE — a downstream generator
    /// maps these onto its CloudEvents envelope instead of deriving
    /// `type`/`source` from the event name. Serde-defaulted for read-compat
    /// with graphs persisted before the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_events: Option<EvtCloudEventsNode>,
    /// Declaration order within the contract file.
    pub ordinal: usize,
}

/// The CloudEvents attribute passthrough of an [`EvtEventNode`] (rex-ir
/// `CloudEventsAttributes`): the attributes exactly as authored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvtCloudEventsNode {
    /// The CloudEvents `type` attribute.
    #[serde(rename = "type")]
    pub event_type: String,
    /// The CloudEvents `source` attribute.
    pub source: String,
    /// The optional CloudEvents `subject` attribute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
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
    /// The parameterized-channel scope (`of`/`scope` clause, rexlang #31's
    /// slice 2: topic templates), verbatim when declared; `None` for a
    /// plain channel. Serde-defaulted for read-compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
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
    /// The `when (...)` filter's source text, verbatim between the
    /// parentheses (rexlang #31's delivery model slice): an envelope-level
    /// delivery filter, as authored. Serde-defaulted for read-compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// The declared `delivery` policy block, when the declaration carried
    /// one (rexlang #31's slice 2): the consumer-side knobs the events
    /// plane lowers (mode, attempt ceiling, backoff, timeout, dead-letter
    /// channel). Serde-defaulted for read-compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<EvtDeliveryPolicyNode>,
    /// Declaration order within the contract file.
    pub ordinal: usize,
}

/// The delivery policy of an [`EvtSubscriptionNode`] (rex-ir
/// `DeliveryPolicy`): mode plus the tuning knobs, each omitted when the
/// declaration didn't pin them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvtDeliveryPolicyNode {
    /// The delivery mode.
    pub mode: EvtDeliveryModeNode,
    /// The `maxAttempts` ceiling, when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    /// The `backoffSeconds` interval, when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_seconds: Option<u32>,
    /// The `timeoutSeconds` budget, when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
    /// The dead-letter channel name, when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_letter: Option<String>,
}

/// The delivery mode of an [`EvtDeliveryPolicyNode`] (rex-ir
/// `DeliveryMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvtDeliveryModeNode {
    /// Delivery may drop: fire-and-forget.
    AtMostOnce,
    /// Delivery may duplicate: acknowledge-then-process.
    AtLeastOnce,
    /// Delivery neither drops nor duplicates: deduplication keys.
    EffectivelyOnce,
}
