//! Typed event emission + subscription scaffolding (issues #454/#455,
//! Phase 3-4): the `evt_events` global generator.
//!
//! When the graph carries a non-empty [`EventArchitecture`] (`.evt`
//! input), this emits the generated app's `src/events/` module —
//! contracts (one payload struct per declared event), emit (one event
//! enum + `publish_*` fn per pgmq-bound channel), consumers (one handler
//! trait + dispatcher per subscription plus the process-wide registry the
//! webhook drain dispatches through) — and flips the webhook dispatch /
//! subscription-API templates into their typed-envelope branches.
//!
//! No `.evt` input ⇒ the architecture is empty ⇒ ZERO files and the
//! untouched pre-#455 rendering of every other template (the acceptance
//! contract: flag-off output is byte-identical).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_naming::{escape_module_keyword, to_pascal_case, to_snake_case};
use serde::Serialize;

use crate::GenerationEntry;
use crate::db::dialect::dialect_for_target;
use crate::error::Result;
use crate::events::{EventArchitecture, EventDefinition, PublicationSource, Transport};
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::DomainConfig;

use crate::events::model::warn_once;

/// Build the run's [`EventArchitecture`] for the project's database
/// target. Shared by this generator, the scaffold generator
/// (`has_events`), and the webhook generators (drain + vocabulary
/// gating): every consumer derives its view from ONE construction, so
/// binding decisions can never disagree.
pub(crate) async fn architecture_for(
    db: &dyn GraphQuerier,
    project: &crate::ProjectConfig,
    config: &DomainConfig,
) -> Result<EventArchitecture> {
    let dialect = dialect_for_target(project.database.database_target);
    EventArchitecture::from_graph(db, &*dialect, config).await
}

// ── template contexts ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct EventsModContext {
    /// Sorted, deduped `.evt` source paths the architecture came from.
    sources: Vec<String>,
    /// Payload struct names, declaration order, deduped.
    payload_names: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ContractsContext {
    /// `use` lines the payload field types require, sorted.
    imports: Vec<String>,
    events: Vec<EventContractContext>,
}

#[derive(Debug, Serialize)]
struct EventContractContext {
    name: String,
    version: Option<String>,
    /// `{EVENT_NAME}_VERSION` const name, when a version is declared.
    version_const: Option<String>,
    fields: Vec<EventFieldContext>,
}

#[derive(Debug, Serialize)]
struct EventFieldContext {
    name: String,
    rust_type: String,
    /// The field name as authored (for the doc comment).
    authored: String,
}

#[derive(Debug, Serialize)]
struct EmitContext {
    channels: Vec<ChannelContext>,
    /// The full `use crate::events::contracts::{…};` line for the payload
    /// types the channel enums reference (empty when no channel is bound).
    contracts_use: String,
}

#[derive(Debug, Serialize)]
struct ChannelContext {
    name: String,
    name_snake: String,
    enum_name: String,
    queue: String,
    variants: Vec<VariantContext>,
}

#[derive(Debug, Serialize)]
struct VariantContext {
    variant: String,
    event_name: String,
    payload: String,
    /// `Some("…")` / `None` literals rendered into the accessors.
    version_expr: String,
    domain_expr: String,
}

#[derive(Debug, Serialize)]
struct ConsumersContext {
    /// The full `use crate::events::contracts::{…};` line for the
    /// payload types the dispatchers reference (empty on no consumers).
    contracts_use: String,
    consumers: Vec<ConsumerContext>,
}

#[derive(Debug, Serialize)]
struct ConsumerContext {
    /// The consumer name as authored (registry label, doc comments).
    name: String,
    name_snake: String,
    context_struct: String,
    handler_trait: String,
    events: Vec<HandlerEventContext>,
}

#[derive(Debug, Serialize)]
struct HandlerEventContext {
    method: String,
    payload: String,
    event_name: String,
}

// ── naming helpers ─────────────────────────────────────────────────────

fn payload_struct_name(event_name: &str) -> String {
    format!("{event_name}Payload")
}

fn version_const_name(event_name: &str) -> String {
    format!("{}_VERSION", to_snake_case(event_name).to_uppercase())
}

fn channel_enum_name(channel: &str) -> String {
    format!("{}Event", to_pascal_case(channel))
}

fn consumer_type_name(consumer: &str) -> String {
    to_pascal_case(consumer)
}

/// Rust types that need no import (scalars + the fully-qualified
/// fallback), or whose import is decided structurally.
#[derive(Debug, PartialEq, Eq)]
enum FieldType {
    NoImport,
    Uuid,
    ChronoDateTime,
    ChronoNaiveDate,
    Decimal,
    /// A named schema type (codelist enum / class) — resolved against the
    /// graph for its module path.
    Named,
}

fn field_type_kind(rust_type: &str) -> FieldType {
    match rust_type {
        "String" | "bool" | "i64" | "f64" | "serde_json::Value" => FieldType::NoImport,
        "Uuid" => FieldType::Uuid,
        "NaiveDate" => FieldType::ChronoNaiveDate,
        "Decimal" => FieldType::Decimal,
        t if t.starts_with("DateTime<") => FieldType::ChronoDateTime,
        _ => FieldType::Named,
    }
}

/// The `use` lines contracts.rs needs for one event's payload field
/// types. Named (schema-typed) fields resolve against the graph:
/// codelists live in `crate::codelist`, everything else under the
/// schema's domain module. Unresolvable named types were already mapped
/// to `serde_json::Value` by the architecture (no import needed).
async fn contract_imports(
    db: &dyn GraphQuerier,
    events: &[EventDefinition],
    type_suffix: &str,
) -> Result<Vec<String>> {
    let mut imports = BTreeSet::new();
    let mut uses_uuid = false;
    let mut uses_date_time = false;
    let mut uses_naive_date = false;
    let mut uses_decimal = false;

    for event in events {
        for field in &event.fields {
            match field_type_kind(&field.rust_type) {
                FieldType::NoImport => {}
                FieldType::Uuid => uses_uuid = true,
                FieldType::ChronoDateTime => uses_date_time = true,
                FieldType::ChronoNaiveDate => uses_naive_date = true,
                FieldType::Decimal => uses_decimal = true,
                FieldType::Named => {
                    let Some(title) = field.resolved_title.as_deref() else {
                        warn_once(
                            &format!("import-unresolved:{}:{}", event.name, field.name),
                            &format!(
                                "event `{}' field `{}` has no resolved schema; its `{}` type must be in scope",
                                event.name, field.name, field.rust_type
                            ),
                        );
                        continue;
                    };
                    let schema = match db.get_schema(title).await? {
                        Some(schema) => schema,
                        None => continue,
                    };
                    if schema.is_codelist {
                        imports.insert(format!("use crate::codelist::{};", field.rust_type));
                    } else if let Some(domain) = &schema.domain {
                        let module = escape_module_keyword(&to_snake_case(
                            &codegraph_naming::strip_suffix(title, type_suffix),
                        ));
                        imports.insert(format!(
                            "use crate::domain::{domain}::{module}::{};",
                            field.rust_type
                        ));
                    }
                }
            }
        }
    }
    if uses_uuid {
        imports.insert("use uuid::Uuid;".to_string());
    }
    if uses_date_time {
        imports.insert("use chrono::{DateTime, Utc};".to_string());
    }
    if uses_naive_date {
        imports.insert("use chrono::NaiveDate;".to_string());
    }
    if uses_decimal {
        imports.insert("use rust_decimal::Decimal;".to_string());
    }
    Ok(imports.into_iter().collect())
}

/// Events deduped by name (first occurrence in (source_path, ordinal)
/// order wins; duplicates are warned once).
fn deduped_events(arch: &EventArchitecture) -> Vec<EventDefinition> {
    let mut seen = std::collections::HashSet::new();
    let mut events = Vec::new();
    for event in &arch.events {
        if seen.insert(event.name.clone()) {
            events.push(event.clone());
        } else {
            warn_once(
                &format!("duplicate-event:{}", event.name),
                &format!(
                    "event `{}' is declared more than once; the first declaration owns the contract",
                    event.name
                ),
            );
        }
    }
    events
}

// ── context builders ───────────────────────────────────────────────────

async fn contracts_context(
    db: &dyn GraphQuerier,
    arch: &EventArchitecture,
    type_suffix: &str,
) -> Result<ContractsContext> {
    let events = deduped_events(arch);
    let imports = contract_imports(db, &events, type_suffix).await?;
    let contracts = events
        .iter()
        .map(|event| EventContractContext {
            name: event.name.clone(),
            version: event.version.clone(),
            version_const: event
                .version
                .as_ref()
                .map(|_| version_const_name(&event.name)),
            fields: event
                .fields
                .iter()
                .map(|field| EventFieldContext {
                    name: to_snake_case(&field.name),
                    rust_type: field.rust_type.clone(),
                    authored: field.name.clone(),
                })
                .collect(),
        })
        .collect();
    Ok(ContractsContext {
        imports,
        events: contracts,
    })
}

fn emit_context(arch: &EventArchitecture) -> EmitContext {
    let mut payload_types = BTreeSet::new();
    let channels = arch
        .bound_channels()
        .map(|channel| {
            let mut variants = Vec::new();
            for publication in &channel.publications {
                if !matches!(publication.source, PublicationSource::Application) {
                    warn_once(
                        &format!("trigger-publication-on-dsl-channel:{}", channel.name),
                        &format!(
                            "channel `{}' carries a database-sourced publication; only application publications get publish fns",
                            channel.name
                        ),
                    );
                    continue;
                }
                let Some(event) = arch.event(&publication.event) else {
                    warn_once(
                        &format!("unpublished:{}", publication.event),
                        &format!(
                            "channel `{}' publishes `{}' which is not a declared event; variant skipped",
                            channel.name, publication.event
                        ),
                    );
                    continue;
                };
                variants.push(VariantContext {
                    variant: event.name.clone(),
                    event_name: event.name.clone(),
                    payload: {
                        payload_types.insert(payload_struct_name(&event.name));
                        payload_struct_name(&event.name)
                    },
                    version_expr: match &event.version {
                        Some(version) => format!("Some(\"{version}\")"),
                        None => "None".to_string(),
                    },
                    domain_expr: match &event.domain {
                        Some(domain) => format!("Some(\"{domain}\")"),
                        None => "None".to_string(),
                    },
                });
            }
            ChannelContext {
                name: channel.name.clone(),
                name_snake: to_snake_case(&channel.name),
                enum_name: channel_enum_name(&channel.name),
                queue: match &channel.transport {
                    Transport::Pgmq(pgmq) => pgmq.queue.clone(),
                    Transport::Unbound => String::new(),
                },
                variants,
            }
        })
        .filter(|channel| !channel.variants.is_empty())
        .collect();
    EmitContext {
        contracts_use: if payload_types.is_empty() {
            String::new()
        } else {
            format!(
                "use crate::events::contracts::{{{}}};",
                payload_types.into_iter().collect::<Vec<_>>().join(", ")
            )
        },
        channels,
    }
}

fn consumers_context(arch: &EventArchitecture) -> ConsumersContext {
    let mut consumers = Vec::new();
    let mut payload_types = BTreeSet::new();
    let mut seen_consumers = std::collections::HashSet::new();
    for subscription in &arch.subscriptions {
        if !seen_consumers.insert(subscription.consumer.clone()) {
            warn_once(
                &format!("duplicate-consumer:{}", subscription.consumer),
                &format!(
                    "consumer `{}' is the target of more than one subscription; one handler trait is emitted",
                    subscription.consumer
                ),
            );
            continue;
        }
        let mut events = Vec::new();
        for event_name in &subscription.events {
            let Some(event) = arch.event(event_name) else {
                warn_once(
                    &format!("subscribed-unresolved:{}", event_name),
                    &format!(
                        "subscription `{}' subscribes to `{}' which is not a declared event; skipped",
                        subscription.name, event_name
                    ),
                );
                continue;
            };
            payload_types.insert(payload_struct_name(&event.name));
            events.push(HandlerEventContext {
                method: format!("on_{}", to_snake_case(&event.name)),
                payload: payload_struct_name(&event.name),
                event_name: event.name.clone(),
            });
        }
        if events.is_empty() {
            warn_once(
                &format!("subscription-empty:{}", subscription.name),
                &format!(
                    "subscription `{}' has no resolvable events; consumer skipped",
                    subscription.name
                ),
            );
            continue;
        }
        let type_name = consumer_type_name(&subscription.consumer);
        consumers.push(ConsumerContext {
            name: subscription.consumer.clone(),
            name_snake: to_snake_case(&subscription.consumer),
            context_struct: format!("{type_name}Context"),
            handler_trait: format!("{type_name}Handler"),
            events,
        });
    }
    ConsumersContext {
        contracts_use: if payload_types.is_empty() {
            String::new()
        } else {
            format!(
                "use crate::events::contracts::{{{}}};",
                payload_types.into_iter().collect::<Vec<_>>().join(", ")
            )
        },
        consumers,
    }
}

// ── the generator ──────────────────────────────────────────────────────

pub struct EvtEventsGenerator {
    output_dir: PathBuf,
}

impl EvtEventsGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for EvtEventsGenerator {
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::EvtEvents
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &crate::ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let arch = architecture_for(db, project, config).await?;
        if arch.is_empty() {
            // The acceptance contract: no `.evt` input ⇒ zero files.
            return Ok(Vec::new());
        }

        let events_dir = self.output_dir.join("src").join("events");
        let mut files = Vec::new();

        // contracts.rs — transport-free, emitted on every dialect.
        let contracts = contracts_context(db, &arch, &config.defaults.type_suffix).await?;
        files.push(GeneratedFile {
            path: events_dir.join("contracts.rs"),
            content: render_template_with_project(
                tera,
                "events/contracts.tera",
                &contracts,
                project,
            )?,
        });

        // emit.rs — publish surface for pgmq-bound channels only
        // (unbound channels emit no publish fns; the binding rule warned
        // at architecture construction).
        let emit = emit_context(&arch);
        files.push(GeneratedFile {
            path: events_dir.join("emit.rs"),
            content: render_template_with_project(tera, "events/emit.tera", &emit, project)?,
        });

        // consumers.rs — handler traits + the drain-facing registry
        // (transport-free).
        let consumers = consumers_context(&arch);
        files.push(GeneratedFile {
            path: events_dir.join("consumers.rs"),
            content: render_template_with_project(
                tera,
                "events/consumers.tera",
                &consumers,
                project,
            )?,
        });

        // mod.rs — module declarations + the source `.evt` files.
        let events_mod = EventsModContext {
            sources: db
                .get_evt_models()
                .await?
                .into_iter()
                .map(|model| model.source_path)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            payload_names: deduped_events(&arch)
                .iter()
                .map(|event| payload_struct_name(&event.name))
                .collect(),
        };
        files.push(GeneratedFile {
            path: events_dir.join("mod.rs"),
            content: render_template_with_project(tera, "events/mod.tera", &events_mod, project)?,
        });

        Ok(files)
    }
}

#[cfg(test)]
mod tests;
