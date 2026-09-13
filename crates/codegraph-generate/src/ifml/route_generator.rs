use crate::ProjectConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_config::{DomainConfig, IfmlComponentMapping, IfmlComponentMappings, SemanticRole};
use codegraph_core::traits::GraphQuerier;
use codegraph_ifml_dsl::{
    BinOp, ChartKind, ChartSpec, ColumnDef, ComponentSpec, Expression, FormSpec, InputFieldType,
    TableSpec, UnaryOp,
};
use serde::Serialize;

use crate::error::Result;
use crate::render_template;
use crate::traits::{GeneratedFile, GlobalGenerator};
use crate::GenerationEntry;

use super::api_paths::{id_param_from, resolve_entity_api, ResolvedApi};
use super::context::{IfmlAction, IfmlComponent, IfmlEvent, IfmlViewContainer};
use super::querier::*;

pub struct IfmlRouteGenerator {
    output_dir: PathBuf,
    framework: String,
    output_paths: super::output_paths::OutputPaths,
    mappings: Option<IfmlComponentMappings>,
}

impl IfmlRouteGenerator {
    pub fn new(output_dir: &Path, framework: &str) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            framework: framework.to_string(),
            output_paths: super::output_paths::OutputPaths::for_framework(framework),
            mappings: None,
        }
    }

    pub fn with_mappings(mut self, mappings: Option<IfmlComponentMappings>) -> Self {
        self.mappings = mappings;
        self
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
        config: &DomainConfig,
        _generation_order: &[GenerationEntry],
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let querier = IfmlGraphQuerier::new(db);
        let model = querier
            .get_ifml_model()
            .await
            .map_err(crate::error::Error::Graph)?;

        if model.view_containers.is_empty() {
            return Ok(vec![]);
        }

        let mut files = vec![];

        let page_template = format!("ifml/{}/page.tera", self.framework);
        let load_template = format!("ifml/{}/page_load.tera", self.framework);

        for vc in ordered_view_containers(&model) {
            let ctx =
                build_page_context(db, config, &project.api_version, vc, self.mappings.as_ref())
                    .await;

            if let Ok(content) = render_template(tera, &page_template, &ctx) {
                files.push(GeneratedFile {
                    path: self
                        .output_dir
                        .join((self.output_paths.route_page)(&vc.name)),
                    content,
                });
            }

            if let Some(ref route_load_fn) = self.output_paths.route_load {
                let load_ctx = build_load_context(&project.api_version, vc, &ctx.components);
                if let Ok(content) = render_template(tera, &load_template, &load_ctx) {
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

/// Order view containers by the model's computed generation order
/// (targets before sources). Containers absent from the order keep their
/// graph order at the end.
fn ordered_view_containers(
    model: &super::context::IfmlModel,
) -> Vec<&super::context::IfmlViewContainer> {
    let position: HashMap<&str, usize> = model
        .generation_order
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    let mut containers: Vec<&super::context::IfmlViewContainer> =
        model.view_containers.iter().collect();
    containers.sort_by_key(|vc| {
        position
            .get(vc.name.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    containers
}

#[derive(Debug, Serialize)]
pub struct PageSvelteContext {
    pub api_version: String,
    name: String,
    label: String,
    components: Vec<PageComponentContext>,
    params: Vec<super::context::ParameterDef>,
    view_events: Vec<RenderEvent>,
    imports: Vec<RenderImport>,
    needs_goto: bool,
    needs_on_mount: bool,
    has_submit: bool,
    /// Semantic slot role of the view: `modal-view` for modals, else
    /// `shell` for landmarks.
    view_role: Option<SemanticRole>,
    /// Semantic slot role of the container grouping: `presentation-container`
    /// for xor/wizard containers.
    container_role: Option<SemanticRole>,
}

#[derive(Debug, Serialize)]
pub struct RenderImport {
    pub export_name: String,
    pub import_path: String,
}

#[derive(Debug, Serialize)]
pub struct PageComponentContext {
    name: String,
    component_type: String,
    /// Semantic slot role of the whole component (`collection`, `display`,
    /// `selection-field`); forms keep `None` — their inputs carry roles.
    role: Option<SemanticRole>,
    entity: String,
    fields: Vec<String>,
    fields_with_types: Vec<(String, String)>,
    filter: String,
    table: Option<RenderTable>,
    form: Option<RenderForm>,
    chart: Option<RenderChart>,
    mapping: Option<RenderMapping>,
    events: Vec<RenderEvent>,
    /// Ready-to-render event callback props for mapped components,
    /// e.g. `on:select={comp_grid_select}`.
    event_props: Vec<String>,
    /// Ready-to-render data prop for mapped components, e.g. `data={data.items}`.
    data_prop: String,
    /// Ready-to-render fields prop for mapped components.
    fields_prop: String,
    /// Ready-to-render submit callback prop for mapped form components.
    submit_prop: Option<String>,
    /// Joined validation expressions for mapped form components
    /// (`data-validate` attribute).
    data_validate: Option<String>,
    /// First validation message for mapped form components
    /// (`data-validate-message` attribute).
    data_validate_message: Option<String>,
    api: Option<ResolvedApi>,
    /// View parameter carrying the entity id (edit mode), when any.
    id_param: Option<String>,
    /// Fields passed to a mapped component: declared fields when present,
    /// else derived from the typed spec (table columns / form fields).
    mapped_fields: Vec<String>,
    /// Handler for row-click navigation on list/table fallback markup.
    row_handler: Option<String>,
    submit: Option<RenderSubmit>,
    submit_handler: Option<String>,
}

/// Submit wiring for a form component: fetch + success navigation.
#[derive(Debug, Serialize)]
pub struct RenderSubmit {
    handler_name: String,
    /// URL expression (JS literal) passed to fetch.
    url_expr: String,
    method: String,
    /// Navigation target URL expression from the view's save event.
    navigate_url: Option<String>,
    /// Emit the conservative client-side check that surfaces the first
    /// failing validation message before the network request.
    client_validate: bool,
}

#[derive(Debug, Serialize)]
pub struct RenderMapping {
    import_name: String,
    import_path: String,
    testid: Option<String>,
    row_testid: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RenderEvent {
    pub handler_name: String,
    pub event_type: String,
    /// "navigate" | "refresh" | "action" | "stay"
    pub action_kind: String,
    pub target: String,
    /// JS string expression evaluating to the navigation target URL.
    pub url_expr: String,
    /// Semantic slot role: `action-control` for save/submit/cancel/back/click
    /// button-style events.
    pub role: Option<SemanticRole>,
}

/// Typed-table render context derived from a `ComponentSpec::Table`
#[derive(Debug, Serialize)]
pub struct RenderTable {
    pagination: bool,
    /// `pagination` when the list spec enables pagination.
    role: Option<SemanticRole>,
    columns: Vec<RenderColumn>,
}

/// One typed table column; `binding` is the ready-to-emit data path
/// (`property` for field/lookup columns, the rendered expression for
/// expression columns)
#[derive(Debug, Serialize)]
pub struct RenderColumn {
    label: String,
    kind: String,
    binding: String,
    lookup: String,
    expr: String,
}

/// Typed-form render context derived from a `ComponentSpec::Form`
#[derive(Debug, Serialize)]
pub struct RenderForm {
    fields: Vec<RenderInputField>,
}

#[derive(Debug, Serialize)]
pub struct RenderInputField {
    name: String,
    input_type: String,
    /// Per-input slot role: `selection-field` for dropdown/radio inputs.
    input_role: Option<SemanticRole>,
    is_textarea: bool,
    is_select: bool,
    is_radio: bool,
    required: bool,
    values: Vec<String>,
    /// Validation expressions joined for a `data-validate` attribute.
    data_validate: String,
    /// Message positionally paired with the first validation; rendered as
    /// `data-validate-message` next to `data-validate`.
    message: Option<String>,
}

/// Typed-chart render context derived from a `ComponentSpec::Chart`
#[derive(Debug, Serialize)]
pub struct RenderChart {
    kind: String,
    label_field: Option<String>,
    value_fields: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PageLoadContext {
    pub api_version: String,
    name: String,
    components: Vec<PageLoadComponentContext>,
    has_fetch: bool,
    /// View parameters carrying a DSL default, rendered as
    /// `url.searchParams.get(name) ?? params.name ?? <default>` fallbacks.
    param_defaults: Vec<RenderParamDefault>,
}

/// A view parameter default: JS literal emitted as the final `??` fallback.
#[derive(Debug, Serialize)]
pub struct RenderParamDefault {
    name: String,
    default: String,
}

#[derive(Debug, Serialize)]
pub struct PageLoadComponentContext {
    /// Sanitized JS identifier for local variable names.
    name: String,
    component_type: String,
    entity: String,
    /// Legacy lowercase entity route (used by frameworks without a resolved
    /// API model).
    route_name: String,
    api: Option<ResolvedApi>,
    id_param: Option<String>,
    /// Default literal for the id param, when the view declares one.
    id_default: Option<String>,
    paginate: bool,
    fetch_list: bool,
    fetch_item: bool,
    fetch_form: bool,
}

async fn build_page_context(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    vc: &IfmlViewContainer,
    mappings: Option<&IfmlComponentMappings>,
) -> PageSvelteContext {
    let id_param = id_param_from(&vc.params);
    let mut api_cache: HashMap<String, Option<ResolvedApi>> = HashMap::new();

    let mut components = Vec::new();
    for c in &vc.components {
        let ctx = page_component_context(
            db,
            config,
            api_version,
            vc,
            c,
            id_param.as_deref(),
            mappings,
            &mut api_cache,
        )
        .await;
        components.push(ctx);
    }

    let view_events: Vec<RenderEvent> = vc.events.iter().map(render_event).collect();

    let mut imports: Vec<RenderImport> = Vec::new();
    let mut seen_imports: HashMap<String, String> = HashMap::new();
    let mut needs_goto = view_events.iter().any(|e| e.action_kind == "navigate");
    let mut has_submit = false;
    for comp in &components {
        for evt in &comp.events {
            if evt.action_kind == "navigate" {
                needs_goto = true;
            }
        }
        if let Some(ref submit) = comp.submit {
            has_submit = true;
            if submit.navigate_url.is_some() {
                needs_goto = true;
            }
        }
        if let Some(ref mapping) = comp.mapping {
            let export = mapping.import_name.clone();
            if seen_imports
                .insert(mapping.import_path.clone(), export.clone())
                .is_none()
            {
                imports.push(RenderImport {
                    export_name: export,
                    import_path: mapping.import_path.clone(),
                });
            }
        }
    }
    let needs_on_mount = view_events
        .iter()
        .any(|e| e.action_kind == "navigate" && e.event_type == "load");

    PageSvelteContext {
        api_version: api_version.to_string(),
        name: vc.name.clone(),
        label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
        components,
        params: vc.params.clone(),
        view_events,
        imports,
        needs_goto,
        needs_on_mount,
        has_submit,
        view_role: semantic_view_role(vc),
        container_role: semantic_container_role(vc),
    }
}

#[allow(clippy::too_many_arguments)]
async fn page_component_context(
    db: &dyn GraphQuerier,
    config: &DomainConfig,
    api_version: &str,
    vc: &IfmlViewContainer,
    c: &IfmlComponent,
    id_param: Option<&str>,
    mappings: Option<&IfmlComponentMappings>,
    api_cache: &mut HashMap<String, Option<ResolvedApi>>,
) -> PageComponentContext {
    let (table, form, chart) = match c.spec {
        Some(ComponentSpec::Table(ref spec)) => (Some(render_table(spec)), None, None),
        Some(ComponentSpec::Form(ref spec)) => (None, Some(render_form(spec)), None),
        Some(ComponentSpec::Chart(ref spec)) => (None, None, Some(render_chart(spec))),
        None => (None, None, None),
    };

    let kind = kind_of(c);
    let slot_role = component_role(c);
    let mapping = mappings
        .and_then(|m| m.resolve_slot(&vc.name, &c.name, &c.component_type, &kind, slot_role))
        .map(mapping_context);
    let events: Vec<RenderEvent> = c.events.iter().map(render_event).collect();
    let event_props: Vec<String> = events
        .iter()
        .filter(|e| {
            e.action_kind == "navigate" && e.event_type != "submit" && e.event_type != "save"
        })
        .map(|e| format!("on:{}={{{}}}", e.event_type, e.handler_name))
        .collect();

    let entity = c.entity.clone().unwrap_or_default();
    let api = match api_cache.get(&entity) {
        Some(resolved) => resolved.clone(),
        None => {
            let resolved = if entity.is_empty() {
                None
            } else {
                resolve_entity_api(db, config, &entity, api_version).await
            };
            api_cache.insert(entity.clone(), resolved.clone());
            resolved
        }
    };

    let mapped_fields = mapped_fields(c, table.as_ref(), form.as_ref(), chart.as_ref());

    let row_handler = if is_collection(c) {
        events
            .iter()
            .find(|e| e.action_kind == "navigate")
            .map(|e| e.handler_name.clone())
    } else {
        None
    };

    let submit = build_submit(c, api.as_ref(), id_param, &events);
    let submit_handler = submit.as_ref().map(|s| s.handler_name.clone());
    let submit_prop = submit_handler
        .as_ref()
        .map(|handler| format!("on:submit={{{handler}}}"));
    let data_validate = form.as_ref().map(|form| {
        form.fields
            .iter()
            .filter(|f| !f.data_validate.is_empty())
            .map(|f| f.data_validate.clone())
            .collect::<Vec<_>>()
            .join(" && ")
    });
    let data_validate_message = form.as_ref().and_then(|form| {
        form.fields
            .iter()
            .find(|f| !f.data_validate.is_empty())
            .and_then(|f| f.message.clone())
    });
    let data_prop = if is_collection(c) || chart.is_some() {
        "data={data.items}".to_string()
    } else if is_form_component(c) {
        "item={data.formData}".to_string()
    } else {
        "item={data.item}".to_string()
    };
    let fields_prop = format!(
        "fields={{[{}]}}",
        mapped_fields
            .iter()
            .map(|f| format!("'{f}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    PageComponentContext {
        name: c.name.clone(),
        component_type: c.component_type.clone(),
        role: slot_role,
        entity,
        fields: c.fields.clone(),
        fields_with_types: c.fields_with_types.clone(),
        filter: c.filter.clone().unwrap_or_default(),
        mapped_fields,
        mapping,
        events,
        event_props,
        data_prop,
        fields_prop,
        submit_prop,
        data_validate,
        data_validate_message,
        api,
        id_param: id_param.map(str::to_string),
        row_handler,
        submit,
        submit_handler,
        table,
        form,
        chart,
    }
}

/// The layout kind used for mapping resolution: the typed spec kind when a
/// spec is present, else the component type.
fn kind_of(c: &IfmlComponent) -> String {
    match c.spec {
        Some(ComponentSpec::Table(_)) => "table".to_string(),
        Some(ComponentSpec::Form(_)) => "form".to_string(),
        Some(ComponentSpec::Chart(_)) => "chart".to_string(),
        None => c.component_type.clone(),
    }
}

/// Whole-component slot role: tables/lists are collections, details/charts
/// are displays, dropdown/radio/checkbox component types are selection
/// fields. Forms keep `None` — the editing context is carried by their
/// per-input roles.
fn component_role(c: &IfmlComponent) -> Option<SemanticRole> {
    if is_form_component(c) {
        return None;
    }
    match c.spec {
        Some(ComponentSpec::Table(_)) => Some(SemanticRole::Collection),
        Some(ComponentSpec::Form(_)) => None,
        Some(ComponentSpec::Chart(_)) => Some(SemanticRole::Display),
        None => match c.component_type.as_str() {
            "table" | "list" => Some(SemanticRole::Collection),
            "details" | "chart" => Some(SemanticRole::Display),
            "dropdown" | "radio" | "checkbox" => Some(SemanticRole::SelectionField),
            _ => None,
        },
    }
}

/// Per-input slot role inside a form: dropdowns and radio groups are
/// selection fields.
fn input_field_role(input_type: &str) -> Option<SemanticRole> {
    match input_type {
        "dropdown" | "radio" => Some(SemanticRole::SelectionField),
        _ => None,
    }
}

/// Event/button slot role: submit-style and click events drive actions.
fn event_role(event_type: &str) -> Option<SemanticRole> {
    match event_type {
        "save" | "submit" | "cancel" | "back" | "click" => Some(SemanticRole::ActionControl),
        _ => None,
    }
}

/// View slot role: modals render as `modal-view`, landmarks are `shell`
/// candidates.
fn semantic_view_role(vc: &IfmlViewContainer) -> Option<SemanticRole> {
    if vc.is_modal {
        Some(SemanticRole::ModalView)
    } else if vc.is_landmark {
        Some(SemanticRole::Shell)
    } else {
        None
    }
}

/// Container slot role: xor/wizard groupings are presentation containers.
fn semantic_container_role(vc: &IfmlViewContainer) -> Option<SemanticRole> {
    if vc.is_xor {
        Some(SemanticRole::PresentationContainer)
    } else {
        None
    }
}

fn is_collection(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Table(_))) || c.component_type == "list"
}

fn is_form_component(c: &IfmlComponent) -> bool {
    matches!(c.spec, Some(ComponentSpec::Form(_))) || c.component_type == "form"
}

fn mapped_fields(
    c: &IfmlComponent,
    table: Option<&RenderTable>,
    form: Option<&RenderForm>,
    chart: Option<&RenderChart>,
) -> Vec<String> {
    if !c.fields.is_empty() {
        return c.fields.clone();
    }
    if let Some(table) = table {
        return table
            .columns
            .iter()
            .filter(|col| col.kind != "expr")
            .map(|col| col.binding.clone())
            .collect();
    }
    if let Some(form) = form {
        return form.fields.iter().map(|f| f.name.clone()).collect();
    }
    if let Some(chart) = chart {
        return chart.value_fields.clone();
    }
    Vec::new()
}

fn mapping_context(m: &IfmlComponentMapping) -> RenderMapping {
    RenderMapping {
        import_name: m.export_name().to_string(),
        import_path: m.path.clone(),
        testid: m.testid("root").map(str::to_string),
        row_testid: m.testid("row").map(str::to_string),
    }
}

/// Build the submit wiring for a form component: POST for create views, PUT
/// for edit views (view carries an id param); success navigates per the
/// view's save event.
fn build_submit(
    c: &IfmlComponent,
    api: Option<&ResolvedApi>,
    id_param: Option<&str>,
    events: &[RenderEvent],
) -> Option<RenderSubmit> {
    if !is_form_component(c) {
        return None;
    }
    let api = api?;
    let handler_name = format!("submit_{}", sanitize_ident(&c.name));
    let (url_expr, method) = match id_param {
        Some(param) if api.has_update => (
            format!("`{}/${{params.{param}}}`", api.base_path),
            "PUT".to_string(),
        ),
        _ => (format!("\"{}\"", api.base_path), "POST".to_string()),
    };
    let navigate_url = events
        .iter()
        .find(|e| {
            e.action_kind == "navigate" && (e.event_type == "save" || e.event_type == "submit")
        })
        .map(|e| e.url_expr.clone());
    Some(RenderSubmit {
        handler_name,
        url_expr,
        method,
        navigate_url,
        client_validate: form_has_messages(c),
    })
}

/// True when any typed form field pairs a validation with a message —
/// the signal for the submit handler's client-side message check.
fn form_has_messages(c: &IfmlComponent) -> bool {
    matches!(&c.spec, Some(ComponentSpec::Form(spec)) if spec.fields.iter().any(|f| {
        !f.validations.is_empty() && !f.messages.is_empty()
    }))
}

fn render_event(evt: &IfmlEvent) -> RenderEvent {
    let (action_kind, target, binding) = match &evt.action {
        IfmlAction::Navigate { target, binding } => ("navigate", target.clone(), binding),
        IfmlAction::Refresh { target, binding } => ("refresh", target.clone(), binding),
        IfmlAction::Action(name) => ("action", name.clone(), &HashMap::new()),
        IfmlAction::Stay => ("stay", String::new(), &HashMap::new()),
    };
    let url_expr = if action_kind == "navigate" {
        nav_url_expr(&target, binding)
    } else {
        String::new()
    };
    RenderEvent {
        handler_name: sanitize_ident(&evt.name),
        event_type: evt.event_type.clone(),
        action_kind: action_kind.to_string(),
        target,
        url_expr,
        role: event_role(&evt.event_type),
    }
}

/// Build a JS template-literal URL for a navigation target: the target view's
/// generated route (`/{name-lowercase}`) plus query params from the binding
/// map (`?key=${expr}`). Binding expressions are emitted verbatim from the
/// model; keys are sorted for deterministic output.
fn nav_url_expr(target: &str, binding: &HashMap<String, String>) -> String {
    let path = format!("/{}", target.to_lowercase());
    if binding.is_empty() {
        return format!("\"{path}\"");
    }
    let mut pairs: Vec<(&String, &String)> = binding.iter().collect();
    pairs.sort();
    let query: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}=${{{v}}}")).collect();
    format!("`{path}?{}`", query.join("&"))
}

fn sanitize_ident(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "component".to_string()
    } else if cleaned.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("_{cleaned}")
    } else {
        cleaned
    }
}

fn render_table(spec: &TableSpec) -> RenderTable {
    RenderTable {
        pagination: spec.pagination,
        role: if spec.pagination {
            Some(SemanticRole::Pagination)
        } else {
            None
        },
        columns: spec.columns.iter().map(render_column).collect(),
    }
}

fn render_column(col: &ColumnDef) -> RenderColumn {
    match col {
        ColumnDef::Field { label, field } => RenderColumn {
            label: label.clone(),
            kind: "field".to_string(),
            binding: field.property.clone(),
            lookup: String::new(),
            expr: String::new(),
        },
        ColumnDef::Lookup {
            label,
            field,
            lookup,
        } => RenderColumn {
            label: label.clone(),
            kind: "lookup".to_string(),
            binding: field.property.clone(),
            lookup: lookup.clone(),
            expr: String::new(),
        },
        ColumnDef::Expression { label, expr } => RenderColumn {
            label: label.clone(),
            kind: "expr".to_string(),
            binding: render_expression(expr),
            lookup: String::new(),
            expr: render_expression(expr),
        },
    }
}

fn render_form(spec: &FormSpec) -> RenderForm {
    RenderForm {
        fields: spec
            .fields
            .iter()
            .map(|field| {
                let (input_type, is_textarea, is_select, is_radio) = match field.input {
                    InputFieldType::TextArea => ("textarea".to_string(), true, false, false),
                    InputFieldType::Dropdown => ("dropdown".to_string(), false, true, false),
                    InputFieldType::RadioGroup => ("radio".to_string(), false, false, true),
                    InputFieldType::Custom(ref custom) => (custom.clone(), false, false, false),
                    InputFieldType::Text => ("text".to_string(), false, false, false),
                    InputFieldType::Password => ("password".to_string(), false, false, false),
                    InputFieldType::Email => ("email".to_string(), false, false, false),
                    InputFieldType::Number => ("number".to_string(), false, false, false),
                    InputFieldType::Date => ("date".to_string(), false, false, false),
                    InputFieldType::Time => ("time".to_string(), false, false, false),
                    InputFieldType::DateTime => ("datetime-local".to_string(), false, false, false),
                    InputFieldType::Checkbox | InputFieldType::Toggle => {
                        ("checkbox".to_string(), false, false, false)
                    }
                    InputFieldType::File => ("file".to_string(), false, false, false),
                    InputFieldType::Hidden => ("hidden".to_string(), false, false, false),
                };
                let validations: Vec<String> =
                    field.validations.iter().map(render_expression).collect();
                let message = if validations.is_empty() {
                    None
                } else {
                    field.messages.first().cloned()
                };
                RenderInputField {
                    name: field.name.clone(),
                    input_role: input_field_role(&input_type),
                    input_type,
                    is_textarea,
                    is_select,
                    is_radio,
                    required: field.required,
                    values: field.values.clone(),
                    data_validate: validations.join(" && "),
                    message,
                }
            })
            .collect(),
    }
}

fn render_chart(spec: &ChartSpec) -> RenderChart {
    RenderChart {
        kind: match spec.kind {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Pie => "pie",
            ChartKind::Radar => "radar",
            ChartKind::Metric => "metric",
        }
        .to_string(),
        label_field: spec.label_field.clone(),
        value_fields: spec.value_fields.clone(),
    }
}

/// Render an IFML expression as a plain-text placeholder binding
/// (not evaluated — the generated markup keeps it verbatim).
fn render_expression(expr: &Expression) -> String {
    match expr {
        Expression::Ident(name) => name.clone(),
        Expression::StringLit(value) => format!("\"{}\"", value.replace('"', "\\\"")),
        Expression::NumLit(value) => format!("{value}"),
        Expression::BoolLit(value) => value.to_string(),
        Expression::FieldExpr { object, field } => {
            format!("{}.{}", render_expression(object), field)
        }
        Expression::BinOp { left, op, right } => format!(
            "{} {} {}",
            render_expression(left),
            bin_op_symbol(op),
            render_expression(right)
        ),
        Expression::UnaryOp { op, operand } => match op {
            UnaryOp::Not => format!("!{}", render_expression(operand)),
            UnaryOp::Neg => format!("-{}", render_expression(operand)),
        },
        Expression::Group(inner) => format!("({})", render_expression(inner)),
        Expression::Call { name, args } => {
            let rendered: Vec<String> = args.iter().map(render_expression).collect();
            format!("{name}({})", rendered.join(", "))
        }
    }
}

fn bin_op_symbol(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::RegexMatch => "=~",
        BinOp::NegRegex => "!~",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Build the `+page.ts` load context. Only the first list/details/form
/// component contributes a fetch per page, mirroring the single-return load
/// contract; each fetch resolves the entity API path from the graph.
fn build_load_context(
    api_version: &str,
    vc: &IfmlViewContainer,
    components: &[PageComponentContext],
) -> PageLoadContext {
    let id_param = id_param_from(&vc.params);
    let param_defaults: Vec<RenderParamDefault> = vc
        .params
        .iter()
        .filter_map(|p| {
            p.default.as_ref().map(|d| RenderParamDefault {
                name: p.name.clone(),
                default: d.clone(),
            })
        })
        .collect();
    let id_default = id_param
        .as_deref()
        .and_then(|name| vc.params.iter().find(|p| p.name == name))
        .and_then(|p| p.default.clone());
    let mut load_components = Vec::new();
    let mut has_list = false;
    let mut has_details = false;
    let mut has_form_fetch = false;
    let mut has_fetch = false;

    for comp in components {
        let is_list = comp.table.is_some() || comp.component_type == "list";
        let is_details = comp.component_type == "details";
        let is_form = comp.form.is_some() || comp.component_type == "form";

        let mut fetch_list = false;
        let mut fetch_item = false;
        let mut fetch_form = false;
        if comp.api.is_some() && !comp.entity.is_empty() {
            if is_list && !has_list {
                fetch_list = true;
            } else if is_details && !has_details && id_param.is_some() {
                fetch_item = true;
            } else if is_form && !has_form_fetch && id_param.is_some() {
                fetch_form = true;
            }
        }
        has_list |= fetch_list;
        has_details |= fetch_item;
        has_form_fetch |= fetch_form;
        has_fetch |= fetch_list || fetch_item || fetch_form;

        load_components.push(PageLoadComponentContext {
            name: sanitize_ident(&comp.name),
            component_type: comp.component_type.clone(),
            entity: comp.entity.clone(),
            route_name: comp.entity.to_lowercase(),
            api: comp.api.clone(),
            id_param: id_param.clone(),
            id_default: id_default.clone(),
            paginate: fetch_list && (comp.table.as_ref().map(|t| t.pagination).unwrap_or(true)),
            fetch_list,
            fetch_item,
            fetch_form,
        });
    }

    PageLoadContext {
        api_version: api_version.to_string(),
        name: vc.name.clone(),
        components: load_components,
        has_fetch,
        param_defaults,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template_engine::create_tera;
    use codegraph_core::mock::MockEngine;
    use codegraph_ifml_dsl::PropertyRef;

    fn table_spec() -> ComponentSpec {
        ComponentSpec::Table(TableSpec {
            columns: vec![
                ColumnDef::Field {
                    label: "Name".to_string(),
                    field: PropertyRef {
                        entity: "Customer".to_string(),
                        property: "name".to_string(),
                    },
                },
                ColumnDef::Lookup {
                    label: "Status".to_string(),
                    field: PropertyRef {
                        entity: "Customer".to_string(),
                        property: "status".to_string(),
                    },
                    lookup: "status_labels".to_string(),
                },
                ColumnDef::Expression {
                    label: "Tenure".to_string(),
                    expr: Expression::Call {
                        name: "tenure_years".to_string(),
                        args: vec![Expression::FieldExpr {
                            object: Box::new(Expression::Ident("Customer".to_string())),
                            field: "hire_date".to_string(),
                        }],
                    },
                },
            ],
            pagination: true,
        })
    }

    fn component_with_spec(spec: Option<ComponentSpec>) -> IfmlComponent {
        IfmlComponent {
            name: "grid".to_string(),
            component_type: "table".to_string(),
            mode: None,
            entity: Some("Customer".to_string()),
            fields: Vec::new(),
            fields_with_types: Vec::new(),
            filter: None,
            properties: HashMap::new(),
            events: Vec::new(),
            parts: Vec::new(),
            spec,
        }
    }

    #[test]
    fn specless_component_yields_no_render_contexts() {
        let ctx = page_component_context_sync(&component_with_spec(None));
        assert!(ctx.table.is_none());
        assert!(ctx.form.is_none());
        assert!(ctx.chart.is_none());
        assert_eq!(ctx.component_type, "table");
    }

    fn page_component_context_sync(c: &IfmlComponent) -> PageComponentContext {
        page_component_context_for_tests(c, &IfmlComponentMappings::default())
    }

    fn test_config() -> DomainConfig {
        toml::from_str(
            r#"
[defaults]
api_version = "v1"

[domains.sales]
label = "Sales"
schema_dir = "sales"
postgres_schema = "sales"
entities = ["CustomerType"]
"#,
        )
        .unwrap()
    }

    fn page_component_context_for_tests(
        c: &IfmlComponent,
        mappings: &IfmlComponentMappings,
    ) -> PageComponentContext {
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let mut cache = HashMap::new();
        futures::executor::block_on(page_component_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            c,
            None,
            Some(mappings),
            &mut cache,
        ))
    }

    #[test]
    fn table_spec_maps_column_kinds_and_bindings() {
        let ctx = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let table = ctx.table.expect("table render context");
        assert!(table.pagination);
        assert_eq!(table.columns.len(), 3);

        assert_eq!(table.columns[0].kind, "field");
        assert_eq!(table.columns[0].binding, "name");
        assert_eq!(table.columns[1].kind, "lookup");
        assert_eq!(table.columns[1].lookup, "status_labels");
        assert_eq!(table.columns[1].binding, "status");
        assert_eq!(table.columns[2].kind, "expr");
        assert_eq!(table.columns[2].binding, "tenure_years(Customer.hire_date)");
    }

    #[test]
    fn form_spec_maps_input_types_and_validations() {
        let spec = ComponentSpec::Form(FormSpec {
            fields: vec![
                codegraph_ifml_dsl::FieldDef {
                    name: "name".to_string(),
                    input: InputFieldType::Text,
                    required: true,
                    validations: vec![Expression::BinOp {
                        left: Box::new(Expression::Call {
                            name: "len".to_string(),
                            args: vec![Expression::Ident("name".to_string())],
                        }),
                        op: BinOp::Gt,
                        right: Box::new(Expression::NumLit(2.0)),
                    }],
                    values: Vec::new(),
                    messages: vec!["Name too short".to_string()],
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "start".to_string(),
                    input: InputFieldType::DateTime,
                    required: false,
                    validations: Vec::new(),
                    values: Vec::new(),
                    messages: Vec::new(),
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "tier".to_string(),
                    input: InputFieldType::Dropdown,
                    required: false,
                    validations: Vec::new(),
                    values: vec!["gold".to_string(), "silver".to_string()],
                    messages: Vec::new(),
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "stars".to_string(),
                    input: InputFieldType::Custom("stars".to_string()),
                    required: false,
                    validations: Vec::new(),
                    values: Vec::new(),
                    messages: Vec::new(),
                },
            ],
        });
        let ctx = page_component_context_sync(&component_with_spec(Some(spec)));
        let form = ctx.form.expect("form render context");
        assert_eq!(form.fields[0].input_type, "text");
        assert!(form.fields[0].required);
        assert_eq!(form.fields[0].data_validate, "len(name) > 2");
        assert_eq!(form.fields[0].message.as_deref(), Some("Name too short"));
        assert_eq!(form.fields[1].input_type, "datetime-local");
        assert!(form.fields[2].is_select);
        assert_eq!(form.fields[2].values, vec!["gold", "silver"]);
        assert_eq!(form.fields[3].input_type, "stars");
    }

    #[test]
    fn form_message_renders_validate_message_and_client_check() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let spec = ComponentSpec::Form(FormSpec {
            fields: vec![codegraph_ifml_dsl::FieldDef {
                name: "title".to_string(),
                input: InputFieldType::Text,
                required: true,
                validations: vec![Expression::BinOp {
                    left: Box::new(Expression::Call {
                        name: "len".to_string(),
                        args: vec![Expression::Ident("title".to_string())],
                    }),
                    op: BinOp::Gt,
                    right: Box::new(Expression::NumLit(2.0)),
                }],
                values: Vec::new(),
                messages: vec!["Title too short".to_string()],
            }],
        });
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            Some(spec),
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "data-validate=\"len(title) > 2\" data-validate-message=\"Title too short\""
            ),
            "{rendered}"
        );
        assert!(rendered.contains("data-testid=\"grid-form\""), "{rendered}");
        assert!(
            rendered.contains("data-testid=\"grid-submit\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("data-testid=\"grid-error\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("form.querySelector(':invalid')"),
            "{rendered}"
        );
        assert!(
            rendered.contains("invalid?.getAttribute('data-validate-message')"),
            "{rendered}"
        );

        let plain = ComponentSpec::Form(FormSpec {
            fields: vec![codegraph_ifml_dsl::FieldDef {
                name: "title".to_string(),
                input: InputFieldType::Text,
                required: true,
                validations: Vec::new(),
                values: Vec::new(),
                messages: Vec::new(),
            }],
        });
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            Some(plain),
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("checkValidity"), "{rendered}");
        assert!(rendered.contains("data-testid=\"grid-form\""), "{rendered}");
        assert!(
            rendered.contains(
                "const formData = Object.fromEntries(new FormData(event.currentTarget as HTMLFormElement));"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn load_context_renders_param_default_fallbacks() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let list = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let mut details = component_with_spec(None);
        details.component_type = "details".to_string();
        let details = page_component_context_sync(&details);
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            params: vec![
                super::super::context::ParameterDef {
                    name: "slug".to_string(),
                    type_ref: "String".to_string(),
                    default: Some("'home'".to_string()),
                },
                super::super::context::ParameterDef {
                    name: "customerId".to_string(),
                    type_ref: "Uuid".to_string(),
                    default: Some("'00000000-0000-0000-0000-000000000000'".to_string()),
                },
            ],
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = build_load_context("v1", &vc, &[list, details]);
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(
            rendered.contains(
                "paramDefaults['slug'] = url.searchParams.get('slug') ?? params.slug ?? 'home';"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("result.params = paramDefaults;"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "const customerId = url.searchParams.get('customerId') ?? params.customerId ?? '00000000-0000-0000-0000-000000000000';"
            ),
            "{rendered}"
        );

        let bare_vc = IfmlViewContainer {
            params: vec![super::super::context::ParameterDef {
                name: "customerId".to_string(),
                type_ref: "Uuid".to_string(),
                default: None,
            }],
            ..vc
        };
        let list = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let ctx = build_load_context("v1", &bare_vc, &[list]);
        let rendered = render_template(&tera, "ifml/svelte/page_load.tera", &ctx).expect("render");
        assert!(!rendered.contains("paramDefaults"), "{rendered}");
    }

    #[test]
    fn chart_spec_maps_kind_and_axes() {
        let spec = ComponentSpec::Chart(ChartSpec {
            kind: ChartKind::Bar,
            label_field: Some("region".to_string()),
            value_fields: vec!["revenue".to_string(), "cost".to_string()],
        });
        let ctx = page_component_context_sync(&component_with_spec(Some(spec)));
        let chart = ctx.chart.expect("chart render context");
        assert_eq!(chart.kind, "bar");
        assert_eq!(chart.label_field.as_deref(), Some("region"));
        assert_eq!(chart.value_fields, vec!["revenue", "cost"]);
    }

    #[test]
    fn render_expression_covers_operators_and_calls() {
        let expr = Expression::BinOp {
            left: Box::new(Expression::Call {
                name: "len".to_string(),
                args: vec![Expression::Ident("name".to_string())],
            }),
            op: BinOp::Gt,
            right: Box::new(Expression::NumLit(2.0)),
        };
        assert_eq!(render_expression(&expr), "len(name) > 2");

        let and = Expression::BinOp {
            left: Box::new(Expression::BoolLit(true)),
            op: BinOp::And,
            right: Box::new(Expression::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(Expression::BoolLit(false)),
            }),
        };
        assert_eq!(render_expression(&and), "true && !false");

        let string = Expression::StringLit("a \"quoted\" b".to_string());
        assert_eq!(render_expression(&string), "\"a \\\"quoted\\\" b\"");
    }

    #[test]
    fn nav_url_builds_query_from_sorted_bindings() {
        let mut binding = HashMap::new();
        binding.insert("customerId".to_string(), "row.id".to_string());
        binding.insert("region".to_string(), "\"eu\"".to_string());
        assert_eq!(
            nav_url_expr("CustomerDetail", &binding),
            "`/customerdetail?customerId=${row.id}&region=${\"eu\"}`"
        );
        assert_eq!(
            nav_url_expr("CustomerList", &HashMap::new()),
            "\"/customerlist\""
        );
    }

    #[test]
    fn navigate_event_resolves_handler_and_url() {
        let evt = IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: vec!["row".to_string()],
            action: IfmlAction::Navigate {
                target: "CustomerDetail".to_string(),
                binding: HashMap::new(),
            },
        };
        let rendered = render_event(&evt);
        assert_eq!(rendered.handler_name, "comp_grid_select");
        assert_eq!(rendered.action_kind, "navigate");
        assert_eq!(rendered.url_expr, "\"/customerdetail\"");
    }

    fn svelte_context(components: Vec<PageComponentContext>) -> PageSvelteContext {
        PageSvelteContext {
            api_version: "v1".to_string(),
            name: "View".to_string(),
            label: "View".to_string(),
            components,
            params: Vec::new(),
            view_events: Vec::new(),
            imports: Vec::new(),
            needs_goto: false,
            needs_on_mount: false,
            has_submit: false,
            view_role: None,
            container_role: None,
        }
    }

    #[test]
    fn template_renders_typed_table_and_form_markup() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let table_ctx = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let ctx = svelte_context(vec![table_ctx]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("<table data-testid=\"grid-table\" data-pagination=\"true\">"),
            "{rendered}"
        );
        assert!(
            rendered.contains("<tr data-testid=\"grid-row\">"),
            "{rendered}"
        );
        assert!(rendered.contains("<th>Name</th>"), "{rendered}");
        assert!(
            rendered.contains("<td>{item.tenure_years(Customer.hire_date)}</td>"),
            "{rendered}"
        );

        let chart = ComponentSpec::Chart(ChartSpec {
            kind: ChartKind::Pie,
            label_field: None,
            value_fields: vec!["revenue".to_string()],
        });
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            Some(chart),
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(rendered.contains("data-chart-kind=\"pie\""), "{rendered}");
        assert!(
            rendered.contains("data-value-fields=\"revenue\""),
            "{rendered}"
        );
    }

    #[test]
    fn template_specless_table_component_renders_nothing() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let ctx = svelte_context(vec![page_component_context_sync(&component_with_spec(
            None,
        ))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("<table"));
        assert!(!rendered.contains("<form"));
        assert!(!rendered.contains("data-chart-kind"));
        assert!(!rendered.contains("<h1>"));
        assert!(rendered.trim_end().ends_with("</svelte:head>"));
    }

    #[test]
    fn mapped_component_renders_invocation_with_import_and_events() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let mut c = component_with_spec(Some(table_spec()));
        c.events.push(IfmlEvent {
            name: "comp_grid_select".to_string(),
            event_type: "select".to_string(),
            params: vec![],
            action: IfmlAction::Navigate {
                target: "CustomerDetail".to_string(),
                binding: HashMap::new(),
            },
        });
        let mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
kind = "table"
path = "$lib/components/DataTable.svelte"
export = "DataTable"
testids = { root = "data-table", row = "data-row" }
"#,
        )
        .unwrap();
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            params: Vec::new(),
            components: vec![c],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc,
            Some(&mappings),
        ));

        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(
            rendered.contains("import DataTable from '$lib/components/DataTable.svelte';"),
            "{rendered}"
        );
        assert!(
            rendered.contains("import { goto } from '$app/navigation';"),
            "{rendered}"
        );
        assert!(rendered.contains("<DataTable"), "{rendered}");
        assert!(rendered.contains("data={data.items}"), "{rendered}");
        assert!(
            rendered.contains("fields={['name', 'status']}"),
            "{rendered}"
        );
        assert!(
            rendered.contains("on:select={comp_grid_select}"),
            "{rendered}"
        );
        assert!(rendered.contains("testid=\"data-table\""), "{rendered}");
        assert!(rendered.contains("rowTestid=\"data-row\""), "{rendered}");
        assert!(!rendered.contains("<table"), "{rendered}");
    }

    #[test]
    fn load_context_wires_first_list_with_pagination() {
        let list = page_component_context_sync(&component_with_spec(Some(table_spec())));
        let second_list = page_component_context_sync(&component_with_spec(None));
        let details = {
            let mut c = component_with_spec(None);
            c.component_type = "details".to_string();
            page_component_context_sync(&c)
        };
        let vc = IfmlViewContainer {
            name: "CustomerList".to_string(),
            label: None,
            is_xor: false,
            is_default: false,
            is_landmark: true,
            is_modal: false,
            params: vec![super::super::context::ParameterDef {
                name: "customerId".to_string(),
                type_ref: "Uuid".to_string(),
                default: None,
            }],
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = build_load_context("v1", &vc, &[list, second_list, details]);
        assert!(ctx.has_fetch);
        assert_eq!(ctx.components.len(), 3);
        assert!(ctx.components[0].fetch_list);
        assert!(ctx.components[0].paginate);
        assert!(
            !ctx.components[1].fetch_list,
            "second list must not double-fetch"
        );
        assert!(ctx.components[2].fetch_item);
        assert_eq!(ctx.components[2].id_param.as_deref(), Some("customerId"));
    }

    fn field_def(name: &str, input: InputFieldType) -> codegraph_ifml_dsl::FieldDef {
        codegraph_ifml_dsl::FieldDef {
            name: name.to_string(),
            input,
            required: false,
            validations: Vec::new(),
            values: Vec::new(),
            messages: Vec::new(),
        }
    }

    #[test]
    fn form_save_event_gets_action_control_role() {
        let evt = |event_type: &str| IfmlEvent {
            name: format!("comp_form_{event_type}"),
            event_type: event_type.to_string(),
            params: Vec::new(),
            action: IfmlAction::Navigate {
                target: "CustomerList".to_string(),
                binding: HashMap::new(),
            },
        };
        for event_type in ["save", "submit", "cancel", "back", "click"] {
            let rendered = render_event(&evt(event_type));
            assert_eq!(
                rendered.role,
                Some(SemanticRole::ActionControl),
                "{event_type}"
            );
        }
        assert_eq!(render_event(&evt("select")).role, None);
        assert_eq!(render_event(&evt("load")).role, None);
    }

    #[test]
    fn dropdown_and_radio_inputs_carry_selection_field_role() {
        let spec = ComponentSpec::Form(FormSpec {
            fields: vec![
                field_def("tier", InputFieldType::Dropdown),
                field_def("channel", InputFieldType::RadioGroup),
                field_def("name", InputFieldType::Text),
            ],
        });
        let ctx = page_component_context_sync(&component_with_spec(Some(spec)));
        let form = ctx.form.expect("form render context");
        assert_eq!(
            form.fields[0].input_role,
            Some(SemanticRole::SelectionField)
        );
        assert_eq!(
            form.fields[1].input_role,
            Some(SemanticRole::SelectionField)
        );
        assert_eq!(form.fields[2].input_role, None);
    }

    #[test]
    fn component_roles_classify_slots() {
        let table = component_with_spec(Some(table_spec()));
        assert_eq!(
            page_component_context_sync(&table).role,
            Some(SemanticRole::Collection)
        );

        let mut details = component_with_spec(None);
        details.component_type = "details".to_string();
        assert_eq!(
            page_component_context_sync(&details).role,
            Some(SemanticRole::Display)
        );

        let mut dropdown = component_with_spec(None);
        dropdown.component_type = "dropdown".to_string();
        assert_eq!(
            page_component_context_sync(&dropdown).role,
            Some(SemanticRole::SelectionField)
        );

        let form = component_with_spec(Some(ComponentSpec::Form(FormSpec { fields: vec![] })));
        assert_eq!(page_component_context_sync(&form).role, None);
    }

    #[test]
    fn view_roles_mark_modal_landmark_and_xor_containers() {
        let vc = |is_modal: bool, is_landmark: bool, is_xor: bool| IfmlViewContainer {
            name: "CustomerDialog".to_string(),
            label: None,
            is_xor,
            is_default: false,
            is_landmark,
            is_modal,
            params: Vec::new(),
            components: vec![],
            events: Vec::new(),
            containers: Vec::new(),
        };
        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(true, false, false),
            None,
        ));
        assert_eq!(ctx.view_role, Some(SemanticRole::ModalView));
        assert_eq!(ctx.container_role, None);

        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(false, true, false),
            None,
        ));
        assert_eq!(ctx.view_role, Some(SemanticRole::Shell));

        let ctx = futures::executor::block_on(build_page_context(
            &MockEngine::new(),
            &test_config(),
            "v1",
            &vc(false, false, true),
            None,
        ));
        assert_eq!(ctx.view_role, None);
        assert_eq!(
            ctx.container_role,
            Some(SemanticRole::PresentationContainer)
        );
    }

    #[test]
    fn paginated_table_gets_pagination_role() {
        let paged = TableSpec {
            columns: vec![],
            pagination: true,
        };
        assert_eq!(render_table(&paged).role, Some(SemanticRole::Pagination));
        let plain = TableSpec {
            columns: vec![],
            pagination: false,
        };
        assert_eq!(render_table(&plain).role, None);
    }

    #[test]
    fn role_mapping_resolves_for_collection_slot() {
        let mappings: IfmlComponentMappings = toml::from_str(
            r#"
[[component]]
role = "collection"
path = "$lib/components/Collection.svelte"
"#,
        )
        .unwrap();
        let mut c = component_with_spec(Some(table_spec()));
        c.component_type = "list".to_string();
        let ctx = page_component_context_for_tests(&c, &mappings);
        let mapping = ctx.mapping.expect("role-mapped component");
        assert_eq!(mapping.import_name, "Collection");
    }
}
