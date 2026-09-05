use crate::generate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use codegraph_ifml_dsl::{
    BinOp, ChartKind, ChartSpec, ColumnDef, ComponentSpec, Expression, FormSpec, InputFieldType,
    TableSpec, UnaryOp,
};
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

        for vc in &model.view_containers {
            if let Ok(content) = render_page_svelte(vc, tera, &page_template, &project.api_version)
            {
                files.push(GeneratedFile {
                    path: self
                        .output_dir
                        .join((self.output_paths.route_page)(&vc.name)),
                    content,
                });
            }

            if let Some(ref route_load_fn) = self.output_paths.route_load {
                if let Ok(content) =
                    render_page_load(vc, tera, &load_template, &project.api_version)
                {
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
    pub api_version: String,
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
    fields_with_types: Vec<(String, String)>,
    filter: String,
    table: Option<RenderTable>,
    form: Option<RenderForm>,
    chart: Option<RenderChart>,
}

/// Typed-table render context derived from a `ComponentSpec::Table`
#[derive(Debug, Serialize)]
pub struct RenderTable {
    pagination: bool,
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
    is_textarea: bool,
    is_select: bool,
    is_radio: bool,
    required: bool,
    values: Vec<String>,
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
}

#[derive(Debug, Serialize)]
pub struct PageLoadComponentContext {
    component_type: String,
    entity: String,
    route_name: String,
}

fn render_page_svelte(
    vc: &super::context::IfmlViewContainer,
    tera: &tera::Tera,
    template: &str,
    api_version: &str,
) -> Result<String> {
    let ctx = PageSvelteContext {
        api_version: api_version.to_string(),
        name: vc.name.clone(),
        label: vc.label.clone().unwrap_or_else(|| vc.name.clone()),
        components: vc.components.iter().map(page_component_context).collect(),
        params: vc.params.clone(),
    };
    render_template(tera, template, &ctx)
}

fn page_component_context(c: &super::context::IfmlComponent) -> PageComponentContext {
    let (table, form, chart) = match c.spec {
        Some(ComponentSpec::Table(ref spec)) => (Some(render_table(spec)), None, None),
        Some(ComponentSpec::Form(ref spec)) => (None, Some(render_form(spec)), None),
        Some(ComponentSpec::Chart(ref spec)) => (None, None, Some(render_chart(spec))),
        None => (None, None, None),
    };
    PageComponentContext {
        name: c.name.clone(),
        component_type: c.component_type.clone(),
        entity: c.entity.clone().unwrap_or_default(),
        fields: c.fields.clone(),
        fields_with_types: c.fields_with_types.clone(),
        filter: c.filter.clone().unwrap_or_default(),
        table,
        form,
        chart,
    }
}

fn render_table(spec: &TableSpec) -> RenderTable {
    RenderTable {
        pagination: spec.pagination,
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
                RenderInputField {
                    name: field.name.clone(),
                    input_type,
                    is_textarea,
                    is_select,
                    is_radio,
                    required: field.required,
                    values: field.values.clone(),
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

fn render_page_load(
    vc: &super::context::IfmlViewContainer,
    tera: &tera::Tera,
    template: &str,
    api_version: &str,
) -> Result<String> {
    let ctx = PageLoadContext {
        api_version: api_version.to_string(),
        name: vc.name.clone(),
        components: vc
            .components
            .iter()
            .map(|c| {
                let route_name = c
                    .entity
                    .as_ref()
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                PageLoadComponentContext {
                    component_type: c.component_type.clone(),
                    entity: c.entity.clone().unwrap_or_default(),
                    route_name,
                }
            })
            .collect(),
    };
    render_template(tera, template, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::template_engine::create_tera;
    use std::collections::HashMap;

    fn table_spec() -> ComponentSpec {
        ComponentSpec::Table(TableSpec {
            columns: vec![
                ColumnDef::Field {
                    label: "Name".to_string(),
                    field: codegraph_ifml_dsl::PropertyRef {
                        entity: "Customer".to_string(),
                        property: "name".to_string(),
                    },
                },
                ColumnDef::Lookup {
                    label: "Status".to_string(),
                    field: codegraph_ifml_dsl::PropertyRef {
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

    fn component_with_spec(spec: Option<ComponentSpec>) -> super::super::context::IfmlComponent {
        super::super::context::IfmlComponent {
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
        let ctx = page_component_context(&component_with_spec(None));
        assert!(ctx.table.is_none());
        assert!(ctx.form.is_none());
        assert!(ctx.chart.is_none());
        assert_eq!(ctx.component_type, "table");
    }

    #[test]
    fn table_spec_maps_column_kinds_and_bindings() {
        let ctx = page_component_context(&component_with_spec(Some(table_spec())));
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
    fn form_spec_maps_input_types() {
        let spec = ComponentSpec::Form(FormSpec {
            fields: vec![
                codegraph_ifml_dsl::FieldDef {
                    name: "name".to_string(),
                    input: InputFieldType::Text,
                    required: true,
                    validations: Vec::new(),
                    values: Vec::new(),
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "start".to_string(),
                    input: InputFieldType::DateTime,
                    required: false,
                    validations: Vec::new(),
                    values: Vec::new(),
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "tier".to_string(),
                    input: InputFieldType::Dropdown,
                    required: false,
                    validations: Vec::new(),
                    values: vec!["gold".to_string(), "silver".to_string()],
                },
                codegraph_ifml_dsl::FieldDef {
                    name: "stars".to_string(),
                    input: InputFieldType::Custom("stars".to_string()),
                    required: false,
                    validations: Vec::new(),
                    values: Vec::new(),
                },
            ],
        });
        let ctx = page_component_context(&component_with_spec(Some(spec)));
        let form = ctx.form.expect("form render context");
        assert_eq!(form.fields[0].input_type, "text");
        assert!(form.fields[0].required);
        assert_eq!(form.fields[1].input_type, "datetime-local");
        assert!(form.fields[2].is_select);
        assert_eq!(form.fields[2].values, vec!["gold", "silver"]);
        assert_eq!(form.fields[3].input_type, "stars");
    }

    #[test]
    fn chart_spec_maps_kind_and_axes() {
        let spec = ComponentSpec::Chart(ChartSpec {
            kind: ChartKind::Bar,
            label_field: Some("region".to_string()),
            value_fields: vec!["revenue".to_string(), "cost".to_string()],
        });
        let ctx = page_component_context(&component_with_spec(Some(spec)));
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

    fn svelte_context(components: Vec<PageComponentContext>) -> PageSvelteContext {
        PageSvelteContext {
            api_version: "v1".to_string(),
            name: "View".to_string(),
            label: "View".to_string(),
            components,
            params: Vec::new(),
        }
    }

    #[test]
    fn template_renders_typed_table_and_form_markup() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let table_ctx = page_component_context(&component_with_spec(Some(table_spec())));
        let ctx = svelte_context(vec![table_ctx]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(rendered.contains("<table data-pagination=\"true\">"));
        assert!(rendered.contains("<th>Name</th>"));
        assert!(rendered.contains("<td>{item.tenure_years(Customer.hire_date)}</td>"));

        let chart = ComponentSpec::Chart(ChartSpec {
            kind: ChartKind::Pie,
            label_field: None,
            value_fields: vec!["revenue".to_string()],
        });
        let ctx = svelte_context(vec![page_component_context(&component_with_spec(Some(
            chart,
        )))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(rendered.contains("data-chart-kind=\"pie\""));
        assert!(rendered.contains("data-value-fields=\"revenue\""));
    }

    #[test]
    fn template_specless_table_component_renders_nothing() {
        let tera = create_tera(Path::new(".")).expect("tera");
        let ctx = svelte_context(vec![page_component_context(&component_with_spec(None))]);
        let rendered = render_template(&tera, "ifml/svelte/page.tera", &ctx).expect("render");
        assert!(!rendered.contains("<table"));
        assert!(!rendered.contains("<form"));
        assert!(!rendered.contains("data-chart-kind"));
        assert!(!rendered.contains("<h1>"));
        assert!(rendered.trim_end().ends_with("</svelte:head>"));
    }
}
