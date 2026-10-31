use crate::ProjectConfig;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use codegraph_core::traits::GraphQuerier;
use serde::Serialize;

use crate::error::Result;
use crate::forms::{ZodField, ZodKind, ZodObject, ts_string_literal};
use crate::render_template_with_project;
use crate::traits::{EntityGenerator, EntityGeneratorKind, GeneratedFile};
use codegraph_config::DomainConfig;
use codegraph_core::types::PropertyNode;
use codegraph_core::types::resolve_field;
use codegraph_type_contracts::RefClassificationKind;

use crate::api::api_model::{resolve_entity_operations, resolve_path_segment};

use super::common::{collect_child_sections, collect_ui_fields};
use super::page::UiField;

/// Maps a PropertyNode to a UiField for use in templates.
#[allow(clippy::too_many_arguments)]
pub fn ui_field_from_property(
    prop: &PropertyNode,
    is_entity_ref: bool,
    is_codelist: bool,
    codelist_values: &[String],
    immutable_fields: &[String],
    pg_type: &str,
    is_range: bool,
    open_end: bool,
) -> UiField {
    let resolved = resolve_field(prop);
    let ts_type = rust_type_to_ts(&prop.rust_field_type, is_entity_ref);
    let input_type = classify_input_type(prop, is_entity_ref, is_codelist);

    // Strip the Rust `r#` prefix — it is only needed for Rust identifiers,
    // not for TypeScript / Svelte property access.
    let ts_field_name = resolved
        .rust_field_name
        .strip_prefix("r#")
        .unwrap_or(&resolved.rust_field_name)
        .to_string();
    // rust_field_name is sanitized at ingestion (no _code suffix),
    // so it matches the DTO field name directly.
    let ts_field_name = if is_entity_ref && !prop.is_array && !ts_field_name.ends_with("_id") {
        // resolve_field already appends _id for scalar EntityReference
        // columns; junction arrays keep raw names.
        format!("{}_id", ts_field_name)
    } else {
        ts_field_name
    };
    let label = field_name_to_label(&ts_field_name);

    UiField {
        name: ts_field_name,
        label,
        ts_type,
        input_type,
        is_required: prop.is_required,
        is_array: prop.is_array,
        is_entity_ref,
        is_immutable: immutable_fields.contains(&resolved.rust_field_name),
        is_codelist,
        is_range,
        codelist_values: codelist_values.to_vec(),
        description: prop.description.as_deref().unwrap_or("").to_string(),
        pg_type: pg_type.to_string(),
        open_end,
        ref_api_path: None, // Populated later by collect_ui_fields
        structured_sub_fields: vec![],
        nested_type_name: None,
    }
}

pub fn rust_type_to_ts(rust_type: &str, is_entity_ref: bool) -> String {
    if is_entity_ref {
        return "string".to_string();
    }
    match rust_type {
        "String" => "string".to_string(),
        "bool" => "boolean".to_string(),
        "i16" | "i32" | "i64" | "f32" | "f64" | "u32" | "u64" => "number".to_string(),
        t if t.contains("Decimal") => "string".to_string(),
        t if t.contains("NaiveDate") => "string".to_string(),
        t if t.contains("DateTime") => "string".to_string(),
        t if t.contains("Uuid") => "string".to_string(),
        t if t.starts_with("Vec<") => "Array<string>".to_string(),
        _ => "string".to_string(),
    }
}

pub fn classify_input_type(prop: &PropertyNode, is_entity_ref: bool, is_codelist: bool) -> String {
    if is_codelist {
        return "select".to_string();
    }
    if is_entity_ref {
        return "text".to_string();
    }
    // Range types: TSTZRANGE, DATERANGE, INT4RANGE, INT8RANGE
    if prop.pg_column_type.contains("RANGE") {
        return "date-range".to_string();
    }
    // Array types (non-codelist)
    if prop.is_array && !is_codelist {
        return "array".to_string();
    }
    match prop.effective_kind() {
        Some(RefClassificationKind::InlineEnum) => "select".to_string(),
        _ => match prop.rust_field_type.as_str() {
            "bool" => "checkbox".to_string(),
            t if t.contains("NaiveDate") => "date".to_string(),
            t if t.contains("DateTime") => "datetime-local".to_string(),
            "i16" | "i32" | "i64" | "f32" | "f64" | "u32" | "u64" => "number".to_string(),
            t if t.contains("Decimal") => "number".to_string(),
            _ if prop.pg_column_type.contains("geometry")
                || prop.pg_column_type.contains("GEOMETRY") =>
            {
                "geometry".to_string()
            }
            _ => "text".to_string(),
        },
    }
}

pub fn field_name_to_label(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One field of a [`TanstackFormPlan`]: the per-control-kind slot the
/// TanStack form component renders (issue #580).
#[derive(Debug, Clone, Serialize)]
pub struct TanstackFieldPlan {
    /// The DTO wire name (what zod validates and the payload sends).
    pub name: String,
    /// Human label (raw — HTML text interpolation in the form snippets).
    pub label: String,
    /// Human label as a TS string literal (`ts_string_literal`) for
    /// component-prop contexts.
    pub label_ts: String,
    /// Model description as a TS string literal (`""` when absent).
    pub description_ts: String,
    /// The input testid — `{module}-field-{name}`, the POM `formInput()`
    /// locator contract (issue #468 W3).
    pub testid: String,
    pub required: bool,
    /// The control kind the template branches on: `text`, `textarea`,
    /// `email`, `url`, `number`, `checkbox`, `select`, `date`,
    /// `datetime`, `uuid`, `junction`, `array`, `range`, `structured`.
    pub kind: String,
    /// The registered field component for the plain kinds
    /// (`<field.{component} ...>` in the form snippet).
    pub component: String,
    /// True for junction arrays (`refers T[]` features): the form renders
    /// `mode="array"` machinery with `pushValue`/`removeValue` rows.
    pub is_junction: bool,
    /// True when this is an array-typed codelist feature — the form value
    /// collapses to a single select (the hand-rolled form's behavior) and
    /// the payload wraps it as `[{ code }]` for the nested child DTO.
    pub is_array_codelist: bool,
    /// True for range-typed fields — rendered through the consumer
    /// `DateRangeField` (the POM fills the `{name}-value` input).
    pub is_range: bool,
    pub open_end: bool,
    pub is_array: bool,
    /// The closed codelist value set as a TS array literal
    /// (`["Open", "Done"]`), escaped via `ts_string_literal`.
    pub values_ts: String,
    /// Structured-wrapper sub-fields (rendered through the consumer
    /// `StructuredWrapperField`, mirroring the hand-rolled form).
    pub structured_sub_fields: Vec<super::page::UiSubField>,
    /// The TS default-value expression for `defaultValues`.
    pub default_ts: String,
}

/// The Rust-lowered TanStack form plan for one entity (issue #580): the
/// zod schema (from the shared `forms::` IR) plus the per-field component
/// plan. Built ONCE per entity by [`tanstack_form_plan`] and consumed by
/// both the form component and the page wrappers, so the two generators
/// cannot drift.
#[derive(Debug, Clone, Serialize)]
pub struct TanstackFormPlan {
    /// lowerCamel zod schema const, e.g. `commonTodoListSchema`.
    pub schema_name: String,
    /// The rendered `export const <name> = z.object({...});` source
    /// (built in RUST via `forms::ZodObject::render_schema` — never in
    /// Tera).
    pub schema_source: String,
    pub fields: Vec<TanstackFieldPlan>,
}

/// lowerCamel-case an entity name for the zod schema const
/// (`CommonTodoList` → `commonTodoList`).
fn lower_camel(name: &str) -> String {
    let snake = codegraph_naming::to_snake_case(name);
    let mut out = String::with_capacity(snake.len());
    for (i, part) in snake.split('_').enumerate() {
        if i == 0 {
            out.push_str(part);
        } else {
            let mut chars = part.chars();
            if let Some(c) = chars.next() {
                out.extend(c.to_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}

/// Lower one [`UiField`] into the zod field it validates. Returns `None`
/// for shapes the entity-form schema deliberately skips (see the module
/// comment).
fn zod_field_for(field: &UiField) -> Option<ZodField> {
    let kind = if field.is_junction() {
        ZodKind::Array(Box::new(ZodField {
            name: String::new(),
            kind: ZodKind::Uuid,
            required: true,
            checks: Vec::new(),
        }))
    } else if field.is_codelist {
        // A codelist whose values failed to resolve degrades to a plain
        // string — `z.enum([])` would reject every input.
        if field.codelist_values.is_empty() {
            ZodKind::String
        } else {
            ZodKind::Enum(field.codelist_values.clone())
        }
    } else if field.is_entity_ref {
        ZodKind::Uuid
    } else if field.is_array {
        ZodKind::Array(Box::new(ZodField {
            name: String::new(),
            kind: ZodKind::String,
            required: true,
            checks: Vec::new(),
        }))
    } else {
        match field.input_type.as_str() {
            "checkbox" => ZodKind::Boolean,
            "number" => ZodKind::Number,
            "date" => ZodKind::Date,
            "email" => ZodKind::Email,
            "url" => ZodKind::Url,
            // datetime-local: SKIPPED — the browser's datetime-local
            // input cannot carry a timezone suffix, while the shared IR's
            // `z.iso.datetime()` is strict RFC3339, so the schema would
            // reject the form's own valid state. Native input validity +
            // the server's 422 (mapped onto fields) cover the shape; the
            // submit transform still normalizes the `Z` suffix.
            "datetime-local" => return None,
            "date-range" | "geometry" => return None,
            // Structured wrappers hold object state the flat zod object
            // cannot express without `forms::` growing an object kind.
            _ if !field.structured_sub_fields.is_empty() => return None,
            _ if field.nested_type_name.is_some() => return None,
            _ => ZodKind::String,
        }
    };
    // Structured-wrapper / nested-VO object state, range strings, and
    // datetimes are skipped above; everything here is a plain scalar.
    if field.is_range || field.input_type == "geometry" {
        return None;
    }
    Some(ZodField {
        name: field.name.clone(),
        kind,
        required: field.is_required || field.is_junction(),
        checks: Vec::new(),
    })
}

impl UiField {
    /// Junction array (`refers T[]` feature): entity-ref + array. These
    /// materialize as junction tables and render TanStack `mode="array"`
    /// machinery (issue #580).
    pub fn is_junction(&self) -> bool {
        self.is_entity_ref && self.is_array
    }
}

/// Build the TanStack form plan for one entity's form fields (issue
/// #580): the zod schema lowered through the shared `forms::` IR plus the
/// per-field component plan. `module` is the snake_case pg table name the
/// testid vocabulary composes from.
pub fn tanstack_form_plan(fields: &[UiField], module: &str, entity_name: &str) -> TanstackFormPlan {
    let schema_name = format!("{}Schema", lower_camel(entity_name));

    let mut zod_fields = Vec::new();
    let mut plan_fields = Vec::new();
    for field in fields {
        if let Some(zf) = zod_field_for(field) {
            zod_fields.push(zf);
        }

        let kind = if field.is_junction() {
            "junction"
        } else if field.is_codelist {
            "select"
        } else if field.is_range {
            "range"
        } else if !field.structured_sub_fields.is_empty() || field.nested_type_name.is_some() {
            "structured"
        } else if field.is_array {
            "array"
        } else {
            match field.input_type.as_str() {
                "checkbox" => "checkbox",
                "number" => "number",
                "date" => "date",
                "datetime-local" => "datetime",
                "email" => "email",
                "url" => "url",
                _ if field.is_entity_ref => "uuid",
                _ if field.pg_type.contains("textarea") => "textarea",
                _ => "text",
            }
        };

        let component = match kind {
            "email" => "EmailField",
            "number" => "NumberField",
            "checkbox" => "CheckboxField",
            "select" => "SelectField",
            "date" => "DateField",
            "datetime" => "DateTimeField",
            "textarea" => "TextareaField",
            // Junction rows render TextField inputs; uuid entity-refs are
            // text inputs holding uuid strings (the hand-rolled form's
            // control mapping).
            _ => "TextField",
        };

        let values: Vec<String> = field
            .codelist_values
            .iter()
            .map(|v| ts_string_literal(v))
            .collect();
        let values_ts = format!("[{}]", values.join(", "));

        let is_array_codelist = field.is_codelist && field.is_array;
        let default_ts = match kind {
            "checkbox" => format!("Boolean(initialData?.{} ?? false)", field.name),
            "number" => format!(
                "initialData?.{n} != null ? Number(initialData?.{n}) : undefined",
                n = field.name
            ),
            "junction" | "array" => format!(
                "Array.isArray(initialData?.{n}) ? initialData?.{n}.map(String) : []",
                n = field.name
            ),
            "structured" if field.is_array => format!(
                "(Array.isArray(initialData?.{n}) ? initialData?.{n} : []) as \
                 Record<string, unknown>[]",
                n = field.name
            ),
            "structured" => format!(
                "(typeof initialData?.{n} === 'object' && initialData?.{n} !== null && \
                 !Array.isArray(initialData?.{n}) ? initialData?.{n} : {{}}) as \
                 Record<string, unknown>",
                n = field.name
            ),
            "datetime" => format!(
                "String(initialData?.{n} ?? '').replace(/Z$/, '')",
                n = field.name
            ),
            // Optional string-family fields default to `undefined` — an
            // optional email/enum zod branch rejects `''`, so a `?? ''`
            // default would leave the form permanently invalid.
            _ if !field.is_required => {
                format!("initialData?.{n} || undefined", n = field.name)
            }
            _ => format!("String(initialData?.{} ?? '')", field.name),
        };

        plan_fields.push(TanstackFieldPlan {
            name: field.name.clone(),
            label: field.label.clone(),
            label_ts: ts_string_literal(&field.label),
            description_ts: ts_string_literal(&field.description),
            testid: format!("{module}-field-{}", field.name),
            required: field.is_required,
            kind: kind.to_string(),
            component: component.to_string(),
            is_junction: field.is_junction(),
            is_array_codelist,
            is_range: field.is_range,
            open_end: field.open_end,
            is_array: field.is_array,
            values_ts,
            structured_sub_fields: field.structured_sub_fields.clone(),
            default_ts,
        });
    }

    let schema = ZodObject {
        name: schema_name.clone(),
        fields: zod_fields,
    };

    TanstackFormPlan {
        schema_name,
        schema_source: schema.render_schema(),
        fields: plan_fields,
    }
}

#[derive(Debug, Serialize)]
pub struct UiFormContext {
    pub entity_name: String,
    pub module_name: String,
    pub domain: String,
    pub path_segment: String,
    pub fields: Vec<UiField>,
    pub create_fields: Vec<UiField>,
    pub update_fields: Vec<UiField>,
    pub has_create: bool,
    pub has_update: bool,
    pub has_child_sections: bool,
    /// This entity's testid vocabulary, projected from `ux::plan::ids`
    /// (issue #468 W3).
    pub tid: crate::ui::testids::TidMap,
    /// The TanStack form plan (issue #580). `None` renders the
    /// hand-rolled `$state`-bound form (flag off = byte-identical); the
    /// key is skipped from the serialized context entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tanstack: Option<TanstackFormPlan>,
}

pub struct UiFormGenerator {
    output_dir: PathBuf,
}

impl UiFormGenerator {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl EntityGenerator for UiFormGenerator {
    fn kind(&self) -> EntityGeneratorKind {
        EntityGeneratorKind::UiForm
    }

    async fn generate(
        &self,
        db: &dyn GraphQuerier,
        schema_title: &str,
        domain: &str,
        config: &DomainConfig,
        tera: &tera::Tera,
        project: &ProjectConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let schema = db
            .get_schema_in_domain(schema_title, domain)
            .await?
            .ok_or_else(|| crate::error::Error::SchemaNotFound(schema_title.into()))?;

        let entity_name = schema.rust_type_name.clone();
        let module_name = schema.pg_table_name.clone();
        let domain = domain.to_string();

        if module_name.is_empty() {
            return Ok(Vec::new());
        }

        let entity_cfg = config
            .domains
            .get(&domain)
            .and_then(|d| d.get_entity_config(&entity_name));

        let path_segment = resolve_path_segment(entity_cfg, &schema);

        let operations = resolve_entity_operations(db, config, &domain, &entity_name).await;

        let has_create = operations.contains(&"create".to_string());
        let has_update = operations.contains(&"update".to_string());

        if !has_create && !has_update {
            return Ok(Vec::new());
        }

        let dto_config = entity_cfg.map(|ec| &ec.dto);
        let immutable_fields: Vec<String> = dto_config
            .map(|d| d.immutable_fields.clone())
            .unwrap_or_default();

        // Workflow-excluded fields
        let workflow = entity_cfg.and_then(|ec| ec.workflow.as_ref());
        let mut all_excluded: Vec<String> = immutable_fields.clone();
        if let Some(wf) = workflow {
            all_excluded.push(wf.status_field.clone());
            if let Some(ref approval_field) = wf.approval_status_field {
                all_excluded.push(approval_field.clone());
            }
        }

        let fields =
            collect_ui_fields(db, schema_title, &immutable_fields, Some(&domain), config).await?;

        let create_fields: Vec<UiField> = fields
            .iter()
            .filter(|f| !all_excluded.contains(&f.name))
            .cloned()
            .collect();

        let update_fields: Vec<UiField> = fields
            .iter()
            .filter(|f| !f.is_immutable && !all_excluded.contains(&f.name))
            .cloned()
            .collect();

        let child_sections = collect_child_sections(db, schema_title, config, &domain).await?;
        let has_child_sections = !child_sections.is_empty();

        // The TanStack form layer (issue #580): variant selection happens
        // HERE in Rust — the hand-rolled template's bytes are untouched.
        let tanstack = if project.codegen.entity_form {
            Some(tanstack_form_plan(
                &create_fields,
                &module_name,
                &entity_name,
            ))
        } else {
            None
        };

        let ctx = UiFormContext {
            tid: crate::ui::testids::for_module(&module_name.clone()),
            entity_name: entity_name.clone(),
            module_name,
            domain,
            path_segment,
            fields,
            create_fields,
            update_fields,
            has_create,
            has_update,
            has_child_sections,
            tanstack,
        };

        // Variant selection in Rust (issue #574 S2 precedent): flag off
        // renders the untouched hand-rolled template.
        let template = if ctx.tanstack.is_some() {
            "ui/entity_form_tanstack.tera"
        } else {
            "ui/entity_form.tera"
        };
        let content = render_template_with_project(tera, template, &ctx, project)?;
        Ok(vec![GeneratedFile {
            path: self
                .output_dir
                .join("ui")
                .join("src")
                .join("lib")
                .join("components")
                .join("forms")
                .join(format!("{}Form.svelte", entity_name)),
            content,
        }])
    }
}
