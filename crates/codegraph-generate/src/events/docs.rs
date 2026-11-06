//! Event document artifacts (issues #614/#610/#616): pure projections of
//! the [`EventArchitecture`] rendered once per run beside the code —
//! per-event JSON Schema documents, the AsyncAPI document, and the event
//! catalogue. Deterministic by construction (BTreeMap ordering,
//! declaration-order properties); inert without `.evt` input (an empty
//! architecture emits nothing).

use serde_json::{Map, Value, json};

use crate::events::{EnvelopeVariant, EventArchitecture, EventFieldDef};

/// One emitted document: its output-root-relative path and content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocArtifact {
    pub rel_path: String,
    pub content: String,
}

/// The rex-primitive / mapped-Rust-type → JSON Schema type mapping
/// (2020-12). Unmapped types (the `serde_json::Value` fallback and any
/// unforeseen spelling) stay PERMISSIVE (`{}`) — a document must never
/// be silently stricter than the wire contract.
pub fn json_type_of(rust_type: &str) -> Value {
    match rust_type {
        "String" => json!({"type": "string"}),
        "bool" => json!({"type": "boolean"}),
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => {
            json!({"type": "integer"})
        }
        "f32" | "f64" => json!({"type": "number"}),
        "uuid::Uuid" => json!({"type": "string", "format": "uuid"}),
        "chrono::NaiveDate" => json!({"type": "string", "format": "date"}),
        "chrono::NaiveDateTime" | "chrono::DateTime" | "chrono::DateTime<Utc>" => {
            json!({"type": "string", "format": "date-time"})
        }
        "rust_decimal::Decimal" => json!({"type": "number"}),
        _ => json!({}),
    }
}

fn json_type_for_field(field: &EventFieldDef, warnings: &mut Vec<String>) -> Value {
    if field.is_primitive {
        json_type_of(&field.rust_type)
    } else {
        match &field.resolved_title {
            Some(title) => {
                // No static DTO schemas are published in the generated
                // tree yet (the OpenAPI plane serves live utoipa
                // documents) — keep the field permissive and record the
                // resolution so a future DTO-schema emitter can upgrade
                // the ref.
                warnings.push(format!(
                    "event schema: field `{}' resolves to schema `{}' which has no emitted JSON Schema; kept permissive",
                    field.name, title
                ));
                json!({"type": "object", "x-resolved-schema": title})
            }
            None => json!({}),
        }
    }
}

fn payload_schema(event_name: &str, fields: &[EventFieldDef]) -> (Value, Vec<String>) {
    let mut warnings = Vec::new();
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in fields {
        // Declaration order is the wire artifact's field order.
        properties.insert(
            field.name.clone(),
            json_type_for_field(field, &mut warnings),
        );
        required.push(Value::String(field.name.clone()));
    }
    let doc = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("urn:event:{event_name}"),
        "title": format!("{event_name}Payload"),
        "description": format!("Payload of the declared event `{event_name}'."),
        "type": "object",
        "properties": Value::Object(properties),
        "required": required,
    });
    (doc, warnings)
}

fn native_envelope_schema(event_name: &str) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("urn:event:{event_name}.Envelope"),
        "title": format!("{event_name}Envelope"),
        "description": format!(
            "The codegraph native envelope wrapping `{event_name}' (issue #455; additive keys: traceparent, issue #613)."
        ),
        "type": "object",
        "properties": {
            "event": { "const": event_name },
            "version": { "type": ["string", "null"] },
            "channel": { "type": "string" },
            "payload": { "$ref": format!("{event_name}.json") },
            "event_type": { "const": event_name },
            "domain": { "type": ["string", "null"] },
            "entity_table": { "type": ["string", "null"] },
            "entity_id": { "type": ["string", "null"] },
            "tenant_id": { "type": "string", "format": "uuid" },
            "occurred_at": { "type": "string" },
            "correlation_id": { "type": "string", "format": "uuid" },
            "traceparent": { "type": ["string", "null"] },
        },
        "required": [
            "event", "channel", "payload", "event_type", "tenant_id",
            "occurred_at", "correlation_id",
        ],
    })
}

fn cloudevents_envelope_schema(event_name: &str) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("urn:event:{event_name}.Envelope"),
        "title": format!("{event_name}Envelope"),
        "description": format!(
            "CloudEvents 1.0 structured envelope wrapping `{event_name}' (issue #609); native identity keys ride as CE extension attributes."
        ),
        "type": "object",
        "properties": {
            "specversion": { "const": "1.0" },
            "id": { "type": "string" },
            "type": { "const": event_name },
            "source": { "type": "string" },
            "subject": { "type": "string" },
            "time": { "type": "string" },
            "datacontenttype": { "const": "application/json" },
            "data": { "$ref": format!("{event_name}.json") },
            "version": { "type": ["string", "null"] },
            "channel": { "type": "string" },
            "domain": { "type": ["string", "null"] },
            "correlationid": { "type": "string", "format": "uuid" },
            "tenantid": { "type": "string", "format": "uuid" },
            "traceparent": { "type": ["string", "null"] },
        },
        "required": [
            "specversion", "id", "type", "source", "datacontenttype", "data",
            "channel", "correlationid", "tenantid",
        ],
    })
}

/// The per-event JSON Schema documents (issue #614): one payload schema
/// per declared event plus one envelope wrapper whose payload `$ref`
/// targets the sibling file.
pub fn event_schemas(arch: &EventArchitecture, variant: EnvelopeVariant) -> Vec<DocArtifact> {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for event in arch.events.iter() {
        let (payload, mut w) = payload_schema(&event.name, &event.fields);
        warnings.append(&mut w);
        out.push(DocArtifact {
            rel_path: format!("schemas/events/{}.json", event.name),
            content: serde_json::to_string_pretty(&payload).unwrap_or_default() + "\n",
        });
        let envelope = match variant {
            EnvelopeVariant::Native => native_envelope_schema(&event.name),
            EnvelopeVariant::CloudEvents => cloudevents_envelope_schema(&event.name),
        };
        out.push(DocArtifact {
            rel_path: format!("schemas/events/{}.Envelope.json", event.name),
            content: serde_json::to_string_pretty(&envelope).unwrap_or_default() + "\n",
        });
    }
    for w in warnings {
        crate::events::model::warn_once(&format!("event-schema:{w}"), &w);
    }
    out
}

/// All document artifacts for a run (issue #614 today; the AsyncAPI and
/// catalogue projections join in #610/#616). Empty architecture ⇒ empty.
pub fn document_artifacts(arch: &EventArchitecture, variant: EnvelopeVariant) -> Vec<DocArtifact> {
    if arch.is_empty() {
        return Vec::new();
    }
    let mut docs = event_schemas(arch, variant);
    docs.push(asyncapi_document(arch, variant));
    docs
}

/// The AsyncAPI 3.0.0 document (issue #610): the `.evt` graph plane's
/// asynchronous-API projection. Channels are keyed by their pgmq queue
/// name (the domain-queue collapse rule falls out of the keying), every
/// declared event becomes one component message whose payload references
/// the #614 JSON Schema (no duplication), each channel carries a publish
/// operation, and each subscription a subscribe operation (`operationId`
/// = subscription name, messages filtered to the subscribed events). The
/// envelope (native or CloudEvents) is documented via the `x-envelope`
/// extension. Custom pgmq binding (`x-pgmq`): no registered AsyncAPI
/// protocol exists for pgmq, so the doc carries the queue binding and no
/// `servers` entry rather than lying about a protocol.
pub fn asyncapi_document(arch: &EventArchitecture, variant: EnvelopeVariant) -> DocArtifact {
    use std::collections::BTreeMap;

    let envelope_note = match variant {
        EnvelopeVariant::Native => {
            "codegraph native envelope (12 keys; `payload` carries the message payload, `event_type` mirrors `event` for webhook matching)"
        }
        EnvelopeVariant::CloudEvents => {
            "CloudEvents 1.0 structured JSON (application/cloudevents+json); the payload rides `data`, native identity keys ride as CE extension attributes"
        }
    };

    // Channels keyed by queue name (deterministic; collapse for free).
    let mut channels: BTreeMap<String, Value> = BTreeMap::new();
    let mut messages: BTreeMap<String, Value> = BTreeMap::new();
    let mut operations: BTreeMap<String, Value> = BTreeMap::new();

    fn push_channel(
        channels: &mut BTreeMap<String, Value>,
        queue: String,
        channel_name: String,
        events: Vec<String>,
        trigger: bool,
    ) {
        let entry = channels.entry(queue.clone()).or_insert_with(|| {
            let mut c = json!({
                "address": queue.clone(),
                "bindings": { "x-pgmq": { "queue": queue.clone() } },
                "messages": {},
            });
            if trigger {
                c["description"] = json!(format!(
                    "Implicit per-domain CRUD trigger channel (`{channel_name}'); publishes created/updated/deleted trigger envelopes."
                ));
            } else {
                c["description"] = json!(format!("Declared `.evt' channel `{channel_name}'."));
            }
            c
        });
        for event in events {
            entry["messages"][event.clone()] =
                json!({ "$ref": format!("#/components/messages/{event}") });
        }
    }

    for channel in arch.channels.iter() {
        let queue = match &channel.transport {
            crate::events::Transport::Pgmq(pgmq) => pgmq.queue.clone(),
            crate::events::Transport::Unbound => {
                // Unbound channels publish nowhere on this target — the
                // doc records them without an address.
                let name = format!("unbound-{}", channel.name);
                channels.insert(
                    name.clone(),
                    json!({
                        "description": format!(
                            "Declared `.evt' channel `{}' — UNBOUND on this database target (no queue substrate); no publish surface is generated.",
                            channel.name
                        ),
                        "x-unbound": true,
                    }),
                );
                let op_key = format!("publish{}", pascal(&name));
                operations.insert(
                    op_key,
                    json!({
                        "action": "send",
                        "channel": { "$ref": format!("#/channels/{name}") },
                        "title": format!("Publish {}", channel.name),
                        "x-unbound": true,
                    }),
                );
                continue;
            }
        };
        let events: Vec<String> = channel
            .publications
            .iter()
            .filter(|p| matches!(p.source, crate::events::PublicationSource::Application))
            .map(|p| p.event.clone())
            .collect();
        for event in &events {
            if let Some(def) = arch.event(event) {
                let mut m = json!({
                    "title": def.name,
                    "contentType": "application/json",
                    "payload": {
                        "$ref": format!("../schemas/events/{}.json", def.name),
                    },
                    "x-envelope": envelope_note,
                });
                if let Some(version) = &def.version {
                    m["schemaFormat"] = json!("application/vnd.aai.asyncapi;version=3.0.0");
                    m["x-event-version"] = json!(version);
                }
                messages.insert(event.clone(), m);
            }
        }
        push_channel(
            &mut channels,
            queue.clone(),
            channel.name.clone(),
            events.clone(),
            false,
        );
        let op_key = format!("publish{}", pascal(&queue));
        let mut op = json!({
            "action": "send",
            "channel": { "$ref": format!("#/channels/{queue}") },
            "title": format!("Publish on {}", channel.name),
            "messages": events
                .iter()
                .map(|e| json!({ "$ref": format!("#/components/messages/{e}") }))
                .collect::<Vec<_>>(),
        });
        op["x-envelope"] = json!(envelope_note);
        operations.insert(op_key, op);
    }

    // Implicit per-domain trigger channels.
    for channel in arch.domain_channels.iter() {
        if let crate::events::Transport::Pgmq(pgmq) = &channel.transport {
            push_channel(
                &mut channels,
                pgmq.queue.clone(),
                channel.name.clone(),
                Vec::new(),
                true,
            );
        }
    }

    // Subscribe operations per subscription.
    for sub in arch.subscriptions.iter() {
        // The subscription's events ride whichever declared channels
        // publish them; the doc attaches the op to every matching queue.
        let mut attached = Vec::new();
        for channel in arch.channels.iter() {
            let publishes: Vec<&String> = channel
                .publications
                .iter()
                .map(|p| &p.event)
                .filter(|e| sub.events.contains(e))
                .collect();
            if publishes.is_empty() {
                continue;
            }
            if let crate::events::Transport::Pgmq(pgmq) = &channel.transport {
                attached.push(pgmq.queue.clone());
            }
        }
        for (idx, queue) in attached.iter().enumerate() {
            let messages_ref: Vec<Value> = sub
                .events
                .iter()
                .map(|e| json!({ "$ref": format!("#/components/messages/{e}") }))
                .collect();
            // Bare subscription key; a multi-queue subscription
            // disambiguates by queue.
            let key = if attached.len() == 1 {
                format!("subscribe{}", pascal(&sub.name))
            } else {
                format!("subscribe{}@{}", pascal(&sub.name), queue)
            };
            let _ = idx;
            operations.insert(
                key,
                json!({
                    "action": "receive",
                    "channel": { "$ref": format!("#/channels/{queue}") },
                    "operationId": sub.consumer,
                    "title": format!("Subscribe `{}` ({})", sub.name, sub.consumer),
                    "messages": messages_ref,
                }),
            );
        }
    }

    let doc = json!({
        "asyncapi": "3.0.0",
        "info": {
            "title": "Events",
            "version": "1.0.0",
            "description": "Generated from the `.evt` graph plane (issues #454/#455); the asynchronous-API projection of the declared events, channels, subscriptions and their transports (issue #610).",
        },
        "channels": channels.into_iter().collect::<serde_json::Map<String, Value>>(),
        "operations": operations.into_iter().collect::<serde_json::Map<String, Value>>(),
        "components": {
            "messages": messages.into_iter().collect::<serde_json::Map<String, Value>>(),
        },
    });
    DocArtifact {
        rel_path: "docs/asyncapi.json".to_string(),
        content: serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n",
    }
}

fn pascal(s: &str) -> String {
    codegraph_naming::to_pascal_case(s)
}
