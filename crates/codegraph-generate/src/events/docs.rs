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

/// The phero-primitive / mapped-Rust-type → JSON Schema type mapping
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
///
/// `producer_report` is the run's events-producer planner output (issue #820
/// slice c): `Some` only when the `events_producer` flag is ON and the run
/// planned at least one publish, so the catalogue's runtime-publication marks
/// are absent (byte-identical) on every flag-OFF run.
pub fn document_artifacts(
    arch: &EventArchitecture,
    variant: EnvelopeVariant,
    ddd_models: &[codegraph_core::types::DddModelGraph],
    producer_report: Option<&crate::events::producer::ProducerReport>,
) -> Vec<DocArtifact> {
    if arch.is_empty() {
        return Vec::new();
    }
    let mut docs = event_schemas(arch, variant);
    docs.push(asyncapi_document(arch, variant));
    docs.push(catalogue_markdown(
        arch,
        variant,
        ddd_models,
        producer_report,
    ));
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

/// The catalogue's runtime-publication mark for one declared event (issue
/// #820 slice c): ONE line, rendered as its own paragraph directly after the
/// per-event metadata table.
///
///  - a planned event: `Runtime publication: published at runtime by
///    {Service}.{op} (arm: …; queue: …)`. Several emitting operations and/or
///    channels stay ONE line: the operations join with `", "` in declaration
///    order, and the arm/queue keys pluralize (`arms:`/`queues:`) with their
///    deduped values when the plans disagree. Queues come from the plan's
///    `publish` list — never re-derived from the channel name.
///  - a declared event no operation binds: `Runtime publication: not
///    emitted — no modeled service operation declares this event`.
///
/// Refusal/warning sentences are deliberately NOT rendered here: a flag-ON run
/// hard-fails refusals before generation (`ValidationPass::
/// check_event_producers`), so a refused binding can never reach a written
/// tree. They stay on the validation and doctor surfaces.
fn runtime_publication_mark(
    report: Option<&crate::events::producer::ProducerReport>,
    event_name: &str,
) -> Option<String> {
    use crate::events::producer::{ProducerArm, ProducerPlan};

    let report = report?;
    let plans: Vec<&ProducerPlan> = report
        .plans
        .iter()
        .filter(|plan| plan.event.name == event_name)
        .collect();
    if plans.is_empty() {
        return Some(
            "Runtime publication: not emitted — no modeled service operation declares this event"
                .to_string(),
        );
    }

    let mut ops: Vec<String> = Vec::new();
    let mut arms: Vec<&'static str> = Vec::new();
    let mut queues: Vec<String> = Vec::new();
    for plan in plans {
        let op = format!("{}.{}", plan.service, plan.op);
        if !ops.contains(&op) {
            ops.push(op);
        }
        let arm = match &plan.arm {
            ProducerArm::Transition(_) => "transition",
            ProducerArm::Patch => "patch",
        };
        if !arms.contains(&arm) {
            arms.push(arm);
        }
        for channel in &plan.publish {
            if !queues.contains(&channel.queue) {
                queues.push(channel.queue.clone());
            }
        }
    }

    let arm_clause = if arms.len() == 1 {
        format!("arm: {}", arms[0])
    } else {
        format!("arms: {}", arms.join(", "))
    };
    // R12 refuses a plan with no persisted channel, so `queues` is non-empty
    // for every planned event; the empty arm keeps the projection total.
    let queue_clause = if queues.is_empty() {
        "queue: —".to_string()
    } else if queues.len() == 1 {
        format!("queue: {}", queues[0])
    } else {
        format!("queues: {}", queues.join(", "))
    };

    Some(format!(
        "Runtime publication: published at runtime by {} ({arm_clause}; {queue_clause})",
        ops.join(", ")
    ))
}

/// The event catalogue (issue #616): the whole
/// event→channel→subscription→consumer graph as a deterministic markdown
/// document. Governance slots (owner/classification/retention/pii —
/// upstream pherolang #31 slice 14) render WHEN the artifact carries them
/// and stay omitted otherwise; today nothing carries them, so no
/// placeholder noise is emitted. `ddd_models` supplies the cross-plane
/// `emits` edges (`.ddd` service operations declaring event emission —
/// pherolang's loosely-coupled dimension). `producer_report` supplies the
/// slice-(c) runtime-publication marks (issue #820): `None` ⇒ no marks.
pub fn catalogue_markdown(
    arch: &EventArchitecture,
    variant: EnvelopeVariant,
    ddd_models: &[codegraph_core::types::DddModelGraph],
    producer_report: Option<&crate::events::producer::ProducerReport>,
) -> DocArtifact {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    // Event → emitting service operations (cross-plane `emits` edges).
    let mut emits_index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for model in ddd_models {
        for service in model.services.iter() {
            for op in service.operations.iter() {
                for event in op.emits.iter() {
                    emits_index
                        .entry(event.clone())
                        .or_default()
                        .push(format!("{}.{}.{}", service.module, service.name, op.name));
                }
            }
        }
    }

    let mut out = String::from("# Event catalogue\n\n");
    out.push_str("Generated from the `.evt` graph plane (issues #454/#455) — the governance projection of the declared events, channels, subscriptions, consumers and their transports (issue #616). Machine-readable companions: the AsyncAPI document (`asyncapi.json`) and the per-event JSON Schemas (`schemas/events/`).\n");

    for event in arch.events.iter() {
        let _ = writeln!(out, "\n## `{}`\n", event.name);
        writeln!(out, "| | |").unwrap();
        writeln!(out, "|---|---|").unwrap();
        if let Some(version) = &event.version {
            writeln!(out, "| Version | {version} |").unwrap();
        }
        if let Some(domain) = &event.domain {
            writeln!(out, "| Domain | {domain} |").unwrap();
        }
        writeln!(
            out,
            "| Schema | [schemas/events/{}.json](../schemas/events/{}.json) |",
            event.name, event.name
        )
        .unwrap();

        // The runtime-publication mark (issue #820 slice c): one line, its
        // own paragraph, before the payload table. Absent on every flag-OFF
        // run (no producer report), so the document stays byte-identical.
        if let Some(mark) = runtime_publication_mark(producer_report, &event.name) {
            let _ = writeln!(out, "\n{mark}");
        }

        out.push_str("\n### Payload\n\n");
        writeln!(out, "| Field | Rust type | Resolved schema |").unwrap();
        writeln!(out, "|---|---|---|").unwrap();
        for field in event.fields.iter() {
            writeln!(
                out,
                "| {} | {} | {} |",
                field.name,
                field.rust_type,
                field.resolved_title.as_deref().unwrap_or("—")
            )
            .unwrap();
        }

        // Publishing channels (declared + implicit trigger channels whose
        // queue attribution resolves through the derived domain).
        let mut published = false;
        out.push_str("\n### Published on\n\n");
        writeln!(out, "| Channel | Queue | Transport |").unwrap();
        writeln!(out, "|---|---|---|").unwrap();
        for channel in arch.channels.iter() {
            if channel.publications.iter().any(|p| p.event == event.name)
                && let crate::events::Transport::Pgmq(pgmq) = &channel.transport
            {
                writeln!(out, "| {} | `{}` | pgmq |", channel.name, pgmq.queue).unwrap();
                published = true;
            }
        }
        if let Some(domain) = &event.domain
            && let Some(dc) = arch
                .domain_channels
                .iter()
                .find(|c| c.name == *domain && c.publications.is_empty())
                .or_else(|| arch.domain_channels.iter().find(|c| c.name == *domain))
            && let crate::events::Transport::Pgmq(pgmq) = &dc.transport
        {
            writeln!(out, "| {} (trigger) | `{}` | pgmq |", dc.name, pgmq.queue).unwrap();
            published = true;
        }
        if !published {
            out.push_str("| — | — | unbound |\n");
        }

        // Emitters (cross-plane `emits` edges), when any resolve.
        if let Some(ops) = emits_index.get(&event.name) {
            out.push_str("\n### Emitted by\n\n");
            writeln!(out, "| Service operation |").unwrap();
            writeln!(out, "|---|").unwrap();
            for op in ops {
                writeln!(out, "| {op} |").unwrap();
            }
        }

        // Subscriptions covering this event.
        let subs: Vec<_> = arch
            .subscriptions
            .iter()
            .filter(|s| s.events.contains(&event.name))
            .collect();
        if !subs.is_empty() {
            out.push_str("\n### Subscriptions\n\n");
            writeln!(out, "| Subscription | Consumer | Delivery policy |").unwrap();
            writeln!(out, "|---|---|---|").unwrap();
            for sub in subs {
                writeln!(
                    out,
                    "| {} | {} | at-least-once ({} retries, {}s backoff) |",
                    sub.name,
                    sub.consumer,
                    sub.delivery.max_retries,
                    sub.delivery.initial_backoff_secs
                )
                .unwrap();
            }
        }
    }

    // Synthetic function consumers (issue #450 slice D): event-exposed
    // modeled functions consume raw envelopes keyed on the fn snake name
    // — they subscribe to their own synthetic event, not to declared
    // events, so they render as their own section.
    if !arch.function_consumers.is_empty() {
        out.push_str("\n## Function consumers\n\n");
        writeln!(out, "| Function | Consumes | Domain |").unwrap();
        writeln!(out, "|---|---|---|").unwrap();
        for f in arch.function_consumers.iter() {
            writeln!(
                out,
                "| {} | {} | {} |",
                f.name,
                f.fn_name,
                f.domain.as_deref().unwrap_or("—")
            )
            .unwrap();
        }
    }

    // Dependency view: event → channel queue → subscription/consumer.
    out.push_str("\n## Dependency graph\n\n```mermaid\ngraph LR\n");
    for event in arch.events.iter() {
        for channel in arch.channels.iter() {
            if channel.publications.iter().any(|p| p.event == event.name)
                && let crate::events::Transport::Pgmq(pgmq) = &channel.transport
            {
                writeln!(out, "  {} --> q_{}", event.name, pgmq.queue).unwrap();
            }
        }
    }
    for sub in arch.subscriptions.iter() {
        for event in sub.events.iter() {
            writeln!(out, "  q_{} --> s_{}", event, sub.name).unwrap();
        }
    }
    for f in arch.function_consumers.iter() {
        writeln!(out, "  {} --> f_{}", f.fn_name, f.fn_name).unwrap();
    }
    out.push_str("```\n");

    // Governance slots (upstream pherolang #31 item 14): rendered when the
    // artifact carries owner/classification/retention/pii; absent today.
    out.push_str("\n## Governance\n\nGovernance metadata (owner, classification, retention, PII) rides the upstream artifact when declared (pherolang #31 item 14); this projection renders the sections as soon as the model carries the fields.\n");

    let _ = &variant;
    DocArtifact {
        rel_path: "docs/events.md".to_string(),
        content: out,
    }
}

#[cfg(test)]
mod producer_mark_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::ddd::service_ops::{ServiceOpState, StateName, WireKey};
    use crate::events::model::{EventDefinition, EventFieldDef};
    use crate::events::producer::planner::ProducerChannel;
    use crate::events::producer::{
        EmissionArm, EmissionSite, ProducerEntitySurface, ProducerInputs, SurfaceField,
    };

    use super::runtime_publication_mark;

    fn payload_field(name: &str, rust_type: &str) -> EventFieldDef {
        EventFieldDef {
            name: name.to_string(),
            rust_type: rust_type.to_string(),
            resolved_title: None,
            is_primitive: true,
        }
    }

    fn event(name: &str) -> EventDefinition {
        EventDefinition {
            name: name.to_string(),
            version: Some("1.0.0".to_string()),
            fields: vec![payload_field("status", "String")],
            domain: Some("orders".to_string()),
        }
    }

    fn surface() -> ProducerEntitySurface {
        ProducerEntitySurface {
            domain: "orders".to_string(),
            title: "WorkerType".to_string(),
            rust_type_name: "WorkerType".to_string(),
            fields: vec![
                SurfaceField {
                    name: "id".to_string(),
                    rust_type: "uuid::Uuid".to_string(),
                    is_optional: false,
                    is_array: false,
                    is_entity_ref: false,
                    is_codelist: false,
                },
                SurfaceField {
                    name: "status".to_string(),
                    rust_type: "WorkerStatus".to_string(),
                    is_optional: false,
                    is_array: false,
                    is_entity_ref: false,
                    is_codelist: true,
                },
            ],
            children: Vec::new(),
            alt_identities: Vec::new(),
            state_field: Some("status".to_string()),
            state_type: Some("WorkerStatus".to_string()),
            has_read: true,
        }
    }

    fn state() -> ServiceOpState {
        ServiceOpState {
            current_expr: "response.status".to_string(),
            wire_key: WireKey::new("status"),
            initial: StateName::new("Draft"),
            machine_id: "worker_type_status".to_string(),
            all: Vec::new(),
            op: Vec::new(),
        }
    }

    fn site(op: &str, arm: EmissionArm, emits: &[&str]) -> EmissionSite {
        EmissionSite {
            service: "WorkerService".to_string(),
            op: op.to_string(),
            module_domain: "orders".to_string(),
            emits: emits.iter().map(|e| (*e).to_string()).collect(),
            is_protected: false,
            child_bound: false,
            entity: Some("WorkerType".to_string()),
            arm,
            own_writes: BTreeSet::from(["status".to_string()]),
            writable_fields: BTreeSet::from(["status".to_string()]),
        }
    }

    fn channel(name: &str) -> ProducerChannel {
        ProducerChannel {
            name: name.to_string(),
            name_snake: name.to_string(),
            enum_name: "OrdersEvent".to_string(),
            queue: format!("events_{name}"),
        }
    }

    /// Two operations (a transition and a patch) and two channels produce ONE
    /// line: the ops join in declaration order, the arm/queue keys pluralize
    /// with their deduped values, and the queues come from the plans' own
    /// `publish` list.
    #[test]
    fn mark_pluralizes_arms_and_queues_on_one_line() {
        let mut inputs = ProducerInputs {
            events: vec![event("StatusPlaced")],
            sites: vec![
                site(
                    "approve",
                    EmissionArm::Transition(state()),
                    &["StatusPlaced"],
                ),
                site("amend", EmissionArm::Patch, &["StatusPlaced"]),
            ],
            entities: BTreeMap::from([(
                ("orders".to_string(), "WorkerType".to_string()),
                surface(),
            )]),
            publish_targets: BTreeMap::from([(
                "StatusPlaced".to_string(),
                vec![channel("orders"), channel("audit")],
            )]),
            type_suffix: "Type".to_string(),
        };
        inputs.sites[1].own_writes.clear();
        let report = super::super::producer::planner::plan(&inputs);
        assert_eq!(report.plans.len(), 2, "{:?}", report.refusals);

        assert_eq!(
            runtime_publication_mark(Some(&report), "StatusPlaced").as_deref(),
            Some(
                "Runtime publication: published at runtime by WorkerService.approve, \
                 WorkerService.amend (arms: transition, patch; queues: events_orders, \
                 events_audit)"
            )
        );
    }

    /// One plan keeps the singular keys, and an event no operation binds
    /// carries the unbound line — the line that tells a reader the payload
    /// exists but nothing publishes it.
    #[test]
    fn mark_singularizes_and_names_the_unbound_case() {
        let inputs = ProducerInputs {
            events: vec![event("StatusPlaced"), event("StatusDropped")],
            sites: vec![site("amend", EmissionArm::Patch, &["StatusPlaced"])],
            entities: BTreeMap::from([(
                ("orders".to_string(), "WorkerType".to_string()),
                surface(),
            )]),
            publish_targets: BTreeMap::from([(
                "StatusPlaced".to_string(),
                vec![channel("orders")],
            )]),
            type_suffix: "Type".to_string(),
        };
        let report = super::super::producer::planner::plan(&inputs);
        assert_eq!(
            runtime_publication_mark(Some(&report), "StatusPlaced").as_deref(),
            Some(
                "Runtime publication: published at runtime by WorkerService.amend \
                 (arm: patch; queue: events_orders)"
            )
        );
        assert_eq!(
            runtime_publication_mark(Some(&report), "StatusDropped").as_deref(),
            Some(
                "Runtime publication: not emitted — no modeled service operation declares \
                 this event"
            )
        );
        // No producer report (the flag-OFF shape) ⇒ no mark at all.
        assert_eq!(runtime_publication_mark(None, "StatusPlaced"), None);
    }
}
