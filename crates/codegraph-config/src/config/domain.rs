use std::collections::HashMap;

use serde::Deserialize;

use super::entity::EntityConfig;

/// A pinned external domain face (issue #276): a foreign domain published
/// as a #275 artifact document and consumed read-only by this project.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainDependency {
    /// The foreign (publisher-side) domain name.
    pub domain: String,
    /// Path to the #275 artifact document, relative to the project root
    /// (the directory holding domains.toml). Absolute paths pass through.
    pub source: String,
    /// The pinned face version. Compared against the version the artifact
    /// carries in its `meta` block (when present) by `doctor`.
    pub version: String,
}

/// A single domain entry in the TOML configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainEntry {
    pub label: String,
    pub schema_dir: String,
    pub postgres_schema: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Pinned external domain faces consumed by this domain (issue #276).
    /// Absent in existing domains.toml files (serde-defaulted) — parsing
    /// and generated output are unchanged when no dependency is declared.
    #[serde(default)]
    pub dependencies: Vec<DomainDependency>,
    #[serde(default)]
    pub entities: Vec<String>,
    /// Per-entity configuration for API generation.
    #[serde(default)]
    pub entity_config: HashMap<String, EntityConfig>,
    /// Auto-discover entities from schema files. None = inherit from [defaults].
    #[serde(default)]
    pub auto_discover: Option<bool>,
    /// Entity names to force-exclude from auto-discovery (treated as value objects).
    #[serde(default)]
    pub exclude_entities: Vec<String>,
    /// Override graph classification → force as entity.
    #[serde(default)]
    pub force_entities: Vec<String>,
    /// Override graph classification → force as value object.
    #[serde(default)]
    pub force_value_objects: Vec<String>,
    /// Skip these types entirely (meta/infra schemas).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Whether entities in this domain support soft delete and audit columns.
    /// Defaults to `true` for entity-bearing domains.
    #[serde(default)]
    pub auditable: Option<bool>,
    /// Domain tier for progressive disclosure: "core" or "extended".
    #[serde(default = "default_tier")]
    pub tier: String,
    /// Cloudflare Worker name for this domain's worker.
    /// When None, consumers default to `{app_name}-{domain}` at template/context
    /// build time. Only meaningful under the Workers deployment topology.
    #[serde(default)]
    pub worker_name: Option<String>,
    /// Optional custom domain / route pattern for this domain's worker
    /// (e.g. "api.example.com/payroll/*"). When None, the gateway default
    /// route (`/{domain}/*`) is used.
    #[serde(default)]
    pub custom_domain: Option<String>,
    /// Names of other domain workers this worker can call via Cloudflare
    /// service bindings. When None, consumers fall back to `depends_on`.
    #[serde(default)]
    pub service_bindings: Option<Vec<String>>,
    /// Hyperdrive binding name for the database connection.
    /// When None, consumers default to "HYPERDRIVE".
    #[serde(default)]
    pub hyperdrive_binding: Option<String>,
    /// Cron expressions for scheduled handlers on this domain's worker.
    /// Each entry becomes a cron trigger in the worker's wrangler config.
    #[serde(default)]
    pub cron_triggers: Option<Vec<String>>,
    /// How cross-domain `include` queries are satisfied: "sql" (default) or
    /// "http". "sql" assumes shared database access; "http" routes include
    /// resolution through the owning domain's worker over service bindings.
    /// When None, consumers default to "sql".
    #[serde(default)]
    pub remote_include_mode: Option<String>,
    /// Enable webhook endpoint/subscription CRUD + dispatch + delivery on this
    /// domain's worker. When true, the worker compiles and serves
    /// `webhook_api.rs`/`webhook_router.rs` and, in workers topology, emits a
    /// cron drain handler plus a Cloudflare Queues consumer. When None,
    /// consumers default to false for workers topology (opt-in per domain) —
    /// the monolith keeps its existing global `has_webhooks` gate.
    #[serde(default)]
    pub webhooks: Option<bool>,
    /// Cloudflare Queue name for webhook delivery jobs (producer + consumer).
    /// When None, consumers default to `{app_name}-{domain}-webhooks`.
    #[serde(default)]
    pub queue_name: Option<String>,
    /// Cloudflare Queue binding name used in `env.queue(binding)` and wrangler.
    /// When None, consumers default to "WEBHOOK_QUEUE".
    #[serde(default)]
    pub queue_binding: Option<String>,
    /// Max delivery attempts before an endpoint is auto-deactivated. Matches
    /// the monolith `WebhookDispatcher.max_retries` (default 5).
    #[serde(default)]
    pub queue_max_retries: Option<u32>,
    /// Cloudflare Queues consumer max_concurrency (consumers only). Emitted as
    /// `max_concurrency` in the wrangler queue consumer config when set.
    #[serde(default)]
    pub queue_max_concurrency: Option<u32>,
    /// Enable Cloudflare Workers native observability for this domain's worker.
    /// When true, the generated wrangler.toml emits an `[observability]` block
    /// (`enabled` + `head_sampling_rate`), the wasm entry installs a console
    /// panic hook + tracing subscriber, and the metrics middleware emits a
    /// structured per-request console log. When None, consumers default to
    /// false (off — generated output stays byte-identical to pre-observability).
    #[serde(default)]
    pub observability: Option<bool>,
    /// Entity-less domain whose router delegates to a hand-written
    /// `handwritten_routes.rs` module. Such a domain generates NO entities,
    /// DDL migrations, DTOs, handlers or lifecycle traits — only the
    /// domain-level router scaffold (`src/api/{domain}/router.rs` for the
    /// monolith) and, in workers topology, a per-domain worker crate plus a
    /// gateway upstream/service binding. Typically paired with `entities = []`
    /// and `auto_discover = false`. Defaults to false.
    #[serde(default)]
    pub custom_routes: bool,
}

fn default_tier() -> String {
    "extended".to_string()
}

/// Normalize a name/key for fuzzy entity-config lookup: drop hyphens and
/// spaces so "LER-RSType", "Screening Result" etc. can match their
/// concatenated rust_type_name forms ("LERRSType", "ScreeningResult").
fn normalize_config_key(name: &str) -> String {
    name.replace(['-', ' '], "")
}

impl DomainEntry {
    /// A synthetic entry for a pinned dependency domain (issue #276): the
    /// foreign face is read from the graph, so the entry carries no local
    /// model config — it only registers the bounded context for generation
    /// ordering and validation.
    pub fn dependency_placeholder(domain: &str) -> DomainEntry {
        DomainEntry {
            label: domain.to_string(),
            schema_dir: String::new(),
            postgres_schema: domain.to_string(),
            depends_on: Vec::new(),
            dependencies: Vec::new(),
            entities: Vec::new(),
            entity_config: HashMap::new(),
            auto_discover: Some(false),
            exclude_entities: Vec::new(),
            force_entities: Vec::new(),
            force_value_objects: Vec::new(),
            exclude: Vec::new(),
            auditable: None,
            tier: default_tier(),
            worker_name: None,
            custom_domain: None,
            service_bindings: None,
            hyperdrive_binding: None,
            cron_triggers: None,
            remote_include_mode: None,
            webhooks: None,
            queue_name: None,
            queue_binding: None,
            queue_max_retries: None,
            queue_max_concurrency: None,
            observability: None,
            custom_routes: false,
        }
    }

    /// Look up entity config by name, trying both `name` and `nameType` variants.
    ///
    /// HR Open schemas use `XxxType` titles, so config keys are conventionally
    /// stored as `XxxType`. Custom schemas (e.g. pricing) may omit the `Type`
    /// suffix from their schema titles, causing `entity_name` = `"Subscription"`.
    /// This helper tries the plain name first, then the `Type`-suffixed form,
    /// so configs work regardless of naming convention.
    pub fn get_entity_config<'a>(&'a self, name: &str) -> Option<&'a EntityConfig> {
        self.entity_config
            .get(name)
            .or_else(|| self.entity_config.get(&format!("{}Type", name)))
            .or_else(|| {
                // Also try stripping a trailing "Type" from the key to match plain config keys
                let stripped = name.strip_suffix("Type").unwrap_or(name);
                if stripped != name {
                    self.entity_config.get(stripped)
                } else {
                    None
                }
            })
            .or_else(|| {
                // Fallback: match config keys whose normalized form (hyphens
                // and spaces removed) equals the input name. Handles
                // LER-RSType → LERRS and "Screening Result" → ScreeningResult
                // (titles with spaces produce concatenated rust_type_names).
                let normalized = normalize_config_key(name);
                self.entity_config.iter().find_map(|(key, cfg)| {
                    let key_normalized = normalize_config_key(key);
                    if key_normalized == normalized
                        || key_normalized == format!("{}Type", normalized)
                    {
                        Some(cfg)
                    } else {
                        None
                    }
                })
            })
    }

    /// Resolved Cloudflare Worker name: the explicit `worker_name`, or `default`
    /// (conventionally `{app_name}-{domain}`) when unset.
    pub fn worker_name_or(&self, default: &str) -> String {
        self.worker_name
            .clone()
            .unwrap_or_else(|| default.to_string())
    }

    /// Service bindings to declare for this worker.
    ///
    /// Falls back to `depends_on` when `service_bindings` is unset.
    pub fn service_bindings_or_depends(&self) -> Vec<&str> {
        match &self.service_bindings {
            Some(bindings) => bindings.iter().map(String::as_str).collect(),
            None => self.depends_on.iter().map(String::as_str).collect(),
        }
    }

    /// Resolved Hyperdrive binding name: the explicit `hyperdrive_binding`,
    /// or `default` (conventionally "HYPERDRIVE") when unset.
    pub fn hyperdrive_binding_or(&self, default: &str) -> String {
        self.hyperdrive_binding
            .clone()
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolved cross-domain include mode: the explicit `remote_include_mode`,
    /// or `default` (conventionally "sql") when unset.
    pub fn remote_include_mode_or(&self, default: &str) -> String {
        self.remote_include_mode
            .clone()
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolved webhook flag: the explicit `webhooks`, or `default` when unset.
    pub fn webhooks_or(&self, default: bool) -> bool {
        self.webhooks.unwrap_or(default)
    }

    /// Resolved Cloudflare Queue binding name: the explicit `queue_binding`,
    /// or `default` (conventionally "WEBHOOK_QUEUE") when unset.
    pub fn queue_binding_or(&self, default: &str) -> String {
        self.queue_binding
            .clone()
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolved Cloudflare Queue name: the explicit `queue_name`, or `default`
    /// (conventionally `{app_name}-{domain}-webhooks`) when unset.
    pub fn queue_name_or(&self, default: &str) -> String {
        self.queue_name
            .clone()
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolved max delivery attempts: the explicit `queue_max_retries`,
    /// or `default` (conventionally 5) when unset.
    pub fn queue_max_retries_or(&self, default: u32) -> u32 {
        self.queue_max_retries.unwrap_or(default)
    }

    /// Resolved observability flag: the explicit `observability`, or `default`
    /// (conventionally false) when unset.
    pub fn observability_or(&self, default: bool) -> bool {
        self.observability.unwrap_or(default)
    }
}
