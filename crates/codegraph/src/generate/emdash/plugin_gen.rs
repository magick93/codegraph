//! `emdash_plugin` — per-domain EmDash plugin package generator.
//!
//! For every domain declared in `plugins.toml`, emits a complete TypeScript
//! plugin package (descriptor, sandbox plugin, Block Kit admin surface,
//! plugin manifest, packaging) plus the domain's public site pages and a
//! generated Playwright CRUD journey — reproducing the generic behavior of
//! the hand-written `packages/emdash-community-events` reference.

use std::collections::BTreeMap;
use std::path::PathBuf;

use async_trait::async_trait;
use codegraph_config::DomainConfig;
use codegraph_core::traits::GraphQuerier;
use codegraph_type_contracts::RefClassificationKind;

use crate::error::Result;
use crate::generate::domain_model::build_entity_model;
use crate::generate::emdash::config::EmdashPluginsConfig;
use crate::generate::emdash::context::{
    build_entity_context, build_package_context, CodelistOption, EmdashPackageContext,
};
use crate::generate::playwright::ts_entity_gen::expand_vo_fields;
use crate::generate::render_template_with_project;
use crate::generate::traits::{DomainGenerator, GeneratedFile};
use crate::generate::ProjectConfig;

/// Template render wrapper: the package context is exposed to templates as
/// `p` (e.g. `{{ p.plugin_id }}`), keeping loop variables (`e`, `f`, ...)
/// unambiguous.
#[derive(serde::Serialize)]
struct RenderContext<'a> {
    p: &'a EmdashPackageContext,
}

/// Per-page site context: which entity backs the public list page, which
/// backs the detail page, and which (if any) carries the bespoke public
/// submission form. Entities are stored sorted, so explicit selection beats
/// positional access. Templates reference this as `s` (`s.p.plugin_id`,
/// `s.list.title_field`, ...).
#[derive(serde::Serialize)]
struct SiteRender<'a> {
    s: SiteContext<'a>,
}

#[derive(serde::Serialize)]
struct SiteContext<'a> {
    p: &'a EmdashPackageContext,
    list: &'a crate::generate::emdash::context::EntityCtx,
    detail: &'a crate::generate::emdash::context::EntityCtx,
    submit: Option<&'a crate::generate::emdash::context::EntityCtx>,
}

/// Render with `project` injected, preserving Tera's full error source chain
/// (its `Display` alone only says "Failed to render").
fn render_site_template<T: serde::Serialize>(
    tera: &tera::Tera,
    name: &str,
    ctx: &T,
    project: &ProjectConfig,
) -> Result<String> {
    let mut context = tera::Context::from_serialize(ctx)
        .map_err(|e| crate::error::Error::Template(e.to_string()))?;
    context.insert("project", project);
    tera.render(name, &context).map_err(|e| {
        let mut msg = format!("{name}: {e}");
        let mut src = std::error::Error::source(&e);
        while let Some(cause) = src {
            msg.push_str(&format!(" | caused by: {cause}"));
            src = cause.source();
        }
        crate::error::Error::Template(msg)
    })
}

pub struct EmdashPluginGenerator {
    output_dir: PathBuf,
    plugins: EmdashPluginsConfig,
}

impl EmdashPluginGenerator {
    pub fn new(output_dir: PathBuf, plugins: EmdashPluginsConfig) -> Self {
        Self {
            output_dir,
            plugins,
        }
    }

    /// Resolve codelist options per top-level property name so selects render
    /// `{value,label}[]` from the graph codelists (enum + display names).
    async fn resolve_codelist_options(
        db: &dyn GraphQuerier,
        properties: &[codegraph_core::types::PropertyNode],
    ) -> BTreeMap<String, Vec<CodelistOption>> {
        let mut out = BTreeMap::new();
        for prop in properties {
            let Some(kind) = prop.effective_kind() else {
                continue;
            };
            if !matches!(
                kind,
                RefClassificationKind::CodelistReference
                    | RefClassificationKind::CodelistCheck
                    | RefClassificationKind::InlineEnum
            ) {
                continue;
            }
            let Some(ref target) = prop.ref_target else {
                continue;
            };
            let filename = target.rsplit('/').next().unwrap_or(target);
            let cl_name = filename
                .strip_suffix(".json#")
                .or_else(|| filename.strip_suffix(".json"))
                .unwrap_or(filename);
            let values = match db.get_enum_values(cl_name).await {
                Ok(v) if !v.is_empty() => v,
                _ => continue,
            };
            let options = values
                .into_iter()
                .map(|v| CodelistOption {
                    label: v.display_name.unwrap_or_else(|| v.value.clone()),
                    value: v.value,
                })
                .collect();
            out.insert(prop.name.clone(), options);
        }
        out
    }

    async fn build_entities(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        config: &DomainConfig,
        project: &ProjectConfig,
    ) -> Result<Vec<crate::generate::emdash::context::EntityCtx>> {
        let plugin_cfg = self.plugins.plugins.get(domain).ok_or_else(|| {
            crate::error::Error::Config(format!(
                "emdash_plugin: no plugins.toml entry for domain \"{domain}\""
            ))
        })?;

        let mut entities = Vec::new();
        for (entity_key, entity_cfg) in &plugin_cfg.entities {
            let Some(title) = entity_titles.iter().find(|t| *t == entity_key) else {
                tracing::warn!(
                    domain = %domain,
                    entity = %entity_key,
                    "emdash_plugin: entity not in generation order for this domain — skipping"
                );
                continue;
            };
            let model =
                build_entity_model(db, title, domain, config, &project.atproto_authority).await?;
            let properties = db.get_properties_in_domain(title, domain).await?;
            let fields = expand_vo_fields(db, title, &model.fields, &properties).await?;
            let codelists = Self::resolve_codelist_options(db, &properties).await;
            entities.push(build_entity_context(
                entity_key,
                domain,
                &model.table_name,
                entity_cfg,
                &fields,
                &codelists,
                &model.operations,
            ));
        }
        Ok(entities)
    }
}

#[async_trait]
impl DomainGenerator for EmdashPluginGenerator {
    fn name(&self) -> &str {
        "emdash_plugin"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        domain: &str,
        entity_titles: &[String],
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let Some(plugin_cfg) = self.plugins.plugins.get(domain) else {
            // Not an EmDash-plugin domain — emit nothing.
            return Ok(Vec::new());
        };

        let entities = self
            .build_entities(db, domain, entity_titles, config, project)
            .await?;
        let ctx: EmdashPackageContext = build_package_context(domain, plugin_cfg, entities);
        if ctx.entities.is_empty() {
            tracing::warn!(
                domain = %domain,
                "emdash_plugin: no configured entities resolved — emitting package skeleton only"
            );
        }

        let root = super::emdash_package_root(&self.output_dir, domain);
        let src = root.join("src");

        let mut files = vec![
            GeneratedFile {
                path: root.join("emdash-plugin.jsonc"),
                content: render_template_with_project(
                    tera,
                    "emdash/plugin_jsonc.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: root.join("package.json"),
                content: render_template_with_project(
                    tera,
                    "emdash/package_json.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: root.join("tsconfig.json"),
                content: render_template_with_project(
                    tera,
                    "emdash/tsconfig_json.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: root.join("types").join("node").join("package.json"),
                content: render_template_with_project(
                    tera,
                    "emdash/types_node_package.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: root.join("types").join("node").join("index.d.ts"),
                content: render_template_with_project(
                    tera,
                    "emdash/types_node_index.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: src.join("index.ts"),
                content: render_template_with_project(
                    tera,
                    "emdash/index_ts.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: src.join("settings.ts"),
                content: render_template_with_project(
                    tera,
                    "emdash/settings_ts.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: src.join("plugin.ts"),
                content: render_template_with_project(
                    tera,
                    "emdash/plugin_ts.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
            GeneratedFile {
                path: src.join("admin.ts"),
                content: render_template_with_project(
                    tera,
                    "emdash/admin_ts.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            },
        ];

        // Public site pages (list + detail) under the site pages root.
        // Only meaningful with at least one configured entity; the detail
        // page additionally requires an entity with a public detail route.
        if !ctx.entities.is_empty() {
            let pages_root = super::emdash_site_pages_root_with_base(
                &self.output_dir,
                &project.emdash_site_pages_base,
            );
            let list_entity = ctx
                .entities
                .iter()
                .find(|e| e.public_list.as_ref().and_then(|pl| pl.home).unwrap_or(false))
                .or_else(|| ctx.entities.iter().find(|e| e.public_list.is_some()))
                .unwrap_or(&ctx.entities[0]);
            files.push(GeneratedFile {
                path: pages_root.join(format!("{}.astro", ctx.domain_kebab)),
                content: render_site_template(
                    tera,
                    "emdash/site_list.tera",
                    &SiteRender {
                        s: SiteContext {
                            p: &ctx,
                            list: list_entity,
                            detail: &ctx.entities[0],
                            submit: None,
                        },
                    },
                    project,
                )?,
            });
            if let Some(detail_entity) = ctx.entities.iter().find(|e| e.public_detail.is_some()) {
                let submit_entity = ctx.entities.iter().find(|e| e.public_submit.is_some());
                files.push(GeneratedFile {
                    path: pages_root.join(&ctx.domain_kebab).join("[id].astro"),
                    content: render_site_template(
                        tera,
                        "emdash/site_detail.tera",
                        &SiteRender {
                            s: SiteContext {
                                p: &ctx,
                                list: list_entity,
                                detail: detail_entity,
                                submit: submit_entity,
                            },
                        },
                        project,
                    )?,
                });
            } else {
                tracing::debug!(
                    domain = %domain,
                    "emdash_plugin: no public_detail entity configured — skipping detail page"
                );
            }

            // Generated Playwright CRUD journey under the site e2e root.
            let e2e_root = super::emdash_site_e2e_root_with_base(
                &self.output_dir,
                &project.emdash_site_e2e_base,
            );
            files.push(GeneratedFile {
                path: e2e_root
                    .join("generated")
                    .join(format!("{}-crud.spec.ts", ctx.domain_kebab)),
                content: render_template_with_project(
                    tera,
                    "emdash/e2e_spec.tera",
                    &RenderContext { p: &ctx },
                    project,
                )?,
            });
        }

        Ok(files)
    }
}
