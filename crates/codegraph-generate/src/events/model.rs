//! The semantic event model (issue #455): [`EventArchitecture`] and its
//! node families.
//!
//! Flow (Phase 2b): `from_graph` reads the `.evt` graph plane
//! (`get_evt_models`) for the declared events/channels/subscriptions and
//! derives the transport bindings + the implicit per-domain trigger
//! channels from the ONE trigger enumeration shared with
//! [`crate::db::ddl::DdlGenerator`] (`enumerate_event_trigger_tables`,
//! which reuses `query_ddl_context` — the exact context builder the
//! per-entity domain-event trigger rendering consumes). No `.evt` models
//! ⇒ an empty architecture is returned before any trigger enumeration,
//! so flag-off runs stay byte-identical and zero-cost.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_core::types::EvtEventField;
use serde::Serialize;

use crate::db::ddl::DdlContext;
use crate::db::dialect::SqlDialect;
use crate::error::Result;

/// One-time stderr warnings (per process). Generators (and the
/// architecture itself) may consult the model several times per run —
/// dedupe so each problem reports at most once. Same pattern as
/// `ddd::design::warn_once`.
static WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(crate) fn warn_once(key: &str, message: &str) {
    let set = WARNED.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut set) = set.lock()
        && set.insert(key.to_string())
    {
        eprintln!("warning: events: {message}");
    }
}

/// One declared event: a named contract with its typed payload fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventDefinition {
    /// Event name, unique within its contract file.
    pub name: String,
    /// The pinned event version, when the declaration carried one.
    pub version: Option<String>,
    /// Payload fields, in declaration order.
    pub fields: Vec<EventFieldDef>,
    /// The domain of the first field whose `resolved_title` resolves to a
    /// schema (the event's owning bounded context); `None` for
    /// all-primitive events.
    pub domain: Option<String>,
}

/// One typed payload field of an [`EventDefinition`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventFieldDef {
    /// Field name, as authored.
    pub name: String,
    /// The mapped Rust type: the rex primitive mapping, the resolved
    /// schema's `rust_type_name`, or `serde_json::Value` when the field's
    /// type neither maps nor resolves (warned once).
    pub rust_type: String,
    /// The schema title the field's type resolved to at ingest.
    pub resolved_title: Option<String>,
    /// `true` when the field mapped to a scalar (rex primitive / the
    /// `Uuid`/`DateTime`/`Decimal`/`Date` datatype family); `false` for
    /// class/enum-typed and unresolved fields.
    pub is_primitive: bool,
}

/// A declared channel: a named stream of publications with one transport
/// binding. DSL channels carry [`PublicationSource::Application`]
/// publications; the implicit per-domain trigger channels carry
/// [`PublicationSource::Database`] publications (the per-table trigger
/// inventory).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventChannel {
    /// Channel name. DSL channels use the declared name; the implicit
    /// per-domain channels use the domain name.
    pub name: String,
    /// The transport binding (capabilities, not technologies).
    pub transport: Transport,
    /// The events published on this channel, in declaration order.
    pub publications: Vec<Publication>,
}

/// One publication on an [`EventChannel`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Publication {
    /// The published event name. For [`PublicationSource::Database`] this
    /// is the table name — the change-stream identity (the
    /// created/updated/deleted variants ride the trigger envelope at
    /// emission time, mirroring today's `emit_domain_event` payload).
    pub event: String,
    /// Where the publication originates.
    pub source: PublicationSource,
}

/// Where a publication originates: the application (a DSL `publishes`) or
/// the database (a per-table domain-event trigger).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum PublicationSource {
    /// Declared in the `.evt` DSL: the application publishes the event.
    Application,
    /// Emitted by the per-table domain-event triggers.
    Database(TriggerPublication),
}

/// The per-table trigger publication — mirrors
/// `templates/db/domain_event_trigger.tera`'s context shape exactly
/// (schema/table/append_only for the table plus its depth-flattened child
/// tables; children inherit the parent's append_only semantics, #284).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TriggerPublication {
    pub schema_name: String,
    pub table_name: String,
    pub append_only: bool,
    pub child_tables: Vec<ChildTrigger>,
}

/// One child table of a [`TriggerPublication`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChildTrigger {
    pub schema_name: String,
    pub table_name: String,
    pub append_only: bool,
}

/// A channel's transport binding. Capabilities, not technologies (#455):
/// consumers branch on [`Transport::capabilities`], never on the concrete
/// transport.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Transport {
    /// A pgmq queue — the ONLY binding this phase.
    Pgmq(PgmqChannel),
    /// No persisted transport on this database target (sqlite today;
    /// future bindings land as new variants).
    Unbound,
}

impl Transport {
    /// What the binding guarantees to a publisher/consumer.
    pub fn capabilities(&self) -> TransportCapabilities {
        match self {
            Transport::Pgmq(_) => TransportCapabilities {
                ordering: true,
                transactional: true,
                persisted: true,
                push: false,
                fanout: false,
            },
            Transport::Unbound => TransportCapabilities {
                ordering: false,
                transactional: false,
                persisted: false,
                push: false,
                fanout: false,
            },
        }
    }
}

/// A pgmq-backed channel: the queue name (`events_{channel}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PgmqChannel {
    pub queue: String,
}

/// The capability set of a [`Transport`] (issue #455 "capabilities, not
/// technologies").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportCapabilities {
    /// Messages are delivered in publish order.
    pub ordering: bool,
    /// Publishing joins the caller's database transaction.
    pub transactional: bool,
    /// Messages survive a broker/process restart.
    pub persisted: bool,
    /// Messages are pushed to consumers (vs polled).
    pub push: bool,
    /// One publication reaches many consumers without extra plumbing.
    pub fanout: bool,
}

/// A declared subscription: a named set of events delivered to one
/// consumer under a delivery policy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventSubscription {
    pub name: String,
    /// The subscribed event names, as authored.
    pub events: Vec<String>,
    /// The consumer receiving the events; an opaque name, verbatim.
    pub consumer: String,
    pub delivery: DeliveryPolicy,
}

/// Delivery policy for an [`EventSubscription`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeliveryPolicy {
    pub guarantee: DeliveryGuarantee,
    pub max_retries: u32,
    /// Exponential-backoff base in seconds (attempt N waits
    /// `initial_backoff_secs * 2^(N-1)` — today's `dispatch.tera`
    /// `delay_for_attempt`).
    pub initial_backoff_secs: u64,
    pub timeout_secs: u64,
}

impl DeliveryPolicy {
    /// Today's dispatch constants (`templates/webhook/dispatch.tera`):
    /// 5 retries, 10s exponential-backoff base, 30s visibility timeout.
    pub fn at_least_once_default() -> Self {
        Self {
            guarantee: DeliveryGuarantee::AtLeastOnce,
            max_retries: 5,
            initial_backoff_secs: 10,
            timeout_secs: 30,
        }
    }
}

/// The delivery guarantee. `ExactlyOnce` is deliberately absent (issue
/// #455 §8: at-least-once + idempotent consumers is the model); a
/// CloudEvents envelope is a future `Envelope` variant (documented, not
/// declared).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DeliveryGuarantee {
    AtLeastOnce,
}

/// One pgmq queue in the project's queue inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueInfo {
    pub queue: String,
    /// The DSL channel owning the queue, when it is a channel queue.
    pub channel: Option<String>,
    /// The domain owning the queue, when it is a per-domain trigger queue.
    pub domain: Option<String>,
}

/// The semantic event architecture: every declared event, channel
/// (DSL + implicit per-domain trigger channels), and subscription, with
/// transport bindings resolved against the generation target.
///
/// Built once per generation run by consumers that need it
/// ([`crate::db::event_trigger::PgmqSetupGenerator`] today; the events
/// generators next phase). Empty when the graph carries no `.evt` models
/// — every consumer gates on [`EventArchitecture::is_empty`], and
/// flag-off runs never reach the (entity-count-proportional) trigger
/// enumeration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventArchitecture {
    /// Declared events, ordered by (source_path, ordinal).
    pub events: Vec<EventDefinition>,
    /// DSL channels (Application-sourced publications), sorted by name.
    pub channels: Vec<EventChannel>,
    /// The implicit per-domain trigger channels — one per domain that has
    /// entities (entity-less custom-routes domains never appear: the
    /// trigger enumeration is entity-driven), sorted by domain name.
    pub domain_channels: Vec<EventChannel>,
    /// Declared subscriptions, ordered by (source_path, ordinal).
    pub subscriptions: Vec<EventSubscription>,
}

/// One trigger-bearing table from the shared enumeration: the domain it
/// belongs to plus its trigger publication.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TriggerTable {
    pub domain: String,
    pub publication: TriggerPublication,
}

impl EventArchitecture {
    /// Build the architecture from the graph. Empty (and warning-free)
    /// when the graph carries no `.evt` models.
    ///
    /// Binding rule: on a `has_plpgsql()` target every DSL channel binds
    /// [`Transport::Pgmq`] (`events_{name}`); otherwise [`Transport::Unbound`]
    /// (warned once per channel — "channel skipped"). The implicit
    /// per-domain channels bind the same way.
    pub async fn from_graph(
        db: &dyn GraphQuerier,
        dialect: &dyn SqlDialect,
        config: &DomainConfig,
    ) -> Result<Self> {
        let models = db.get_evt_models().await?;
        if models.is_empty() {
            // Byte-identity + cost gate: no `.evt` input ⇒ empty
            // architecture; the trigger enumeration (the only
            // expensive part — one DDL context per entity) never runs.
            return Ok(Self::default());
        }

        let events = Self::collect_events(db, &models).await?;
        let channels = Self::collect_channels(&models, dialect);
        let domain_channels = Self::collect_domain_channels(db, config, dialect).await?;
        let subscriptions = models
            .iter()
            .flat_map(|m| m.subscriptions.iter())
            .map(|s| EventSubscription {
                name: s.name.clone(),
                events: s.events.clone(),
                consumer: s.consumer.clone(),
                delivery: DeliveryPolicy::at_least_once_default(),
            })
            .collect();

        Ok(Self {
            events,
            channels,
            domain_channels,
            subscriptions,
        })
    }

    /// `true` when the graph carried no `.evt` input. Every consumer must
    /// gate on this — the no-`.evt` output must stay byte-identical to
    /// the pre-#455 generator output.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
            && self.channels.is_empty()
            && self.domain_channels.is_empty()
            && self.subscriptions.is_empty()
    }

    /// Look up a declared event by name (first match in
    /// (source_path, ordinal) order).
    pub fn event(&self, name: &str) -> Option<&EventDefinition> {
        self.events.iter().find(|e| e.name == name)
    }

    /// Every declared event name, sorted and deduped.
    pub fn declared_event_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.events.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// The DSL channels with a persisted transport binding (unbound
    /// channels were warned once at construction — "channel skipped").
    pub fn bound_channels(&self) -> impl Iterator<Item = &EventChannel> {
        self.channels
            .iter()
            .filter(|c| matches!(c.transport, Transport::Pgmq(_)))
    }

    /// EVERY pgmq queue in the architecture: the per-domain trigger
    /// queues plus the DSL channel queues, sorted by queue name and
    /// deduped. A channel named like a domain collapses into ONE entry
    /// carrying both sources (`channel` + `domain` both set), warned
    /// once. Unbound channels contribute nothing.
    pub fn queue_inventory(&self) -> Vec<QueueInfo> {
        let mut entries: Vec<QueueInfo> = Vec::new();
        for channel in &self.channels {
            if let Transport::Pgmq(pgmq) = &channel.transport {
                entries.push(QueueInfo {
                    queue: pgmq.queue.clone(),
                    channel: Some(channel.name.clone()),
                    domain: None,
                });
            }
        }
        for domain_channel in &self.domain_channels {
            if let Transport::Pgmq(pgmq) = &domain_channel.transport {
                entries.push(QueueInfo {
                    queue: pgmq.queue.clone(),
                    channel: None,
                    domain: Some(domain_channel.name.clone()),
                });
            }
        }
        // Stable sort keeps channel queues ahead of domain queues for an
        // equal name (channels are pushed first), so the merged entry
        // carries both sources deterministically.
        entries.sort_by(|a, b| a.queue.cmp(&b.queue));

        let mut merged: Vec<QueueInfo> = Vec::new();
        for info in entries {
            match merged.last_mut() {
                Some(last) if last.queue == info.queue => {
                    warn_once(
                        &format!("queue-collision:{}", info.queue),
                        &format!(
                            "queue '{}' is declared by both a channel and a domain; the domain migration band owns its creation",
                            info.queue
                        ),
                    );
                    if last.channel.is_none() {
                        last.channel = info.channel;
                    }
                    if last.domain.is_none() {
                        last.domain = info.domain;
                    }
                }
                _ => merged.push(info),
            }
        }
        merged
    }

    /// The DSL channel queue names for the pgmq setup migration: sorted,
    /// deduped, and with domain-owned queues EXCLUDED (`pgmq.create` is
    /// not idempotent — the domain loop already creates colliding
    /// queues; each collision is warned once). Empty when no DSL channel
    /// binds a transport.
    pub fn channel_queues(&self) -> Vec<String> {
        let domain_queues: HashSet<&str> = self
            .domain_channels
            .iter()
            .filter_map(|dc| match &dc.transport {
                Transport::Pgmq(pgmq) => Some(pgmq.queue.as_str()),
                Transport::Unbound => None,
            })
            .collect();
        let mut queues: Vec<String> = self
            .bound_channels()
            .filter_map(|c| match &c.transport {
                Transport::Pgmq(pgmq) => Some(pgmq.queue.clone()),
                Transport::Unbound => None,
            })
            .collect();
        queues.sort();
        queues.dedup();
        queues.retain(|queue| {
            if domain_queues.contains(queue.as_str()) {
                warn_once(
                    &format!("queue-collision:{}", queue),
                    &format!(
                        "channel queue '{queue}' collides with a per-domain trigger queue; the domain migration band owns it"
                    ),
                );
                false
            } else {
                true
            }
        });
        queues
    }

    async fn collect_events(
        db: &dyn GraphQuerier,
        models: &[codegraph_core::types::EvtModelGraph],
    ) -> Result<Vec<EventDefinition>> {
        let mut events = Vec::new();
        // `get_evt_models` returns models grouped by source_path (sorted)
        // with events in declaration order — iteration IS the
        // (source_path, ordinal) order.
        for model in models {
            for evt in &model.events {
                let mut fields = Vec::with_capacity(evt.fields.len());
                let mut domain = None;
                for field in &evt.fields {
                    if domain.is_none()
                        && let Some(title) = field.resolved_title.as_deref()
                        && let Some(schema) = db.get_schema(title).await?
                    {
                        domain = schema.domain;
                    }
                    fields.push(Self::map_field(evt, field, db).await);
                }
                events.push(EventDefinition {
                    name: evt.name.clone(),
                    version: evt.version.clone(),
                    fields,
                    domain,
                });
            }
        }
        Ok(events)
    }

    async fn map_field(
        evt: &codegraph_core::types::EvtEventNode,
        field: &EvtEventField,
        db: &dyn GraphQuerier,
    ) -> EventFieldDef {
        let resolved_title = field.resolved_title.clone();
        let type_ref: Option<rex_ir::TypeRef> = serde_json::from_value(field.type_json.clone())
            .map_err(|e| {
                warn_once(
                    &format!("field-type:{}:{}", evt.name, field.name),
                    &format!(
                        "event `{}' field `{}` has an unparseable rex type ({e}); mapping to `serde_json::Value`",
                        evt.name, field.name
                    ),
                );
            })
            .ok();
        if let Some(type_ref) = &type_ref
            && let Some(rust_type) = crate::ddd::design::rex_type_to_rust(type_ref)
        {
            return EventFieldDef {
                name: field.name.clone(),
                rust_type,
                resolved_title,
                is_primitive: true,
            };
        }
        // Named type (class/enum/datatype beyond the scalar family):
        // resolve against the graph's schemas.
        if let Some(title) = &field.resolved_title
            && let Ok(Some(schema)) = db.get_schema(title).await
        {
            return EventFieldDef {
                name: field.name.clone(),
                rust_type: schema.rust_type_name,
                resolved_title,
                is_primitive: false,
            };
        }
        let what = match &field.resolved_title {
            Some(title) => format!("type `{title}`"),
            None => "type".to_string(),
        };
        warn_once(
            &format!("field-unresolved:{}:{}", evt.name, field.name),
            &format!(
                "event `{}' field `{}` {what} does not resolve to a schema; mapping to `serde_json::Value`",
                evt.name, field.name
            ),
        );
        EventFieldDef {
            name: field.name.clone(),
            rust_type: "serde_json::Value".to_string(),
            resolved_title,
            is_primitive: false,
        }
    }

    fn collect_channels(
        models: &[codegraph_core::types::EvtModelGraph],
        dialect: &dyn SqlDialect,
    ) -> Vec<EventChannel> {
        let mut channel_nodes: Vec<&codegraph_core::types::EvtChannelNode> =
            models.iter().flat_map(|m| m.channels.iter()).collect();
        channel_nodes.sort_by(|a, b| a.name.cmp(&b.name));
        channel_nodes
            .into_iter()
            .map(|ch| EventChannel {
                name: ch.name.clone(),
                transport: bind_transport(dialect, &ch.name),
                publications: ch
                    .publishes
                    .iter()
                    .map(|event| Publication {
                        event: event.clone(),
                        source: PublicationSource::Application,
                    })
                    .collect(),
            })
            .collect()
    }

    async fn collect_domain_channels(
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        dialect: &dyn SqlDialect,
    ) -> Result<Vec<EventChannel>> {
        // Group the shared trigger enumeration by domain; BTreeMap gives
        // sorted domain names. Domains without entities (including
        // entity-less custom-routes domains) never appear — the
        // enumeration is entity-driven, mirroring PgmqSetupGenerator's
        // filter from the other side.
        let mut by_domain: BTreeMap<String, Vec<TriggerPublication>> = BTreeMap::new();
        for table in enumerate_event_trigger_tables(db, config).await? {
            by_domain
                .entry(table.domain)
                .or_default()
                .push(table.publication);
        }
        Ok(by_domain
            .into_iter()
            .map(|(domain, publications)| EventChannel {
                transport: bind_transport(dialect, &domain),
                name: domain.clone(),
                publications: publications
                    .into_iter()
                    .map(|publication| Publication {
                        event: publication.table_name.clone(),
                        source: PublicationSource::Database(publication),
                    })
                    .collect(),
            })
            .collect())
    }
}

/// The binding rule (issue #455 Phase 2b): on a `has_plpgsql()` target
/// every channel binds [`Transport::Pgmq`] (`events_{name}`); otherwise
/// [`Transport::Unbound`], warned once per channel.
fn bind_transport(dialect: &dyn SqlDialect, name: &str) -> Transport {
    if dialect.has_plpgsql() {
        Transport::Pgmq(PgmqChannel {
            queue: format!("events_{name}"),
        })
    } else {
        warn_once(
            &format!("unbound:{name}"),
            &format!(
                "channel '{name}' requires a persisted transport; the {} target provides none — channel skipped",
                dialect.name()
            ),
        );
        Transport::Unbound
    }
}

/// The per-table domain-event trigger enumeration — the ONE place the
/// trigger-bearing table set is derived. Both the per-entity
/// domain-event trigger rendering ([`crate::db::ddl::DdlGenerator`], via
/// the very same `query_ddl_context` this function calls per generation
/// order entry) and the semantic event model (the implicit per-domain
/// channels' trigger publications) consume it, so the trigger set is
/// identical by construction: every entity with a non-empty `table_name`
/// in generation order, depth-flattened child tables, the append_only
/// flag, and the domain per table.
///
/// Only runs when `.evt` models are present (see
/// [`EventArchitecture::from_graph`]'s early return).
pub(crate) async fn enumerate_event_trigger_tables(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
) -> Result<Vec<TriggerTable>> {
    let order = crate::ordering::compute_generation_order(db, config).await?;
    let candidates = db.get_parent_candidates().await?;
    // Scratch generator: only its context builder (and the parent
    // candidates threaded through it) is consulted — no output_dir use.
    let ddl = crate::db::ddl::DdlGenerator::new(Path::new("")).with_parent_candidates(candidates);
    let mut tables = Vec::new();
    for entry in &order {
        let ctx = ddl
            .query_ddl_context(db, &entry.schema_title, &entry.domain, config)
            .await?;
        if ctx.table_name.is_empty() {
            continue;
        }
        tables.push(TriggerTable {
            domain: ctx.domain.clone(),
            publication: trigger_publication_from_context(&ctx),
        });
    }
    Ok(tables)
}

/// The trigger view of a DDL context — mirrors
/// `templates/db/domain_event_trigger.tera`'s context shape exactly.
/// `DdlGenerator` consults it for its trigger-bearing-table skip and the
/// event model consumes it for the trigger publications.
pub(crate) fn trigger_publication_from_context(ctx: &DdlContext) -> TriggerPublication {
    TriggerPublication {
        schema_name: ctx.schema_name.clone(),
        table_name: ctx.table_name.clone(),
        append_only: ctx.append_only,
        child_tables: ctx
            .child_tables
            .iter()
            .map(|child| ChildTrigger {
                schema_name: child.schema_name.clone(),
                table_name: child.table_name.clone(),
                // Children inherit the parent's append_only semantics
                // (#284) — exactly what the trigger template renders.
                append_only: ctx.append_only,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::dialect::{DatabaseTarget, dialect_for_target};
    use crate::project_config::ProjectConfig;
    use crate::template_engine::create_tera;
    use crate::traits::{EntityGenerator, GeneratedFile};
    use codegraph_core::mock::MockEngine;
    use codegraph_core::traits::GraphIngestor;
    use codegraph_core::types::{PropertyNode, SchemaNode};
    use codegraph_type_contracts::RefClassificationKind;

    // ── fixtures ──────────────────────────────────────────────────────

    fn entity_schema(title: &str, table: &str, domain: &str) -> SchemaNode {
        SchemaNode {
            namespace: None,
            schema_id: format!("{domain}/json/{title}.json"),
            title: title.to_string(),
            description: None,
            schema_type: "object".to_string(),
            classification: "entity".to_string(),
            domain: Some(domain.to_string()),
            rel_path: format!("{domain}/json/{title}.json"),
            pg_type: "UUID".to_string(),
            rust_type: format!("{}Root", table), // unused by the model
            sea_orm_type: "Entity".to_string(),
            rust_type_name: table.to_string(),
            pg_table_name: table.to_string(),
            api_path_segment: String::new(),
            parent_schema: None,
            is_entity: true,
            is_codelist: false,
            is_primitive_wrapper: false,
            has_all_of: false,
            has_one_of: false,
            has_any_of: false,
            has_definitions: false,
            custom_annotations: Default::default(),
            access: None,
            annotations: None,
        }
    }

    fn vo_schema(title: &str, table: &str, domain: &str) -> SchemaNode {
        let mut schema = entity_schema(title, table, domain);
        schema.classification = "value_object".to_string();
        schema.is_entity = false;
        schema
    }

    fn string_prop(name: &str) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: "string".to_string(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: true,
            is_id: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: name.to_string(),
            pg_column_type: "TEXT".to_string(),
            rust_field_name: name.to_string(),
            rust_field_type: "String".to_string(),
            sea_orm_type: "Text".to_string(),
            render_strategy: "direct_column".to_string(),
            ref_target: None,
            classification: None,
            projection: None,
            classification_kind: Some(RefClassificationKind::PrimitiveWrapper),
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn vo_prop(name: &str, target: &str) -> PropertyNode {
        PropertyNode {
            name: name.to_string(),
            prop_type: "object".to_string(),
            description: None,
            format: None,
            is_required: false,
            is_nullable: true,
            is_id: false,
            is_array: false,
            min_items: None,
            max_items: None,
            pattern: None,
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            pg_column_name: name.to_string(),
            pg_column_type: "JSONB".to_string(),
            rust_field_name: name.to_string(),
            rust_field_type: name.to_string(),
            sea_orm_type: "JsonBinary".to_string(),
            render_strategy: "value_object".to_string(),
            ref_target: Some(target.to_string()),
            classification: Some("value_object".to_string()),
            projection: None,
            classification_kind: None,
            ui_override_detail: None,
            ui_override_list_cell: None,
            ui_override_form: None,
            ui_override_inline: None,
            type_expr: None,
        }
    }

    fn event(
        source: &str,
        name: &str,
        version: Option<&str>,
        fields: Vec<EvtEventField>,
        ordinal: usize,
    ) -> codegraph_core::types::EvtEventNode {
        codegraph_core::types::EvtEventNode {
            source_path: source.to_string(),
            name: name.to_string(),
            version: version.map(|v| v.to_string()),
            fields,
            ordinal,
        }
    }

    fn field(name: &str, type_json: serde_json::Value, resolved: Option<&str>) -> EvtEventField {
        EvtEventField {
            name: name.to_string(),
            type_json,
            resolved_title: resolved.map(|t| t.to_string()),
        }
    }

    fn primitive_string_json() -> serde_json::Value {
        serde_json::json!({"type": "primitive", "value": "string"})
    }

    fn class_json(name: &str) -> serde_json::Value {
        serde_json::json!({"type": "class", "value": {"package": "app", "name": name}})
    }

    fn channel(
        source: &str,
        name: &str,
        publishes: Vec<&str>,
        ordinal: usize,
    ) -> codegraph_core::types::EvtChannelNode {
        codegraph_core::types::EvtChannelNode {
            source_path: source.to_string(),
            name: name.to_string(),
            publishes: publishes.into_iter().map(|s| s.to_string()).collect(),
            ordinal,
        }
    }

    fn subscription(
        source: &str,
        name: &str,
        events: Vec<&str>,
        consumer: &str,
        ordinal: usize,
    ) -> codegraph_core::types::EvtSubscriptionNode {
        codegraph_core::types::EvtSubscriptionNode {
            source_path: source.to_string(),
            name: name.to_string(),
            events: events.into_iter().map(|s| s.to_string()).collect(),
            consumer: consumer.to_string(),
            ordinal,
        }
    }

    fn model(
        source: &str,
        events: Vec<codegraph_core::types::EvtEventNode>,
        channels: Vec<codegraph_core::types::EvtChannelNode>,
        subscriptions: Vec<codegraph_core::types::EvtSubscriptionNode>,
    ) -> codegraph_core::types::EvtModelGraph {
        codegraph_core::types::EvtModelGraph {
            source_path: source.to_string(),
            events,
            channels,
            subscriptions,
        }
    }

    fn config() -> DomainConfig {
        toml::from_str(
            r#"
[domains.trading]
label = "Trading"
schema_dir = "schemas/trading"
postgres_schema = "trading"
"#,
        )
        .expect("config parses")
    }

    fn tera() -> tera::Tera {
        create_tera(Path::new("")).expect("embedded templates")
    }

    /// Entity `TradeType` (table `trade`) with a VO child `detail`
    /// (table `trade_detail` — `{parent_table}_{field_name}`).
    async fn db_with_entity_and_child() -> MockEngine {
        let trade_detail = vo_schema("TradeDetailType", "trade_detail", "trading");
        MockEngine::builder()
            .with_schema(entity_schema("TradeType", "trade", "trading"))
            .with_schema(trade_detail.clone())
            .with_properties(
                "TradeType",
                vec![string_prop("symbol"), vo_prop("detail", "TradeDetailType")],
            )
            .with_properties("TradeDetailType", vec![string_prop("note")])
            // Wire up $ref resolution for the ValueObject property.
            .with_ref_target("detail", "TradeType", trade_detail)
            .build()
    }

    // ── capabilities / policy truth tables ────────────────────────────

    #[test]
    fn pgmq_transport_capabilities_truth_table() {
        let caps = Transport::Pgmq(PgmqChannel {
            queue: "events_orders".to_string(),
        })
        .capabilities();
        assert!(caps.ordering);
        assert!(caps.transactional);
        assert!(caps.persisted);
        assert!(!caps.push);
        assert!(!caps.fanout);
    }

    #[test]
    fn unbound_transport_capabilities_all_false() {
        let caps = Transport::Unbound.capabilities();
        assert!(!caps.ordering);
        assert!(!caps.transactional);
        assert!(!caps.persisted);
        assert!(!caps.push);
        assert!(!caps.fanout);
    }

    #[test]
    fn delivery_policy_defaults_match_dispatch_constants() {
        let policy = DeliveryPolicy::at_least_once_default();
        assert_eq!(policy.guarantee, DeliveryGuarantee::AtLeastOnce);
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_backoff_secs, 10);
        assert_eq!(policy.timeout_secs, 30);
    }

    // ── empty graph ⇒ empty architecture ─────────────────────────────

    #[tokio::test]
    async fn graph_without_evt_models_yields_empty_architecture() {
        let db = db_with_entity_and_child().await;
        let arch = EventArchitecture::from_graph(
            &db,
            &*dialect_for_target(DatabaseTarget::Postgres),
            &config(),
        )
        .await
        .expect("architecture builds");
        assert!(arch.is_empty());
        assert!(arch.events.is_empty());
        assert!(arch.channels.is_empty());
        assert!(arch.domain_channels.is_empty());
        assert!(arch.subscriptions.is_empty());
        assert!(arch.queue_inventory().is_empty());
        assert!(arch.channel_queues().is_empty());
        assert_eq!(arch.declared_event_names(), Vec::<&str>::new());
        assert!(arch.event("whatever").is_none());
    }

    // ── events: fields, versions, domain derivation ──────────────────

    #[tokio::test]
    async fn events_map_fields_and_derive_domain() {
        let db = db_with_entity_and_child().await;
        db.ingest_evt_model(&model(
            "contracts.evt",
            vec![
                event(
                    "contracts.evt",
                    "TradeExecuted",
                    Some("2.1.0"),
                    vec![
                        field("symbol", primitive_string_json(), None),
                        field("trade", class_json("TradeType"), Some("TradeType")),
                        field("ghost", class_json("GhostType"), Some("GhostType")),
                    ],
                    0,
                ),
                event(
                    "contracts.evt",
                    "Heartbeat",
                    None,
                    vec![field("note", primitive_string_json(), None)],
                    1,
                ),
            ],
            vec![],
            vec![],
        ))
        .await
        .expect("ingest evt model");

        let arch = EventArchitecture::from_graph(
            &db,
            &*dialect_for_target(DatabaseTarget::Postgres),
            &config(),
        )
        .await
        .expect("architecture builds");
        assert!(!arch.is_empty());

        assert_eq!(
            arch.declared_event_names(),
            vec!["Heartbeat", "TradeExecuted"]
        );

        let executed = arch.event("TradeExecuted").expect("event found");
        assert_eq!(executed.version.as_deref(), Some("2.1.0"));
        assert_eq!(executed.domain.as_deref(), Some("trading"));
        assert_eq!(executed.fields.len(), 3);
        assert_eq!(executed.fields[0].name, "symbol");
        assert_eq!(executed.fields[0].rust_type, "String");
        assert!(executed.fields[0].is_primitive);
        assert_eq!(executed.fields[1].name, "trade");
        assert_eq!(executed.fields[1].rust_type, "trade");
        assert!(!executed.fields[1].is_primitive);
        assert_eq!(
            executed.fields[1].resolved_title.as_deref(),
            Some("TradeType")
        );
        assert_eq!(executed.fields[2].rust_type, "serde_json::Value");
        assert!(!executed.fields[2].is_primitive);

        let heartbeat = arch.event("Heartbeat").expect("event found");
        assert!(heartbeat.version.is_none());
        assert_eq!(
            heartbeat.domain, None,
            "all-primitive events derive no domain"
        );
    }

    // ── channels: binding rule ────────────────────────────────────────

    async fn db_with_channels() -> MockEngine {
        let db = db_with_entity_and_child().await;
        db.ingest_evt_model(&model(
            "contracts.evt",
            vec![],
            vec![channel("contracts.evt", "orders", vec!["TradeExecuted"], 0)],
            vec![subscription(
                "contracts.evt",
                "audit",
                vec!["TradeExecuted"],
                "audit-worker",
                0,
            )],
        ))
        .await
        .expect("ingest evt model");
        db
    }

    #[tokio::test]
    async fn dsl_channels_bind_pgmq_on_postgres() {
        let db = db_with_channels().await;
        let arch = EventArchitecture::from_graph(
            &db,
            &*dialect_for_target(DatabaseTarget::Postgres),
            &config(),
        )
        .await
        .expect("architecture builds");
        assert_eq!(arch.channels.len(), 1);
        let ch = &arch.channels[0];
        assert_eq!(ch.name, "orders");
        assert_eq!(
            ch.transport,
            Transport::Pgmq(PgmqChannel {
                queue: "events_orders".to_string()
            })
        );
        assert_eq!(ch.publications.len(), 1);
        assert_eq!(ch.publications[0].event, "TradeExecuted");
        assert_eq!(ch.publications[0].source, PublicationSource::Application);
        assert_eq!(arch.bound_channels().count(), 1);

        let sub = &arch.subscriptions[0];
        assert_eq!(sub.name, "audit");
        assert_eq!(sub.consumer, "audit-worker");
        assert_eq!(sub.delivery, DeliveryPolicy::at_least_once_default());
    }

    #[tokio::test]
    async fn dsl_channels_are_unbound_on_sqlite() {
        let db = db_with_channels().await;
        let arch = EventArchitecture::from_graph(
            &db,
            &*dialect_for_target(DatabaseTarget::Sqlite),
            &config(),
        )
        .await
        .expect("architecture builds");
        assert_eq!(arch.channels.len(), 1);
        assert_eq!(arch.channels[0].transport, Transport::Unbound);
        assert!(!arch.channels[0].transport.capabilities().persisted);
        assert_eq!(arch.bound_channels().count(), 0);
        assert!(arch.channel_queues().is_empty());
        assert!(arch.queue_inventory().is_empty());
    }

    // ── domain channels: trigger-publication parity with the DDL ─────

    #[tokio::test]
    async fn domain_channels_reproduce_the_ddl_trigger_set() {
        let db = db_with_entity_and_child().await;
        let config = config();
        // The domain channels exist only when the graph carries `.evt`
        // models — a minimal channel-less contract suffices.
        db.ingest_evt_model(&model("contracts.evt", vec![], vec![], vec![]))
            .await
            .expect("ingest evt model");

        // The DDL generator's event-trigger output for the fixture — one
        // file per generation-order entry (the entity AND its VO, which
        // carries a pg_table_name and is therefore its own entry).
        let dir = tempfile::TempDir::new().unwrap();
        let generator = crate::db::ddl::DdlGenerator::new(dir.path());
        let mut trigger_files: Vec<GeneratedFile> = Vec::new();
        for title in ["TradeDetailType", "TradeType"] {
            for file in generator
                .generate(
                    &db,
                    title,
                    "trading",
                    &config,
                    &tera(),
                    &ProjectConfig::default(),
                )
                .await
                .expect("ddl generates")
            {
                if file.path.to_string_lossy().contains("event_trigger") {
                    trigger_files.push(file);
                }
            }
        }
        assert!(!trigger_files.is_empty(), "event triggers emitted");

        // The model's domain channel publications.
        let arch = EventArchitecture::from_graph(
            &db,
            &*dialect_for_target(DatabaseTarget::Postgres),
            &config,
        )
        .await
        .expect("architecture builds");
        assert_eq!(arch.domain_channels.len(), 1);
        let domain_channel = &arch.domain_channels[0];
        assert_eq!(domain_channel.name, "trading");
        assert_eq!(
            domain_channel.transport,
            Transport::Pgmq(PgmqChannel {
                queue: "events_trading".to_string()
            })
        );
        // Two generation-order entries (TradeDetailType sorts before
        // TradeType in the BTreeSet title order) ⇒ two publications, in
        // generation order.
        assert_eq!(domain_channel.publications.len(), 2);
        let publications: Vec<&TriggerPublication> = domain_channel
            .publications
            .iter()
            .map(|p| match &p.source {
                PublicationSource::Database(publication) => publication,
                PublicationSource::Application => panic!("expected Database publications"),
            })
            .collect();
        assert_eq!(publications[0].schema_name, "trading");
        assert_eq!(publications[0].table_name, "trade_detail");
        assert!(!publications[0].append_only);
        assert!(publications[0].child_tables.is_empty());
        let publication = publications[1];
        assert_eq!(publication.schema_name, "trading");
        assert_eq!(publication.table_name, "trade");
        assert!(!publication.append_only);
        assert_eq!(publication.child_tables.len(), 1);
        assert_eq!(publication.child_tables[0].schema_name, "trading");
        assert_eq!(publication.child_tables[0].table_name, "trade_detail");
        assert!(!publication.child_tables[0].append_only);

        // Parity: every trigger table a publication carries is rendered
        // by the matching DDL event-trigger file (matched on the exact
        // `{schema}_{table}_event_trigger.sql` name — `trade` is a prefix
        // of `trade_detail`).
        let file_for = |schema: &str, table: &str| {
            let name = format!("{}_{}_event_trigger.sql", schema, table);
            trigger_files
                .iter()
                .find(|f| f.path.to_string_lossy().ends_with(&name))
                .unwrap_or_else(|| panic!("no trigger file for {name}"))
        };
        for publication in &publications {
            let mut tables = vec![publication.table_name.as_str()];
            tables.extend(
                publication
                    .child_tables
                    .iter()
                    .map(|c| c.table_name.as_str()),
            );
            for table in tables {
                assert!(
                    file_for(&publication.schema_name, table)
                        .content
                        .contains(&format!("trg_{table}_domain_event")),
                    "trigger file must cover `{table}`"
                );
            }
        }
    }

    // ── queue inventory ───────────────────────────────────────────────

    #[tokio::test]
    async fn queue_inventory_lists_domain_and_channel_queues_sorted() {
        let db = db_with_entity_and_child().await;
        db.ingest_evt_model(&model(
            "contracts.evt",
            vec![],
            vec![
                channel("contracts.evt", "orders", vec!["TradeExecuted"], 0),
                channel("contracts.evt", "billing", vec!["TradeExecuted"], 1),
            ],
            vec![],
        ))
        .await
        .expect("ingest evt model");

        let arch = EventArchitecture::from_graph(
            &db,
            &*dialect_for_target(DatabaseTarget::Postgres),
            &config(),
        )
        .await
        .expect("architecture builds");
        let inventory = arch.queue_inventory();
        let queues: Vec<&str> = inventory.iter().map(|q| q.queue.as_str()).collect();
        assert_eq!(
            queues,
            vec!["events_billing", "events_orders", "events_trading"]
        );
        assert_eq!(inventory[0].channel.as_deref(), Some("billing"));
        assert_eq!(inventory[0].domain, None);
        assert_eq!(inventory[1].channel.as_deref(), Some("orders"));
        assert_eq!(inventory[2].domain.as_deref(), Some("trading"));
        assert_eq!(inventory[2].channel, None);

        // Channel queues for the migration: sorted, domain-owned excluded.
        assert_eq!(
            arch.channel_queues(),
            vec!["events_billing", "events_orders"]
        );
    }

    #[tokio::test]
    async fn queue_inventory_dedupes_channel_domain_collision() {
        let db = db_with_entity_and_child().await;
        // Channel named like the domain: both sources declare
        // `events_trading`.
        db.ingest_evt_model(&model(
            "contracts.evt",
            vec![],
            vec![channel(
                "contracts.evt",
                "trading",
                vec!["TradeExecuted"],
                0,
            )],
            vec![],
        ))
        .await
        .expect("ingest evt model");

        let arch = EventArchitecture::from_graph(
            &db,
            &*dialect_for_target(DatabaseTarget::Postgres),
            &config(),
        )
        .await
        .expect("architecture builds");
        let inventory = arch.queue_inventory();
        assert_eq!(inventory.len(), 1, "collision dedupes to one queue");
        assert_eq!(inventory[0].queue, "events_trading");
        assert_eq!(inventory[0].channel.as_deref(), Some("trading"));
        assert_eq!(inventory[0].domain.as_deref(), Some("trading"));

        // The migration creates the colliding queue exactly once.
        assert!(arch.channel_queues().is_empty());
    }

    // ── pgmq_setup template gating (flag-off byte-identity) ──────────

    /// The pre-#455 `db/pgmq_setup.tera` (verbatim). The gated channel
    /// block must render ZERO bytes when `channels` is empty — asserted
    /// byte-for-byte against this baseline.
    const PGMQ_SETUP_PRE_EVT: &str = include!("pgmq_setup_pre_evt_template.rs");

    #[test]
    fn pgmq_setup_template_renders_zero_channel_bytes_when_empty() {
        let tera = tera();
        let project = ProjectConfig::default();
        let ctx = crate::db::event_trigger::PgmqSetupContext {
            domains: vec!["core".to_string()],
            channels: Vec::new(),
        };
        let out = crate::render_template_with_project(&tera, "db/pgmq_setup.tera", &ctx, &project)
            .unwrap();

        let mut baseline = tera::Tera::default();
        baseline
            .add_raw_template("pre_evt", PGMQ_SETUP_PRE_EVT)
            .unwrap();
        let expected = crate::render_template_with_project(
            &baseline,
            "pre_evt",
            &crate::db::event_trigger::PgmqSetupContext {
                domains: vec!["core".to_string()],
                channels: Vec::new(),
            },
            &project,
        )
        .unwrap();
        assert_eq!(out, expected, "flag-off pgmq_setup rendering changed");

        // The domain queue is created exactly once; no channel additions.
        assert_eq!(out.matches("pgmq.create").count(), 1);
        assert!(out.contains("pgmq.create('events_core');"));
    }

    #[test]
    fn pgmq_setup_template_renders_channel_queues_when_present() {
        let tera = tera();
        let project = ProjectConfig::default();
        let ctx = crate::db::event_trigger::PgmqSetupContext {
            domains: vec!["core".to_string()],
            channels: vec!["events_orders".to_string()],
        };
        let out = crate::render_template_with_project(&tera, "db/pgmq_setup.tera", &ctx, &project)
            .unwrap();
        assert_eq!(out.matches("pgmq.create").count(), 2);
        assert!(out.contains("SELECT pgmq.create('events_orders');"));
        assert!(out.contains("GRANT ALL ON TABLE pgmq.q_events_orders TO app_user, api_key;"));
        // Domain loop renders first (migration order: domains, then channels).
        let core_pos = out.find("events_core").unwrap();
        let orders_pos = out.find("events_orders").unwrap();
        assert!(core_pos < orders_pos);
    }
}
