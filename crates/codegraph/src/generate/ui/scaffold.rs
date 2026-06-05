use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template_with_project;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct UiScaffoldContext {
    pub app_name: String,
    pub domains: Vec<UiDomain>,
    pub has_integrations: bool,
    pub has_webhooks: bool,
}

#[derive(Debug, Serialize)]
pub struct UiDomain {
    pub name: String,
    pub label: String,
    pub tier: String,
    pub entities: Vec<UiNavEntity>,
}

#[derive(Debug, Serialize)]
pub struct UiNavEntity {
    pub name: String,
    pub module_name: String,
    pub path_segment: String,
    pub label: String,
    pub fields: Vec<UiNavField>,
}

/// Lightweight field descriptor for i18n message key generation.
#[derive(Debug, Serialize)]
pub struct UiNavField {
    pub name: String,
    pub label: String,
}

pub struct UiScaffoldGenerator {
    output_dir: PathBuf,
    has_integrations: bool,
    has_webhooks: bool,
}

impl UiScaffoldGenerator {
    pub fn new(output_dir: &Path, has_integrations: bool, has_webhooks: bool) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            has_integrations,
            has_webhooks,
        }
    }
}

#[async_trait]
impl GlobalGenerator for UiScaffoldGenerator {
    fn name(&self) -> &str {
        "ui-scaffold"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        // Group generation_order entries by domain
        let mut domain_entity_map: std::collections::HashMap<String, Vec<UiNavEntity>> =
            std::collections::HashMap::new();
        for entry in generation_order {
            let stripped = config.defaults.strip_suffix(&entry.schema_title);
            let module_name = codegraph_naming::to_snake_case(&stripped);
            let path_segment = codegraph_naming::to_kebab_case(&stripped);
            let label = codegraph_naming::to_display_name(&stripped);

            // Collect lightweight field names + labels for i18n message keys.
            let no_immutables: Vec<String> = Vec::new();
            let fields = match super::common::collect_ui_fields(
                db,
                &entry.schema_title,
                &no_immutables,
                Some(&entry.domain),
            )
            .await
            {
                Ok(ui_fields) => ui_fields
                    .iter()
                    .map(|f| UiNavField {
                        name: f.name.clone(),
                        label: f.label.clone(),
                    })
                    .collect(),
                Err(e) => {
                    eprintln!(
                        "warning: failed to collect i18n fields for '{}': {e}",
                        entry.schema_title
                    );
                    Vec::new()
                }
            };

            domain_entity_map
                .entry(entry.domain.clone())
                .or_default()
                .push(UiNavEntity {
                    name: stripped,
                    module_name,
                    path_segment,
                    label,
                    fields,
                });
        }

        let mut domains: Vec<UiDomain> = config
            .domains
            .iter()
            .filter_map(|(name, entry)| {
                let entities = domain_entity_map.remove(name.as_str())?;
                Some(UiDomain {
                    name: name.clone(),
                    label: entry.label.clone(),
                    tier: entry.tier.clone(),
                    entities,
                })
            })
            .collect();
        domains.sort_by(|a, b| a.name.cmp(&b.name));

        let ctx = UiScaffoldContext {
            app_name: config.defaults.app_name.clone(),
            domains,
            has_integrations: self.has_integrations,
            has_webhooks: self.has_webhooks,
        };

        let ui = self.output_dir.join("ui");
        let src = ui.join("src");
        let lib = src.join("lib");

        let templates: &[(&str, PathBuf)] = &[
            ("ui/scaffold/package_json.tera", ui.join("package.json")),
            (
                "ui/scaffold/svelte_config.tera",
                ui.join("svelte.config.js"),
            ),
            ("ui/scaffold/vite_config.tera", ui.join("vite.config.ts")),
            (
                "ui/scaffold/components_json.tera",
                ui.join("components.json"),
            ),
            ("ui/scaffold/app_html.tera", src.join("app.html")),
            ("ui/scaffold/app_css.tera", src.join("app.css")),
            ("ui/scaffold/utils_ts.tera", lib.join("utils.ts")),
            (
                "ui/scaffold/api_client.tera",
                lib.join("api").join("client.ts"),
            ),
            ("ui/scaffold/supabase_client.tera", lib.join("supabase.ts")),
            (
                "ui/scaffold/auth_callback.tera",
                src.join("routes")
                    .join("auth")
                    .join("callback")
                    .join("+server.ts"),
            ),
            (
                "ui/scaffold/app_guard_layout.tera",
                src.join("routes").join("(app)").join("+layout.server.ts"),
            ),
            (
                "ui/scaffold/login_page.tera",
                src.join("routes")
                    .join("(auth)")
                    .join("login")
                    .join("+page.svelte"),
            ),
            (
                "ui/scaffold/signup_page.tera",
                src.join("routes")
                    .join("(auth)")
                    .join("signup")
                    .join("+page.svelte"),
            ),
            (
                "ui/scaffold/dashboard_page.tera",
                src.join("routes")
                    .join("(app)")
                    .join("dashboard")
                    .join("+page.svelte"),
            ),
            (
                "ui/scaffold/settings_team.tera",
                src.join("routes")
                    .join("(app)")
                    .join("settings")
                    .join("team")
                    .join("+page.svelte"),
            ),
            (
                "ui/scaffold/settings_api_keys.tera",
                src.join("routes")
                    .join("(app)")
                    .join("settings")
                    .join("api-keys")
                    .join("+page.svelte"),
            ),
            (
                "ui/scaffold/app_layout.tera",
                src.join("routes").join("(app)").join("+layout.svelte"),
            ),
            (
                "ui/scaffold/root_redirect.tera",
                src.join("routes").join("+page.svelte"),
            ),
            (
                "ui/scaffold/workflow_panel.tera",
                lib.join("components")
                    .join("ui")
                    .join("WorkflowPanel.svelte"),
            ),
            (
                "ui/scaffold/search_input.tera",
                lib.join("components").join("ui").join("SearchInput.svelte"),
            ),
            (
                "ui/scaffold/structured_wrapper_field.tera",
                lib.join("components")
                    .join("ui")
                    .join("structured-wrapper-field")
                    .join("StructuredWrapperField.svelte"),
            ),
            (
                "ui/scaffold/structured_wrapper_field_index.tera",
                lib.join("components")
                    .join("ui")
                    .join("structured-wrapper-field")
                    .join("index.ts"),
            ),
            (
                "ui/scaffold/error_page.tera",
                src.join("routes").join("+error.svelte"),
            ),
            // Playwright E2E test scaffold
            (
                "ui/scaffold/playwright_config.tera",
                ui.join("playwright.config.ts"),
            ),
            (
                "ui/scaffold/test_helpers.tera",
                ui.join("tests").join("e2e").join("helpers.ts"),
            ),
            (
                "ui/test/auth.test.tera",
                ui.join("tests").join("e2e").join("auth.test.ts"),
            ),
            // Persona test infrastructure
            (
                "ui/scaffold/global_setup.tera",
                ui.join("tests")
                    .join("e2e")
                    .join("auth")
                    .join("global-setup.ts"),
            ),
            (
                "ui/scaffold/global_teardown.tera",
                ui.join("tests")
                    .join("e2e")
                    .join("auth")
                    .join("global-teardown.ts"),
            ),
            (
                "ui/scaffold/persona_fixtures.tera",
                ui.join("tests")
                    .join("e2e")
                    .join("fixtures")
                    .join("personas.ts"),
            ),
            (
                "ui/scaffold/entity_navigation_store.tera",
                lib.join("stores").join("entity-navigation.ts"),
            ),
            // Paraglide.js i18n infrastructure
            (
                "ui/scaffold/paraglide_settings.tera",
                ui.join("project.inlang").join("settings.json"),
            ),
            ("ui/scaffold/hooks_server.tera", src.join("hooks.server.ts")),
            // reroute hook lives in src/hooks.ts (SvelteKit universal hooks file)
            ("ui/scaffold/hooks_reroute.tera", src.join("hooks.ts")),
            ("ui/scaffold/app_d.tera", src.join("app.d.ts")),
            (
                "ui/scaffold/version_server.tera",
                src.join("routes").join("version").join("+server.ts"),
            ),
            (
                "ui/scaffold/version_page.tera",
                src.join("routes").join("version").join("+page.svelte"),
            ),
            (
                "ui/scaffold/messages_en.tera",
                ui.join("messages").join("en.json"),
            ),
        ];

        let mut files = Vec::new();
        for (template, path) in templates {
            let content = render_template_with_project(tera, template, &ctx, project)?;
            files.push(GeneratedFile {
                path: path.clone(),
                content,
            });
        }

        // Webhook UI templates (conditional on has_webhooks)
        if self.has_webhooks {
            let webhooks_dir = src
                .join("routes")
                .join("(app)")
                .join("settings")
                .join("webhooks");
            let new_dir = webhooks_dir.join("new");
            let id_dir = webhooks_dir.join("[id]");
            let edit_dir = id_dir.join("edit");

            let webhook_templates: &[(&str, PathBuf)] = &[
                (
                    "ui/scaffold/settings_webhooks.tera",
                    webhooks_dir.join("+page.svelte"),
                ),
                (
                    "ui/scaffold/settings_webhook_form.tera",
                    new_dir.join("+page.svelte"),
                ),
                (
                    "ui/scaffold/settings_webhook_detail.tera",
                    id_dir.join("+page.svelte"),
                ),
                (
                    "ui/scaffold/settings_webhook_form.tera",
                    edit_dir.join("+page.svelte"),
                ),
            ];

            for (template, path) in webhook_templates {
                let content = render_template_with_project(tera, template, &ctx, project)?;
                files.push(GeneratedFile {
                    path: path.clone(),
                    content,
                });
            }

            // Webhook test files
            let webhooks_test_dir = ui.join("tests").join("e2e").join("webhooks");
            let webhook_test_templates: &[(&str, PathBuf)] = &[
                (
                    "ui/test/webhooks_crud.test.tera",
                    webhooks_test_dir.join("webhooks-crud.test.ts"),
                ),
                (
                    "ui/test/webhooks_delivery.test.tera",
                    webhooks_test_dir.join("webhooks-delivery.test.ts"),
                ),
            ];

            for (template, path) in webhook_test_templates {
                let content = render_template_with_project(tera, template, &ctx, project)?;
                files.push(GeneratedFile {
                    path: path.clone(),
                    content,
                });
            }
        }

        // Integration UI templates (conditional on has_integrations)
        if self.has_integrations {
            let settings_integrations = src
                .join("routes")
                .join("(app)")
                .join("settings")
                .join("integrations");
            let install_dir = settings_integrations.join("install").join("[catalogId]");
            let detail_dir = settings_integrations.join("[installationId]");
            let edit_dir = detail_dir.join("settings");

            let integration_templates: &[(&str, PathBuf)] = &[
                (
                    "ui/scaffold/integrations_store.tera",
                    lib.join("stores").join("integrations.ts"),
                ),
                (
                    "ui/scaffold/settings_integrations.tera",
                    settings_integrations.join("+page.svelte"),
                ),
                (
                    "ui/scaffold/settings_integrations_server.tera",
                    settings_integrations.join("+page.server.ts"),
                ),
                (
                    "ui/scaffold/settings_integrations_install.tera",
                    install_dir.join("+page.svelte"),
                ),
                (
                    "ui/scaffold/settings_integrations_install_server.tera",
                    install_dir.join("+page.server.ts"),
                ),
                (
                    "ui/scaffold/settings_integrations_detail.tera",
                    detail_dir.join("+page.svelte"),
                ),
                (
                    "ui/scaffold/settings_integrations_detail_server.tera",
                    detail_dir.join("+page.server.ts"),
                ),
                (
                    "ui/scaffold/settings_integrations_edit.tera",
                    edit_dir.join("+page.svelte"),
                ),
            ];

            for (template, path) in integration_templates {
                let content = render_template_with_project(tera, template, &ctx, project)?;
                files.push(GeneratedFile {
                    path: path.clone(),
                    content,
                });
            }
        }

        Ok(files)
    }
}
