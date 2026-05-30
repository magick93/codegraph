use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::generate::render_template;
use crate::generate::traits::{GeneratedFile, GlobalGenerator};
use crate::generate::GenerationEntry;
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
impl GlobalGenerator for IfmlRouteGenerator {
    fn name(&self) -> &str {
        "ifml-route"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        _config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        _tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let querier = IfmlGraphQuerier::new(db);
        let model = querier.get_ifml_model().await.map_err(|e| {
            crate::error::Error::Graph(e)
        })?;

        if model.view_containers.is_empty() {
            return Ok(vec![]);
        }

        let mut files = vec![];

        for vc in &model.view_containers {
            let route_name = vc.name.to_lowercase();

            if let Ok(content) = render_page_svelte(&vc) {
                files.push(GeneratedFile {
                    path: self.output_dir.join(format!("src/routes/{route_name}/+page.svelte")),
                    content,
                });
            }

            if let Ok(content) = render_page_load(&vc) {
                files.push(GeneratedFile {
                    path: self.output_dir.join(format!("src/routes/{route_name}/+page.ts")),
                    content,
                });
            }
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

fn render_page_svelte(vc: &super::context::IfmlViewContainer) -> Result<String> {
    Ok(generate_page_svelte_inline(vc))
}

fn render_page_load(vc: &super::context::IfmlViewContainer) -> Result<String> {
    Ok(generate_page_load_inline(vc))
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
