use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::GenerationEntry;
use crate::error::Result;
use crate::render_template_with_project;
use crate::traits::{GeneratedFile, GlobalGenerator, GlobalGeneratorKind};
use codegraph_config::DomainConfig;

#[derive(Debug, Serialize)]
pub struct UiScaffoldContext {
    pub app_name: String,
    pub domains: Vec<UiDomain>,
    pub has_integrations: bool,
    pub has_webhooks: bool,
    /// The ux-rules plane is active (`project.ux` resolved, issue #293).
    /// Gates ux-only i18n message keys so flag-off scaffold output stays
    /// byte-identical.
    pub has_ux_rules: bool,
    /// The shadcn-svelte primitives generated pages import (issue #299).
    /// Drives the emitted `ui/PRIMITIVES.md` install surface; the surface
    /// itself is gated on [`Self::has_ux_rules`].
    pub shadcn_primitives: Vec<&'static str>,
    /// Entity remote-functions flag (issue #574 S2): the webhook settings
    /// pages render the load/command variants and the scaffold opts into
    /// the remote-functions toolchain.
    pub entity_remote_functions: bool,
    /// TanStack form layer (issue #580): the scaffold emits the form
    /// creator scaffold (if-absent) and pins the npm deps.
    pub entity_form: bool,
    /// Chart nav entries (issue #577) — one per modeled chart route.
    /// Empty (no charts) keeps the layout byte-identical.
    pub nav_charts: Vec<crate::charts::ChartNavItem>,
    /// Pinned `@tanstack/svelte-form` version (from `crate::forms`).
    pub tanstack_svelte_form_version: &'static str,
    /// Pinned `zod` version (from `crate::forms`).
    pub zod_version: &'static str,
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

/// The shadcn-svelte primitive set a generated app's UI is built on
/// (`#lib/components/ui/<name>/index.js` imports). Existing generated pages
/// already import the first entries; the ux-rules list page (#297) and the
/// detail-page child sections (#299) add `dropdown-menu` and `tooltip`.
/// The generator does NOT vendor these components — exactly like the
/// pre-existing primitives (`table`, `dialog`, `badge`, …), real projects
/// obtain them with the shadcn-svelte CLI against the emitted
/// `components.json`. Issue #299 makes this constant drive the emitted
/// `ui/PRIMITIVES.md` install surface (gated on the ux plane) so the
/// component set is documented and one command away instead of inferred
/// from page imports.
pub const SHADCN_PRIMITIVES: &[&str] = &[
    "alert-dialog",
    "avatar",
    "badge",
    "breadcrumb",
    "button",
    "card",
    "checkbox",
    "collapsible",
    "dialog",
    "dropdown-menu",
    "empty",
    "input",
    "label",
    "popover",
    "select",
    "separator",
    "sheet",
    "sidebar",
    "sonner",
    "table",
    "tooltip",
];

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
    fn kind(&self) -> GlobalGeneratorKind {
        GlobalGeneratorKind::UiScaffold
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let domains = build_ui_domains(db, config, generation_order).await;
        let nav_charts = crate::charts::chart_nav_items(db).await?;

        let ctx = UiScaffoldContext {
            app_name: config.defaults.app_name.clone(),
            domains,
            has_integrations: self.has_integrations,
            has_webhooks: self.has_webhooks,
            has_ux_rules: project.ux.ux.is_some(),
            shadcn_primitives: SHADCN_PRIMITIVES.to_vec(),
            entity_remote_functions: project.codegen.entity_remote_functions,
            entity_form: project.codegen.entity_form,
            tanstack_svelte_form_version: crate::forms::TANSTACK_SVELTE_FORM_VERSION,
            zod_version: crate::forms::ZOD_VERSION,
            nav_charts,
        };

        let ui = self.output_dir.join("ui");
        let src = ui.join("src");
        let lib = src.join("lib");

        let mut files = render_template_set(
            tera,
            &ctx,
            project,
            core_scaffold_templates(&ui, &src, &lib),
        )
        .await?;
        files.extend(
            render_template_set(
                tera,
                &ctx,
                project,
                test_scaffold_templates(&ui, &src, &lib),
            )
            .await?,
        );

        // Issue #299: the primitive install surface (see
        // `SHADCN_PRIMITIVES`). Gated on the ux plane so flag-off scaffold
        // output stays byte-identical.
        if ctx.has_ux_rules {
            files.extend(render_template_set(tera, &ctx, project, primitive_docs(&ui)).await?);
        }

        // Webhook UI templates (conditional on has_webhooks)
        if self.has_webhooks {
            files.extend(
                render_template_set(
                    tera,
                    &ctx,
                    project,
                    webhook_scaffold_templates(&ui, &src, ctx.entity_remote_functions),
                )
                .await?,
            );
        }

        // Integration UI templates (conditional on has_integrations)
        if self.has_integrations {
            files.extend(
                render_template_set(
                    tera,
                    &ctx,
                    project,
                    integration_scaffold_templates(&src, &lib),
                )
                .await?,
            );
        }

        // The shared server-side API helper (issue #574 S1): the ONE
        // origin/auth contract for every server-side fetch — the entity
        // +page.server.ts loads now, the IFML remote functions share the
        // identical source. Emitted if-absent so hand edits survive
        // regeneration; `$lib/server/**` is SvelteKit's server-only
        // convention (a client import is a build error), the structural
        // guardrail keeping credentials server-side.
        let server_api = lib.join("server").join("api.ts");
        if !server_api.exists() {
            files.push(GeneratedFile {
                path: server_api,
                content: crate::server_api::server_api_helper_ts().to_string(),
            });
        }

        // The TanStack form layer's shared scaffold (issue #580): the
        // form creator context + registration (Rust consts — the IFML
        // plane imports the identical spellings, so the two planes cannot
        // drift) and one field component per control kind. Emitted
        // if-absent exactly like the server helper so hand edits survive
        // regeneration; gated on the flag so flag-off trees stay
        // byte-identical.
        if project.codegen.entity_form {
            let forms_dir = lib.join("forms");
            let form_context = forms_dir.join("form-context.ts");
            if !form_context.exists() {
                files.push(GeneratedFile {
                    path: form_context,
                    content: TANSTACK_FORM_CONTEXT_TS.to_string(),
                });
            }
            let form_module = forms_dir.join("form.ts");
            if !form_module.exists() {
                files.push(GeneratedFile {
                    path: form_module,
                    content: TANSTACK_FORM_TS.to_string(),
                });
            }
            for (template, out_name) in TANSTACK_FIELD_COMPONENTS {
                let out = forms_dir.join("fields").join(out_name);
                if !out.exists() {
                    files.push(GeneratedFile {
                        path: out,
                        content: render_template_with_project(tera, template, &ctx, project)?,
                    });
                }
            }
        }

        Ok(files)
    }
}

/// The TanStack form creator context (issue #580): field components
/// consume `useFieldContext()`, form components `useFormContext()`. A
/// Rust const so both generated planes share one spelling.
pub const TANSTACK_FORM_CONTEXT_TS: &str = r#"// Generated by codegraph. DO NOT EDIT.
//
// The TanStack Form field/form contexts (issue #580): field components
// call `useFieldContext()` inside their component script; form components
// call `useFormContext()` (inside a `form.AppForm` wrapper).
import { createFormCreatorContexts } from '@tanstack/svelte-form';

export const { useFieldContext, useFormContext } = createFormCreatorContexts();
"#;

/// The TanStack form creator registration (issue #580): one field
/// component per control kind plus the form components, all consuming the
/// contexts above. A Rust const for the same one-spelling reason.
pub const TANSTACK_FORM_TS: &str = r#"// Generated by codegraph. DO NOT EDIT.
//
// The app's TanStack Form creator (issue #580): registers the field
// components (one per control kind — reachable as `<field.{Component}>`
// inside an `AppField` snippet) and the form components (reachable as
// `<form.{Component}>` on the created form).
import { createFormCreator } from '@tanstack/svelte-form';
import TextField from './fields/TextField.svelte';
import EmailField from './fields/EmailField.svelte';
import NumberField from './fields/NumberField.svelte';
import CheckboxField from './fields/CheckboxField.svelte';
import SelectField from './fields/SelectField.svelte';
import DateField from './fields/DateField.svelte';
import DateTimeField from './fields/DateTimeField.svelte';
import TextareaField from './fields/TextareaField.svelte';
import SubscribeButton from './fields/SubscribeButton.svelte';
import ResetButton from './fields/ResetButton.svelte';

export const { createAppForm } = createFormCreator({
	fieldComponents: {
		TextField,
		EmailField,
		NumberField,
		CheckboxField,
		SelectField,
		DateField,
		DateTimeField,
		TextareaField,
	},
	formComponents: {
		SubscribeButton,
		ResetButton,
	},
});
"#;

/// The field-component templates (issue #580), shadowable via
/// `--template-dir` like every template. Rendered if-absent under the
/// `entity_form` flag.
pub const TANSTACK_FIELD_COMPONENTS: &[(&str, &str)] = &[
    ("ui/forms/fields/text_field.tera", "TextField.svelte"),
    ("ui/forms/fields/email_field.tera", "EmailField.svelte"),
    ("ui/forms/fields/number_field.tera", "NumberField.svelte"),
    (
        "ui/forms/fields/checkbox_field.tera",
        "CheckboxField.svelte",
    ),
    ("ui/forms/fields/select_field.tera", "SelectField.svelte"),
    ("ui/forms/fields/date_field.tera", "DateField.svelte"),
    (
        "ui/forms/fields/datetime_field.tera",
        "DateTimeField.svelte",
    ),
    (
        "ui/forms/fields/textarea_field.tera",
        "TextareaField.svelte",
    ),
    (
        "ui/forms/fields/subscribe_button.tera",
        "SubscribeButton.svelte",
    ),
    ("ui/forms/fields/reset_button.tera", "ResetButton.svelte"),
];

async fn build_ui_domains(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    generation_order: &[GenerationEntry],
) -> Vec<UiDomain> {
    // Group generation_order entries by domain
    let mut domain_entity_map: std::collections::HashMap<String, Vec<UiNavEntity>> =
        std::collections::HashMap::new();
    for entry in generation_order {
        let stripped = config.defaults.strip_suffix(&entry.schema_title);
        let module_name = codegraph_naming::to_snake_case(&stripped);
        // Respect the entity's configured path_segment override (e.g.
        // "workers") so dashboard/nav links match the actual routes
        // emitted by the page generator (#162 phase 4 route convention).
        let path_segment = config
            .domains
            .get(&entry.domain)
            .and_then(|d| d.get_entity_config(&entry.schema_title))
            .and_then(|ec| ec.path_segment.clone())
            .unwrap_or_else(|| codegraph_naming::to_kebab_case(&stripped));
        let label = codegraph_naming::to_display_name(&stripped);

        // Collect lightweight field names + labels for i18n message keys.
        let no_immutables: Vec<String> = Vec::new();
        let fields = match super::common::collect_ui_fields(
            db,
            &entry.schema_title,
            &no_immutables,
            Some(&entry.domain),
            config,
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
    domains
}

async fn render_template_set(
    tera: &tera::Tera,
    ctx: &UiScaffoldContext,
    project: &ProjectConfig,
    templates: Vec<(&'static str, PathBuf)>,
) -> Result<Vec<GeneratedFile>> {
    let mut files = Vec::new();
    for (template, path) in templates {
        let content = render_template_with_project(tera, template, ctx, project)?;
        files.push(GeneratedFile { path, content });
    }
    Ok(files)
}

/// The shadcn primitive install surface (issue #299): one emitted
/// `PRIMITIVES.md` carrying the copy-paste `npx shadcn-svelte add` command
/// for the full [`SHADCN_PRIMITIVES`] set. Emitted only under the ux
/// plane (see the caller) so flag-off scaffold output stays
/// byte-identical.
fn primitive_docs(ui: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![("ui/scaffold/primitives_md.tera", ui.join("PRIMITIVES.md"))]
}

fn core_scaffold_templates(ui: &Path, src: &Path, lib: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("ui/scaffold/package_json.tera", ui.join("package.json")),
        ("ui/scaffold/vite_config.tera", ui.join("vite.config.ts")),
        ("ui/scaffold/env_ts.tera", src.join("env.ts")),
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
    ]
}

fn test_scaffold_templates(ui: &Path, src: &Path, lib: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
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
    ]
}

fn webhook_scaffold_templates(
    ui: &Path,
    src: &Path,
    entity_remote_functions: bool,
) -> Vec<(&'static str, PathBuf)> {
    let webhooks_dir = src
        .join("routes")
        .join("(app)")
        .join("settings")
        .join("webhooks");
    let new_dir = webhooks_dir.join("new");
    let id_dir = webhooks_dir.join("[id]");
    let edit_dir = id_dir.join("edit");

    // Webhook test files
    let webhooks_test_dir = ui.join("tests").join("e2e").join("webhooks");

    // Issue #574 S2: under the entity remote-functions flag the settings
    // pages render the load/command variants and each route gains its
    // colocated `data.remote.ts` (+ `+page.server.ts` where a load is
    // needed). Flag-off keeps the browser apiGet plane byte-identical.
    let mut files: Vec<(&'static str, PathBuf)> = if entity_remote_functions {
        vec![
            (
                "ui/scaffold/settings_webhooks_remote.tera",
                webhooks_dir.join("+page.svelte"),
            ),
            (
                "ui/scaffold/settings_webhooks_load.tera",
                webhooks_dir.join("+page.server.ts"),
            ),
            (
                "ui/scaffold/settings_webhooks_remote_ts.tera",
                webhooks_dir.join("data.remote.ts"),
            ),
            (
                "ui/scaffold/settings_webhook_form_remote.tera",
                new_dir.join("+page.svelte"),
            ),
            (
                "ui/scaffold/settings_webhook_form_remote_ts.tera",
                new_dir.join("data.remote.ts"),
            ),
            (
                "ui/scaffold/settings_webhook_detail_remote.tera",
                id_dir.join("+page.svelte"),
            ),
            (
                "ui/scaffold/settings_webhook_detail_load.tera",
                id_dir.join("+page.server.ts"),
            ),
            (
                "ui/scaffold/settings_webhook_detail_remote_ts.tera",
                id_dir.join("data.remote.ts"),
            ),
            (
                "ui/scaffold/settings_webhook_form_remote.tera",
                edit_dir.join("+page.svelte"),
            ),
            (
                "ui/scaffold/settings_webhook_edit_load.tera",
                edit_dir.join("+page.server.ts"),
            ),
            (
                "ui/scaffold/settings_webhook_form_remote_ts.tera",
                edit_dir.join("data.remote.ts"),
            ),
        ]
    } else {
        vec![
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
        ]
    };

    files.push((
        "ui/test/webhooks_crud.test.tera",
        webhooks_test_dir.join("webhooks-crud.test.ts"),
    ));
    files.push((
        "ui/test/webhooks_delivery.test.tera",
        webhooks_test_dir.join("webhooks-delivery.test.ts"),
    ));
    files
}

fn integration_scaffold_templates(src: &Path, lib: &Path) -> Vec<(&'static str, PathBuf)> {
    let settings_integrations = src
        .join("routes")
        .join("(app)")
        .join("settings")
        .join("integrations");
    let install_dir = settings_integrations.join("install").join("[catalogId]");
    let detail_dir = settings_integrations.join("[installationId]");
    let edit_dir = detail_dir.join("settings");

    vec![
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #297: the ux-rules list page's row-actions menu and
    /// truncated-cell tooltips require the two new primitives in every
    /// generated app's component set.
    #[test]
    fn shadcn_primitive_set_covers_ux_list_page_requirements() {
        for primitive in ["dropdown-menu", "tooltip"] {
            assert!(
                SHADCN_PRIMITIVES.contains(&primitive),
                "primitive {primitive:?} must be declared in SHADCN_PRIMITIVES"
            );
        }
        // Pre-existing primitives stay declared (no duplicates).
        let unique: std::collections::HashSet<_> = SHADCN_PRIMITIVES.iter().collect();
        assert_eq!(unique.len(), SHADCN_PRIMITIVES.len());
    }
}
