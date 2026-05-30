use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{EntityGenerator, GeneratedFile};
use codegraph_config::DomainConfig;

use super::querier::*;

pub struct IfmlRouteGenerator {
    output_dir: PathBuf,
}

impl IfmlRouteGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for IfmlRouteGenerator {
    fn name(&self) -> &str {
        "ifml-route"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        _domain: &str,
        _config: &DomainConfig,
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let querier = IfmlGraphQuerier::new(db);

        // Get the view container for this schema.
        // Schema title might not match view name, so we need the graph to link them.
        let view_container = match querier.get_view_container(schema_title).await {
            Ok(Some(vc)) => vc,
            _ => return Ok(vec![]), // No IFML view for this schema
        };

        let mut files = vec![];

        // Generate SvelteKit route: +page.svelte and +page.ts
        let route_name = view_container.name.to_lowercase();

        // +page.svelte
        if let Ok(content) = render_page_svelte(tera, &view_container) {
            files.push(GeneratedFile {
                path: PathBuf::from(format!("src/routes/{route_name}/+page.svelte")),
                content,
            });
        }

        // +page.ts (load function)
        if let Ok(content) = render_page_load(tera, &view_container) {
            files.push(GeneratedFile {
                path: PathBuf::from(format!("src/routes/{route_name}/+page.ts")),
                content,
            });
        }

        Ok(files)
    }
}

#[derive(Debug, Serialize)]
struct PageSvelteContext {
    name: String,
    label: String,
    components: Vec<PageComponentContext>,
    params: Vec<super::context::ParameterDef>,
}

#[derive(Debug, Serialize)]
struct PageComponentContext {
    name: String,
    component_type: String,
    entity: String,
    fields: Vec<String>,
    filter: String,
}

#[derive(Debug, Serialize)]
struct PageLoadContext {
    name: String,
    components: Vec<PageLoadComponentContext>,
}

#[derive(Debug, Serialize)]
struct PageLoadComponentContext {
    component_type: String,
    entity: String,
    route_name: String,
}

fn render_page_svelte(tera: &tera::Tera, vc: &super::context::IfmlViewContainer) -> Result<String> {
    let ctx = PageSvelteContext {
        name: vc.name.clone(),
        label: vc
            .label
            .clone()
            .unwrap_or_else(|| vc.name.clone()),
        components: vc
            .components
            .iter()
            .map(|c| PageComponentContext {
                name: c.name.clone(),
                component_type: c.component_type.clone(),
                entity: c.entity.clone().unwrap_or_default(),
                fields: c.fields.clone(),
                filter: c.filter.clone().unwrap_or_default(),
            })
            .collect(),
        params: vc.params.clone(),
    };

    if tera.get_template("ifml/page_svelte.tera").is_ok() {
        render_template(tera, "ifml/page_svelte.tera", &ctx)
    } else {
        // Fallback: generate inline without template
        Ok(generate_page_svelte_inline(vc))
    }
}

fn render_page_load(tera: &tera::Tera, vc: &super::context::IfmlViewContainer) -> Result<String> {
    let ctx = PageLoadContext {
        name: vc.name.clone(),
        components: vc
            .components
            .iter()
            .filter(|c| c.entity.is_some())
            .map(|c| PageLoadComponentContext {
                component_type: c.component_type.clone(),
                entity: c.entity.clone().unwrap_or_default(),
                route_name: c.entity.clone().unwrap_or_default().to_lowercase(),
            })
            .collect(),
    };

    if tera.get_template("ifml/page_load.tera").is_ok() {
        render_template(tera, "ifml/page_load.tera", &ctx)
    } else {
        // Fallback: generate inline without template
        Ok(generate_page_load_inline(vc))
    }
}

fn generate_page_svelte_inline(vc: &super::context::IfmlViewContainer) -> String {
    let mut s = String::new();
    s.push_str("<script lang=\"ts\">\n");
    s.push_str("  import type { PageData } from './$types';\n");
    s.push_str("  export let data: PageData;\n");
    s.push_str("</script>\n\n");

    for comp in &vc.components {
        match comp.component_type.as_str() {
            "list" => {
                s.push_str("<h1>");
                s.push_str(vc.label.as_deref().unwrap_or(&vc.name));
                s.push_str("</h1>\n<table>\n");
                s.push_str("  <thead><tr>\n");
                for field in &comp.fields {
                    s.push_str(&format!("    <th>{}</th>\n", field));
                }
                s.push_str("  </tr></thead>\n");
                s.push_str("  <tbody>\n");
                s.push_str("    {#each data.items as item}\n");
                s.push_str("      <tr>\n");
                for field in &comp.fields {
                    s.push_str(&format!("        <td>{{item.{}}}</td>\n", field));
                }
                s.push_str("      </tr>\n");
                s.push_str("    {/each}\n");
                s.push_str("  </tbody>\n");
                s.push_str("</table>\n");
            }
            "form" => {
                s.push_str("<form method=\"POST\">\n");
                for field in &comp.fields {
                    s.push_str(&format!(
                        "  <label>{field}\n    <input name=\"{field}\" />\n  </label>\n"
                    ));
                }
                s.push_str("  <button type=\"submit\">Submit</button>\n");
                s.push_str("</form>\n");
            }
            "details" => {
                s.push_str("<dl>\n");
                for field in &comp.fields {
                    s.push_str(&format!(
                        "  <dt>{field}</dt>\n  <dd>{{data.{field}}}</dd>\n"
                    ));
                }
                s.push_str("</dl>\n");
            }
            _ => {}
        }
    }

    s
}

fn generate_page_load_inline(vc: &super::context::IfmlViewContainer) -> String {
    let mut s = String::new();
    s.push_str("import type { PageLoad } from './$types';\n\n");
    s.push_str("export const load: PageLoad = async ({ params, fetch }) => {\n");

    for comp in &vc.components {
        if comp.component_type == "list" {
            if let Some(entity) = &comp.entity {
                let api_path = entity.to_lowercase();
                s.push_str(&format!(
                    "  const response = await fetch('/api/{api_path}');\n"
                ));
                s.push_str("  const items = await response.json();\n\n");
                s.push_str("  return { items };\n");
            }
        }
    }

    s.push_str("};\n");
    s
}
