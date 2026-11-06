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
    event_schemas(arch, variant)
}
