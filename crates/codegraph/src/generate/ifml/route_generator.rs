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
    framework: String,
    output_paths: super::output_paths::OutputPaths,
}

impl IfmlRouteGenerator {
    pub fn new(output_dir: &Path, framework: &str) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            framework: framework.to_string(),
            output_paths: super::output_paths::OutputPaths::for_framework(framework),
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
        tera: &tera::Tera,
    ) -> Result<Vec<GeneratedFile>> {
        let querier = IfmlGraphQuerier::new(db);
        let model = querier.get_ifml_model().await.map_err(|e| {
            crate::error::Error::Graph(e)
        })?;

        if model.view_containers.is_empty() {
            return Ok(vec![]);
        }

        let mut files = vec![];

        let page_template = format!("ifml/{}/page.tera", self.framework);
        let load_template = format!("ifml/{}/page_load.tera", self.framework);

        for vc in &model.view_containers {
            if let Ok(content) = render_page_svelte(&vc, tera, &page_template) {
                files.push(GeneratedFile {
                    path: self.output_dir.join((self.output_paths.route_page)(&vc.name)),
                    content,
                });
            }

            if let Some(ref route_load_fn) = self.output_paths.route_load {
                if let Ok(content) = render_page_load(&vc, tera, &load_template) {
                    files.push(GeneratedFile {
                        path: self.output_dir.join(route_load_fn(&vc.name)),
                        content,
                    });
                }
            }
        }

        Ok(files)
    }
}

#[derive(Debug, Serialize)]
pub struct PageSvelteContext {
    name: String,
    label: String,
    components: Vec<PageComponentContext>,
    params: Vec<super::context::ParameterDef>,
}

#[derive(Debug, Serialize)]
pub struct PageComponentContext {
    name: String,
    component_type: String,
    entity: String,
    fields: Vec<String>,
    filter: String,
}

#[derive(Debug, Serialize)]
pub struct PageLoadContext {
    name: String,
    components: Vec<PageLoadComponentContext>,
}

#[derive(Debug, Serialize)]
pub struct PageLoadComponentContext {
    component_type: String,
    entity: String,
    route_name: String,
}

fn render_page_svelte(vc: &super::context::IfmlViewContainer, tera: &tera::Tera, template: &str) -> Result<String> {
    let ctx = PageSvelteContext {
        name: vc.name.clone(),
        label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
        components: vc.components.iter().map(|c| PageComponentContext {
            name: c.name.clone(),
            component_type: c.component_type.clone(),
            entity: c.entity.clone().unwrap_or_default(),
            fields: c.fields.clone(),
            filter: c.filter.clone().unwrap_or_default(),
        }).collect(),
        params: vc.params.clone(),
    };
    render_template(tera, template, &ctx)
}

fn render_page_load(vc: &super::context::IfmlViewContainer, tera: &tera::Tera, template: &str) -> Result<String> {
    let ctx = PageLoadContext {
        name: vc.name.clone(),
        components: vc.components.iter().map(|c| {
            let route_name = c.entity.as_ref().map(|e| e.to_lowercase()).unwrap_or_default();
            PageLoadComponentContext {
                component_type: c.component_type.clone(),
                entity: c.entity.clone().unwrap_or_default(),
                route_name,
            }
        }).collect(),
    };
    render_template(tera, template, &ctx)
}
