//! Workers-topology scaffold generator.
//!
//! Emits, for every domain that owns entities in the generation order, a
//! self-contained Cloudflare Worker crate under `workers/{domain}/`
//! (Cargo.toml, native `main.rs`, wasm `worker.rs` + `lib.rs`, per-domain
//! `app_state.rs`, standalone `error.rs`, worker-mode auth middleware, and a
//! wrangler.toml), plus a shared workspace manifest and a thin gateway worker
//! crate under `workers/gateway/` that fans `/api/v1/{domain}/*` out to the
//! per-domain workers over Cloudflare service bindings.
//!
//! The monolith scaffold (`ScaffoldGenerator`) is gated off in Workers
//! topology, so this generator owns everything under `workers/`. It reuses the
//! exact same domain grouping as the monolith scaffold
//! ([`build_scaffold_domains`](super::gen::build_scaffold_domains)).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::scaffold::gen::{
    build_scaffold_domains, resolve_path, ScaffoldDomain, ScaffoldEntity,
};
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::{GenerationEntry, ProjectConfig};
use codegraph_config::DomainConfig;

/// Conventional default worker name for a domain: `{app_name}-{domain}`.
pub fn default_worker_name(app_name: &str, domain: &str) -> String {
    format!("{app_name}-{domain}")
}

/// Conventional service-binding name for a domain: uppercase, hyphens as
/// underscores (matches the binding strings used in code and wrangler.toml).
pub fn worker_binding_name(domain: &str) -> String {
    domain.replace('-', "_").to_ascii_uppercase()
}

/// Path from a worker crate dir (`{output}/workers/{domain}/`) to a shared
/// crate whose base path is absolute or declared relative to the process CWD
/// (repo root). Empty bases (disabled crates) yield an empty path.
fn worker_crate_path(base: &str, abs_output: &Path, domain: &str) -> String {
    if base.is_empty() {
        return String::new();
    }
    let base_path = Path::new(base);
    let abs_base = if base_path.is_absolute() {
        base_path.to_path_buf()
    } else {
        std::env::current_dir()
            .expect("current_dir should be accessible")
            .join(base_path)
    };
    let worker_dir = abs_output.join("workers").join(domain);
    pathdiff::diff_paths(&abs_base, &worker_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether any entity in a domain's config declares a workflow with SLA timers.
fn domain_has_workflow_timers(entry: &codegraph_config::DomainEntry) -> bool {
    entry
        .entity_config
        .values()
        .any(|ec| ec.workflow.as_ref().is_some_and(|wf| !wf.timers.is_empty()))
}

/// A Cloudflare service binding declared by a domain worker.
#[derive(Debug, Serialize)]
pub struct WorkerServiceBinding {
    pub domain: String,
    /// Uppercase binding name used in `env.service(...)` calls and wrangler.
    pub binding: String,
    /// The target Worker's deployed name (its resolved `worker_name`).
    pub service_name: String,
}

/// One domain worker's scaffold context.
#[derive(Debug, Serialize)]
pub struct WorkerDomain {
    pub name: String,
    pub label: String,
    pub postgres_schema: String,
    pub entities: Vec<ScaffoldEntity>,
    /// Deployed Cloudflare Worker name (`worker_name` or the convention).
    pub worker_name: String,
    /// Cargo `[lib]` name (valid identifier, snake_case of `worker_name`).
    pub worker_lib_name: String,
    /// This worker's own service-binding name (uppercase domain).
    pub binding: String,
    pub custom_domain: Option<String>,
    /// Service bindings this worker declares (explicit or `depends_on`).
    pub service_bindings: Vec<WorkerServiceBinding>,
    pub hyperdrive_binding: String,
    pub cron_triggers: Vec<String>,
    /// Whether any workflow entity in this domain declares SLA timers — the
    /// worker then emits a `#[event(scheduled)]` timer sweep and (unless the
    /// domain already lists crons) a default `*/1 * * * *` trigger.
    pub has_workflow_timers: bool,
    /// Whether webhooks are enabled for this domain (per-domain `webhooks`
    /// config key). When true the worker compiles and serves the webhook
    /// endpoint/subscription API, emits a cron drain handler, and declares a
    /// Cloudflare Queues producer + consumer for delivery jobs.
    pub has_webhooks: bool,
    /// Resolved Cloudflare Queue name for webhook delivery jobs.
    pub queue_name: String,
    /// Resolved Cloudflare Queue binding name (used in `env.queue(binding)`
    /// and the wrangler queue config).
    pub queue_binding: String,
    /// Max delivery attempts before an endpoint is auto-deactivated.
    pub queue_max_retries: u32,
    /// Optional Cloudflare Queues consumer max_concurrency.
    pub queue_max_concurrency: Option<u32>,
    /// Whether Workers native observability is enabled for this domain
    /// (per-domain `observability` config key). When true the worker's
    /// wrangler.toml emits an `[observability]` block, the wasm entry installs
    /// a console panic hook + tracing subscriber, and the metrics middleware
    /// emits a structured per-request console log.
    pub observability: bool,
    /// Path to the domain-types crate relative to this worker's crate dir.
    pub domain_types_path: String,
    /// Path to the hooks-api crate relative to this worker's crate dir
    /// (empty when hooks are disabled).
    pub hooks_api_path: String,
}

/// Context for the workers scaffold templates.
#[derive(Debug, Serialize)]
pub struct WorkerScaffoldContext {
    pub app_name: String,
    pub gateway_name: String,
    pub gateway_lib_name: String,
    pub domains: Vec<WorkerDomain>,
    /// Path to the codegraph-workflow crate relative to the output root
    /// (empty → git+rev dependency).
    pub codegraph_workflow_path: String,
    /// Path to the codegraph-type-contracts crate relative to the output
    /// root (empty → git+rev dependency).
    pub type_contracts_path: String,
    /// Whether the gateway worker emits an `[observability]` block. Derived
    /// from the domains: true when any domain enables observability (the
    /// gateway is the shared entry point, so its logs are useful then too).
    pub gateway_observability: bool,
}

/// Build the per-domain worker contexts from the shared scaffold domains.
///
/// Resolves worker names, service bindings (falling back to `depends_on`),
/// hyperdrive bindings, cron triggers and custom domains from the domain
/// config. `domain_types_path` is filled in later (it depends on the absolute
/// output dir), so it starts empty here.
pub fn build_worker_domains(
    app_name: &str,
    config: &DomainConfig,
    scaffold_domains: Vec<ScaffoldDomain>,
) -> Vec<WorkerDomain> {
    scaffold_domains
        .into_iter()
        .map(|d| {
            let entry = config.domains.get(&d.name);
            let worker_name = entry
                .map(|e| e.worker_name_or(&default_worker_name(app_name, &d.name)))
                .unwrap_or_else(|| default_worker_name(app_name, &d.name));
            let service_bindings = entry
                .map(|e| e.service_bindings_or_depends())
                .unwrap_or_default()
                .into_iter()
                .map(|dep| WorkerServiceBinding {
                    domain: dep.to_string(),
                    binding: worker_binding_name(dep),
                    service_name: config
                        .domains
                        .get(dep)
                        .map(|de| de.worker_name_or(&default_worker_name(app_name, dep)))
                        .unwrap_or_else(|| default_worker_name(app_name, dep)),
                })
                .collect();
            let has_workflow_timers = entry.map(domain_has_workflow_timers).unwrap_or(false);
            let has_webhooks = entry.map(|e| e.webhooks_or(false)).unwrap_or(false);
            let mut cron_triggers = entry
                .and_then(|e| e.cron_triggers.clone())
                .unwrap_or_default();
            // A workflow-bearing domain needs a periodic timer sweep; fall back
            // to an every-minute cron unless the config already lists one.
            if cron_triggers.is_empty() && has_workflow_timers {
                cron_triggers.push("*/1 * * * *".to_string());
            }
            // A webhook-bearing domain needs a periodic pgmq drain sweep.
            if has_webhooks && !cron_triggers.iter().any(|c| c == "*/1 * * * *") {
                cron_triggers.push("*/1 * * * *".to_string());
            }
            let queue_binding = entry
                .map(|e| e.queue_binding_or("WEBHOOK_QUEUE"))
                .unwrap_or_else(|| "WEBHOOK_QUEUE".to_string());
            let queue_name = entry
                .map(|e| e.queue_name_or(&format!("{app_name}-{}-webhooks", d.name)))
                .unwrap_or_else(|| format!("{app_name}-{}-webhooks", d.name));
            WorkerDomain {
                name: d.name.clone(),
                label: d.label.clone(),
                postgres_schema: d.postgres_schema.clone(),
                entities: d.entities,
                worker_lib_name: codegraph_naming::to_snake_case(&worker_name),
                worker_name,
                binding: worker_binding_name(&d.name),
                custom_domain: entry.and_then(|e| e.custom_domain.clone()),
                service_bindings,
                hyperdrive_binding: entry
                    .map(|e| e.hyperdrive_binding_or("HYPERDRIVE"))
                    .unwrap_or_else(|| "HYPERDRIVE".to_string()),
                cron_triggers,
                has_workflow_timers,
                has_webhooks,
                queue_name,
                queue_binding,
                queue_max_retries: entry.map(|e| e.queue_max_retries_or(5)).unwrap_or(5),
                queue_max_concurrency: entry.and_then(|e| e.queue_max_concurrency),
                observability: entry.map(|e| e.observability_or(false)).unwrap_or(false),
                domain_types_path: String::new(),
                hooks_api_path: String::new(),
            }
        })
        .collect()
}

/// Global generator producing the per-domain worker crates, the workers
/// workspace manifest, and the gateway worker crate.
pub struct WorkerScaffoldGenerator {
    output_dir: PathBuf,
}

impl WorkerScaffoldGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl GlobalGenerator for WorkerScaffoldGenerator {
    fn name(&self) -> &str {
        "worker_scaffold"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let scaffold_domains = build_scaffold_domains(db, config, generation_order).await;
        if scaffold_domains.is_empty() {
            return Ok(Vec::new());
        }

        // Absolute output dir (shared by all path calculations).
        let abs_output = if self.output_dir.is_absolute() {
            self.output_dir.clone()
        } else {
            std::env::current_dir()
                .expect("current_dir should be accessible")
                .join(&self.output_dir)
        };

        // Domain-types / hooks-api crate paths, re-based to each worker crate
        // dir (`workers/{domain}/`). Crate bases are declared relative to the
        // process CWD (repo root); a fixed `../../` prefix (as used by the
        // monolith scaffold's Cargo.toml, which sits directly at the output
        // root) would land one level short here because worker crates nest
        // under `workers/{domain}/`.
        let app_name = project.app_name.clone();
        let gateway_name = format!("{app_name}-gateway");
        let gateway_lib_name = codegraph_naming::to_snake_case(&gateway_name);

        // Shared codegraph crate paths. The workspace manifest lives in the
        // `workers/` directory, so dependency paths are re-based from there
        // (empty when the consuming project pins them via git rev).
        let workers_abs = abs_output.join("workers");
        let codegraph_workflow_rel = resolve_path(&project.codegraph_workflow_base, &workers_abs);
        let type_contracts_rel = resolve_path(&project.type_contracts_base, &workers_abs);

        let mut domains = build_worker_domains(&app_name, config, scaffold_domains);
        for domain in &mut domains {
            domain.domain_types_path =
                worker_crate_path(&project.domain_types_base, &abs_output, &domain.name);
            domain.hooks_api_path = if project.hooks_api_crate.is_empty() {
                String::new()
            } else {
                worker_crate_path(&project.hooks_api_base, &abs_output, &domain.name)
            };
        }
        let gateway_observability = domains.iter().any(|d| d.observability);

        // Schemas the API-key migration grants app_user DML on: every
        // domain schema plus common (codelists) and the platform infra
        // schema — the same list the monolith scaffold uses.
        let mut grant_schemas: Vec<String> = domains
            .iter()
            .map(|d| d.postgres_schema.clone())
            .chain(["common".to_string(), "platform".to_string()].into_iter())
            .collect();
        grant_schemas.sort();
        grant_schemas.dedup();

        let ctx = WorkerScaffoldContext {
            app_name,
            gateway_name,
            gateway_lib_name,
            domains,
            codegraph_workflow_path: codegraph_workflow_rel,
            type_contracts_path: type_contracts_rel,
            gateway_observability,
        };

        let workers_dir = self.output_dir.join("workers");
        let gateway_dir = workers_dir.join("gateway");

        let mut files = Vec::new();

        // ── API-key management migration ─────────────────────────────────
        // The worker auth middleware depends on `public.verify_api_key` /
        // `public.create_api_key` / `public.log_api_key_usage` and the
        // `api_keys_private` schema. The monolith scaffold emits this as
        // `0002_api_key_management.sql`; workers topology gates the monolith
        // scaffold off, so the worker scaffold owns it here (same numbering,
        // same template).
        let grant_ctx = serde_json::json!({ "grant_schemas": grant_schemas });
        let api_key_migration =
            render_template_with_project(tera, "db/api_key_migration.tera", &grant_ctx, project)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0002_api_key_management.sql"),
            content: api_key_migration,
        });

        // ── Workspace manifest ───────────────────────────────────────────
        let workspace_cargo = render_template_with_project(
            tera,
            "scaffold/workers_workspace_cargo_toml.tera",
            &ctx,
            project,
        )?;
        files.push(GeneratedFile {
            path: workers_dir.join("Cargo.toml"),
            content: workspace_cargo,
        });

        // ── Gateway worker crate ─────────────────────────────────────────
        let gateway_cargo =
            render_template_with_project(tera, "scaffold/gateway_cargo_toml.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: gateway_dir.join("Cargo.toml"),
            content: gateway_cargo,
        });
        let gateway_main =
            render_template_with_project(tera, "scaffold/gateway_main.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: gateway_dir.join("src").join("main.rs"),
            content: gateway_main,
        });
        let gateway_lib =
            render_template_with_project(tera, "scaffold/gateway_lib.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: gateway_dir.join("src").join("lib.rs"),
            content: gateway_lib,
        });
        let gateway_worker =
            render_template_with_project(tera, "scaffold/gateway.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: gateway_dir.join("src").join("worker.rs"),
            content: gateway_worker,
        });
        let gateway_wrangler =
            render_template_with_project(tera, "scaffold/gateway_wrangler.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: gateway_dir.join("wrangler.toml"),
            content: gateway_wrangler,
        });

        // ── Per-domain worker crates ─────────────────────────────────────
        for domain in &ctx.domains {
            let base = workers_dir.join(&domain.name);

            let cargo = render_template_with_project(
                tera,
                "scaffold/worker_cargo_toml.tera",
                domain,
                project,
            )?;
            files.push(GeneratedFile {
                path: base.join("Cargo.toml"),
                content: cargo,
            });

            let main =
                render_template_with_project(tera, "scaffold/worker_main.tera", domain, project)?;
            files.push(GeneratedFile {
                path: base.join("src").join("main.rs"),
                content: main,
            });

            let lib =
                render_template_with_project(tera, "scaffold/worker_lib.tera", domain, project)?;
            files.push(GeneratedFile {
                path: base.join("src").join("lib.rs"),
                content: lib,
            });

            let worker =
                render_template_with_project(tera, "scaffold/worker.tera", domain, project)?;
            files.push(GeneratedFile {
                path: base.join("src").join("worker.rs"),
                content: worker,
            });

            let app_state = render_template_with_project(
                tera,
                "scaffold/worker_app_state.tera",
                domain,
                project,
            )?;
            files.push(GeneratedFile {
                path: base.join("src").join("app_state.rs"),
                content: app_state,
            });

            // Per-domain webhooks: when enabled, the worker compiles and serves
            // the webhook endpoint/subscription API plus a Queue-backed
            // dispatch/delivery module. These are provider-conditional
            // templates (cornucopia `DbClient`), unlike the monolith's SeaORM
            // `webhook/*.tera` generators which stay byte-identical.
            if domain.has_webhooks && project.is_cornucopia() {
                let webhook_api = render_template_with_project(
                    tera,
                    "webhook/api_endpoints_cornucopia.tera",
                    domain,
                    project,
                )?;
                files.push(GeneratedFile {
                    path: base.join("src").join("webhook_api.rs"),
                    content: webhook_api,
                });

                let webhook_router = render_template_with_project(
                    tera,
                    "webhook/api_router_cornucopia.tera",
                    domain,
                    project,
                )?;
                files.push(GeneratedFile {
                    path: base.join("src").join("webhook_router.rs"),
                    content: webhook_router,
                });

                let webhook_dispatch = render_template_with_project(
                    tera,
                    "webhook/dispatch_worker.tera",
                    domain,
                    project,
                )?;
                files.push(GeneratedFile {
                    path: base.join("src").join("webhook_dispatch.rs"),
                    content: webhook_dispatch,
                });
            }

            // Cornucopia client plumbing: deadpool pool (native) / per-request
            // Hyperdrive client (wasm32) behind a single ClientSource trait.
            if project.is_cornucopia() {
                let db_client =
                    render_template_with_project(tera, "scaffold/db_client.tera", domain, project)?;
                files.push(GeneratedFile {
                    path: base.join("src").join("db_client.rs"),
                    content: db_client,
                });

                // Client-generic workflow engine adapter: bridges
                // codegraph_workflow's WorkflowTx/WorkflowClient to the
                // cornucopia client (per-request Hyperdrive client on wasm).
                let workflow_client = render_template_with_project(
                    tera,
                    "scaffold/worker_workflow_client.tera",
                    domain,
                    project,
                )?;
                files.push(GeneratedFile {
                    path: base.join("src").join("workflow_client.rs"),
                    content: workflow_client,
                });
            }

            let error = render_template_with_project(tera, "scaffold/error.tera", domain, project)?;
            files.push(GeneratedFile {
                path: base.join("src").join("error.rs"),
                content: error,
            });

            // Query-string extractor used by generated list handlers
            // (`crate::qs_query::QsQuery`) — reuses the monolith template.
            let qs_query =
                render_template_with_project(tera, "scaffold/qs_query.tera", domain, project)?;
            files.push(GeneratedFile {
                path: base.join("src").join("qs_query.rs"),
                content: qs_query,
            });

            let middleware = render_template_with_project(
                tera,
                "scaffold/worker_middleware.tera",
                domain,
                project,
            )?;
            files.push(GeneratedFile {
                path: base.join("src").join("middleware.rs"),
                content: middleware,
            });

            // Shared API metadata type — routed handlers reference
            // `crate::api::meta::Meta`, so each worker crate needs its own
            // copy (same template as the monolith scaffold).
            let meta = render_template_with_project(tera, "scaffold/meta.tera", domain, project)?;
            files.push(GeneratedFile {
                path: base.join("src").join("api").join("meta.rs"),
                content: meta,
            });

            // Hook registry re-export — worker_app_state.rs references
            // `crate::hooks::HookRegistry`; the registry itself lives in the
            // shared hooks-api crate (single source of truth for all domains'
            // lifecycle traits), so each worker re-exports it from there.
            if !project.hooks_api_crate.is_empty() {
                let hooks_mod = render_template_with_project(
                    tera,
                    "scaffold/worker_hooks_mod.tera",
                    domain,
                    project,
                )?;
                files.push(GeneratedFile {
                    path: base.join("src").join("hooks").join("mod.rs"),
                    content: hooks_mod,
                });
            }

            let wrangler = render_template_with_project(
                tera,
                "scaffold/worker_wrangler.tera",
                domain,
                project,
            )?;
            files.push(GeneratedFile {
                path: base.join("wrangler.toml"),
                content: wrangler,
            });
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::config::parse_domain_config_str;

    fn test_config() -> DomainConfig {
        parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
entities = ["PayRunType"]
worker_name = "hr-payroll-worker"
custom_domain = "payroll.example.com/*"
hyperdrive_binding = "PAYROLL_DB"
cron_triggers = ["0 0 * * *"]
depends_on = ["common"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["CodeType"]
depends_on = ["payroll"]
service_bindings = ["timecard"]
"#,
        )
        .unwrap()
    }

    fn scaffold_domain(name: &str) -> ScaffoldDomain {
        ScaffoldDomain {
            name: name.to_string(),
            label: format!("{name} label"),
            postgres_schema: name.to_string(),
            entities: vec![ScaffoldEntity {
                name: "PayRun".to_string(),
                module_name: "pay_run".to_string(),
                domain: name.to_string(),
                table_name: "pay_run".to_string(),
                has_commands: true,
                append_only: false,
                has_query_hooks: true,
            }],
        }
    }

    #[test]
    fn worker_name_defaults_to_app_domain() {
        let config = test_config();
        let domains = build_worker_domains("hr", &config, vec![scaffold_domain("common")]);
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].worker_name, "hr-common");
        assert_eq!(domains[0].worker_lib_name, "hr_common");
    }

    #[test]
    fn explicit_worker_name_wins_over_convention() {
        let config = test_config();
        let domains = build_worker_domains("hr", &config, vec![scaffold_domain("payroll")]);
        assert_eq!(domains[0].worker_name, "hr-payroll-worker");
        assert_eq!(domains[0].worker_lib_name, "hr_payroll_worker");
    }

    #[test]
    fn service_bindings_fallback_to_depends_on() {
        let config = test_config();
        let domains = build_worker_domains("hr", &config, vec![scaffold_domain("payroll")]);
        assert_eq!(domains.len(), 1);
        let bindings = &domains[0].service_bindings;
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].domain, "common");
        assert_eq!(bindings[0].binding, "COMMON");
        assert_eq!(bindings[0].service_name, "hr-common");
    }

    #[test]
    fn explicit_service_bindings_override_depends_on() {
        // `common` declares both `depends_on = ["payroll"]` and an explicit
        // `service_bindings = ["timecard"]` — the explicit list must win.
        let config = test_config();
        let domains = build_worker_domains(
            "hr",
            &config,
            vec![scaffold_domain("common"), scaffold_domain("payroll")],
        );
        let common = domains.iter().find(|d| d.name == "common").unwrap();
        assert_eq!(common.service_bindings.len(), 1);
        assert_eq!(common.service_bindings[0].domain, "timecard");
        assert_eq!(common.service_bindings[0].binding, "TIMECARD");
        assert_eq!(common.service_bindings[0].service_name, "hr-timecard");
    }

    #[test]
    fn hyperdrive_binding_and_cron_passthrough() {
        let config = test_config();
        let domains = build_worker_domains(
            "hr",
            &config,
            vec![scaffold_domain("common"), scaffold_domain("payroll")],
        );
        let payroll = domains.iter().find(|d| d.name == "payroll").unwrap();
        assert_eq!(payroll.hyperdrive_binding, "PAYROLL_DB");
        assert_eq!(payroll.cron_triggers, vec!["0 0 * * *"]);
        assert_eq!(
            payroll.custom_domain.as_deref(),
            Some("payroll.example.com/*")
        );
        assert_eq!(payroll.binding, "PAYROLL");

        let common = domains.iter().find(|d| d.name == "common").unwrap();
        assert_eq!(common.hyperdrive_binding, "HYPERDRIVE");
        assert!(common.cron_triggers.is_empty());
        assert!(common.custom_domain.is_none());
        assert_eq!(common.binding, "COMMON");
    }

    #[test]
    fn binding_names_are_uppercase_with_underscores() {
        assert_eq!(worker_binding_name("common"), "COMMON");
        assert_eq!(worker_binding_name("pay-roll"), "PAY_ROLL");
        assert_eq!(default_worker_name("hr", "payroll"), "hr-payroll");
    }

    #[test]
    fn workflow_timers_enable_scheduled_sweep_and_default_cron() {
        let config = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read", "update", "delete", "list"]

[domains.recruiting]
label = "Recruiting"
schema_dir = "recruiting"
postgres_schema = "recruiting"
entities = ["CandidateType"]

[domains.recruiting.entity_config.CandidateType.workflow]
status_field = "candidate_status_code"
initial_state = "new"
terminal_states = ["hired", "rejected"]

[domains.recruiting.entity_config.CandidateType.workflow.timers.sla]
trigger_on_enter = "screening"
type = "deadline"
duration_hours = 48
target_state = "escalated"
"#,
        )
        .unwrap();

        let domains = build_worker_domains("hr", &config, vec![scaffold_domain("recruiting")]);
        assert_eq!(domains.len(), 1);
        assert!(domains[0].has_workflow_timers);
        assert_eq!(domains[0].cron_triggers, vec!["*/1 * * * *"]);

        // A domain without workflow timers does not get the default cron.
        let no_timers = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["CodeType"]
"#,
        )
        .unwrap();
        let common = build_worker_domains("hr", &no_timers, vec![scaffold_domain("common")]);
        assert!(!common[0].has_workflow_timers);
        assert!(common[0].cron_triggers.is_empty());
    }

    #[test]
    fn webhooks_resolve_queue_binding_name_and_drain_cron() {
        let config = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read"]

[domains.timecard]
label = "Timecard"
schema_dir = "timecard"
postgres_schema = "timecard"
entities = ["TimeEntryType"]
webhooks = true
queue_binding = "TIMECARD_WEBHOOKS"
queue_max_retries = 7
queue_max_concurrency = 10
"#,
        )
        .unwrap();

        let domains = build_worker_domains("hr", &config, vec![scaffold_domain("timecard")]);
        assert_eq!(domains.len(), 1);
        let d = &domains[0];
        assert!(d.has_webhooks);
        assert_eq!(d.queue_binding, "TIMECARD_WEBHOOKS");
        assert_eq!(d.queue_name, "hr-timecard-webhooks");
        assert_eq!(d.queue_max_retries, 7);
        assert_eq!(d.queue_max_concurrency, Some(10));
        // Webhook-bearing domains get a periodic drain cron.
        assert_eq!(d.cron_triggers, vec!["*/1 * * * *"]);
    }

    #[test]
    fn webhooks_use_defaults_when_config_keys_absent() {
        let config = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
entities = ["PayRunType"]
webhooks = true
"#,
        )
        .unwrap();

        let domains = build_worker_domains("hr", &config, vec![scaffold_domain("payroll")]);
        let d = &domains[0];
        assert!(d.has_webhooks);
        assert_eq!(d.queue_binding, "WEBHOOK_QUEUE");
        assert_eq!(d.queue_name, "hr-payroll-webhooks");
        assert_eq!(d.queue_max_retries, 5);
        assert_eq!(d.queue_max_concurrency, None);
        assert_eq!(d.cron_triggers, vec!["*/1 * * * *"]);

        // Webhooks default off for domains that don't opt in.
        let config = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["CodeType"]
"#,
        )
        .unwrap();
        let common = build_worker_domains("hr", &config, vec![scaffold_domain("common")]);
        assert!(!common[0].has_webhooks);
    }

    /// Render the per-domain wrangler template and assert the TOML output.
    #[test]
    fn worker_wrangler_renders_hyperdrive_services_crons_and_routes() {
        let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let tera = crate::generate::template_engine::create_tera(&template_dir).unwrap();
        let project = ProjectConfig::default();

        let config = test_config();
        let mut domains = build_worker_domains("hr", &config, vec![scaffold_domain("payroll")]);
        domains[0].domain_types_path = "../../crates/hr-domain-types".to_string();

        let rendered = render_template_with_project(
            &tera,
            "scaffold/worker_wrangler.tera",
            &domains[0],
            &project,
        )
        .unwrap();
        let parsed: toml::Value = toml::from_str(&rendered).expect("wrangler.toml must parse");

        assert_eq!(parsed["name"].as_str(), Some("hr-payroll-worker"));
        assert_eq!(parsed["main"].as_str(), Some("build/worker/shim.mjs"));
        assert!(
            parsed["build"]["command"]
                .as_str()
                .unwrap()
                .contains("--features cloudflare-worker"),
            "build command must enable the cloudflare-worker feature"
        );
        assert_eq!(
            parsed["hyperdrive"][0]["binding"].as_str(),
            Some("PAYROLL_DB")
        );
        assert_eq!(parsed["services"][0]["binding"].as_str(), Some("COMMON"));
        assert_eq!(parsed["services"][0]["service"].as_str(), Some("hr-common"));
        assert_eq!(parsed["triggers"]["crons"][0].as_str(), Some("0 0 * * *"));
        assert_eq!(
            parsed["routes"][0]["pattern"].as_str(),
            Some("payroll.example.com/*")
        );
    }

    /// Render the per-domain wrangler template for a webhook-bearing domain and
    /// assert the Cloudflare Queues producer + consumer + drain cron.
    #[test]
    fn worker_wrangler_renders_queues_when_webhooks_enabled() {
        let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let tera = crate::generate::template_engine::create_tera(&template_dir).unwrap();
        let project = ProjectConfig::default();

        let config = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read"]

[domains.timecard]
label = "Timecard"
schema_dir = "timecard"
postgres_schema = "timecard"
entities = ["TimeEntryType"]
webhooks = true
queue_name = "hr-timecard-webhooks"
queue_binding = "TIMECARD_WEBHOOKS"
queue_max_retries = 7
queue_max_concurrency = 10
"#,
        )
        .unwrap();

        let mut domains = build_worker_domains("hr", &config, vec![scaffold_domain("timecard")]);
        domains[0].domain_types_path = "../../crates/hr-domain-types".to_string();

        let rendered = render_template_with_project(
            &tera,
            "scaffold/worker_wrangler.tera",
            &domains[0],
            &project,
        )
        .unwrap();
        let parsed: toml::Value = toml::from_str(&rendered).expect("wrangler.toml must parse");

        let producers = parsed["queues"]["producers"].as_array().expect("producers");
        assert_eq!(producers.len(), 1);
        assert_eq!(producers[0]["binding"].as_str(), Some("TIMECARD_WEBHOOKS"));
        assert_eq!(producers[0]["queue"].as_str(), Some("hr-timecard-webhooks"));

        let consumers = parsed["queues"]["consumers"].as_array().expect("consumers");
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0]["queue"].as_str(), Some("hr-timecard-webhooks"));
        assert_eq!(consumers[0]["max_retries"].as_integer(), Some(7));
        assert_eq!(consumers[0]["max_concurrency"].as_integer(), Some(10));

        // Webhook-bearing domains get the drain cron even without workflow timers.
        assert_eq!(parsed["triggers"]["crons"][0].as_str(), Some("*/1 * * * *"));
    }

    /// The `observability` config key resolves into `WorkerDomain.observability`
    /// (default off) and emits an `[observability]` block in the wrangler only
    /// when enabled.
    #[test]
    fn worker_wrangler_renders_observability_block_only_when_enabled() {
        let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let tera = crate::generate::template_engine::create_tera(&template_dir).unwrap();
        let project = ProjectConfig::default();

        let enabled = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read"]

[domains.payroll]
label = "Payroll"
schema_dir = "payroll"
postgres_schema = "payroll"
entities = ["PayRunType"]
observability = true
"#,
        )
        .unwrap();

        let domains = build_worker_domains("hr", &enabled, vec![scaffold_domain("payroll")]);
        assert!(domains[0].observability);
        let rendered = render_template_with_project(
            &tera,
            "scaffold/worker_wrangler.tera",
            &domains[0],
            &project,
        )
        .unwrap();
        let parsed: toml::Value = toml::from_str(&rendered).expect("wrangler.toml must parse");
        assert_eq!(parsed["observability"]["enabled"].as_bool(), Some(true));
        assert_eq!(
            parsed["observability"]["head_sampling_rate"].as_integer(),
            Some(1)
        );

        // A domain without the key does not get the block (byte-identical to
        // the pre-observability output).
        let disabled = parse_domain_config_str(
            r#"
[defaults]
operations = ["create", "read"]

[domains.common]
label = "Common"
schema_dir = "common"
postgres_schema = "common"
entities = ["CodeType"]
"#,
        )
        .unwrap();
        let domains = build_worker_domains("hr", &disabled, vec![scaffold_domain("common")]);
        assert!(!domains[0].observability);
        let rendered = render_template_with_project(
            &tera,
            "scaffold/worker_wrangler.tera",
            &domains[0],
            &project,
        )
        .unwrap();
        let parsed: toml::Value = toml::from_str(&rendered).expect("wrangler.toml must parse");
        assert!(parsed.get("observability").is_none());
    }

    /// Render the gateway wrangler + workspace manifest templates.
    #[test]
    fn gateway_wrangler_and_workspace_manifest_render() {
        let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let tera = crate::generate::template_engine::create_tera(&template_dir).unwrap();
        let project = ProjectConfig::default();

        let config = test_config();
        let mut domains = build_worker_domains(
            "hr",
            &config,
            vec![scaffold_domain("common"), scaffold_domain("payroll")],
        );
        for domain in &mut domains {
            domain.domain_types_path = "../../crates/hr-domain-types".to_string();
        }
        let ctx = WorkerScaffoldContext {
            app_name: "hr".to_string(),
            gateway_name: "hr-gateway".to_string(),
            gateway_lib_name: "hr_gateway".to_string(),
            domains,
            codegraph_workflow_path: String::new(),
            type_contracts_path: String::new(),
            gateway_observability: false,
        };

        let rendered =
            render_template_with_project(&tera, "scaffold/gateway_wrangler.tera", &ctx, &project)
                .unwrap();
        let parsed: toml::Value = toml::from_str(&rendered).expect("gateway wrangler must parse");
        assert_eq!(parsed["name"].as_str(), Some("hr-gateway"));
        let services = parsed["services"].as_array().expect("services array");
        assert_eq!(services.len(), 2, "one service binding per domain");
        let bindings: Vec<&str> = services
            .iter()
            .map(|s| s["binding"].as_str().unwrap())
            .collect();
        assert!(bindings.contains(&"COMMON"));
        assert!(bindings.contains(&"PAYROLL"));

        let manifest = render_template_with_project(
            &tera,
            "scaffold/workers_workspace_cargo_toml.tera",
            &ctx,
            &project,
        )
        .unwrap();
        let parsed: toml::Value = toml::from_str(&manifest).expect("workspace manifest must parse");
        let members = parsed["workspace"]["members"].as_array().unwrap();
        let member_names: Vec<&str> = members.iter().map(|m| m.as_str().unwrap()).collect();
        assert!(member_names.contains(&"gateway"));
        assert!(member_names.contains(&"common"));
        assert!(member_names.contains(&"payroll"));
        assert_eq!(member_names.len(), 3);
        assert!(parsed["workspace"]["dependencies"].get("worker").is_some());
    }

    /// Render worker_app_state.tera: command handlers are only emitted for
    /// entities with commands, and the query-handler hooks argument is only
    /// passed when the entity has query hooks (matches the generated
    /// command.rs/query.rs `new` signatures).
    #[test]
    fn worker_app_state_guards_commands_and_query_hooks() {
        let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let tera = crate::generate::template_engine::create_tera(&template_dir).unwrap();
        let project = ProjectConfig {
            deployment_topology: "workers".to_string(),
            hooks_api_crate: "hr_hooks_api".to_string(),
            ..Default::default()
        };
        let config = test_config();

        let full = scaffold_domain("common");
        let query_only = ScaffoldDomain {
            name: "compliance".to_string(),
            label: "compliance label".to_string(),
            postgres_schema: "compliance".to_string(),
            entities: vec![ScaffoldEntity {
                name: "ComplianceCheckResult".to_string(),
                module_name: "compliance_check_result".to_string(),
                domain: "compliance".to_string(),
                table_name: "compliance_check_result".to_string(),
                has_commands: false,
                append_only: false,
                has_query_hooks: false,
            }],
        };
        let mut domains = build_worker_domains("hr", &config, vec![full, query_only]);
        for d in &mut domains {
            d.domain_types_path = "crates/hr-domain-types".to_string();
            d.hooks_api_path = "crates/hr-hooks-api".to_string();
        }
        let full_domain = domains.iter().find(|d| d.name == "common").unwrap();
        let rendered = render_template_with_project(
            &tera,
            "scaffold/worker_app_state.tera",
            full_domain,
            &project,
        )
        .unwrap();
        assert!(
            rendered.contains("common_pay_run_commands"),
            "entity with commands must get a command handler field:\n{rendered}"
        );
        assert!(
            rendered.contains("common_pay_run_queries"),
            "entity with query hooks must get a query handler field:\n{rendered}"
        );

        let query_only_domain = domains.iter().find(|d| d.name == "compliance").unwrap();
        let rendered = render_template_with_project(
            &tera,
            "scaffold/worker_app_state.tera",
            query_only_domain,
            &project,
        )
        .unwrap();
        assert!(
            !rendered.contains("compliance_compliance_check_result_commands"),
            "query-only entity must NOT get a command handler field:\n{rendered}"
        );
        assert!(
            rendered.contains("compliance_compliance_check_result_queries"),
            "query-only entity must still get a query handler field:\n{rendered}"
        );
    }

    /// Render worker_lib.tera: cornucopia workers keep `pub mod entity;` (the
    /// routed media handlers reference `crate::entity::...` even though CRUD
    /// persistence runs through cornucopia) in addition to the cornucopia
    /// client modules.
    #[test]
    fn worker_lib_declares_entity_in_cornucopia_workers() {
        let template_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let tera = crate::generate::template_engine::create_tera(&template_dir).unwrap();
        let project = ProjectConfig {
            deployment_topology: "workers".to_string(),
            persistence_provider: "cornucopia".to_string(),
            hooks_api_crate: "hr_hooks_api".to_string(),
            ..Default::default()
        };
        let config = test_config();
        let domains = build_worker_domains("hr", &config, vec![scaffold_domain("payroll")]);
        let rendered =
            render_template_with_project(&tera, "scaffold/worker_lib.tera", &domains[0], &project)
                .unwrap();
        for needle in [
            "pub mod entity;",
            "pub mod db_client;",
            "pub mod workflow_client;",
        ] {
            assert!(
                rendered.contains(needle),
                "worker lib.rs must declare `{needle}`, got:\n{rendered}"
            );
        }
    }
}
