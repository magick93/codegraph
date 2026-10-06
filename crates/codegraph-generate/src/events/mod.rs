//! Semantic event architecture (issues #454/#455, Phase 2b): the bridge
//! between the `.evt` DSL (ingested as [`codegraph_core::types::EvtModelGraph`])
//! and the Tera templates — event semantics independent of transport,
//! publications, channels with a transport binding, delivery policy.
//! Capabilities, not technologies (issue #455).
//!
//! The module is inert without `.evt` input: [`EventArchitecture::from_graph`]
//! short-circuits to an empty architecture when the graph carries no event
//! models, so every consumer (and the pgmq setup migration) keeps its
//! pre-#455 byte-identical output.

pub mod model;

pub use model::{
    ChildTrigger, DeliveryGuarantee, DeliveryPolicy, EventArchitecture, EventChannel,
    EventDefinition, EventFieldDef, EventSubscription, PgmqChannel, Publication, PublicationSource,
    QueueInfo, Transport, TransportCapabilities, TriggerPublication,
};

pub(crate) use model::trigger_publication_from_context;
