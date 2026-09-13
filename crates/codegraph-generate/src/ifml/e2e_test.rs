use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::{DomainConfig, IfmlComponentMappings};
use codegraph_core::traits::GraphQuerier;
use codegraph_ifml_dsl::{ComponentSpec, FormSpec};

use crate::error::Result;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;

use super::api_paths::{id_param_from, resolve_entity_api, ResolvedApi};
use super::context::{IfmlAction, IfmlComponent, IfmlModel, IfmlViewContainer};
use super::querier::{IfmlGraphQuerier, IfmlQuerier};

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
pub struct IfmlE2eTestGenerator {
    output_dir: PathBuf,
    framework: String,
    mappings: Option<IfmlComponentMappings>,
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

    fn selectors(&self, vc: &IfmlViewContainer, c: &IfmlComponent) -> ComponentSelectors {
        let kind = component_kind(c);
        if let Some(mapped) = self
            .mappings
            .as_ref()
            .and_then(|m| m.resolve(&vc.name, &c.name, &c.component_type, &kind))
        {
            return ComponentSelectors {
                root: mapped
                    .testid("root")
                    .or_else(|| mapped.testid("form"))
                    .map(str::to_string),
                row: mapped.testid("row").map(str::to_string),
                form: mapped.testid("form").map(str::to_string),
                submit: mapped.testid("submit").map(str::to_string),
            };
        }
        if is_collection(c) {
            ComponentSelectors {
                root: Some(format!("{}-table", c.name)),
                row: Some(format!("{}-row", c.name)),
                form: None,
                submit: None,
            }
        } else if is_form(c) {
            ComponentSelectors {
                root: Some(format!("{}-form", c.name)),
                row: None,
                form: Some(format!("{}-form", c.name)),
                submit: Some(format!("{}-submit", c.name)),
            }
        } else if is_details(c) {
            ComponentSelectors {
                root: Some(format!("{}-details", c.name)),
                row: None,
                form: None,
                submit: None,
            }
        } else {
            ComponentSelectors::default()
        }
    }

    async fn build_view_spec(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        model: &IfmlModel,
        vc: &IfmlViewContainer,
    ) -> ViewTestSpec {
        let id_param = id_param_from(&vc.params);
        let label = vc.label.clone().unwrap_or_else(|| vc.name.clone());
        let route = view_route(&vc.name);
        let mut spec = ViewTestSpec {
            view_name: vc.name.clone(),
            label,
            route: route.clone(),
            render: None,
            click_throughs: Vec::new(),
            validations: Vec::new(),
            round_trips: Vec::new(),
        };

        if id_param.is_none() {
            spec.render = self.build_render_test(vc);
        }

        for edge in &model.navigation_edges {
            if edge.source_container != vc.name {
                continue;
            }
            if let Some(test) = self
                .build_click_through(db, config, api_version, vc, edge)
                .await
            {
                spec.click_throughs.push(test);
            }
        }

        for c in vc.components.iter().filter(|c| is_form(c)) {
            if let Some(test) = self
                .build_validation_test(db, config, api_version, vc, c, id_param.as_deref())
                .await
            {
                spec.validations.push(test);
            }
            if let Some(test) = self
                .build_round_trip_test(db, config, api_version, vc, c, id_param.as_deref())
                .await
            {
                spec.round_trips.push(test);
            }
        }

        spec
    }

    fn build_render_test(&self, vc: &IfmlViewContainer) -> Option<RenderTest> {
        let (_component, selectors, is_collection) = primary_component(vc, self)?;
        Some(RenderTest {
            label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
            route: view_route(&vc.name),
            primary_testid: selectors.root?,
            assert_heading: is_collection,
        })
    }

    async fn build_click_through(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        edge: &super::context::NavigationEdge,
    ) -> Option<ClickThroughTest> {
        let component_name = edge.source_component.as_deref()?;
        let component = vc
            .components
            .iter()
            .find(|c| c.name == component_name)
            .filter(|c| is_collection(c))?;
        let selectors = self.selectors(vc, component);
        let row_testid = selectors.row.clone()?;

        let entity = component.entity.as_deref()?;
        let api = schema_backed_api(db, config, entity, api_version).await?;
        if !api.has_create {
            return None;
        }

        Some(ClickThroughTest {
            title: format!(
                "{} navigates to {}",
                edge.source_event, edge.target_container
            ),
            source_route: view_route(&vc.name),
            row_testid,
            fixture: Fixture {
                base_path: api.base_path.clone(),
                entries: fixture_entries(component),
            },
            target_pattern: url_pattern(
                &view_route(&edge.target_container),
                &edge.parameter_binding,
            ),
        })
    }

    async fn build_validation_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
        id_param: Option<&str>,
    ) -> Option<ValidationTest> {
        let selectors = self.selectors(vc, c);
        let form_testid = selectors.form.clone()?;
        let submit_testid = selectors.submit.clone()?;
        let form = form_spec(c)?;
        let first_required = form
            .fields
            .iter()
            .find(|f| f.required)
            .map(|f| f.name.clone())?;

        let fixture = if id_param.is_some() {
            let entity = c.entity.as_deref()?;
            let api = schema_backed_api(db, config, entity, api_version).await?;
            if !api.has_create || !api.has_read {
                return None;
            }
            Some(Fixture {
                base_path: api.base_path,
                entries: fixture_entries(c),
            })
        } else {
            None
        };

        Some(ValidationTest {
            route: view_route(&vc.name),
            id_param: id_param.map(str::to_string),
            form_testid,
            submit_testid,
            field: first_required,
            fixture,
        })
    }

    async fn build_round_trip_test(
        &self,
        db: &dyn GraphQuerier,
        config: &DomainConfig,
        api_version: &str,
        vc: &IfmlViewContainer,
        c: &IfmlComponent,
        id_param: Option<&str>,
    ) -> Option<RoundTripTest> {
        let id_param = id_param?;
        let selectors = self.selectors(vc, c);
        let form_testid = selectors.form.clone()?;
        let submit_testid = selectors.submit.clone()?;
        let form = form_spec(c)?;

        let entity = c.entity.as_deref()?;
        let api = schema_backed_api(db, config, entity, api_version).await?;
        if !api.has_create || !api.has_read || !api.has_update {
            return None;
        }

        let (field, original, updated) = modifiable_field(c, form)?;
        let save = save_navigation_target(vc, c)?;

        Some(RoundTripTest {
            route: view_route(&vc.name),
            id_param: id_param.to_string(),
            form_testid,
            submit_testid,
            field,
            original,
            updated,
            fixture: Fixture {
                base_path: api.base_path,
                entries: fixture_entries(c),
            },
            target_pattern: url_pattern(&view_route(&save.0), &save.1),
        })
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
        for vc in &model.view_containers {
            let spec = self
                .build_view_spec(db, config, &project.api_version, &model, vc)
                .await;
            if has_tests(&spec) {
                specs.push(spec);
            }
        }
        if specs.is_empty() {
            return Ok(vec![]);
        }

        let mut files: Vec<GeneratedFile> = specs
            .iter()
            .map(|spec| GeneratedFile {
                path: self.output_dir.join("tests").join("ifml").join(format!(
                    "{}.spec.ts",
                    codegraph_naming::to_kebab_case(&spec.view_name)
                )),
                content: render_spec(spec),
            })
            .collect();

        let config_path = self.output_dir.join("playwright.config.ts");
        if !config_path.exists() {
            files.push(GeneratedFile {
                path: config_path,
                content: PLAYWRIGHT_CONFIG.to_string(),
            });
        }
        let package_path = self.output_dir.join("package.json");
        if !package_path.exists() {
            files.push(GeneratedFile {
                path: package_path,
                content: PACKAGE_JSON.to_string(),
            });
        }

        Ok(files)
    }
}

// ── Test models ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ViewTestSpec {
    pub view_name: String,
    pub label: String,
    pub route: String,
    pub render: Option<RenderTest>,
    pub click_throughs: Vec<ClickThroughTest>,
    pub validations: Vec<ValidationTest>,
    pub round_trips: Vec<RoundTripTest>,
}

#[derive(Debug)]
pub struct RenderTest {
    pub label: String,
    pub route: String,
    pub primary_testid: String,
    pub assert_heading: bool,
}

#[derive(Debug)]
pub struct ClickThroughTest {
    pub title: String,
    pub source_route: String,
    pub row_testid: String,
    pub fixture: Fixture,
    pub target_pattern: String,
}

#[derive(Debug)]
pub struct ValidationTest {
    pub route: String,
    pub id_param: Option<String>,
    pub form_testid: String,
    pub submit_testid: String,
    pub field: String,
    pub fixture: Option<Fixture>,
}

#[derive(Debug)]
pub struct RoundTripTest {
    pub route: String,
    pub id_param: String,
    pub form_testid: String,
    pub submit_testid: String,
    pub field: String,
    pub original: String,
    pub updated: String,
    pub fixture: Fixture,
    pub target_pattern: String,
}

#[derive(Debug)]
pub struct Fixture {
    pub base_path: String,
    pub entries: Vec<(String, String)>,
}

impl Fixture {
    fn data_literal(&self) -> String {
        let inner = self
            .entries
            .iter()
            .map(|(k, v)| format!("'{k}': {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {inner} }}")
    }
}

#[derive(Debug, Default)]
pub struct ComponentSelectors {
    root: Option<String>,
    row: Option<String>,
    form: Option<String>,
    submit: Option<String>,
}

// ── Spec rendering ───────────────────────────────────────────────────────────

const PLAYWRIGHT_CONFIG: &str = r#"// Generated by codegraph. DO NOT EDIT.

import { defineConfig } from '@playwright/test';

export default defineConfig({
	testDir: './tests',
	use: {
		baseURL: process.env.BASE_URL ?? 'http://localhost:5173',
	},
});
"#;

const PACKAGE_JSON: &str = r#"{
  "name": "ifml-e2e-tests",
  "private": true,
  "scripts": {
    "test:e2e": "playwright test"
  },
  "devDependencies": {
    "@playwright/test": "^1.49.1",
    "typescript": "^5.7.2"
  }
}
"#;

fn render_spec(spec: &ViewTestSpec) -> String {
    let mut s = String::new();
    s.push_str("// Generated by codegraph. DO NOT EDIT.\n");
    s.push_str(&format!(
        "// IFML Playwright E2E tests for view {}.\n\n",
        spec.view_name
    ));
    s.push_str("import { test, expect } from '@playwright/test';\n\n");
    s.push_str(&format!(
        "test.describe('{}', () => {{\n",
        js_string(&spec.label)
    ));

    if let Some(render) = &spec.render {
        s.push_str(&format!(
            "\ttest('renders {}', async ({{ page }}) => {{\n",
            js_string(&render.label)
        ));
        s.push_str(&format!("\t\tawait page.goto('{}');\n", render.route));
        if render.assert_heading {
            s.push_str(&format!(
                "\t\tawait expect(page.getByRole('heading', {{ name: '{}' }})).toBeVisible();\n",
                js_string(&render.label)
            ));
        }
        s.push_str(&format!(
            "\t\tawait expect(page.getByTestId('{}')).toBeVisible();\n",
            render.primary_testid
        ));
        s.push_str("\t});\n\n");
    }

    for flow in &spec.click_throughs {
        s.push_str(&format!(
            "\ttest('{}', async ({{ page, request }}) => {{\n",
            js_string(&flow.title)
        ));
        s.push_str(&format!(
            "\t\tconst fixture = await request.post('{}', {{ data: {} }});\n",
            flow.fixture.base_path,
            flow.fixture.data_literal()
        ));
        s.push_str("\t\texpect(fixture.ok(), 'fixture entity should be created').toBeTruthy();\n");
        s.push_str(&format!("\t\tawait page.goto('{}');\n", flow.source_route));
        s.push_str(&format!(
            "\t\tawait page.getByTestId('{}').first().click();\n",
            flow.row_testid
        ));
        s.push_str(&format!(
            "\t\tawait page.waitForURL(new RegExp('{}'));\n",
            js_string(&flow.target_pattern)
        ));
        s.push_str("\t});\n\n");
    }

    for validation in &spec.validations {
        let fixtures = validation.fixture.is_some();
        s.push_str(&format!(
            "\ttest('form validation blocks empty submit', async ({{ page{}}}) => {{\n",
            if fixtures { ", request " } else { " " }
        ));
        if let Some(fixture) = &validation.fixture {
            s.push_str(&format!(
                "\t\tconst created = await (await request.post('{}', {{ data: {} }})).json();\n",
                fixture.base_path,
                fixture.data_literal()
            ));
            s.push_str(&format!(
                "\t\tawait page.goto(`{}?{}=${{created.id}}`);\n",
                validation.route,
                validation.id_param.as_deref().unwrap_or("id")
            ));
        } else {
            s.push_str(&format!("\t\tawait page.goto('{}');\n", validation.route));
        }
        s.push_str(&format!(
            "\t\tawait page.getByTestId('{}').click();\n",
            validation.submit_testid
        ));
        s.push_str(&format!(
            "\t\tawait expect(page.getByTestId('{}').locator('[name=\"{}\"]')).toBeInvalid();\n",
            validation.form_testid, validation.field
        ));
        s.push_str("\t});\n\n");
    }

    for round_trip in &spec.round_trips {
        s.push_str("\ttest('form round trip persists changes', async ({ page, request }) => {\n");
        s.push_str(&format!(
            "\t\tconst created = await (await request.post('{}', {{ data: {} }})).json();\n",
            round_trip.fixture.base_path,
            round_trip.fixture.data_literal()
        ));
        s.push_str(&format!(
            "\t\tawait page.goto(`{}?{}=${{created.id}}`);\n",
            round_trip.route, round_trip.id_param
        ));
        s.push_str(&format!(
            "\t\tconst form = page.getByTestId('{}');\n",
            round_trip.form_testid
        ));
        s.push_str(&format!(
            "\t\tawait expect(form.locator('[name=\"{}\"]')).not.toHaveValue('');\n",
            round_trip.field
        ));
        s.push_str(&format!(
            "\t\tawait form.locator('[name=\"{}\"]').fill('{}');\n",
            round_trip.field, round_trip.updated
        ));
        s.push_str(&format!(
            "\t\tawait page.getByTestId('{}').click();\n",
            round_trip.submit_testid
        ));
        s.push_str(&format!(
            "\t\tawait page.waitForURL(new RegExp('{}'));\n",
            js_string(&round_trip.target_pattern)
        ));
        s.push_str(&format!(
            "\t\tconst persisted = await (await request.get(`{}/${{created.id}}`)).json();\n",
            round_trip.fixture.base_path
        ));
        s.push_str(&format!(
            "\t\texpect(persisted.{}).toBe('{}');\n",
            round_trip.field, round_trip.updated
        ));
        s.push_str("\t});\n\n");
    }

    while s.ends_with("\n\n") {
        s.pop();
    }
    s.push_str("});\n");
    s
}

// ── Model helpers ────────────────────────────────────────────────────────────

/// The URL route for a view, matching the route generator's svelte output
/// (`/{view-name-lowercase}`).
fn view_route(name: &str) -> String {
    format!("/{}", name.to_lowercase())
}

fn has_tests(spec: &ViewTestSpec) -> bool {
    spec.render.is_some()
        || !spec.click_throughs.is_empty()
        || !spec.validations.is_empty()
        || !spec.round_trips.is_empty()
}

fn component_kind(c: &IfmlComponent) -> String {
    match c.spec {
        Some(ComponentSpec::Table(_)) => "table".to_string(),
        Some(ComponentSpec::Form(_)) => "form".to_string(),
        Some(ComponentSpec::Chart(_)) => "chart".to_string(),
        None => c.component_type.clone(),
    }
}

fn is_collection(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Table(_)))
        || c.component_type == "list"
        || c.component_type == "table"
}

fn is_form(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Form(_))) || c.component_type == "form"
}

fn is_details(c: &IfmlComponent) -> bool {
    c.component_type == "details"
}

fn form_spec(c: &IfmlComponent) -> Option<&FormSpec> {
    match &c.spec {
        Some(ComponentSpec::Form(form)) => Some(form),
        _ => None,
    }
}

/// First collection component, else first component with a testable root.
fn primary_component<'a>(
    vc: &'a IfmlViewContainer,
    gen: &IfmlE2eTestGenerator,
) -> Option<(&'a IfmlComponent, ComponentSelectors, bool)> {
    let mut fallback = None;
    for c in &vc.components {
        let selectors = gen.selectors(vc, c);
        if selectors.root.is_none() {
            continue;
        }
        if is_collection(c) {
            return Some((c, selectors, true));
        }
        if fallback.is_none() {
            fallback = Some((c, selectors, false));
        }
    }
    fallback
}

/// Resolve the API surface for an entity only when a schema for it exists in
/// the graph. Without schemas (`ifml-generate` without `--schemas`) this
/// returns `None`, which keeps API-dependent test kinds out of the output.
async fn schema_backed_api(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    entity: &str,
    api_version: &str,
) -> Option<ResolvedApi> {
    let in_graph = matches!(db.get_schema(entity).await, Ok(Some(_)))
        || matches!(db.get_schema(&format!("{entity}Type")).await, Ok(Some(_)));
    if !in_graph {
        return None;
    }
    resolve_entity_api(db, config, entity, api_version).await
}

/// Fixture payload entries for a component: `(field, js_literal)` pairs for
/// the component's fields, skipping primary-key and FK fields. Typed form
/// specs declare fields outside `fields`, so they are used as a fallback.
fn fixture_entries(c: &IfmlComponent) -> Vec<(String, String)> {
    let names: Vec<String> = if c.fields.is_empty() {
        form_spec(c)
            .map(|f| f.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default()
    } else {
        c.fields.clone()
    };
    let types: std::collections::HashMap<&str, &str> = c
        .fields_with_types
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    names
        .iter()
        .filter(|f| f.as_str() != "id" && !f.ends_with("_id"))
        .map(|f| {
            let rust_type = types.get(f.as_str()).copied().unwrap_or("String");
            (f.clone(), js_value_for_type(f, rust_type))
        })
        .collect()
}

/// First form field eligible for a round-trip modification: a plain string
/// field (skipping id/FK fields), with its fixture and updated values.
fn modifiable_field(c: &IfmlComponent, form: &FormSpec) -> Option<(String, String, String)> {
    let types: std::collections::HashMap<&str, &str> = c
        .fields_with_types
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    form.fields
        .iter()
        .filter(|f| f.name != "id" && !f.name.ends_with("_id"))
        .find(|f| {
            let rust_type = types
                .get(f.name.as_str())
                .copied()
                .unwrap_or("String")
                .to_ascii_lowercase();
            !rust_type.contains("bool")
                && ![
                    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
                ]
                .iter()
                .any(|n| rust_type.contains(n))
        })
        .map(|f| {
            let original = js_value_for_type(&f.name, "String");
            let updated = format!("Updated {}", f.name);
            (f.name.clone(), original, updated)
        })
}

fn js_value_for_type(field: &str, rust_type: &str) -> String {
    let t = rust_type.to_ascii_lowercase();
    if t.contains("bool") {
        return "false".to_string();
    }
    let numeric = [
        "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
    ]
    .iter()
    .any(|n| t.contains(n))
        || t.contains("decimal")
        || t.contains("integer")
        || t.contains("bigint");
    if numeric {
        return "42".to_string();
    }
    format!("'Test {field}'")
}

/// The view-level or component-level `on save`/`on submit` navigation target:
/// `(target_view, bindings)`.
fn save_navigation_target(
    vc: &IfmlViewContainer,
    c: &IfmlComponent,
) -> Option<(String, std::collections::HashMap<String, String>)> {
    let is_save =
        |e: &super::context::IfmlEvent| e.event_type == "save" || e.event_type == "submit";
    let target = |a: &IfmlAction| match a {
        IfmlAction::Navigate { target, binding } => Some((target.clone(), binding.clone())),
        _ => None,
    };
    c.events
        .iter()
        .chain(vc.events.iter())
        .find(|e| is_save(e))
        .and_then(|e| target(&e.action))
}

/// Build a `RegExp` source matching the target URL: the route plus query
/// params for each binding key (sorted, mirroring the page's `nav_url_expr`
/// ordering). Binding expressions substitute to `[^&]+`; quoted literals to
/// their escaped text.
fn url_pattern(route: &str, binding: &std::collections::HashMap<String, String>) -> String {
    let escaped_route = escape_regex(route);
    if binding.is_empty() {
        return format!("{escaped_route}$");
    }
    let mut pairs: Vec<(&String, &String)> = binding.iter().collect();
    pairs.sort();
    let params = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", escape_regex(k), binding_value_pattern(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{escaped_route}\\?{params}")
}

fn binding_value_pattern(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        escape_regex(&trimmed[1..trimmed.len() - 1])
    } else {
        "[^&]+".to_string()
    }
}

fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if "\\.+*?()|[]{}^$#".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn url_pattern_sorts_bindings_and_substitutes_expressions() {
        let mut binding = HashMap::new();
        binding.insert("region".to_string(), "\"eu\"".to_string());
        binding.insert("customerId".to_string(), "row.id".to_string());
        assert_eq!(
            url_pattern("/customerdetail", &binding),
            "/customerdetail\\?customerId=[^&]+&region=eu"
        );
        assert_eq!(
            url_pattern("/customerlist", &HashMap::new()),
            "/customerlist$"
        );
    }

    #[test]
    fn binding_value_pattern_handles_literals_and_expressions() {
        assert_eq!(binding_value_pattern("\"eu\""), "eu");
        assert_eq!(binding_value_pattern("row.id"), "[^&]+");
        assert_eq!(binding_value_pattern("a.b(c)"), "[^&]+");
    }

    #[test]
    fn escape_regex_escapes_metacharacters() {
        assert_eq!(escape_regex("/customer-detail"), "/customer-detail");
        assert_eq!(escape_regex("a.b*c"), "a\\.b\\*c");
    }

    #[test]
    fn js_value_follows_rust_types() {
        assert_eq!(js_value_for_type("active", "bool"), "false");
        assert_eq!(js_value_for_type("age", "Option< i32 >"), "42");
        assert_eq!(js_value_for_type("age", "i32"), "42");
        assert_eq!(js_value_for_type("name", "String"), "'Test name'");
    }

    #[test]
    fn spec_render_includes_all_test_kinds() {
        let spec = ViewTestSpec {
            view_name: "CustomerList".to_string(),
            label: "Customer Management".to_string(),
            route: "/customerlist".to_string(),
            render: Some(RenderTest {
                label: "Customer Management".to_string(),
                route: "/customerlist".to_string(),
                primary_testid: "grid-table".to_string(),
                assert_heading: true,
            }),
            click_throughs: vec![ClickThroughTest {
                title: "comp_grid_select navigates to CustomerDetail".to_string(),
                source_route: "/customerlist".to_string(),
                row_testid: "grid-row".to_string(),
                fixture: Fixture {
                    base_path: "/api/v1/sales/customer".to_string(),
                    entries: vec![("name".to_string(), "'Test name'".to_string())],
                },
                target_pattern: "/customerdetail\\?customerId=[^&]+".to_string(),
            }],
            validations: Vec::new(),
            round_trips: Vec::new(),
        };

        let rendered = render_spec(&spec);
        assert!(rendered.contains("import { test, expect } from '@playwright/test';"));
        assert!(rendered.contains("test.describe('Customer Management'"));
        assert!(rendered.contains("page.goto('/customerlist')"));
        assert!(
            rendered.contains("page.getByRole('heading', { name: 'Customer Management' })"),
            "{rendered}"
        );
        assert!(rendered.contains("page.getByTestId('grid-table')"));
        assert!(rendered.contains("request.post('/api/v1/sales/customer'"));
        assert!(rendered.contains("page.getByTestId('grid-row').first().click()"));
        assert!(rendered.contains("waitForURL(new RegExp('/customerdetail\\\\?customerId=[^&]+'))"));
        assert!(rendered.ends_with("});\n"));
    }
}
