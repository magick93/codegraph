use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::api::api_model::resolve_entity_operations;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct ScaffoldContext {
    pub app_name: String,
    pub domains: Vec<ScaffoldDomain>,
    /// Schemas the generated API role (`app_user`) needs DML access to:
    /// every configured domain's postgres schema plus `common` (codelists)
    /// and the `platform` infra schema. Rendered into the grant migrations.
    pub grant_schemas: Vec<String>,
    pub codegraph_workflow_path: String,
    pub type_contracts_path: String,
    pub domain_types_path: String,
    pub hooks_api_path: String,
    pub extensions_path: String,
    pub app_config_path: String,
    pub decision_engine_path: String,
    pub has_webhooks: bool,
    pub has_reports: bool,
    pub has_grpc: bool,
    pub has_admin_cli: bool,
    pub migration_strategy: String,
}

#[derive(Debug, Serialize)]
pub struct ScaffoldEntity {
    pub name: String,
    pub module_name: String,
    pub domain: String,
    /// Physical table name in Postgres (from the graph, e.g. "case").
    pub table_name: String,
    /// Whether any command operations (create/update/delete) are enabled for
    /// this entity — hook-only entities get no command handler usage, so the
    /// AppState field would otherwise be dead code.
    pub has_commands: bool,
    /// True when the entity's operations exclude update AND delete (append-only
    /// event streams): app_user must not be granted UPDATE/DELETE on the table.
    pub append_only: bool,
    /// Whether the generated query handler takes a hooks argument (mirrors the
    /// `uses_find_by_id` condition in ddd/query.tera).
    pub has_query_hooks: bool,
}

#[derive(Debug, Serialize)]
pub struct ScaffoldDomain {
    pub name: String,
    pub label: String,
    pub postgres_schema: String,
    pub entities: Vec<ScaffoldEntity>,
}

pub struct ScaffoldGenerator {
    output_dir: PathBuf,
    has_webhooks: bool,
    has_reports: bool,
    has_grpc: bool,
    has_admin_cli: bool,
    migration_strategy: String,
}

impl ScaffoldGenerator {
    pub fn new(
        output_dir: &Path,
        has_webhooks: bool,
        has_reports: bool,
        has_grpc: bool,
        has_admin_cli: bool,
        migration_strategy: &str,
    ) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            has_webhooks,
            has_reports,
            has_grpc,
            has_admin_cli,
            migration_strategy: migration_strategy.to_string(),
        }
    }
}

/// Resolve a base path (relative to CWD or absolute) to a path relative
/// to the output directory. Returns empty string if `base` is empty.
pub(crate) fn resolve_path(base: &str, abs_output: &Path) -> String {
    if base.is_empty() {
        return String::new();
    }
    let abs_base = abs_output.join(base);
    pathdiff::diff_paths(&abs_base, abs_output)
        .unwrap_or_else(|| PathBuf::from(base))
        .to_string_lossy()
        .into_owned()
}

/// Group the generation order into per-domain scaffold domains (entities with
/// resolved operation flags), exactly as the monolith `ScaffoldGenerator` does.
///
/// Shared with the workers-topology `WorkerScaffoldGenerator` so both
/// topologies produce the same domain groupings. Only domains that actually
/// have entities in the generation order are returned.
pub async fn build_scaffold_domains(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    generation_order: &[GenerationEntry],
) -> Vec<ScaffoldDomain> {
    let mut domain_entity_map: std::collections::HashMap<String, Vec<ScaffoldEntity>> =
        std::collections::HashMap::new();
    let mut seen_scaffold_entities = std::collections::HashSet::new();
    for entry in generation_order {
        let stripped = config.defaults.strip_suffix(&entry.schema_title);
        // Titles may contain spaces (e.g. "Review Decision") — sanitize to
        // PascalCase so generated Rust identifiers compile.
        let entity_name = codegraph_naming::to_pascal_case(&stripped);
        let module_name = codegraph_naming::to_snake_case(&stripped);
        // Dedup by (domain, module_name) to prevent cross-domain name collisions
        if !seen_scaffold_entities.insert((entry.domain.clone(), module_name.clone())) {
            continue;
        }
        let operations = resolve_entity_operations(db, config, &entry.domain, &entity_name).await;
        let has_commands = operations
            .iter()
            .any(|op| op == "create" || op == "update" || op == "delete");
        let has_create = operations.iter().any(|op| op == "create");
        let has_read = operations.iter().any(|op| op == "read");
        let has_update = operations.iter().any(|op| op == "update");
        let has_delete = operations.iter().any(|op| op == "delete");
        let table_name = db
            .get_schema_in_domain(&stripped, &entry.domain)
            .await
            .ok()
            .flatten()
            .map(|s| s.pg_table_name)
            .unwrap_or_else(|| module_name.clone());
        let has_config_parent = config
            .domains
            .get(&entry.domain)
            .and_then(|d| d.get_entity_config(&entity_name))
            .and_then(|ec| ec.parent_ref.as_ref())
            .is_some();
        let has_query_hooks = has_create || (has_read && !has_config_parent);
        domain_entity_map
            .entry(entry.domain.clone())
            .or_default()
            .push(ScaffoldEntity {
                module_name: module_name.clone(),
                name: entity_name,
                domain: entry.domain.clone(),
                table_name,
                has_commands,
                append_only: !has_update && !has_delete,
                has_query_hooks,
            });
    }

    let mut domains: Vec<ScaffoldDomain> = config
        .domains
        .iter()
        .filter_map(|(name, entry)| {
            let entities = domain_entity_map.remove(name.as_str())?;
            Some(ScaffoldDomain {
                name: name.clone(),
                label: entry.label.clone(),
                postgres_schema: entry.postgres_schema.clone(),
                entities,
            })
        })
        .collect();
    domains.sort_by(|a, b| a.name.cmp(&b.name));
    domains
}

#[async_trait]
impl GlobalGenerator for ScaffoldGenerator {
    fn name(&self) -> &str {
        "scaffold"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let domains = build_scaffold_domains(db, config, generation_order).await;

        // Compute absolute output dir (shared by all path calculations)
        let abs_output = if self.output_dir.is_absolute() {
            self.output_dir.clone()
        } else {
            std::env::current_dir()
                .expect("current_dir should be accessible")
                .join(&self.output_dir)
        };

        let codegraph_workflow_path = resolve_path(&project.codegraph_workflow_base, &abs_output);
        let type_contracts_path = resolve_path(&project.type_contracts_base, &abs_output);
        let domain_types_path = resolve_path(&project.domain_types_base, &abs_output);
        let hooks_api_path = resolve_path(&project.hooks_api_base, &abs_output);
        let extensions_path = resolve_path(&project.extensions_base, &abs_output);
        let app_config_path = resolve_path(&project.app_config_base, &abs_output);
        let decision_engine_path = resolve_path(&project.decision_engine_base, &abs_output);

        // Physical Postgres schemas that need app_user grants: every domain
        // schema plus "common" (codelists) and the platform infra schema.
        // Sorted + deduped for stable SQL.
        let mut grant_schemas: Vec<String> = domains
            .iter()
            .map(|d| d.postgres_schema.clone())
            .chain(["common".to_string(), "platform".to_string()].into_iter())
            .collect();
        grant_schemas.sort();
        grant_schemas.dedup();

        let ctx = ScaffoldContext {
            app_name: crate::generate::get_project_config().app_name.clone(),
            domains,
            grant_schemas,
            codegraph_workflow_path,
            type_contracts_path,
            domain_types_path,
            hooks_api_path,
            extensions_path,
            app_config_path,
            decision_engine_path,
            has_webhooks: self.has_webhooks,
            has_reports: self.has_reports,
            has_grpc: self.has_grpc,
            has_admin_cli: self.has_admin_cli,
            migration_strategy: self.migration_strategy.clone(),
        };

        let mut files = Vec::new();

        let main_rs = render_template_with_project(tera, "scaffold/main.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("main.rs"),
            content: main_rs,
        });

        let server_rs = render_template_with_project(tera, "scaffold/server.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("server.rs"),
            content: server_rs,
        });

        if self.has_admin_cli {
            let config_rs =
                render_template_with_project(tera, "scaffold/config.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: self.output_dir.join("src").join("config.rs"),
                content: config_rs,
            });

            let doctor_rs =
                render_template_with_project(tera, "scaffold/doctor.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: self.output_dir.join("src").join("doctor.rs"),
                content: doctor_rs,
            });

            let migration_rs =
                render_template_with_project(tera, "scaffold/migration.tera", &ctx, project)?;
            files.push(GeneratedFile {
                path: self.output_dir.join("src").join("migration.rs"),
                content: migration_rs,
            });
        }

        let app_state =
            render_template_with_project(tera, "scaffold/app_state.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("app_state.rs"),
            content: app_state,
        });

        let cargo_toml =
            render_template_with_project(tera, "scaffold/cargo_toml.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("Cargo.toml"),
            content: cargo_toml,
        });

        let build_rs = render_template_with_project(tera, "scaffold/build_rs.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("build.rs"),
            content: build_rs,
        });

        let lib_rs = render_template_with_project(tera, "scaffold/lib.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("lib.rs"),
            content: lib_rs,
        });

        let error_rs = render_template_with_project(tera, "scaffold/error.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("error.rs"),
            content: error_rs,
        });

        let middleware_rs =
            render_template_with_project(tera, "scaffold/middleware.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("middleware")
                .join("mod.rs"),
            content: middleware_rs,
        });

        let permission_rs = render_template_with_project(
            tera,
            "scaffold/permission_middleware.tera",
            &ctx,
            project,
        )?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("src")
                .join("middleware")
                .join("permission.rs"),
            content: permission_rs,
        });

        let metrics_middleware_rs =
            render_template_with_project(tera, "scaffold/metrics_middleware.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("metrics_middleware.rs"),
            content: metrics_middleware_rs,
        });

        let qs_query_rs =
            render_template_with_project(tera, "scaffold/qs_query.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("qs_query.rs"),
            content: qs_query_rs,
        });

        let meta_content = render_template_with_project(
            tera,
            "scaffold/meta.tera",
            &serde_json::json!({}),
            project,
        )?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("api").join("meta.rs"),
            content: meta_content,
        });

        let integrations_rs = render_template_with_project(
            tera,
            "scaffold/integrations_handler.tera",
            &ctx,
            project,
        )?;
        files.push(GeneratedFile {
            path: self.output_dir.join("src").join("integrations.rs"),
            content: integrations_rs,
        });

        let api_key_migration =
            render_template_with_project(tera, "db/api_key_migration.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("0002_api_key_management.sql"),
            content: api_key_migration,
        });

        // Late-binding app_user grants: 0002 runs before entity DDL, so its
        // IF EXISTS-guarded grants never fire on a fresh database. This
        // migration sorts after every codelist/entity band and derives its
        // schema list + append-only revokes from the project config.
        let app_user_grants =
            render_template_with_project(tera, "db/app_user_grants.tera", &ctx, project)?;
        files.push(GeneratedFile {
            path: self
                .output_dir
                .join("migrations")
                .join("9000_app_user_grants.sql"),
            content: app_user_grants,
        });

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ScaffoldContext {
        ScaffoldContext {
            app_name: "onboarding-app".to_string(),
            grant_schemas: vec![
                "common".to_string(),
                "compliance".to_string(),
                "core".to_string(),
            ],
            domains: vec![ScaffoldDomain {
                name: "core".to_string(),
                label: "Core".to_string(),
                postgres_schema: "core".to_string(),
                entities: vec![ScaffoldEntity {
                    name: "CaseStatusChanged".to_string(),
                    module_name: "case_status_changed".to_string(),
                    domain: "core".to_string(),
                    table_name: "case_status_changed".to_string(),
                    has_commands: true,
                    append_only: true,
                    has_query_hooks: true,
                }],
            }],
            codegraph_workflow_path: String::new(),
            type_contracts_path: String::new(),
            domain_types_path: String::new(),
            hooks_api_path: String::new(),
            extensions_path: String::new(),
            app_config_path: String::new(),
            decision_engine_path: String::new(),
            has_webhooks: false,
            has_reports: false,
            has_grpc: false,
            has_admin_cli: false,
            migration_strategy: "sea-orm".to_string(),
        }
    }

    fn tera() -> tera::Tera {
        crate::generate::template_engine::create_tera(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
        )
        .unwrap()
    }

    #[test]
    fn api_key_migration_schema_list_is_dynamic() {
        let sql = render_template_with_project(
            &tera(),
            "db/api_key_migration.tera",
            &test_ctx(),
            &crate::generate::ProjectConfig::default(),
        )
        .unwrap();
        assert!(sql.contains("'compliance'"), "dynamic schema list");
        assert!(sql.contains("'core'"));
        assert!(
            !sql.contains("'recruiting'"),
            "hardcoded HR schema list must be gone"
        );
    }

    #[test]
    fn app_user_grants_cover_schemas_and_pgmq() {
        let sql = render_template_with_project(
            &tera(),
            "db/app_user_grants.tera",
            &test_ctx(),
            &crate::generate::ProjectConfig::default(),
        )
        .unwrap();
        for schema in ["common", "compliance", "core"] {
            assert!(
                sql.contains(&format!("'{}'", schema)),
                "missing schema {schema} in grant_schemas array"
            );
        }
        assert!(sql.contains("GRANT USAGE ON SCHEMA %I TO app_user"));
        assert!(sql.contains("GRANT USAGE ON SCHEMA pgmq TO app_user"));
        assert!(sql.contains("pgmq.q_events_core"));
    }

    #[test]
    fn app_user_grants_revoke_mutation_for_append_only() {
        let sql = render_template_with_project(
            &tera(),
            "db/app_user_grants.tera",
            &test_ctx(),
            &crate::generate::ProjectConfig::default(),
        )
        .unwrap();
        assert!(sql.contains("REVOKE UPDATE, DELETE ON core.case_status_changed FROM app_user"));
    }
}
