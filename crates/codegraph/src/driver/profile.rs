//! Profile resolution: the `BuildPlan` and the `ProjectConfig` the
//! templates render from, for the full pipeline ([`resolve_run_profile`])
//! and the IFML-only path ([`resolve_ifml_profile`]), plus the
//! `codegraph-*` dependency-pin chain.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::generate::{
    AtprotoConfig, CargoConfig, CodegenConfig, DatabaseConfig, DeploymentConfig, DtoKeyCasing,
    IdentityConfig, IntegrationFlags, PathsConfig, ProjectConfig, UxConfig,
};

/// Load and resolve the profile: CLI `--ifml-framework` injection, the
/// `BuildPlan`, and the `ProjectConfig` the templates render from
/// (profile meta overriding defaults). Plan-less fallback (profiles.toml
/// missing AND default profile) keeps the issue-#447 coherent-workspace
/// contract. Returns the plan, the project config, and the
/// `domain_types_base` path (profile meta or the conventional fallback).
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_run_profile(
    profiles_path: &Path,
    profile_name: &str,
    variant: Option<&str>,
    ifml_framework: &[String],
    codegraph_rev: Option<&str>,
    output: &Path,
    domain_config: &codegraph_config::config::DomainConfig,
) -> Result<(
    Option<crate::profile::BuildPlan>,
    ProjectConfig,
    Option<PathBuf>,
)> {
    let registry = crate::profile::CapabilityRegistry::new();
    if profiles_path.exists() || profile_name != "default" {
        let mut resolved =
            crate::profile::load_and_resolve_profile(profiles_path, profile_name, variant)?;
        // CLI --ifml-framework wins when the profile declares no
        // per-framework IFML targets: without the targets
        // `expand_ifml_sections` keeps the unsuffixed `ifml_*` generator
        // names and the run emits no IFML artifacts at all (the same gap
        // `ifml_generate`'s ifml_only fallback covers).
        if !ifml_framework.is_empty() && resolved.ifml_frameworks.is_empty() {
            resolved.ifml_frameworks = ifml_framework
                .iter()
                .map(|fw| crate::profile::IfmlFrameworkTarget {
                    name: fw.clone(),
                    output: None,
                    target: "ui".to_string(),
                })
                .collect();
            for fw in ifml_framework {
                resolved
                    .features
                    .entry(format!("framework_{fw}"))
                    .or_insert(toml::Value::Boolean(true));
            }
        }
        let plan = crate::profile::BuildPlan::from_profile(&resolved, &registry)?;

        // Build project config from profile meta (optional fields override defaults).
        let meta = &resolved.meta;
        check_ui_package_scope(meta)?;
        let domain_types_base_path = meta.domain_types_base.as_ref().map(|p| output.join(p));
        let project_config = ProjectConfig {
            identity: IdentityConfig {
                app_name: meta.app_name.clone().unwrap_or_else(|| "app".into()),
                lib_name: "cosmos".into(),
                domain_types_crate: meta
                    .domain_types_crate
                    .clone()
                    .unwrap_or_else(|| "domain_types".into()),
                hooks_api_crate: meta.hooks_api_crate.clone().unwrap_or_default(),
                api_title: meta
                    .api_title
                    .clone()
                    .unwrap_or_else(|| "HR Open API".into()),
                generator_name: meta
                    .generator_name
                    .clone()
                    .unwrap_or_else(|| "codegraph".into()),
                api_version: domain_config.defaults.api_version.clone(),
                ui_package_scope: meta.ui_package_scope.clone(),
            },
            paths: PathsConfig {
                domain_types_base: meta.domain_types_base.clone().unwrap_or_default(),
                hooks_api_base: meta.hooks_api_base.clone().unwrap_or_default(),
                extensions_base: meta.extensions_base.clone().unwrap_or_default(),
                app_config_base: meta.app_config_base.clone().unwrap_or_default(),
                decision_engine_base: meta.decision_engine_base.clone().unwrap_or_default(),
                codegraph_workflow_base: meta.codegraph_workflow_base.clone().unwrap_or_default(),
                type_contracts_base: meta.type_contracts_base.clone().unwrap_or_default(),
            },
            database: DatabaseConfig {
                database_target: plan.database_target(),
                persistence_provider: plan.persistence_provider(),
            },
            codegen: CodegenConfig {
                dto_key_casing: DtoKeyCasing::from_config(&plan.dto_key_casing),
                namespace_layout: plan.namespace_layout,
                expr_ir: plan.expr_ir,
                ifml_e2e_auth: plan.ifml_e2e_auth,
                ifml_remote_functions: plan.ifml_remote_functions,
                entity_remote_functions: plan.entity_remote_functions,
                entity_form: plan.entity_form,
                ui_live_queries: plan.ui_live_queries,
                ifml_tanstack_forms: plan.ifml_tanstack_forms,
                testid_module: plan.testid_module,
                structural_gates: plan.structural_gates,
                api_key_mode: plan.api_key_authz.mode,
                unmodified_grants: plan.api_key_authz.unmodified_grants,
                event_envelope: plan.event_envelope.as_str().to_string(),
                event_idempotency: plan.event_idempotency,
                event_archive: plan.event_archive,
                events_producer: plan.events_producer,
                types_import_prefix: domain_config.defaults.types_import_prefix.clone(),
            },
            deployment: DeploymentConfig {
                deployment_topology: plan.deployment_topology(),
            },
            cargo: CargoConfig {
                codegraph_rev: resolve_codegraph_rev(codegraph_rev),
                dependency_strategy: plan.dependency_strategy,
                codegraph_path_root: codegraph_checkout_root(),
                ..CargoConfig::default()
            },
            integration: IntegrationFlags {
                has_atproto: plan.has_atproto,
                has_fern: plan.has_fern,
                fern_sdk_languages: plan.fern_sdk_languages.clone(),
                has_emdash: plan.has_emdash,
                has_extension_points: false,
                public_operations_rls: plan.public_operations_rls,
                has_function_postconditions: resolved
                    .features
                    .get("function_postconditions")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                emdash_site_pages_base: String::new(),
                emdash_site_e2e_base: String::new(),
            },
            atproto: AtprotoConfig {
                atproto_authority: String::new(),
                atproto_tenancy: plan.atproto_tenancy.clone(),
                atproto_float_policy: resolved
                    .features
                    .get("atproto_float_policy")
                    .and_then(|v| v.as_str())
                    .unwrap_or("integer_scaled")
                    .to_string(),
            },
            ux: UxConfig { ux: None },
        };

        println!(
            "Using profile \"{}\" (variant: {:?}) — {} entity, {} domain, {} global generators",
            resolved.meta.name,
            variant,
            plan.entity_generators.len(),
            plan.domain_generators.len(),
            plan.global_generators.len(),
        );
        Ok((Some(plan), project_config, domain_types_base_path))
    } else {
        // profiles.toml doesn't exist and profile is "default" —
        // backward compat: run all generators without a plan.
        eprintln!(
            "Warning: profiles.toml not found at {} — running all generators (no profile filtering)",
            profiles_path.display()
        );
        // Plan-less runs still render a coherent, compilable workspace
        // (issue #447):
        //
        // - The separate domain-types crate is load-bearing even without a
        //   profile: the app's command/query/dto/handler code re-exports
        //   from it (`domain_types::SourceContext`, the DTO re-export
        //   modules) and the scaffold manifest path-deps it. With no
        //   profile meta to name `domain_types_base`, the companion-crate
        //   generators fell back to the OUTPUT ROOT and clobbered the app
        //   manifest/lib — so fall back to the conventional crate location
        //   instead.
        // - The `codegraph-*` dependency lines sit behind
        //   `{% if project.codegraph_rev %}` gates; an empty rev drops
        //   them entirely while the generated code references those
        //   crates. The pin chain is CLI value → git HEAD → the binary's
        //   embedded checkout rev.
        //
        // Only these fields deviate from the pipeline's default config —
        // every other template key keeps its default value, and the build
        // plan stays `None` so no plan semantics leak into generator
        // gating.
        let domain_types_base_path = Some(output.join("crates/domain-types"));
        let project_config = ProjectConfig {
            paths: PathsConfig {
                domain_types_base: "crates/domain-types".into(),
                ..PathsConfig::default()
            },
            cargo: CargoConfig {
                codegraph_rev: resolve_codegraph_rev(codegraph_rev),
                ..CargoConfig::default()
            },
            ..ProjectConfig::default()
        };
        Ok((None, project_config, domain_types_base_path))
    }
}

/// Resolve the IFML-only path's build plan and project config: either from
/// a provided profiles.toml, or a synthesized minimal IFML-only plan. When a
/// profile resolves but does not declare the per-framework generator names
/// (`ifml_route_{fw}`), `frameworks` is authoritative and the plan falls
/// back to `ifml_only`.
pub(super) fn resolve_ifml_profile(
    profiles_config_path: Option<&Path>,
    frameworks: &[String],
    domain_config: &codegraph_config::config::DomainConfig,
) -> Result<(Option<crate::profile::BuildPlan>, ProjectConfig)> {
    let registry = crate::profile::CapabilityRegistry::new();
    let mut project_config = ProjectConfig {
        identity: IdentityConfig {
            api_version: domain_config.defaults.api_version.clone(),
            ..IdentityConfig::default()
        },
        codegen: CodegenConfig {
            types_import_prefix: domain_config.defaults.types_import_prefix.clone(),
            ..CodegenConfig::default()
        },
        ..ProjectConfig::default()
    };
    let build_plan = if let Some(profiles_path) = profiles_config_path {
        if !profiles_path.exists() {
            return Err(crate::error::Error::Config(format!(
                "profiles config not found at {}",
                profiles_path.display()
            )));
        }
        let resolved = crate::profile::load_and_resolve_profile(profiles_path, "default", None)?;
        let plan = crate::profile::BuildPlan::from_profile(&resolved, &registry)?;

        let meta = &resolved.meta;
        check_ui_package_scope(meta)?;
        project_config.identity.app_name = meta
            .app_name
            .clone()
            .unwrap_or(project_config.identity.app_name);
        project_config.identity.domain_types_crate = meta
            .domain_types_crate
            .clone()
            .unwrap_or(project_config.identity.domain_types_crate);
        project_config.identity.hooks_api_crate = meta.hooks_api_crate.clone().unwrap_or_default();
        project_config.identity.api_title = meta
            .api_title
            .clone()
            .unwrap_or(project_config.identity.api_title);
        project_config.identity.generator_name = meta
            .generator_name
            .clone()
            .unwrap_or(project_config.identity.generator_name);
        project_config.identity.ui_package_scope = meta.ui_package_scope.clone();
        project_config.paths.domain_types_base = meta.domain_types_base.clone().unwrap_or_default();
        project_config.paths.hooks_api_base = meta.hooks_api_base.clone().unwrap_or_default();
        project_config.paths.extensions_base = meta.extensions_base.clone().unwrap_or_default();
        project_config.paths.app_config_base = meta.app_config_base.clone().unwrap_or_default();
        project_config.paths.decision_engine_base =
            meta.decision_engine_base.clone().unwrap_or_default();
        project_config.paths.codegraph_workflow_base =
            meta.codegraph_workflow_base.clone().unwrap_or_default();
        project_config.paths.type_contracts_base =
            meta.type_contracts_base.clone().unwrap_or_default();
        project_config.database.database_target = plan.database_target();
        project_config.database.persistence_provider = plan.persistence_provider();
        project_config.deployment.deployment_topology = plan.deployment_topology();
        project_config.codegen.expr_ir = plan.expr_ir;
        project_config.codegen.ifml_e2e_auth = plan.ifml_e2e_auth;
        project_config.codegen.ifml_remote_functions = plan.ifml_remote_functions;
        project_config.codegen.entity_remote_functions = plan.entity_remote_functions;
        project_config.codegen.entity_form = plan.entity_form;
        project_config.codegen.ui_live_queries = plan.ui_live_queries;
        project_config.codegen.ifml_tanstack_forms = plan.ifml_tanstack_forms;
        project_config.codegen.testid_module = plan.testid_module;
        project_config.codegen.structural_gates = plan.structural_gates;
        project_config.codegen.api_key_mode = plan.api_key_authz.mode;
        project_config.codegen.unmodified_grants = plan.api_key_authz.unmodified_grants;
        project_config.codegen.event_envelope = plan.event_envelope.as_str().to_string();
        project_config.codegen.event_idempotency = plan.event_idempotency;
        project_config.codegen.event_archive = plan.event_archive;
        project_config.codegen.events_producer = plan.events_producer;
        project_config.integration.public_operations_rls = plan.public_operations_rls;

        println!(
            "Using profile \"{}\" — {} global generators",
            resolved.meta.name,
            plan.global_generators.len(),
        );

        let missing: Vec<&String> = frameworks
            .iter()
            .filter(|fw| !plan.has_global_gen(&format!("ifml_route_{fw}")))
            .collect();
        if missing.is_empty() {
            Some(plan)
        } else {
            eprintln!(
                "Warning: profile \"{}\" does not declare per-framework IFML \
                 generators (ifml_route_{}) — falling back to --framework-driven \
                 IFML-only plan",
                resolved.meta.name,
                missing
                    .iter()
                    .map(|fw| fw.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            Some(crate::profile::BuildPlan::ifml_only(frameworks)?)
        }
    } else {
        Some(crate::profile::BuildPlan::ifml_only(frameworks)?)
    };
    Ok((build_plan, project_config))
}

/// Return the current Git revision hash. Falls back to the binary's
/// embedded checkout rev when the invocation cwd carries no git metadata —
/// a generated `codegraph-*` dep pin must never render empty (issue #447:
/// an empty rev drops the dependency lines the generated code needs).
fn current_git_rev() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::rev::codegraph_rev().to_string())
}

/// Resolve the `codegraph-*` dependency pin for generated manifests:
/// explicit CLI value wins, else [`current_git_rev`] (git HEAD, else the
/// binary's embedded checkout rev).
fn resolve_codegraph_rev(explicit: Option<&str>) -> String {
    match explicit {
        Some(rev) if !rev.is_empty() => rev.to_string(),
        _ => current_git_rev(),
    }
}

/// Absolute path of the codegraph checkout that produced this binary:
/// this crate compiles at `<checkout>/crates/codegraph`, so the checkout
/// root is two parents up from its compile-time manifest dir. Consumed by
/// the `dependency_strategy = "path"` templates (issue #347) — empty only
/// if the manifest dir has fewer than two parents (never in practice).
fn codegraph_checkout_root() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// An explicit profile `meta.ui_package_scope` must be a valid npm scope
/// (issue #18) — a hard config error naming the value, before any tree is
/// written.
fn check_ui_package_scope(meta: &crate::profile::ProfileMeta) -> Result<()> {
    if let Some(scope) = &meta.ui_package_scope {
        crate::generate::validate_ui_package_scope(scope).map_err(crate::error::Error::Config)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_codegraph_rev_explicit_value_wins() {
        assert_eq!(
            resolve_codegraph_rev(Some("determinism-test-pin")),
            "determinism-test-pin"
        );
    }

    #[test]
    fn resolve_codegraph_rev_empty_explicit_falls_through() {
        // An empty explicit value must fall through to the git/embedded
        // chain, never render as an empty pin (issue #447).
        assert_eq!(resolve_codegraph_rev(Some("")), current_git_rev());
        assert_eq!(resolve_codegraph_rev(None), current_git_rev());
    }

    #[test]
    fn current_git_rev_is_never_empty() {
        // Inside this checkout git supplies HEAD; outside any checkout the
        // binary's embedded rev does. Either way the pin chain bottoms out
        // non-empty (issue #447).
        let rev = current_git_rev();
        assert!(!rev.is_empty(), "rev chain must never render empty");
        assert!(
            rev.chars().all(|c| c.is_ascii_hexdigit()),
            "rev must be a bare SHA: {rev}"
        );
    }
}
