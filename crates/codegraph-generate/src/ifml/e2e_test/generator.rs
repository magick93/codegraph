//! The IFML e2e generator (issue #317 module split): per-view spec
//! assembly, POM emission (kernel + page classes), stale-file cleanup, and
//! the Playwright harness scaffold.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;
use crate::ProjectConfig;
use async_trait::async_trait;
use codegraph_config::{DomainConfig, IfmlComponentMappings};
use codegraph_core::traits::GraphQuerier;

use super::super::context::{IfmlComponent, IfmlViewContainer};
use super::super::querier::{IfmlGraphQuerier, IfmlQuerier};
use super::super::route_generator::denial_target;
use super::super::selectors::ComponentSelectors;
use super::pom::pom_file_set;
use super::render::{
    package_json, playwright_config, render_spec, render_ux_spec, render_workflow_spec,
};
use super::spec_payload::{UxViewTest, ViewTestSpec, WorkflowTest};
use super::ux_plans::build_ux_plans;
use codegraph_naming::to_kebab_case;

/// Global generator emitting IFML-driven Playwright E2E specs.
///
/// One spec file per view container under `{fw}/tests/ifml/{view-kebab}.spec.ts`,
/// with test kinds selected by what the model supports:
///
/// - render (always): goto the view route, expect the heading + primary
///   component testid.
/// - click-through (schema-backed API required): seed a fixture entity via the
///   API, click a row, assert the navigation-flow target URL with the
///   parameter bindings substituted.
/// - form validation negative (typed form spec with required fields): submit
///   empty, assert the input is in the invalid state.
/// - form CRUD round trip (schema-backed API + save navigation): create a
///   fixture, load the edit form, submit a modified value, assert the save
///   navigation and API persistence.
///
/// API-dependent kinds are skipped when the bound entity has no schema in the
/// graph (e.g. `ifml-generate` without `--schemas`), so a schema-less run
/// emits render tests only.
///
/// Since #317 every spec kind drives the view through the emitted POM
/// (kernel + per-view page classes under `tests/pages/`), which is emitted
/// on the same spec-infra gate: whenever any spec is emitted.
pub struct IfmlE2eTestGenerator {
    output_dir: PathBuf,
    framework: String,
    pub(super) mappings: Option<IfmlComponentMappings>,
}

impl IfmlE2eTestGenerator {
    pub fn new(output_dir: &Path, framework: &str) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            framework: framework.to_string(),
            mappings: None,
        }
    }

    pub fn with_mappings(mut self, mappings: Option<IfmlComponentMappings>) -> Self {
        self.mappings = mappings;
        self
    }

    pub(super) fn selectors(
        &self,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
    ) -> ComponentSelectors {
        ComponentSelectors::for_component(self.mappings.as_ref(), &vc.name, c)
    }
}

#[async_trait]
impl GlobalGenerator for IfmlE2eTestGenerator {
    fn name(&self) -> &str {
        "ifml-e2e-test"
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        _tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        if self.framework != "svelte" {
            return Ok(vec![]);
        }

        let querier = IfmlGraphQuerier::new(db);
        let model = querier
            .get_ifml_model()
            .await
            .map_err(crate::error::Error::Graph)?;
        if model.view_containers.is_empty() {
            return Ok(vec![]);
        }

        let mut specs = Vec::new();
        let mut workflow_specs: Vec<(String, Vec<WorkflowTest>)> = Vec::new();
        // ux-rules plane (issue #303): one plan per bound entity plus one
        // ux spec per view with an eligible fallback list. Flag off → no
        // plans, no ux specs (byte-identical output).
        let ux_rules = project.ux.ux.as_ref();
        let ux_plans = match ux_rules {
            Some(rules) => build_ux_plans(db, config, &model, rules).await?,
            None => HashMap::new(),
        };
        let mut ux_specs: Vec<(String, UxViewTest)> = Vec::new();
        let human_actors: Vec<String> = match &model.policy {
            Some(_) => db
                .get_actors()
                .await
                .map_err(crate::error::Error::Graph)?
                .into_iter()
                .filter(|actor| actor.kind.as_deref() != Some("agent"))
                .map(|actor| actor.name)
                .collect(),
            None => Vec::new(),
        };
        let denial = denial_target(&model);
        for vc in &model.view_containers {
            let mut spec = self
                .build_view_spec(db, config, &project.identity.api_version, &model, vc)
                .await;
            if let Some(policy) = &model.policy {
                spec.personas = self.build_persona_tests(policy, &human_actors, vc, &denial);
            }
            if has_tests(&spec) {
                specs.push(spec);
            }
            if let Some(rules) = ux_rules {
                if let Some(ux_test) = self
                    .build_view_ux_test(
                        db,
                        config,
                        &project.identity.api_version,
                        vc,
                        rules,
                        &ux_plans,
                    )
                    .await
                {
                    ux_specs.push((vc.name.clone(), ux_test));
                }
            }
            let workflow_tests = self
                .build_view_workflow_tests(db, config, &project.identity.api_version, vc)
                .await;
            if !workflow_tests.is_empty() {
                workflow_specs.push((vc.name.clone(), workflow_tests));
            }
        }
        if specs.is_empty() && workflow_specs.is_empty() {
            return Ok(vec![]);
        }

        // Remove stale per-view spec files for views no longer in the model,
        // stale workflow specs whose view no longer carries workflow
        // tests (e.g. the workflow config or a mapped collection changed
        // across regenerations into the same root), stale ux specs
        // (issue #303) whose view lost its ux eligibility (flag off, view
        // removed, or a mapping now replaces the fallback), and stale POM
        // page classes (#317) for removed views.
        let active_specs: HashSet<String> = model
            .view_containers
            .iter()
            .map(|vc| format!("{}.spec.ts", to_kebab_case(&vc.name)))
            .collect();
        let active_workflow_specs: HashSet<String> = workflow_specs
            .iter()
            .map(|(view_name, _)| format!("{}.workflow.spec.ts", to_kebab_case(view_name)))
            .collect();
        let active_ux_specs: HashSet<String> = ux_specs
            .iter()
            .map(|(view_name, _)| format!("{}.ux.spec.ts", to_kebab_case(view_name)))
            .collect();
        let active_pages: HashSet<String> = model
            .view_containers
            .iter()
            .map(|vc| format!("{}-page.ts", to_kebab_case(&vc.name)))
            .collect();
        let specs_dir = self.output_dir.join("tests").join("ifml");
        if specs_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&specs_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if !name.ends_with(".spec.ts") {
                        continue;
                    }
                    if name.strip_suffix(".workflow.spec.ts").is_some() {
                        if !active_workflow_specs.contains(name) {
                            let _ = std::fs::remove_file(&path);
                        }
                        continue;
                    }
                    if name.strip_suffix(".ux.spec.ts").is_some() {
                        if !active_ux_specs.contains(name) {
                            let _ = std::fs::remove_file(&path);
                        }
                        continue;
                    }
                    if !active_specs.contains(name) {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
        let pages_dir = self.output_dir.join("tests").join("pages");
        if pages_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&pages_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    // Only page classes are view-scoped; `support/` holds the
                    // stable kernel.
                    if name.ends_with("-page.ts") && !active_pages.contains(name) {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }

        let mut files: Vec<GeneratedFile> = specs
            .iter()
            .map(|spec| GeneratedFile {
                path: self
                    .output_dir
                    .join("tests")
                    .join("ifml")
                    .join(format!("{}.spec.ts", to_kebab_case(&spec.view_name))),
                content: render_spec(spec),
            })
            .collect();

        for (view_name, workflow_tests) in &workflow_specs {
            if let Some(vc) = model
                .view_containers
                .iter()
                .find(|vc| &vc.name == view_name)
            {
                files.push(GeneratedFile {
                    path: self
                        .output_dir
                        .join("tests")
                        .join("ifml")
                        .join(format!("{}.workflow.spec.ts", to_kebab_case(view_name))),
                    content: render_workflow_spec(vc, workflow_tests),
                });
            }
        }

        for (view_name, ux_test) in &ux_specs {
            files.push(GeneratedFile {
                path: self
                    .output_dir
                    .join("tests")
                    .join("ifml")
                    .join(format!("{}.ux.spec.ts", to_kebab_case(view_name))),
                content: render_ux_spec(view_name, ux_test),
            });
        }

        // POM infra (issue #317): kernel + one page class per view
        // container, emitted on the same spec-infra gate as the specs.
        let spec_by_view: HashMap<&str, &ViewTestSpec> = specs
            .iter()
            .map(|spec| (spec.view_name.as_str(), spec))
            .collect();
        let views: Vec<(&IfmlViewContainer, Option<&ViewTestSpec>)> = model
            .view_containers
            .iter()
            .map(|vc| (vc, spec_by_view.get(vc.name.as_str()).copied()))
            .collect();
        for (path, content) in
            pom_file_set(config, self.mappings.as_ref(), &views, ux_rules, &ux_plans)
        {
            files.push(GeneratedFile {
                path: self.output_dir.join(path),
                content,
            });
        }

        let config_path = self.output_dir.join("playwright.config.ts");
        if !config_path.exists() {
            files.push(GeneratedFile {
                path: config_path,
                content: playwright_config().to_string(),
            });
        }
        let package_path = self.output_dir.join("package.json");
        if !package_path.exists() {
            files.push(GeneratedFile {
                path: package_path,
                content: package_json().to_string(),
            });
        }

        Ok(files)
    }
}

fn has_tests(spec: &ViewTestSpec) -> bool {
    spec.render.is_some()
        || !spec.personas.is_empty()
        || !spec.click_throughs.is_empty()
        || !spec.validations.is_empty()
        || !spec.round_trips.is_empty()
}
